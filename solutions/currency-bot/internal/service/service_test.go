package service

import (
	"context"
	"errors"
	"testing"
	"time"

	"ricardo.local/monorepo/solutions/currency-bot/internal/alerts"
	"ricardo.local/monorepo/solutions/currency-bot/internal/model"
)

var now = time.Date(2026, 10, 31, 12, 0, 0, 0, time.UTC)

type fakeSource struct {
	rates model.Rates
	err   error
	calls int
}

func (f *fakeSource) Fetch(context.Context) (model.Rates, error) {
	f.calls++
	return f.rates, f.err
}

type fakeHistory struct {
	saveErr  error
	saved    []model.Record
	before   model.Record
	beforeOK bool
}

func (f *fakeHistory) SaveRecord(_ context.Context, record model.Record) (int, error) {
	f.saved = append(f.saved, record)
	return len(f.saved), f.saveErr
}

func (f *fakeHistory) RecordBefore(context.Context, time.Time) (model.Record, error) {
	if !f.beforeOK {
		return model.Record{}, errors.New("no history")
	}
	return f.before, nil
}

type fakeSent struct{ recorded []model.SentAlert }

func (f *fakeSent) GetSentAlert(context.Context, model.AlertKey) (model.SentAlert, error) {
	return model.SentAlert{}, errors.New("not found")
}
func (f *fakeSent) ClearSentAlert(context.Context, model.AlertKey) error { return nil }
func (f *fakeSent) RecordSentAlert(_ context.Context, alert model.SentAlert) error {
	f.recorded = append(f.recorded, alert)
	return nil
}

type fakeSender struct {
	err  error
	sent []string
}

func (f *fakeSender) Send(_ context.Context, _ string, text string) error {
	if f.err != nil {
		return f.err
	}
	f.sent = append(f.sent, text)
	return nil
}

func ratesAt(value float64) model.Rates {
	return model.Rates{
		USDToKZT: model.ExchangeRate{Buy: value, Sell: value},
		RUBToKZT: model.ExchangeRate{Buy: value, Sell: value},
	}
}

func newService(source RatesSource, history *fakeHistory, sender *fakeSender) *Service {
	checker := alerts.New(history, &fakeSent{}, DiscardLogger(), func() time.Time { return now })
	return &Service{
		Source: source, History: history, Checker: checker,
		Recorder: &fakeSent{}, Sender: sender, Logger: DiscardLogger(),
		Now: func() time.Time { return now },
	}
}

func TestRunSkipsSendWithoutAlerts(t *testing.T) {
	history := &fakeHistory{}
	sender := &fakeSender{}
	svc := newService(&fakeSource{rates: ratesAt(500)}, history, sender)
	if err := svc.Run(context.Background(), "hourly", "1", false, true); err != nil {
		t.Fatal(err)
	}
	if len(sender.sent) != 0 {
		t.Fatalf("expected no message, got %d", len(sender.sent))
	}
	if len(history.saved) != 1 {
		t.Fatalf("expected one history row, got %d", len(history.saved))
	}
	if !history.saved[0].Timestamp.Equal(now) {
		t.Fatalf("saved timestamp = %v", history.saved[0].Timestamp)
	}
}

func TestRunForceSendsWithoutSaving(t *testing.T) {
	history := &fakeHistory{}
	sender := &fakeSender{}
	svc := newService(&fakeSource{rates: ratesAt(500)}, history, sender)
	if err := svc.Run(context.Background(), "trigger", "-100", true, false); err != nil {
		t.Fatal(err)
	}
	if len(sender.sent) != 1 {
		t.Fatalf("force notification must send, got %d", len(sender.sent))
	}
	if len(history.saved) != 0 {
		t.Fatalf("manual trigger must not save history, got %d", len(history.saved))
	}
}

func TestRunRecordsSentAlertsAfterSend(t *testing.T) {
	history := &fakeHistory{before: model.Record{Timestamp: now.Add(-time.Hour), Rates: ratesAt(100)}, beforeOK: true}
	sender := &fakeSender{}
	recorder := &fakeSent{}
	checker := alerts.New(history, recorder, DiscardLogger(), func() time.Time { return now })
	svc := &Service{
		Source: &fakeSource{rates: ratesAt(103)}, History: history, Checker: checker,
		Recorder: recorder, Sender: sender, Logger: DiscardLogger(), Now: func() time.Time { return now },
	}
	if err := svc.Run(context.Background(), "hourly", "1", false, true); err != nil {
		t.Fatal(err)
	}
	if len(sender.sent) != 1 {
		t.Fatalf("expected one message, got %d", len(sender.sent))
	}
	if len(recorder.recorded) == 0 {
		t.Fatal("expected sent alerts to be recorded")
	}
}

func TestRunToleratesSaveAndThresholdFailures(t *testing.T) {
	history := &fakeHistory{saveErr: errors.New("disk full")}
	sender := &fakeSender{}
	svc := newService(&fakeSource{rates: ratesAt(500)}, history, sender)
	if err := svc.Run(context.Background(), "hourly", "1", false, true); err != nil {
		t.Fatalf("history failure must not fail the run: %v", err)
	}
}

func TestRunFetchAndSendFailures(t *testing.T) {
	history := &fakeHistory{}
	sender := &fakeSender{}
	svc := newService(&fakeSource{err: errors.New("network")}, history, sender)
	if err := svc.Run(context.Background(), "hourly", "1", false, true); err == nil {
		t.Fatal("fetch failure must propagate")
	}
	if len(sender.sent) != 0 || len(history.saved) != 0 {
		t.Fatal("fetch failure must not save or send")
	}

	sender = &fakeSender{err: errors.New("telegram down")}
	svc = newService(&fakeSource{rates: ratesAt(500)}, &fakeHistory{}, sender)
	if err := svc.Run(context.Background(), "trigger", "1", true, false); err == nil {
		t.Fatal("send failure must propagate")
	}
}
