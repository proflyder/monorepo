package store

import (
	"context"
	"errors"
	"path/filepath"
	"testing"
	"time"

	"ricardo.local/monorepo/solutions/currency-bot/internal/model"
)

func openTest(t *testing.T) *Store {
	t.Helper()
	st, err := Open(context.Background(), filepath.Join(t.TempDir(), "test.db"))
	if err != nil {
		t.Fatal(err)
	}
	t.Cleanup(func() { _ = st.Close() })
	return st
}

func recordAt(ts time.Time, value float64) model.Record {
	return model.Record{Timestamp: ts, Rates: model.Rates{
		USDToKZT: model.ExchangeRate{Buy: value, Sell: value + 1},
		RUBToKZT: model.ExchangeRate{Buy: value / 100, Sell: value/100 + 0.1},
	}}
}

func TestSaveAndRoundTrip(t *testing.T) {
	ctx := context.Background()
	st := openTest(t)
	ts := time.Date(2026, 10, 31, 12, 0, 0, 0, time.UTC)
	count, err := st.SaveRecord(ctx, recordAt(ts, 480))
	if err != nil {
		t.Fatal(err)
	}
	if count != 1 {
		t.Fatalf("count = %d, want 1", count)
	}
	if _, err := st.SaveRecord(ctx, recordAt(ts.Add(time.Hour), 481)); err != nil {
		t.Fatal(err)
	}
	records, err := st.AllRecords(ctx)
	if err != nil {
		t.Fatal(err)
	}
	if len(records) != 2 {
		t.Fatalf("records = %d, want 2", len(records))
	}
	if !records[0].Timestamp.Equal(ts.Add(time.Hour)) {
		t.Fatalf("expected newest first, got %v", records[0].Timestamp)
	}
	if records[1].Rates.USDToKZT.Buy != 480 || records[1].Rates.USDToKZT.Sell != 481 {
		t.Fatalf("values did not round-trip: %+v", records[1].Rates)
	}
}

func TestRecordBeforeSemantics(t *testing.T) {
	ctx := context.Background()
	st := openTest(t)
	base := time.Date(2026, 10, 31, 12, 0, 0, 0, time.UTC)
	for _, offset := range []time.Duration{0, -30 * time.Minute, -50 * time.Minute, -3 * time.Hour} {
		if _, err := st.SaveRecord(ctx, recordAt(base.Add(offset), 500)); err != nil {
			t.Fatal(err)
		}
	}

	// Cutoff at base-1h: the newest record at or before it is base-3h.
	got, err := st.RecordBefore(ctx, base.Add(-time.Hour))
	if err != nil {
		t.Fatal(err)
	}
	if !got.Timestamp.Equal(base.Add(-3 * time.Hour)) {
		t.Fatalf("got %v, want the newest qualifying record at -3h", got.Timestamp)
	}

	// No record older than base-4h.
	if _, err := st.RecordBefore(ctx, base.Add(-4*time.Hour)); !errors.Is(err, ErrNotFound) {
		t.Fatalf("expected ErrNotFound, got %v", err)
	}
}

func TestDateRangeIncludesBothBoundaries(t *testing.T) {
	ctx := context.Background()
	st := openTest(t)
	base := time.Date(2026, 10, 31, 0, 0, 0, 0, time.UTC)
	for _, offset := range []time.Duration{-time.Hour, 0, 12 * time.Hour, 24 * time.Hour, 25 * time.Hour} {
		if _, err := st.SaveRecord(ctx, recordAt(base.Add(offset), 500)); err != nil {
			t.Fatal(err)
		}
	}
	got, err := st.RecordsByDateRange(ctx, base, base.Add(24*time.Hour))
	if err != nil {
		t.Fatal(err)
	}
	if len(got) != 3 {
		t.Fatalf("expected both inclusive boundaries, got %d", len(got))
	}
}

func TestLatestDeleteAndCleanup(t *testing.T) {
	ctx := context.Background()
	st := openTest(t)
	now := time.Date(2026, 10, 31, 12, 0, 0, 0, time.UTC)
	if _, err := st.LatestRecord(ctx); !errors.Is(err, ErrNotFound) {
		t.Fatalf("empty latest = %v", err)
	}
	if _, err := st.SaveRecord(ctx, recordAt(now.Add(-40*24*time.Hour), 1)); err != nil {
		t.Fatal(err)
	}
	if _, err := st.SaveRecord(ctx, recordAt(now.Add(-time.Hour), 2)); err != nil {
		t.Fatal(err)
	}
	latest, err := st.LatestRecord(ctx)
	if err != nil {
		t.Fatal(err)
	}
	if latest.Rates.USDToKZT.Buy != 2 {
		t.Fatalf("latest = %+v", latest.Rates)
	}

	removed, err := st.CleanOldRecords(ctx, 35*24*time.Hour, now)
	if err != nil {
		t.Fatal(err)
	}
	if removed != 1 {
		t.Fatalf("removed = %d, want 1", removed)
	}
	if remaining, _ := st.CountRecords(ctx); remaining != 1 {
		t.Fatalf("remaining = %d", remaining)
	}

	deleted, err := st.DeleteAll(ctx)
	if err != nil || deleted != 1 {
		t.Fatalf("deleted = %d, err = %v", deleted, err)
	}
	if deleted, _ := st.DeleteAll(ctx); deleted != 0 {
		t.Fatalf("repeat delete = %d", deleted)
	}
}

func TestSentAlertsUpsertAndClear(t *testing.T) {
	ctx := context.Background()
	st := openTest(t)
	key := model.AlertKey{Pair: model.USDToKZT, Period: "HOUR", RateType: model.Sell}
	if _, err := st.GetSentAlert(ctx, key); !errors.Is(err, ErrNotFound) {
		t.Fatalf("missing key = %v", err)
	}
	first := model.SentAlert{Key: key, Level: model.Warning, Direction: model.Up, RateAtAlert: 100, ChangePercent: 0.6}
	if err := st.RecordSentAlert(ctx, first); err != nil {
		t.Fatal(err)
	}
	second := model.SentAlert{Key: key, Level: model.Critical, Direction: model.Up, RateAtAlert: 101, ChangePercent: 1.2}
	if err := st.RecordSentAlert(ctx, second); err != nil {
		t.Fatal(err)
	}
	got, err := st.GetSentAlert(ctx, key)
	if err != nil {
		t.Fatal(err)
	}
	if got.Level != model.Critical || got.RateAtAlert != 101 {
		t.Fatalf("upsert did not replace: %+v", got)
	}
	if n, _ := st.CountSentAlerts(ctx); n != 1 {
		t.Fatalf("expected one row, got %d", n)
	}
	other := model.AlertKey{Pair: model.RUBToKZT, Period: "DAY", RateType: model.Buy}
	if err := st.RecordSentAlert(ctx, model.SentAlert{Key: other, Level: model.Warning, Direction: model.Down, RateAtAlert: 5}); err != nil {
		t.Fatal(err)
	}
	if n, _ := st.CountSentAlerts(ctx); n != 2 {
		t.Fatalf("expected two independent rows, got %d", n)
	}
	if err := st.ClearSentAlert(ctx, key); err != nil {
		t.Fatal(err)
	}
	if _, err := st.GetSentAlert(ctx, key); !errors.Is(err, ErrNotFound) {
		t.Fatalf("key should be cleared, got %v", err)
	}
	if _, err := st.GetSentAlert(ctx, other); err != nil {
		t.Fatalf("other key must survive: %v", err)
	}
	if err := st.ClearSentAlert(ctx, key); err != nil {
		t.Fatalf("clearing a missing key must be safe: %v", err)
	}
}

func TestInsertRecordIfAbsentIsIdempotent(t *testing.T) {
	ctx := context.Background()
	st := openTest(t)
	rec := recordAt(time.Date(2026, 10, 31, 12, 0, 0, 0, time.UTC), 500)
	inserted, err := st.InsertRecordIfAbsent(ctx, rec)
	if err != nil || !inserted {
		t.Fatalf("first insert = %v, %v", inserted, err)
	}
	inserted, err = st.InsertRecordIfAbsent(ctx, rec)
	if err != nil || inserted {
		t.Fatalf("second insert should be skipped: %v, %v", inserted, err)
	}
	if n, _ := st.CountRecords(ctx); n != 1 {
		t.Fatalf("count = %d", n)
	}
}
