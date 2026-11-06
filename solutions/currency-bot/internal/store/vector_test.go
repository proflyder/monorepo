package store

import (
	"context"
	"flag"
	"path/filepath"
	"testing"
	"time"

	"github.com/proflyder/monorepo/solutions/currency-bot/internal/golden"
	"github.com/proflyder/monorepo/solutions/currency-bot/internal/model"
)

const storeVectorPath = "../../../../solutions/currency-bot/testdata/vectors/store.json"

var updateGolden = flag.Bool("update", false, "rewrite golden vectors")

type recordInput struct {
	Timestamp string  `json:"timestamp"`
	USDBuy    float64 `json:"usd_buy"`
	USDSell   float64 `json:"usd_sell"`
	RUBBuy    float64 `json:"rub_buy"`
	RUBSell   float64 `json:"rub_sell"`
}

type sentInput struct {
	Pair          string  `json:"pair"`
	Period        string  `json:"period"`
	RateType      string  `json:"rate_type"`
	Level         string  `json:"level"`
	Direction     string  `json:"direction"`
	RateAtAlert   float64 `json:"rate_at_alert"`
	ChangePercent float64 `json:"change_percent"`
}

type recordBeforeCase struct {
	Name      string `json:"name"`
	Cutoff    string `json:"cutoff"`
	Found     bool   `json:"found"`
	Timestamp string `json:"timestamp,omitempty"`
}

type rangeCase struct {
	Name       string   `json:"name"`
	From       string   `json:"from"`
	To         string   `json:"to"`
	Timestamps []string `json:"timestamps"`
}

type cleanupCase struct {
	Name        string   `json:"name"`
	RetentionHr float64  `json:"retention_hours"`
	Now         string   `json:"now"`
	Removed     int      `json:"removed"`
	Remaining   []string `json:"remaining"`
}

type storeVectors struct {
	Records      []recordInput      `json:"records"`
	SentAlerts   []sentInput        `json:"sent_alerts"`
	RecordBefore []recordBeforeCase `json:"record_before"`
	Ranges       []rangeCase        `json:"ranges"`
	Latest       string             `json:"latest"`
	Cleanup      cleanupCase        `json:"cleanup"`
	DeleteCount  int                `json:"delete_count"`
	SentUpsert   struct {
		Level   string  `json:"level"`
		Rate    float64 `json:"rate"`
		Count   int     `json:"count"`
		Cleared bool    `json:"cleared"`
		Other   bool    `json:"other_survives"`
	} `json:"sent_upsert"`
}

var (
	seedBase    = time.Date(2026, 10, 31, 12, 0, 0, 0, time.UTC)
	seedRecords = []recordInput{
		{Timestamp: "2026-10-31T12:00:00Z", USDBuy: 500, USDSell: 501, RUBBuy: 5, RUBSell: 5.1},
		{Timestamp: "2026-10-31T11:30:00Z", USDBuy: 499, USDSell: 500, RUBBuy: 4.99, RUBSell: 5.09},
		{Timestamp: "2026-10-31T09:00:00Z", USDBuy: 498, USDSell: 499, RUBBuy: 4.98, RUBSell: 5.08},
		{Timestamp: "2026-10-30T12:00:00Z", USDBuy: 497, USDSell: 498, RUBBuy: 4.97, RUBSell: 5.07},
		{Timestamp: "2026-09-26T00:00:00Z", USDBuy: 490, USDSell: 491, RUBBuy: 4.9, RUBSell: 5.0},
	}
	seedSent = []sentInput{
		{Pair: "USD_TO_KZT", Period: "HOUR", RateType: "SELL", Level: "WARNING", Direction: "UP", RateAtAlert: 500, ChangePercent: 0.6},
		{Pair: "RUB_TO_KZT", Period: "DAY", RateType: "BUY", Level: "CRITICAL", Direction: "DOWN", RateAtAlert: 5, ChangePercent: -2.1},
	}
)

func mustTime(t *testing.T, value string) time.Time {
	t.Helper()
	parsed, err := time.Parse(time.RFC3339, value)
	if err != nil {
		t.Fatal(err)
	}
	return parsed
}

func toRecord(input recordInput) model.Record {
	parsed, _ := time.Parse(time.RFC3339, input.Timestamp)
	return model.Record{
		Timestamp: parsed,
		Rates: model.Rates{
			USDToKZT: model.ExchangeRate{Buy: input.USDBuy, Sell: input.USDSell},
			RUBToKZT: model.ExchangeRate{Buy: input.RUBBuy, Sell: input.RUBSell},
		},
	}
}

func seededStore(t *testing.T) *Store {
	t.Helper()
	st, err := Open(context.Background(), filepath.Join(t.TempDir(), "vector.db"))
	if err != nil {
		t.Fatal(err)
	}
	t.Cleanup(func() { _ = st.Close() })
	for _, input := range seedRecords {
		if _, err := st.SaveRecord(context.Background(), toRecord(input)); err != nil {
			t.Fatal(err)
		}
	}
	for _, input := range seedSent {
		alert := model.SentAlert{
			Key:           model.AlertKey{Pair: model.Pair(input.Pair), Period: input.Period, RateType: model.RateType(input.RateType)},
			Level:         model.Level(input.Level),
			Direction:     model.Direction(input.Direction),
			RateAtAlert:   input.RateAtAlert,
			ChangePercent: input.ChangePercent,
		}
		if err := st.RecordSentAlert(context.Background(), alert); err != nil {
			t.Fatal(err)
		}
	}
	return st
}

func timestamps(records []model.Record) []string {
	out := make([]string, len(records))
	for index, record := range records {
		out[index] = record.Timestamp.UTC().Format(time.RFC3339)
	}
	return out
}

func TestStoreVectors(t *testing.T) {
	ctx := context.Background()
	vectors := storeVectors{
		Records:    seedRecords,
		SentAlerts: seedSent,
		RecordBefore: []recordBeforeCase{
			{Name: "exact_match", Cutoff: "2026-10-31T12:00:00Z"},
			{Name: "not_closest", Cutoff: "2026-10-31T11:00:00Z"},
			{Name: "before_everything", Cutoff: "2026-09-25T00:00:00Z"},
		},
		Ranges: []rangeCase{
			{Name: "inclusive_boundaries", From: "2026-10-31T00:00:00Z", To: "2026-11-01T00:00:00Z"},
			{Name: "single_record", From: "2026-10-31T11:00:00Z", To: "2026-10-31T11:30:00Z"},
		},
		Cleanup: cleanupCase{
			Name:        "retention_35_days",
			RetentionHr: 35 * 24,
			Now:         "2026-10-31T12:00:00Z",
		},
	}

	st := seededStore(t)
	for index := range vectors.RecordBefore {
		c := vectors.RecordBefore[index]
		record, err := st.RecordBefore(ctx, mustTime(t, c.Cutoff))
		if err == nil {
			c.Found = true
			c.Timestamp = record.Timestamp.UTC().Format(time.RFC3339)
		}
		vectors.RecordBefore[index] = c
	}
	for index := range vectors.Ranges {
		c := vectors.Ranges[index]
		records, err := st.RecordsByDateRange(ctx, mustTime(t, c.From), mustTime(t, c.To))
		if err != nil {
			t.Fatal(err)
		}
		c.Timestamps = timestamps(records)
		vectors.Ranges[index] = c
	}
	latest, err := st.LatestRecord(ctx)
	if err != nil {
		t.Fatal(err)
	}
	vectors.Latest = latest.Timestamp.UTC().Format(time.RFC3339)

	cleanupStore := seededStore(t)
	removed, err := cleanupStore.CleanOldRecords(ctx, time.Duration(vectors.Cleanup.RetentionHr)*time.Hour, mustTime(t, vectors.Cleanup.Now))
	if err != nil {
		t.Fatal(err)
	}
	vectors.Cleanup.Removed = removed
	remaining, err := cleanupStore.AllRecords(ctx)
	if err != nil {
		t.Fatal(err)
	}
	vectors.Cleanup.Remaining = timestamps(remaining)

	deleteStore := seededStore(t)
	deleted, err := deleteStore.DeleteAll(ctx)
	if err != nil {
		t.Fatal(err)
	}
	vectors.DeleteCount = deleted

	sentStore := seededStore(t)
	key := model.AlertKey{Pair: model.USDToKZT, Period: "HOUR", RateType: model.Sell}
	if err := sentStore.RecordSentAlert(ctx, model.SentAlert{Key: key, Level: model.Critical, Direction: model.Up, RateAtAlert: 501, ChangePercent: 1.2}); err != nil {
		t.Fatal(err)
	}
	got, err := sentStore.GetSentAlert(ctx, key)
	if err != nil {
		t.Fatal(err)
	}
	vectors.SentUpsert.Level = string(got.Level)
	vectors.SentUpsert.Rate = got.RateAtAlert
	vectors.SentUpsert.Count, err = sentStore.CountSentAlerts(ctx)
	if err != nil {
		t.Fatal(err)
	}
	if err := sentStore.ClearSentAlert(ctx, key); err != nil {
		t.Fatal(err)
	}
	if _, err := sentStore.GetSentAlert(ctx, key); err != nil {
		vectors.SentUpsert.Cleared = true
	}
	other := model.AlertKey{Pair: model.RUBToKZT, Period: "DAY", RateType: model.Buy}
	if _, err := sentStore.GetSentAlert(ctx, other); err == nil {
		vectors.SentUpsert.Other = true
	}

	golden.Run(t, *updateGolden, storeVectorPath, vectors)
}
