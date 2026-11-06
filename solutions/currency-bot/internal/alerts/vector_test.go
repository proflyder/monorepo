package alerts

import (
	"context"
	"flag"
	"math"
	"testing"
	"time"

	"github.com/proflyder/monorepo/solutions/currency-bot/internal/golden"
	"github.com/proflyder/monorepo/solutions/currency-bot/internal/model"
)

const alertsVectorPath = "../../../../solutions/currency-bot/testdata/vectors/alerts.json"

var updateGolden = flag.Bool("update", false, "rewrite golden vectors")

type evaluateOut struct {
	Level         string  `json:"level,omitempty"`
	Direction     string  `json:"direction,omitempty"`
	ChangePercent float64 `json:"change_percent"`
}

type thresholdCase struct {
	Name       string       `json:"name"`
	Current    any          `json:"current"`
	Historical any          `json:"historical"`
	Period     string       `json:"period"`
	Want       *evaluateOut `json:"want,omitempty"`
}

type dedupInput struct {
	Level     string  `json:"level"`
	Direction string  `json:"direction"`
	Rate      float64 `json:"rate"`
}

type dedupCase struct {
	Name      string      `json:"name"`
	Candidate dedupInput  `json:"candidate"`
	Last      *dedupInput `json:"last,omitempty"`
	Period    string      `json:"period"`
	Send      bool        `json:"send"`
}

type checkCase struct {
	Name       string  `json:"name"`
	Current    float64 `json:"current"`
	Historical float64 `json:"historical"`
	Alerts     int     `json:"alerts"`
	Clears     int     `json:"clears"`
}

type alertsVectors struct {
	Thresholds []thresholdCase `json:"thresholds"`
	Dedup      []dedupCase     `json:"dedup"`
	Checks     []checkCase     `json:"checks"`
}

func specialFloat(value any) float64 {
	switch typed := value.(type) {
	case float64:
		return typed
	case string:
		switch typed {
		case "NaN":
			return math.NaN()
		case "+Inf":
			return math.Inf(1)
		case "-Inf":
			return math.Inf(-1)
		}
	}
	panic("unsupported vector number")
}

func vectorPeriod(name string) model.Period {
	for _, period := range model.Periods {
		if period.Name == name {
			return period
		}
	}
	panic("unknown period " + name)
}

func TestAlertVectors(t *testing.T) {
	checker := New(nil, nil, nil, func() time.Time { return fixedNow })

	thresholds := []thresholdCase{
		{Name: "warning_up", Current: 100.59, Historical: 100.0, Period: "HOUR"},
		{Name: "critical_up", Current: 103.09, Historical: 100.0, Period: "HOUR"},
		{Name: "warning_down", Current: 99.41, Historical: 100.0, Period: "HOUR"},
		{Name: "below_warning", Current: 100.1, Historical: 100.0, Period: "HOUR"},
		{Name: "inclusive_critical", Current: 101.0, Historical: 100.0, Period: "HOUR"},
		{Name: "half_percent_boundary", Current: 100.505, Historical: 100.0, Period: "HOUR"},
		{Name: "zero_historical", Current: 100.0, Historical: 0.0, Period: "HOUR"},
		{Name: "negative_historical", Current: 100.0, Historical: -100.0, Period: "HOUR"},
		{Name: "nan_historical", Current: 100.0, Historical: "NaN", Period: "HOUR"},
		{Name: "inf_historical", Current: 100.0, Historical: "+Inf", Period: "HOUR"},
		{Name: "neg_inf_historical", Current: 100.0, Historical: "-Inf", Period: "HOUR"},
		{Name: "zero_current", Current: 0.0, Historical: 100.0, Period: "HOUR"},
		{Name: "nan_current", Current: "NaN", Historical: 100.0, Period: "HOUR"},
		{Name: "inf_current", Current: "+Inf", Historical: 100.0, Period: "HOUR"},
	}
	for index := range thresholds {
		c := thresholdCase{
			Name:       thresholds[index].Name,
			Current:    thresholds[index].Current,
			Historical: thresholds[index].Historical,
			Period:     thresholds[index].Period,
		}
		alert := checker.evaluate(vectorPeriod(c.Period), model.USDToKZT, model.Sell,
			specialFloat(c.Current), specialFloat(c.Historical))
		if alert != nil {
			c.Want = &evaluateOut{
				Level:         string(alert.Level),
				Direction:     string(alert.Direction),
				ChangePercent: alert.ChangePercent,
			}
		}
		thresholds[index] = c
	}

	dedup := []dedupCase{
		{Name: "no_previous", Candidate: dedupInput{Level: "WARNING", Direction: "UP", Rate: 100.6}, Period: "HOUR"},
		{Name: "escalation", Candidate: dedupInput{Level: "CRITICAL", Direction: "UP", Rate: 100.6},
			Last: &dedupInput{Level: "WARNING", Direction: "UP", Rate: 100.6}, Period: "HOUR"},
		{Name: "direction_flip", Candidate: dedupInput{Level: "WARNING", Direction: "DOWN", Rate: 100.6},
			Last: &dedupInput{Level: "WARNING", Direction: "UP", Rate: 100.6}, Period: "HOUR"},
		{Name: "move_beyond_band", Candidate: dedupInput{Level: "WARNING", Direction: "UP", Rate: 498},
			Last: &dedupInput{Level: "WARNING", Direction: "UP", Rate: 495}, Period: "HOUR"},
		{Name: "suppressed", Candidate: dedupInput{Level: "WARNING", Direction: "UP", Rate: 498},
			Last: &dedupInput{Level: "WARNING", Direction: "UP", Rate: 498}, Period: "HOUR"},
		{Name: "zero_last_rate", Candidate: dedupInput{Level: "WARNING", Direction: "UP", Rate: 498},
			Last: &dedupInput{Level: "WARNING", Direction: "UP", Rate: 0}, Period: "HOUR"},
	}
	for index := range dedup {
		c := dedup[index]
		candidate := candidateFor(c.Candidate)
		var last *model.SentAlert
		if c.Last != nil {
			last = &model.SentAlert{
				Key:         candidate.Key(),
				Level:       model.Level(c.Last.Level),
				Direction:   model.Direction(c.Last.Direction),
				RateAtAlert: c.Last.Rate,
			}
		}
		c.Send = shouldSend(candidate, last, vectorPeriod(c.Period))
		dedup[index] = c
	}

	checks := []checkCase{
		{Name: "four_alert_slots", Current: 100.59, Historical: 100.0},
		{Name: "no_thresholds_clears_all", Current: 100.01, Historical: 100.0},
		{Name: "broken_history_clears_all", Current: 100.0, Historical: 0.0},
	}
	for index := range checks {
		c := checks[index]
		history := &fakeHistory{now: fixedNow, byDuration: historyAt(fixedNow, time.Hour, c.Historical)}
		sent := newFakeSent()
		alerts, err := newChecker(history, sent).Check(context.Background(), pureRates(c.Current), false)
		if err != nil {
			t.Fatal(err)
		}
		c.Alerts = len(alerts)
		c.Clears = sent.clearCalls
		checks[index] = c
	}

	golden.Run(t, *updateGolden, alertsVectorPath, alertsVectors{
		Thresholds: thresholds,
		Dedup:      dedup,
		Checks:     checks,
	})
}

func candidateFor(input dedupInput) model.Alert {
	return model.Alert{
		Level:         model.Level(input.Level),
		Period:        model.Periods[0],
		Pair:          model.USDToKZT,
		RateType:      model.Sell,
		Direction:     model.Direction(input.Direction),
		ChangePercent: 1,
		OldRate:       100,
		NewRate:       input.Rate,
	}
}
