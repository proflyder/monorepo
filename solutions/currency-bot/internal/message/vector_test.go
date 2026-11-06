package message

import (
	"flag"
	"testing"

	"github.com/proflyder/monorepo/solutions/currency-bot/internal/golden"
	"github.com/proflyder/monorepo/solutions/currency-bot/internal/model"
)

const formatVectorPath = "../../../../solutions/currency-bot/testdata/vectors/format.json"

var updateGolden = flag.Bool("update", false, "rewrite golden vectors")

type rateVector struct {
	Buy  float64 `json:"buy"`
	Sell float64 `json:"sell"`
}

type ratesVector struct {
	USD rateVector `json:"usd"`
	RUB rateVector `json:"rub"`
}

type alertVector struct {
	Level         string  `json:"level"`
	Period        string  `json:"period"`
	Pair          string  `json:"pair"`
	RateType      string  `json:"rate_type"`
	Direction     string  `json:"direction"`
	ChangePercent float64 `json:"change_percent"`
	OldRate       float64 `json:"old_rate"`
	NewRate       float64 `json:"new_rate"`
}

type formatCase struct {
	Name   string        `json:"name"`
	Rates  ratesVector   `json:"rates"`
	Alerts []alertVector `json:"alerts"`
	Text   string        `json:"text"`
}

type formatVectors struct {
	Cases []formatCase `json:"cases"`
}

func toModelRates(in ratesVector) model.Rates {
	return model.Rates{
		USDToKZT: model.ExchangeRate{Buy: in.USD.Buy, Sell: in.USD.Sell},
		RUBToKZT: model.ExchangeRate{Buy: in.RUB.Buy, Sell: in.RUB.Sell},
	}
}

func toModelAlert(in alertVector) model.Alert {
	return model.Alert{
		Level:         model.Level(in.Level),
		Period:        period(in.Period),
		Pair:          model.Pair(in.Pair),
		RateType:      model.RateType(in.RateType),
		Direction:     model.Direction(in.Direction),
		ChangePercent: in.ChangePercent,
		OldRate:       in.OldRate,
		NewRate:       in.NewRate,
	}
}

func TestFormatVectors(t *testing.T) {
	baseRates := ratesVector{
		USD: rateVector{Buy: 485.50, Sell: 487.20},
		RUB: rateVector{Buy: 4.85, Sell: 4.92},
	}
	x5Rates := ratesVector{
		USD: rateVector{Buy: 1.005, Sell: 2.675},
		RUB: rateVector{Buy: 0.125, Sell: 480.005},
	}

	cases := []formatCase{
		{Name: "no_alerts", Rates: baseRates},
		{
			Name:  "warning_up_sell",
			Rates: baseRates,
			Alerts: []alertVector{{
				Level: "WARNING", Period: "HOUR", Pair: "USD_TO_KZT", RateType: "SELL",
				Direction: "UP", ChangePercent: 0.80, OldRate: 480, NewRate: 483.84,
			}},
		},
		{
			Name:  "buy_label_inverted",
			Rates: baseRates,
			Alerts: []alertVector{{
				Level: "WARNING", Period: "DAY", Pair: "USD_TO_KZT", RateType: "BUY",
				Direction: "UP", ChangePercent: 1.2, OldRate: 480, NewRate: 485.76,
			}},
		},
		{
			Name:  "critical_down",
			Rates: baseRates,
			Alerts: []alertVector{{
				Level: "CRITICAL", Period: "DAY", Pair: "USD_TO_KZT", RateType: "SELL",
				Direction: "DOWN", ChangePercent: -2.5, OldRate: 500, NewRate: 487.5,
			}},
		},
		{
			Name:  "warnings_before_critical",
			Rates: baseRates,
			Alerts: []alertVector{
				{
					Level: "CRITICAL", Period: "DAY", Pair: "USD_TO_KZT", RateType: "SELL",
					Direction: "DOWN", ChangePercent: 2.5, OldRate: 500, NewRate: 487.5,
				},
				{
					Level: "WARNING", Period: "HOUR", Pair: "USD_TO_KZT", RateType: "SELL",
					Direction: "UP", ChangePercent: 0.8, OldRate: 480, NewRate: 483.84,
				},
			},
		},
		{
			Name:  "all_periods",
			Rates: baseRates,
			Alerts: []alertVector{
				{
					Level: "WARNING", Period: "HOUR", Pair: "USD_TO_KZT", RateType: "SELL",
					Direction: "UP", ChangePercent: 0.8, OldRate: 480, NewRate: 483.84,
				},
				{
					Level: "WARNING", Period: "DAY", Pair: "USD_TO_KZT", RateType: "SELL",
					Direction: "UP", ChangePercent: 1.2, OldRate: 480, NewRate: 485.76,
				},
				{
					Level: "WARNING", Period: "WEEK", Pair: "RUB_TO_KZT", RateType: "BUY",
					Direction: "DOWN", ChangePercent: -2.1, OldRate: 5.0, NewRate: 4.895,
				},
				{
					Level: "WARNING", Period: "MONTH", Pair: "RUB_TO_KZT", RateType: "SELL",
					Direction: "UP", ChangePercent: 3.1, OldRate: 5.0, NewRate: 5.155,
				},
			},
		},
		{
			Name:  "x5_rates_and_change",
			Rates: x5Rates,
			Alerts: []alertVector{
				{
					Level: "WARNING", Period: "HOUR", Pair: "USD_TO_KZT", RateType: "SELL",
					Direction: "UP", ChangePercent: 0.805, OldRate: 1.005, NewRate: 1.015,
				},
				{
					Level: "CRITICAL", Period: "MONTH", Pair: "RUB_TO_KZT", RateType: "BUY",
					Direction: "DOWN", ChangePercent: -2.675, OldRate: 2.675, NewRate: 2.595,
				},
			},
		},
	}

	for index := range cases {
		alerts := make([]model.Alert, 0, len(cases[index].Alerts))
		for _, item := range cases[index].Alerts {
			alerts = append(alerts, toModelAlert(item))
		}
		cases[index].Text = Format(toModelRates(cases[index].Rates), alerts)
	}

	golden.Run(t, *updateGolden, formatVectorPath, formatVectors{Cases: cases})
}
