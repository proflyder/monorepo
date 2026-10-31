// Command currency-bot-import loads H2 CSV exports into the SQLite database.
package main

import (
	"context"
	"flag"
	"fmt"
	"os"

	"ricardo.local/monorepo/solutions/currency-bot/internal/importer"
	"ricardo.local/monorepo/solutions/currency-bot/internal/store"
)

func main() {
	dbPath := flag.String("db", envOr("DB_PATH", "/data/currency-bot.db"), "SQLite database path")
	historyPath := flag.String("history", "", "CURRENCY_HISTORY CSV export")
	alertsPath := flag.String("alerts", "", "SENT_ALERTS CSV export")
	flag.Parse()

	if *historyPath == "" && *alertsPath == "" {
		fmt.Fprintln(os.Stderr, "currency-bot-import: provide -history and/or -alerts")
		os.Exit(2)
	}

	ctx := context.Background()
	st, err := store.Open(ctx, *dbPath)
	if err != nil {
		fmt.Fprintf(os.Stderr, "currency-bot-import: open database: %v\n", err)
		os.Exit(1)
	}
	defer func() { _ = st.Close() }()

	if *historyPath != "" {
		file, err := os.Open(*historyPath)
		if err != nil {
			fmt.Fprintf(os.Stderr, "currency-bot-import: open history CSV: %v\n", err)
			os.Exit(1)
		}
		result, err := importer.ImportHistory(ctx, st, file)
		_ = file.Close()
		if err != nil {
			fmt.Fprintf(os.Stderr, "currency-bot-import: import history: %v\n", err)
			os.Exit(1)
		}
		fmt.Printf("currency_history: inserted=%d skipped=%d\n", result.Inserted, result.Skipped)
	}

	if *alertsPath != "" {
		file, err := os.Open(*alertsPath)
		if err != nil {
			fmt.Fprintf(os.Stderr, "currency-bot-import: open alerts CSV: %v\n", err)
			os.Exit(1)
		}
		result, err := importer.ImportSentAlerts(ctx, st, file)
		_ = file.Close()
		if err != nil {
			fmt.Fprintf(os.Stderr, "currency-bot-import: import alerts: %v\n", err)
			os.Exit(1)
		}
		fmt.Printf("sent_alerts: upserted=%d\n", result.Upserted)
	}
}

func envOr(key, def string) string {
	if value := os.Getenv(key); value != "" {
		return value
	}
	return def
}
