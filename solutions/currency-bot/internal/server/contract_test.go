package server

import (
	"context"
	"errors"
	"net/http"
	"net/http/httptest"
	"os"
	"path/filepath"
	"strings"
	"testing"
	"time"

	"github.com/proflyder/monorepo/packages/go/contracttest"
	"github.com/proflyder/monorepo/solutions/currency-bot/internal/metrics"
	"github.com/proflyder/monorepo/solutions/currency-bot/internal/model"
	"github.com/proflyder/monorepo/solutions/currency-bot/internal/service"
)

const contractDirectory = "solutions/currency-bot/testdata/contract"

// contractFixtureDir resolves the shared fixture directory under `go test`
// (package cwd) and Bazel (runfiles workspace root).
func contractFixtureDir(t *testing.T) string {
	t.Helper()
	candidates := []string{
		contractDirectory,
		filepath.Join("..", "..", "..", "..", contractDirectory),
	}
	for _, candidate := range candidates {
		if info, err := os.Stat(candidate); err == nil && info.IsDir() {
			return candidate
		}
	}
	t.Fatalf("contract fixtures not found (tried %s)", strings.Join(candidates, ", "))
	return ""
}

func contractRecords() []model.Record {
	return []model.Record{
		{
			Timestamp: time.Date(2026, 10, 31, 12, 0, 0, 0, time.UTC),
			Rates: model.Rates{
				USDToKZT: model.ExchangeRate{Buy: 500.1, Sell: 501.2},
				RUBToKZT: model.ExchangeRate{Buy: 5.05, Sell: 5.15},
			},
		},
		{
			Timestamp: time.Date(2026, 10, 31, 11, 0, 0, 0, time.UTC),
			Rates: model.Rates{
				USDToKZT: model.ExchangeRate{Buy: 499.5, Sell: 500.25},
				RUBToKZT: model.ExchangeRate{Buy: 4.99, Sell: 5.09},
			},
		},
	}
}

func filterRecords() []model.Record {
	values := []struct {
		at                               time.Time
		usdBuy, usdSell, rubBuy, rubSell float64
	}{
		{time.Date(2026, 3, 19, 0, 0, 0, 0, time.UTC), 503, 504, 5.1, 5.2},
		{time.Date(2026, 3, 18, 12, 0, 0, 0, time.UTC), 502, 503, 5.0, 5.1},
		{time.Date(2026, 3, 18, 0, 0, 0, 0, time.UTC), 501, 502, 4.9, 5.0},
		{time.Date(2026, 3, 17, 23, 59, 59, 999_000_000, time.UTC), 500, 501, 4.8, 4.9},
	}
	out := make([]model.Record, 0, len(values))
	for _, value := range values {
		out = append(out, model.Record{
			Timestamp: value.at,
			Rates: model.Rates{
				USDToKZT: model.ExchangeRate{Buy: value.usdBuy, Sell: value.usdSell},
				RUBToKZT: model.ExchangeRate{Buy: value.rubBuy, Sell: value.rubSell},
			},
		})
	}
	return out
}

func newContractHandler(t *testing.T, scenario string) http.Handler {
	t.Helper()
	st := &fakeStore{}
	sender := &fakeSender{}
	source := &fakeSource{rates: model.Rates{
		USDToKZT: model.ExchangeRate{Buy: 485.5, Sell: 487.2},
		RUBToKZT: model.ExchangeRate{Buy: 4.85, Sell: 4.92},
	}}

	switch scenario {
	case "api_history_shape":
		st.records = contractRecords()
	case "api_history_filters":
		st.records = filterRecords()
	case "api_latest":
		st.records = []model.Record{{
			Timestamp: time.Date(2026, 10, 31, 12, 0, 0, 0, time.UTC),
			Rates: model.Rates{
				USDToKZT: model.ExchangeRate{Buy: 509.8, Sell: 512.4},
				RUBToKZT: model.ExchangeRate{Buy: 6.21, Sell: 6.35},
			},
		}}
	case "api_delete_history":
		st.deleted = 3
	case "api_trigger_failure":
		source.err = errors.New("parse exploded")
	}

	pipeline := &service.Service{
		Source: source, History: st, Recorder: fakeRecorder{}, Sender: sender,
		Logger: service.DiscardLogger(), Now: func() time.Time { return testNow },
	}
	srv := New(Options{
		APIToken:      "test-api-key",
		WebhookSecret: "test-secret",
		AllowedChats:  []string{"123", "-100999"},
		DefaultChatID: "123",
		Pipeline:      pipeline,
		History:       st,
		Sender:        sender,
		Metrics:       metrics.New(),
		Logger:        service.DiscardLogger(),
		Now:           func() time.Time { return testNow },
		Dispatch:      func(fn func()) { fn() },
	})
	if scenario == "api_ready_unavailable" {
		srv.opts.Ready = func(context.Context) error { return errors.New("database is unavailable") }
	}

	if strings.HasPrefix(scenario, "webhook_") {
		return srv.WebhookHandler()
	}
	return srv.APIHandler()
}

// normalizeContract replaces nondeterministic durations with <any> so -update
// writes a stable fixture.
func normalizeContract(_ string, value any) any {
	switch typed := value.(type) {
	case map[string]any:
		for key, item := range typed {
			if key == "executionTimeMs" {
				typed[key] = "<any>"
				continue
			}
			typed[key] = normalizeContract("", item)
		}
		return typed
	case []any:
		for index := range typed {
			typed[index] = normalizeContract("", typed[index])
		}
		return typed
	default:
		return value
	}
}

func TestContractFixtures(t *testing.T) {
	contracttest.Run(t, contractFixtureDir(t), newContractHandler,
		contracttest.WithHeaders("X-Content-Type-Options"),
		contracttest.WithNormalizer(normalizeContract),
	)
}

func TestWebhookOversizedBodyIsAcknowledged(t *testing.T) {
	h := newHarness()
	body := strings.NewReader(strings.Repeat("x", maxWebhook+1))
	req, err := http.NewRequest(http.MethodPost, "/telegram/webhook", body)
	if err != nil {
		t.Fatal(err)
	}
	req.Header.Set(secretHeader, "test-secret")
	recorder := httptest.NewRecorder()
	h.server.WebhookHandler().ServeHTTP(recorder, req)
	if recorder.Code != http.StatusOK {
		t.Fatalf("status = %d, want 200", recorder.Code)
	}
}
