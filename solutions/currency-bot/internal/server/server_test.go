package server

import (
	"bytes"
	"context"
	"encoding/json"
	"errors"
	"net/http"
	"net/http/httptest"
	"strings"
	"testing"
	"time"

	"ricardo.local/monorepo/solutions/currency-bot/internal/metrics"
	"ricardo.local/monorepo/solutions/currency-bot/internal/model"
	"ricardo.local/monorepo/solutions/currency-bot/internal/service"
	"ricardo.local/monorepo/solutions/currency-bot/internal/store"
)

var testNow = time.Date(2026, 10, 31, 12, 0, 0, 0, time.UTC)

type fakeStore struct {
	records   []model.Record
	latestErr error
	from, to  time.Time
	ranged    bool
	deleted   int
	saveErr   error
	saved     []model.Record
}

func (f *fakeStore) AllRecords(context.Context) ([]model.Record, error) { return f.records, nil }
func (f *fakeStore) RecordsByDateRange(_ context.Context, from, to time.Time) ([]model.Record, error) {
	f.ranged, f.from, f.to = true, from, to
	return f.records, nil
}
func (f *fakeStore) LatestRecord(context.Context) (model.Record, error) {
	if f.latestErr != nil {
		return model.Record{}, f.latestErr
	}
	if len(f.records) == 0 {
		return model.Record{}, store.ErrNotFound
	}
	return f.records[0], nil
}
func (f *fakeStore) DeleteAll(context.Context) (int, error) { return f.deleted, nil }
func (f *fakeStore) Ping(context.Context) error             { return nil }
func (f *fakeStore) SaveRecord(_ context.Context, record model.Record) (int, error) {
	f.saved = append(f.saved, record)
	return len(f.saved), f.saveErr
}
func (f *fakeStore) RecordBefore(context.Context, time.Time) (model.Record, error) {
	return model.Record{}, errors.New("no history")
}

type fakeSource struct {
	rates model.Rates
	err   error
}

func (f *fakeSource) Fetch(context.Context) (model.Rates, error) { return f.rates, f.err }

type fakeSender struct {
	err  error
	sent []string
	to   []string
}

func (f *fakeSender) Send(_ context.Context, chatID, text string) error {
	if f.err != nil {
		return f.err
	}
	f.sent = append(f.sent, text)
	f.to = append(f.to, chatID)
	return nil
}

type fakeRecorder struct{}

func (fakeRecorder) RecordSentAlert(context.Context, model.SentAlert) error { return nil }

type harness struct {
	server *Server
	store  *fakeStore
	sender *fakeSender
	source *fakeSource
}

func newHarness(records ...model.Record) *harness {
	st := &fakeStore{records: records, deleted: len(records)}
	sender := &fakeSender{}
	source := &fakeSource{rates: model.Rates{
		USDToKZT: model.ExchangeRate{Buy: 485.5, Sell: 487.2},
		RUBToKZT: model.ExchangeRate{Buy: 4.85, Sell: 4.92},
	}}
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
	})
	return &harness{server: srv, store: st, sender: sender, source: source}
}

func (h *harness) api(method, path string, body []byte, token string) *httptest.ResponseRecorder {
	req := httptest.NewRequest(method, path, bytes.NewReader(body))
	if token != "" {
		req.Header.Set("Authorization", "Bearer "+token)
	}
	recorder := httptest.NewRecorder()
	h.server.APIHandler().ServeHTTP(recorder, req)
	return recorder
}

func (h *harness) webhook(secret string, body []byte) *httptest.ResponseRecorder {
	req := httptest.NewRequest(http.MethodPost, "/telegram/webhook", bytes.NewReader(body))
	if secret != "" {
		req.Header.Set(secretHeader, secret)
	}
	recorder := httptest.NewRecorder()
	h.server.WebhookHandler().ServeHTTP(recorder, req)
	return recorder
}

func TestHealthAndReadiness(t *testing.T) {
	h := newHarness()
	for _, path := range []string{"/healthz", "/readyz"} {
		rec := h.api(http.MethodGet, path, nil, "")
		if rec.Code != http.StatusOK {
			t.Fatalf("%s = %d", path, rec.Code)
		}
	}
}

func TestAPIRequiresBearerToken(t *testing.T) {
	h := newHarness()
	cases := []struct {
		name   string
		header string
	}{
		{"missing", ""},
		{"wrong", "Bearer nope"},
		{"no bearer prefix", "test-api-key"},
		{"empty bearer", "Bearer "},
	}
	for _, tc := range cases {
		t.Run(tc.name, func(t *testing.T) {
			req := httptest.NewRequest(http.MethodGet, "/api/history", nil)
			if tc.header != "" {
				req.Header.Set("Authorization", tc.header)
			}
			rec := httptest.NewRecorder()
			h.server.APIHandler().ServeHTTP(rec, req)
			if rec.Code != http.StatusUnauthorized {
				t.Fatalf("status = %d, want 401", rec.Code)
			}
		})
	}
	h.api(http.MethodGet, "/api/history", nil, "test-api-key")
}

func TestHistoryShape(t *testing.T) {
	record := model.Record{Timestamp: testNow, Rates: model.Rates{
		USDToKZT: model.ExchangeRate{Buy: 498.5, Sell: 502.3},
		RUBToKZT: model.ExchangeRate{Buy: 4.85, Sell: 4.95},
	}}
	h := newHarness(record)
	rec := h.api(http.MethodGet, "/api/history", nil, "test-api-key")
	if rec.Code != http.StatusOK {
		t.Fatalf("status = %d", rec.Code)
	}
	var got historyResponse
	if err := json.Unmarshal(rec.Body.Bytes(), &got); err != nil {
		t.Fatal(err)
	}
	if got.TotalCount != 1 || len(got.Records) != 1 {
		t.Fatalf("unexpected shape: %+v", got)
	}
	if got.Records[0].Rates.USDToKZT.Buy != 498.5 || got.Records[0].Rates.RUBToKZT.Sell != 4.95 {
		t.Fatalf("unexpected rates: %+v", got.Records[0].Rates)
	}
	if got.Records[0].Timestamp != "2026-10-31T12:00:00Z" {
		t.Fatalf("timestamp = %q", got.Records[0].Timestamp)
	}
}

func TestHistoryDateAndRangeFilters(t *testing.T) {
	h := newHarness()
	rec := h.api(http.MethodGet, "/api/history?date=2026-03-18", nil, "test-api-key")
	if rec.Code != http.StatusOK || !h.store.ranged {
		t.Fatalf("date filter not applied: %d ranged=%v", rec.Code, h.store.ranged)
	}
	if !h.store.from.Equal(time.Date(2026, 3, 18, 0, 0, 0, 0, time.UTC)) || !h.store.to.Equal(time.Date(2026, 3, 19, 0, 0, 0, 0, time.UTC)) {
		t.Fatalf("date bounds = %v..%v", h.store.from, h.store.to)
	}

	h.store.ranged = false
	rec = h.api(http.MethodGet, "/api/history?from=2026-03-01T00:00:00Z&to=2026-03-18T23:59:59Z", nil, "test-api-key")
	if rec.Code != http.StatusOK || !h.store.ranged {
		t.Fatalf("range filter not applied: %d", rec.Code)
	}

	h.store.ranged = false
	rec = h.api(http.MethodGet, "/api/history?from=2026-03-01T00:00:00Z", nil, "test-api-key")
	if rec.Code != http.StatusOK || h.store.ranged {
		t.Fatalf("from-only must return full history, ranged=%v", h.store.ranged)
	}

	rec = h.api(http.MethodGet, "/api/history?date=not-a-date", nil, "test-api-key")
	if rec.Code != http.StatusBadRequest {
		t.Fatalf("invalid date status = %d, want 400", rec.Code)
	}
	if got := decodeError(t, rec); got.ErrorCode != "BAD_REQUEST" || got.Status != http.StatusBadRequest {
		t.Fatalf("invalid date envelope = %+v", got)
	}

	rec = h.api(http.MethodGet, "/api/history?from=not-a-time&to=2026-03-18T23:59:59Z", nil, "test-api-key")
	if rec.Code != http.StatusBadRequest {
		t.Fatalf("invalid from status = %d, want 400", rec.Code)
	}
	if got := decodeError(t, rec); got.ErrorCode != "BAD_REQUEST" {
		t.Fatalf("invalid from envelope = %+v", got)
	}

	rec = h.api(http.MethodGet, "/api/history?from=2026-03-01T00:00:00Z&to=not-a-time", nil, "test-api-key")
	if rec.Code != http.StatusBadRequest {
		t.Fatalf("invalid to status = %d, want 400", rec.Code)
	}
	if got := decodeError(t, rec); got.ErrorCode != "BAD_REQUEST" {
		t.Fatalf("invalid to envelope = %+v", got)
	}
}

func decodeError(t *testing.T, rec *httptest.ResponseRecorder) errorResponse {
	t.Helper()
	var got errorResponse
	if err := json.Unmarshal(rec.Body.Bytes(), &got); err != nil {
		t.Fatalf("decode error envelope: %v", err)
	}
	return got
}

func TestLatestAndDelete(t *testing.T) {
	h := newHarness()
	rec := h.api(http.MethodGet, "/api/latest", nil, "test-api-key")
	if rec.Code != http.StatusNotFound {
		t.Fatalf("empty latest = %d", rec.Code)
	}

	h = newHarness(model.Record{Timestamp: testNow, Rates: model.Rates{}})
	rec = h.api(http.MethodGet, "/api/latest", nil, "test-api-key")
	if rec.Code != http.StatusOK {
		t.Fatalf("latest = %d", rec.Code)
	}
	var latest latestResponse
	if err := json.Unmarshal(rec.Body.Bytes(), &latest); err != nil {
		t.Fatal(err)
	}
	if latest.Timestamp != "2026-10-31T12:00:00Z" {
		t.Fatalf("timestamp = %q", latest.Timestamp)
	}

	rec = h.api(http.MethodDelete, "/api/history", nil, "test-api-key")
	if rec.Code != http.StatusOK {
		t.Fatalf("delete = %d", rec.Code)
	}
	var deleted deleteResponse
	if err := json.Unmarshal(rec.Body.Bytes(), &deleted); err != nil {
		t.Fatal(err)
	}
	if deleted.DeletedCount != 1 || deleted.Message != "Successfully deleted 1 currency history records" {
		t.Fatalf("delete response = %+v", deleted)
	}
}

func TestTriggerForcesSendAndUsesBodyChat(t *testing.T) {
	h := newHarness()
	rec := h.api(http.MethodPost, "/api/trigger", []byte(`{"chatId":"-100999"}`), "test-api-key")
	if rec.Code != http.StatusOK {
		t.Fatalf("trigger = %d: %s", rec.Code, rec.Body.String())
	}
	var got triggerResponse
	if err := json.Unmarshal(rec.Body.Bytes(), &got); err != nil {
		t.Fatal(err)
	}
	if got.Message != "Currency rates updated and sent to Telegram successfully" {
		t.Fatalf("message = %q", got.Message)
	}
	if len(h.sender.to) != 1 || h.sender.to[0] != "-100999" {
		t.Fatalf("chat targets = %v", h.sender.to)
	}
	if len(h.store.saved) != 0 {
		t.Fatal("manual trigger must not save history")
	}
}

func TestTriggerFailureEnvelope(t *testing.T) {
	h := newHarness()
	h.source.err = errors.New("parse exploded")
	rec := h.api(http.MethodPost, "/api/trigger", []byte(`{}`), "test-api-key")
	if rec.Code != http.StatusBadGateway {
		t.Fatalf("status = %d, want 502", rec.Code)
	}
	var got errorResponse
	if err := json.Unmarshal(rec.Body.Bytes(), &got); err != nil {
		t.Fatal(err)
	}
	if got.ErrorCode != "EXTERNAL_SERVICE_ERROR" || !strings.Contains(got.Message, "parse exploded") {
		t.Fatalf("envelope = %+v", got)
	}
}

func TestWebhookRejectsBadSecret(t *testing.T) {
	h := newHarness()
	for _, secret := range []string{"", "wrong"} {
		rec := h.webhook(secret, []byte(`{"update_id":1}`))
		if rec.Code != http.StatusUnauthorized {
			t.Fatalf("secret %q = %d", secret, rec.Code)
		}
	}
}

func updateBody(chatID int64, text string) []byte {
	payload := map[string]any{
		"update_id": 1,
		"message": map[string]any{
			"chat": map[string]any{"id": chatID},
			"text": text,
		},
	}
	body, _ := json.Marshal(payload)
	return body
}

func TestWebhookChatAllowlist(t *testing.T) {
	h := newHarness()
	rec := h.webhook("test-secret", updateBody(999, "/start"))
	if rec.Code != http.StatusOK {
		t.Fatalf("status = %d", rec.Code)
	}
	if len(h.sender.sent) != 0 {
		t.Fatalf("unknown chat must be ignored, sent %v", h.sender.sent)
	}
	if strings.TrimSpace(rec.Body.String()) != `"OK"` {
		t.Fatalf("body = %q", rec.Body.String())
	}
}

func TestWebhookCommands(t *testing.T) {
	h := newHarness()
	rec := h.webhook("test-secret", updateBody(123, "/start"))
	if rec.Code != http.StatusOK || len(h.sender.sent) != 1 || h.sender.sent[0] != startMessage {
		t.Fatalf("start reply wrong: %d %v", rec.Code, h.sender.sent)
	}
	if h.sender.to[0] != "123" {
		t.Fatalf("reply chat = %s", h.sender.to[0])
	}

	h = newHarness()
	h.webhook("test-secret", updateBody(123, "/help"))
	if len(h.sender.sent) != 1 || h.sender.sent[0] != helpMessage {
		t.Fatalf("help reply wrong: %v", h.sender.sent)
	}

	// Prefix matching, exactly like the Kotlin startsWith dispatch.
	h = newHarness()
	h.webhook("test-secret", updateBody(123, "/helpme"))
	if len(h.sender.sent) != 1 || h.sender.sent[0] != helpMessage {
		t.Fatalf("prefix match failed")
	}

	h = newHarness()
	rec = h.webhook("test-secret", updateBody(123, "hello"))
	if rec.Code != http.StatusOK || len(h.sender.sent) != 0 {
		t.Fatalf("non-command should be ignored")
	}
}

func TestWebhookTriggerSuccessAndFailure(t *testing.T) {
	h := newHarness()
	rec := h.webhook("test-secret", updateBody(-100999, "/trigger"))
	if rec.Code != http.StatusOK {
		t.Fatalf("status = %d", rec.Code)
	}
	if len(h.sender.sent) != 1 || !strings.Contains(h.sender.sent[0], "Курсы валют на kurs.kz") {
		t.Fatalf("expected the rates message, got %v", h.sender.sent)
	}
	if h.sender.to[0] != "-100999" {
		t.Fatalf("rates sent to %s", h.sender.to[0])
	}

	h = newHarness()
	h.source.err = errors.New("upstream down")
	rec = h.webhook("test-secret", updateBody(123, "/trigger"))
	if rec.Code != http.StatusOK {
		t.Fatalf("trigger failure reply should still be 200, got %d", rec.Code)
	}
	if len(h.sender.sent) != 1 || !strings.Contains(h.sender.sent[0], "Не удалось обновить курсы") {
		t.Fatalf("expected failure reply, got %v", h.sender.sent)
	}
}

func TestWebhookEditedMessageAndMalformed(t *testing.T) {
	h := newHarness()
	body := []byte(`{"update_id":2,"edited_message":{"chat":{"id":123},"text":"/start"}}`)
	rec := h.webhook("test-secret", body)
	if rec.Code != http.StatusOK || len(h.sender.sent) != 1 {
		t.Fatalf("edited_message should be handled: %d %v", rec.Code, h.sender.sent)
	}

	h = newHarness()
	rec = h.webhook("test-secret", []byte(`{not json`))
	if rec.Code != http.StatusOK || strings.TrimSpace(rec.Body.String()) != `"OK"` {
		t.Fatalf("malformed body must still answer 200: %d %q", rec.Code, rec.Body.String())
	}

	h = newHarness()
	rec = h.webhook("test-secret", []byte(`{"update_id":3}`))
	if rec.Code != http.StatusOK || len(h.sender.sent) != 0 {
		t.Fatalf("non-message update = %d", rec.Code)
	}

	// Delivery failures are logged, never surfaced to Telegram, otherwise the
	// update is retried and /trigger runs again.
	h = newHarness()
	h.sender.err = errors.New("telegram down")
	rec = h.webhook("test-secret", updateBody(123, "/start"))
	if rec.Code != http.StatusOK || strings.TrimSpace(rec.Body.String()) != `"OK"` {
		t.Fatalf("delivery failure must still answer 200: %d %q", rec.Code, rec.Body.String())
	}
}
