package importer

import (
	"context"
	"path/filepath"
	"strings"
	"testing"

	"ricardo.local/monorepo/solutions/currency-bot/internal/store"
)

func openStore(t *testing.T) *store.Store {
	t.Helper()
	st, err := store.Open(context.Background(), filepath.Join(t.TempDir(), "import.db"))
	if err != nil {
		t.Fatal(err)
	}
	t.Cleanup(func() { _ = st.Close() })
	return st
}

const historyCSV = `"ID";"TIMESTAMP";"USD_BUY";"USD_SELL";"RUB_BUY";"RUB_SELL";
"1";"2026-10-01 10:00:00";"480.5";"482.5";"4.8";"4.9";
"2";"2026-10-01 11:00:00";"481.5";"483.5";"4.85";"4.95";
`

func TestImportHistoryIsIdempotent(t *testing.T) {
	ctx := context.Background()
	st := openStore(t)

	result, err := ImportHistory(ctx, st, strings.NewReader(historyCSV))
	if err != nil {
		t.Fatal(err)
	}
	if result.Inserted != 2 || result.Skipped != 0 {
		t.Fatalf("first import = %+v", result)
	}
	if count, _ := st.CountRecords(ctx); count != 2 {
		t.Fatalf("count = %d", count)
	}

	result, err = ImportHistory(ctx, st, strings.NewReader(historyCSV))
	if err != nil {
		t.Fatal(err)
	}
	if result.Inserted != 0 || result.Skipped != 2 {
		t.Fatalf("second import = %+v", result)
	}
	if count, _ := st.CountRecords(ctx); count != 2 {
		t.Fatalf("count after reimport = %d", count)
	}

	records, err := st.AllRecords(ctx)
	if err != nil {
		t.Fatal(err)
	}
	if records[0].Rates.USDToKZT.Buy != 481.5 || records[0].Rates.RUBToKZT.Sell != 4.95 {
		t.Fatalf("values = %+v", records[0].Rates)
	}
	if records[0].Timestamp.UTC().Format("2006-01-02T15:04:05Z") != "2026-10-01T11:00:00Z" {
		t.Fatalf("timestamp = %v", records[0].Timestamp)
	}
}

func TestImportCommaCSVAndAlerts(t *testing.T) {
	ctx := context.Background()
	st := openStore(t)
	csvData := `ID,TIMESTAMP,USD_BUY,USD_SELL,RUB_BUY,RUB_SELL
1,2026-10-01T10:00:00Z,480.5,482.5,4.8,4.9
`
	if _, err := ImportHistory(ctx, st, strings.NewReader(csvData)); err != nil {
		t.Fatal(err)
	}

	alerts := `"ID";"PAIR";"PERIOD";"RATE_TYPE";"LEVEL";"DIRECTION";"RATE_AT_ALERT";"CHANGE_PERCENT";
"1";"USD_TO_KZT";"HOUR";"SELL";"WARNING";"UP";"482.5";"0.8";
`
	first, err := ImportSentAlerts(ctx, st, strings.NewReader(alerts))
	if err != nil {
		t.Fatal(err)
	}
	if first.Upserted != 1 {
		t.Fatalf("alerts = %+v", first)
	}
	second, err := ImportSentAlerts(ctx, st, strings.NewReader(alerts))
	if err != nil {
		t.Fatal(err)
	}
	if second.Upserted != 1 {
		t.Fatalf("upsert = %+v", second)
	}
	if count, _ := st.CountSentAlerts(ctx); count != 1 {
		t.Fatalf("dedup rows = %d", count)
	}
}

func TestImportRejectsMissingColumns(t *testing.T) {
	_, err := ImportHistory(context.Background(), openStore(t), strings.NewReader("A,B\n1,2\n"))
	if err == nil {
		t.Fatal("expected missing column error")
	}
}
