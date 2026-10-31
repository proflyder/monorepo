package alerts

import (
	"context"
	"errors"
	"testing"
	"time"

	"ricardo.local/monorepo/solutions/currency-bot/internal/model"
)

type fakeHistory struct {
	now        time.Time
	byDuration map[time.Duration]model.Record
	failAll    bool
	calls      int
}

func (f *fakeHistory) RecordBefore(_ context.Context, cutoff time.Time) (model.Record, error) {
	f.calls++
	if f.failAll {
		return model.Record{}, errors.New("db down")
	}
	record, ok := f.byDuration[f.now.Sub(cutoff)]
	if !ok {
		return model.Record{}, errors.New("not found")
	}
	return record, nil
}

type fakeSent struct {
	alerts     map[model.AlertKey]model.SentAlert
	getCalls   int
	clearCalls int
	clearErr   error
}

func newFakeSent() *fakeSent { return &fakeSent{alerts: map[model.AlertKey]model.SentAlert{}} }

func (f *fakeSent) GetSentAlert(_ context.Context, key model.AlertKey) (model.SentAlert, error) {
	f.getCalls++
	alert, ok := f.alerts[key]
	if !ok {
		return model.SentAlert{}, errors.New("not found")
	}
	return alert, nil
}

func (f *fakeSent) ClearSentAlert(_ context.Context, _ model.AlertKey) error {
	f.clearCalls++
	return f.clearErr
}

var fixedNow = time.Date(2026, 10, 31, 12, 0, 0, 0, time.UTC)

func pureRates(value float64) model.Rates {
	return model.Rates{
		USDToKZT: model.ExchangeRate{Buy: value, Sell: value},
		RUBToKZT: model.ExchangeRate{Buy: value, Sell: value},
	}
}

func historyAt(now time.Time, period time.Duration, value float64) map[time.Duration]model.Record {
	return map[time.Duration]model.Record{
		period: {Timestamp: now.Add(-period), Rates: pureRates(value)},
	}
}

func newChecker(history *fakeHistory, sent *fakeSent) *Checker {
	return New(history, sent, nil, func() time.Time { return fixedNow })
}

func TestThresholds(t *testing.T) {
	tests := []struct {
		name       string
		current    float64
		historical float64
		period     time.Duration
		wantLevel  model.Level
		wantDir    model.Direction
	}{
		{"warning 0.59", 100.59, 100, time.Hour, model.Warning, model.Up},
		{"critical 3.09", 103.09, 100, time.Hour, model.Critical, model.Up},
		{"down", 99.41, 100, time.Hour, model.Warning, model.Down},
		{"zero change below warning", 100.1, 100, time.Hour, "", model.Down},
		{"exactly 1.0 is critical for HOUR", 101, 100, time.Hour, model.Critical, model.Up},
	}
	for _, tc := range tests {
		t.Run(tc.name, func(t *testing.T) {
			history := &fakeHistory{now: fixedNow, byDuration: historyAt(fixedNow, tc.period, tc.historical)}
			sent := newFakeSent()
			checker := newChecker(history, sent)
			alerts, err := checker.Check(context.Background(), pureRates(tc.current), false)
			if err != nil {
				t.Fatal(err)
			}
			if tc.wantLevel == "" {
				if len(alerts) != 0 {
					t.Fatalf("expected no alerts, got %+v", alerts)
				}
				return
			}
			if len(alerts) != 4 {
				t.Fatalf("expected 4 alerts (both pairs, both sides), got %d: %+v", len(alerts), alerts)
			}
			for _, alert := range alerts {
				if alert.Level != tc.wantLevel || alert.Direction != tc.wantDir {
					t.Fatalf("got %s/%s, want %s/%s", alert.Level, alert.Direction, tc.wantLevel, tc.wantDir)
				}
			}
			if history.calls != 4 {
				t.Fatalf("expected exactly 4 lookbacks, got %d", history.calls)
			}
		})
	}
}

func TestBothPairsAndBothSidesAlert(t *testing.T) {
	history := &fakeHistory{now: fixedNow, byDuration: historyAt(fixedNow, time.Hour, 100)}
	checker := newChecker(history, newFakeSent())
	alerts, err := checker.Check(context.Background(), pureRates(100.59), false)
	if err != nil {
		t.Fatal(err)
	}
	if len(alerts) != 4 {
		t.Fatalf("expected 4 alerts, got %d: %+v", len(alerts), alerts)
	}
	seen := map[model.AlertKey]bool{}
	for _, a := range alerts {
		seen[a.Key()] = true
	}
	for _, key := range []model.AlertKey{
		{Pair: model.USDToKZT, Period: "HOUR", RateType: model.Sell},
		{Pair: model.USDToKZT, Period: "HOUR", RateType: model.Buy},
		{Pair: model.RUBToKZT, Period: "HOUR", RateType: model.Sell},
		{Pair: model.RUBToKZT, Period: "HOUR", RateType: model.Buy},
	} {
		if !seen[key] {
			t.Fatalf("missing alert %+v", key)
		}
	}
}

func TestNoHistoryAndFailures(t *testing.T) {
	history := &fakeHistory{now: fixedNow, failAll: true}
	checker := newChecker(history, newFakeSent())
	alerts, err := checker.Check(context.Background(), pureRates(200), false)
	if err != nil {
		t.Fatalf("check should tolerate repository failures: %v", err)
	}
	if len(alerts) != 0 {
		t.Fatalf("expected none, got %+v", alerts)
	}
	if history.calls != 4 {
		t.Fatalf("expected 4 lookbacks, got %d", history.calls)
	}
}

func candidate(level model.Level, dir model.Direction, newRate float64) model.Alert {
	return model.Alert{
		Level: level, Period: model.Periods[0], Pair: model.USDToKZT, RateType: model.Sell,
		Direction: dir, ChangePercent: 1, OldRate: 100, NewRate: newRate,
	}
}

func TestDedupRules(t *testing.T) {
	key := model.AlertKey{Pair: model.USDToKZT, Period: "HOUR", RateType: model.Sell}
	tests := []struct {
		name     string
		last     *model.SentAlert
		cand     model.Alert
		wantSend bool
	}{
		{"rule 1 no previous", nil, candidate(model.Warning, model.Up, 100.6), true},
		{"rule 2 escalation", &model.SentAlert{Key: key, Level: model.Warning, Direction: model.Up, RateAtAlert: 100.6}, candidate(model.Critical, model.Up, 100.6), true},
		{"rule 3 direction flip", &model.SentAlert{Key: key, Level: model.Warning, Direction: model.Up, RateAtAlert: 100.6}, candidate(model.Warning, model.Down, 100.6), true},
		{"rule 4 move beyond re-alert band", &model.SentAlert{Key: key, Level: model.Warning, Direction: model.Up, RateAtAlert: 495}, candidate(model.Warning, model.Up, 498), true},
		{"rule 5 suppress", &model.SentAlert{Key: key, Level: model.Warning, Direction: model.Up, RateAtAlert: 498}, candidate(model.Warning, model.Up, 498), false},
	}
	for _, tc := range tests {
		t.Run(tc.name, func(t *testing.T) {
			if got := shouldSend(tc.cand, tc.last, model.Periods[0]); got != tc.wantSend {
				t.Fatalf("shouldSend = %v, want %v", got, tc.wantSend)
			}
		})
	}
}

func TestSkipDedupReadsNothing(t *testing.T) {
	history := &fakeHistory{now: fixedNow, byDuration: historyAt(fixedNow, time.Hour, 100)}
	sent := newFakeSent()
	sent.alerts[model.AlertKey{Pair: model.USDToKZT, Period: "HOUR", RateType: model.Sell}] = model.SentAlert{
		Level: model.Warning, Direction: model.Up, RateAtAlert: 100.59,
	}
	checker := newChecker(history, sent)

	normal, err := checker.Check(context.Background(), pureRates(100.59), false)
	if err != nil {
		t.Fatal(err)
	}
	if len(normal) != 3 {
		t.Fatalf("one key should be suppressed, expected 3, got %d", len(normal))
	}
	if sent.getCalls != 4 {
		t.Fatalf("expected 4 dedup reads, got %d", sent.getCalls)
	}

	sent.getCalls, sent.clearCalls = 0, 0
	skipped, err := checker.Check(context.Background(), pureRates(100.59), true)
	if err != nil {
		t.Fatal(err)
	}
	if len(skipped) != 4 {
		t.Fatalf("skipDedup should accept all candidates, got %d", len(skipped))
	}
	if sent.getCalls != 0 || sent.clearCalls != 0 {
		t.Fatalf("skipDedup must not read or clear: get=%d clear=%d", sent.getCalls, sent.clearCalls)
	}
}

func TestNormalisedCourseClearsSentAlert(t *testing.T) {
	history := &fakeHistory{now: fixedNow, byDuration: historyAt(fixedNow, time.Hour, 100)}
	sent := newFakeSent()
	checker := newChecker(history, sent)
	if _, err := checker.Check(context.Background(), pureRates(100.01), false); err != nil {
		t.Fatal(err)
	}
	if sent.clearCalls != 4 {
		t.Fatalf("expected one clear per side/period slot, got %d", sent.clearCalls)
	}
}

func TestPartialHistoryStillReturnsAlerts(t *testing.T) {
	history := &fakeHistory{now: fixedNow, byDuration: historyAt(fixedNow, time.Hour, 100)}
	checker := newChecker(history, newFakeSent())
	alerts, err := checker.Check(context.Background(), pureRates(100.59), false)
	if err != nil {
		t.Fatal(err)
	}
	if len(alerts) == 0 {
		t.Fatal("expected HOUR alerts from the only available period")
	}
}
