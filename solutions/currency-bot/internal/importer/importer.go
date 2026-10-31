// Package importer loads H2 CSV exports of CURRENCY_HISTORY and SENT_ALERTS
// into the SQLite store. Imports are idempotent: a history row is inserted only
// when its timestamp is absent, and a sent alert is upserted by its unique key.
package importer

import (
	"bufio"
	"context"
	"encoding/csv"
	"fmt"
	"io"
	"strconv"
	"strings"
	"time"

	"ricardo.local/monorepo/solutions/currency-bot/internal/model"
	"ricardo.local/monorepo/solutions/currency-bot/internal/store"
)

// HistoryResult summarises a CURRENCY_HISTORY import.
type HistoryResult struct {
	Inserted int
	Skipped  int
}

// AlertsResult summarises a SENT_ALERTS import.
type AlertsResult struct {
	Upserted int
}

// ImportHistory reads a CURRENCY_HISTORY CSV export.
func ImportHistory(ctx context.Context, st *store.Store, r io.Reader) (HistoryResult, error) {
	var result HistoryResult
	records, header, err := readCSV(r)
	if err != nil {
		return result, err
	}
	col := indexer(header)
	required := []string{"TIMESTAMP", "USD_BUY", "USD_SELL", "RUB_BUY", "RUB_SELL"}
	if err := requireColumns(col, required); err != nil {
		return result, err
	}
	for line, row := range records {
		record, err := recordFromRow(col, row)
		if err != nil {
			return result, fmt.Errorf("row %d: %w", line+2, err)
		}
		inserted, err := st.InsertRecordIfAbsent(ctx, record)
		if err != nil {
			return result, fmt.Errorf("row %d: %w", line+2, err)
		}
		if inserted {
			result.Inserted++
		} else {
			result.Skipped++
		}
	}
	return result, nil
}

// ImportSentAlerts reads a SENT_ALERTS CSV export.
func ImportSentAlerts(ctx context.Context, st *store.Store, r io.Reader) (AlertsResult, error) {
	var result AlertsResult
	records, header, err := readCSV(r)
	if err != nil {
		return result, err
	}
	col := indexer(header)
	required := []string{"PAIR", "PERIOD", "RATE_TYPE", "LEVEL", "DIRECTION", "RATE_AT_ALERT", "CHANGE_PERCENT"}
	if err := requireColumns(col, required); err != nil {
		return result, err
	}
	for line, row := range records {
		alert, err := alertFromRow(col, row)
		if err != nil {
			return result, fmt.Errorf("row %d: %w", line+2, err)
		}
		if err := st.RecordSentAlert(ctx, alert); err != nil {
			return result, fmt.Errorf("row %d: %w", line+2, err)
		}
		result.Upserted++
	}
	return result, nil
}

func recordFromRow(col map[string]int, row []string) (model.Record, error) {
	ts, err := parseTimestamp(cell(row, col["TIMESTAMP"]))
	if err != nil {
		return model.Record{}, err
	}
	usdBuy, err := parseFloat(cell(row, col["USD_BUY"]))
	if err != nil {
		return model.Record{}, fmt.Errorf("USD_BUY: %w", err)
	}
	usdSell, err := parseFloat(cell(row, col["USD_SELL"]))
	if err != nil {
		return model.Record{}, fmt.Errorf("USD_SELL: %w", err)
	}
	rubBuy, err := parseFloat(cell(row, col["RUB_BUY"]))
	if err != nil {
		return model.Record{}, fmt.Errorf("RUB_BUY: %w", err)
	}
	rubSell, err := parseFloat(cell(row, col["RUB_SELL"]))
	if err != nil {
		return model.Record{}, fmt.Errorf("RUB_SELL: %w", err)
	}
	return model.Record{Timestamp: ts, Rates: model.Rates{
		USDToKZT: model.ExchangeRate{Buy: usdBuy, Sell: usdSell},
		RUBToKZT: model.ExchangeRate{Buy: rubBuy, Sell: rubSell},
	}}, nil
}

func alertFromRow(col map[string]int, row []string) (model.SentAlert, error) {
	rateAtAlert, err := parseFloat(cell(row, col["RATE_AT_ALERT"]))
	if err != nil {
		return model.SentAlert{}, fmt.Errorf("RATE_AT_ALERT: %w", err)
	}
	changePercent, err := parseFloat(cell(row, col["CHANGE_PERCENT"]))
	if err != nil {
		return model.SentAlert{}, fmt.Errorf("CHANGE_PERCENT: %w", err)
	}
	return model.SentAlert{
		Key: model.AlertKey{
			Pair:     model.Pair(strings.TrimSpace(cell(row, col["PAIR"]))),
			Period:   strings.TrimSpace(cell(row, col["PERIOD"])),
			RateType: model.RateType(strings.TrimSpace(cell(row, col["RATE_TYPE"]))),
		},
		Level:         model.Level(strings.TrimSpace(cell(row, col["LEVEL"]))),
		Direction:     model.Direction(strings.TrimSpace(cell(row, col["DIRECTION"]))),
		RateAtAlert:   rateAtAlert,
		ChangePercent: changePercent,
	}, nil
}

func readCSV(r io.Reader) ([][]string, []string, error) {
	br := bufio.NewReader(r)
	peek, _ := br.Peek(4096)
	comma := detectDelimiter(string(peek))
	reader := csv.NewReader(br)
	reader.Comma = comma
	reader.FieldsPerRecord = -1
	rows, err := reader.ReadAll()
	if err != nil {
		return nil, nil, err
	}
	if len(rows) == 0 {
		return nil, nil, fmt.Errorf("empty CSV")
	}
	header := make([]string, len(rows[0]))
	for i, name := range rows[0] {
		header[i] = strings.ToUpper(strings.TrimSpace(strings.TrimPrefix(name, "\ufeff")))
	}
	return rows[1:], header, nil
}

func detectDelimiter(sample string) rune {
	counts := map[rune]int{',': 0, ';': 0, '\t': 0}
	for _, r := range sample {
		if _, ok := counts[r]; ok {
			counts[r]++
		}
	}
	best := ','
	for _, candidate := range []rune{';', '\t'} {
		if counts[candidate] > counts[best] {
			best = candidate
		}
	}
	return best
}

func indexer(header []string) map[string]int {
	out := make(map[string]int, len(header))
	for i, name := range header {
		out[name] = i
	}
	return out
}

func requireColumns(col map[string]int, required []string) error {
	var missing []string
	for _, name := range required {
		if _, ok := col[name]; !ok {
			missing = append(missing, name)
		}
	}
	if len(missing) > 0 {
		return fmt.Errorf("missing columns: %s", strings.Join(missing, ", "))
	}
	return nil
}

func cell(row []string, index int) string {
	if index < 0 || index >= len(row) {
		return ""
	}
	return row[index]
}

func parseFloat(raw string) (float64, error) {
	value := strings.TrimSpace(raw)
	if value == "" {
		return 0, fmt.Errorf("empty number")
	}
	return strconv.ParseFloat(value, 64)
}

var timestampLayouts = []string{
	time.RFC3339Nano,
	"2006-01-02T15:04:05.999999999",
	"2006-01-02 15:04:05.999999999",
	"2006-01-02 15:04:05",
	"2006-01-02",
}

func parseTimestamp(raw string) (time.Time, error) {
	value := strings.TrimSpace(raw)
	if value == "" {
		return time.Time{}, fmt.Errorf("empty timestamp")
	}
	for _, layout := range timestampLayouts {
		if ts, err := time.ParseInLocation(layout, value, time.UTC); err == nil {
			return ts.UTC(), nil
		}
	}
	if millis, err := strconv.ParseInt(value, 10, 64); err == nil {
		return time.UnixMilli(millis).UTC(), nil
	}
	return time.Time{}, fmt.Errorf("unsupported timestamp %q", raw)
}
