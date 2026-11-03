package voice

import (
	"bytes"
	"context"
	"encoding/json"
	"errors"
	"io"
	"log/slog"
	"net/http"
	"net/http/httptest"
	"strings"
	"sync"
	"testing"
	"time"

	"golang.org/x/time/rate"

	"ricardo.local/monorepo/solutions/mcp-gateway/internal/agentmode"
	"ricardo.local/monorepo/solutions/mcp-gateway/internal/bb"
)

type fakeDispatcher struct {
	mu      sync.Mutex
	sends   []bb.SendRequest
	sendErr error

	turns []bb.TurnResultResult
	err   error

	updates     []bb.UpdatesResult
	updatesErr  error
	updateCalls []bb.UpdatesRequest
}

func (f *fakeDispatcher) Send(_ context.Context, req bb.SendRequest) (bb.SendResult, error) {
	f.mu.Lock()
	defer f.mu.Unlock()
	f.sends = append(f.sends, req)
	if f.sendErr != nil {
		return bb.SendResult{}, f.sendErr
	}
	return bb.SendResult{ThreadID: req.ThreadID, AfterSeq: 42}, nil
}

func (f *fakeDispatcher) TurnResult(_ context.Context, _ bb.TurnResultRequest) (bb.TurnResultResult, error) {
	f.mu.Lock()
	defer f.mu.Unlock()
	if f.err != nil {
		return bb.TurnResultResult{}, f.err
	}
	if len(f.turns) == 0 {
		return bb.TurnResultResult{Status: "running"}, nil
	}
	result := f.turns[0]
	f.turns = f.turns[1:]
	return result, nil
}

func (f *fakeDispatcher) Updates(_ context.Context, req bb.UpdatesRequest) (bb.UpdatesResult, error) {
	f.mu.Lock()
	defer f.mu.Unlock()
	f.updateCalls = append(f.updateCalls, req)
	if f.updatesErr != nil {
		return bb.UpdatesResult{}, f.updatesErr
	}
	if len(f.updates) == 0 {
		return bb.UpdatesResult{Cursor: 0}, nil
	}
	result := f.updates[0]
	f.updates = f.updates[1:]
	return result, nil
}

func (f *fakeDispatcher) sent() []bb.SendRequest {
	f.mu.Lock()
	defer f.mu.Unlock()
	return append([]bb.SendRequest(nil), f.sends...)
}

func newTestHandler(d Dispatcher, mode *agentmode.State, work string) *Handler {
	return New(Options{
		Dispatcher:   d,
		WorkThreadID: work,
		Mode:         mode,
		Logger:       slog.New(slog.NewTextHandler(io.Discard, nil)),
		Limiter:      rate.NewLimiter(rate.Inf, 1),
		PollInterval: time.Millisecond,
		MaxWait:      time.Second,
	})
}

func post(t *testing.T, handler http.Handler, path string, body any) (*httptest.ResponseRecorder, map[string]any) {
	t.Helper()
	payload, err := json.Marshal(body)
	if err != nil {
		t.Fatal(err)
	}
	rec := httptest.NewRecorder()
	handler.ServeHTTP(rec, httptest.NewRequest(http.MethodPost, path, bytes.NewReader(payload)))
	var decoded map[string]any
	if err := json.Unmarshal(rec.Body.Bytes(), &decoded); err != nil {
		t.Fatalf("response is not JSON (%d): %s", rec.Code, rec.Body.String())
	}
	return rec, decoded
}

func TestPersonalWithoutAgentModeRoutesToHermes(t *testing.T) {
	dispatcher := &fakeDispatcher{}
	handler := newTestHandler(dispatcher, agentmode.New(), "thr_work").Handler()
	rec, body := post(t, handler, "/v1/voice/utterance", UtteranceRequest{Text: "какая погода", Profile: "personal"})
	if rec.Code != http.StatusOK || body["route"] != "hermes" {
		t.Fatalf("status=%d body=%v", rec.Code, body)
	}
	if len(dispatcher.sent()) != 0 {
		t.Fatal("personal phrase was sent to bb")
	}
}

func TestWorkProfileSendsToDispatcher(t *testing.T) {
	dispatcher := &fakeDispatcher{}
	handler := newTestHandler(dispatcher, agentmode.New(), "thr_work").Handler()
	_, body := post(t, handler, "/v1/voice/utterance", UtteranceRequest{Text: "почини тесты", Profile: "work"})
	if body["route"] != "bb" || body["ok"] != true || body["threadId"] != "thr_work" {
		t.Fatalf("body = %v", body)
	}
	if got := body["speech"]; got != SpeechWorkSent {
		t.Fatalf("speech = %v", got)
	}
	sends := dispatcher.sent()
	if len(sends) != 1 || sends[0].ThreadID != "thr_work" || sends[0].Text != "почини тесты" {
		t.Fatalf("sends = %+v", sends)
	}
	if sends[0].IdempotencyKey == "" {
		t.Fatal("send has no idempotency key")
	}
}

func TestAgentModeAppliesOnlyToPersonal(t *testing.T) {
	mode := agentmode.New()
	mode.Enter("thr_agent")
	dispatcher := &fakeDispatcher{}
	handler := newTestHandler(dispatcher, mode, "thr_work").Handler()

	_, body := post(t, handler, "/v1/voice/utterance", UtteranceRequest{Text: "продолжай", Profile: "personal"})
	if body["threadId"] != "thr_agent" || body["speech"] != SpeechAgentSent {
		t.Fatalf("personal body = %v", body)
	}

	_, body = post(t, handler, "/v1/voice/utterance", UtteranceRequest{Text: "почини тесты", Profile: "work"})
	if body["threadId"] != "thr_work" || body["speech"] != SpeechWorkSent {
		t.Fatalf("work body = %v", body)
	}
	if got, active := mode.Active(); !active || got != "thr_agent" {
		t.Fatalf("work phrase changed agent mode: %q, %v", got, active)
	}
}

func TestExitPhraseDisablesAgentMode(t *testing.T) {
	mode := agentmode.New()
	mode.Enter("thr_agent")
	dispatcher := &fakeDispatcher{}
	handler := newTestHandler(dispatcher, mode, "thr_work").Handler()
	_, body := post(t, handler, "/v1/voice/utterance", UtteranceRequest{Text: "Выйди из режима агента.", Profile: "personal"})
	if body["route"] != "local" || body["speech"] != SpeechModeOff {
		t.Fatalf("body = %v", body)
	}
	if _, active := mode.Active(); active {
		t.Fatal("agent mode survived the exit phrase")
	}
	if len(dispatcher.sent()) != 0 {
		t.Fatal("exit phrase was forwarded")
	}
}

func TestExitPhraseWorksInWorkProfile(t *testing.T) {
	mode := agentmode.New()
	mode.Enter("thr_agent")
	handler := newTestHandler(&fakeDispatcher{}, mode, "thr_work").Handler()
	_, body := post(t, handler, "/v1/voice/utterance", UtteranceRequest{Text: "закончи разговор с агентом", Profile: "work"})
	if body["route"] != "local" || body["speech"] != SpeechModeOff {
		t.Fatalf("body = %v", body)
	}
	if _, active := mode.Active(); active {
		t.Fatal("exit phrase in work profile did not disable agent mode")
	}
}

func TestExitPhraseIsWholeUtteranceOnly(t *testing.T) {
	mode := agentmode.New()
	mode.Enter("thr_agent")
	dispatcher := &fakeDispatcher{}
	handler := newTestHandler(dispatcher, mode, "").Handler()
	_, body := post(t, handler, "/v1/voice/utterance", UtteranceRequest{
		Text: "выйди из режима агента и почини тесты", Profile: "personal",
	})
	if body["route"] != "bb" {
		t.Fatalf("partial exit phrase changed routing: %v", body)
	}
	if _, active := mode.Active(); !active {
		t.Fatal("mode was disabled by a partial match")
	}
}

func TestWorkFailureNeverFallsBackToHermes(t *testing.T) {
	dispatcher := &fakeDispatcher{sendErr: &bb.Error{Code: "bridge_error", Speech: "Сервер агентов недоступен."}}
	handler := newTestHandler(dispatcher, agentmode.New(), "thr_work").Handler()
	_, body := post(t, handler, "/v1/voice/utterance", UtteranceRequest{Text: "почини", Profile: "work"})
	if body["route"] != "bb" || body["ok"] != false {
		t.Fatalf("body = %v", body)
	}
	if body["speech"] != "Сервер агентов недоступен." {
		t.Fatalf("speech = %v", body["speech"])
	}
}

func TestWorkWithoutDispatcherThreadFailsClosed(t *testing.T) {
	dispatcher := &fakeDispatcher{}
	handler := newTestHandler(dispatcher, agentmode.New(), "").Handler()
	_, body := post(t, handler, "/v1/voice/utterance", UtteranceRequest{Text: "почини", Profile: "work"})
	if body["route"] != "bb" || body["speech"] != SpeechWorkMissing {
		t.Fatalf("body = %v", body)
	}
	if len(dispatcher.sent()) != 0 {
		t.Fatal("a send was attempted without a target")
	}
}

func TestAgentModeFailureStaysOnBB(t *testing.T) {
	mode := agentmode.New()
	mode.Enter("thr_agent")
	dispatcher := &fakeDispatcher{sendErr: errors.New("dial")}
	handler := newTestHandler(dispatcher, mode, "").Handler()
	_, body := post(t, handler, "/v1/voice/utterance", UtteranceRequest{Text: "продолжай", Profile: "personal"})
	if body["route"] != "bb" || body["ok"] != false {
		t.Fatalf("body = %v", body)
	}
}

func TestUtteranceValidation(t *testing.T) {
	handler := newTestHandler(&fakeDispatcher{}, agentmode.New(), "").Handler()
	rec, _ := post(t, handler, "/v1/voice/utterance", UtteranceRequest{Text: strings.Repeat("я", MaxTextRunes+1), Profile: "personal"})
	if rec.Code != http.StatusBadRequest {
		t.Fatalf("long text status = %d", rec.Code)
	}
	rec, _ = post(t, handler, "/v1/voice/utterance", UtteranceRequest{Text: "привет", Profile: "office"})
	if rec.Code != http.StatusBadRequest {
		t.Fatalf("unknown profile status = %d", rec.Code)
	}
}

func TestResultReturnsCompletedTurn(t *testing.T) {
	spoken := "Готово, тесты починены."
	dispatcher := &fakeDispatcher{turns: []bb.TurnResultResult{
		{Status: "running"},
		{Status: "idle", Spoken: &spoken, Text: "details"},
	}}
	handler := newTestHandler(dispatcher, agentmode.New(), "").Handler()
	rec, body := post(t, handler, "/v1/voice/result", ResultRequest{ThreadID: "thr_1", AfterSeq: 42, WaitSeconds: 1})
	if rec.Code != http.StatusOK || body["status"] != "idle" || body["speech"] != spoken {
		t.Fatalf("status=%d body=%v", rec.Code, body)
	}
}

func TestResultTimesOutAsRunning(t *testing.T) {
	dispatcher := &fakeDispatcher{}
	handler := newTestHandler(dispatcher, agentmode.New(), "").Handler()
	_, body := post(t, handler, "/v1/voice/result", ResultRequest{ThreadID: "thr_1", AfterSeq: 42, WaitSeconds: 0})
	if body["status"] != "running" {
		t.Fatalf("body = %v", body)
	}
}

func TestResultFallsBackToDoneWithoutSpokenLine(t *testing.T) {
	dispatcher := &fakeDispatcher{turns: []bb.TurnResultResult{{Status: "idle"}}}
	handler := newTestHandler(dispatcher, agentmode.New(), "").Handler()
	_, body := post(t, handler, "/v1/voice/result", ResultRequest{ThreadID: "thr_1", AfterSeq: 0, WaitSeconds: 1})
	if body["speech"] != SpeechDone {
		t.Fatalf("body = %v", body)
	}
}

func TestStateEndpoint(t *testing.T) {
	mode := agentmode.New()
	mode.Enter("thr_agent")
	rec := httptest.NewRecorder()
	newTestHandler(&fakeDispatcher{}, mode, "").Handler().ServeHTTP(rec, httptest.NewRequest(http.MethodGet, "/v1/voice/state", nil))
	var body StateResponse
	if err := json.Unmarshal(rec.Body.Bytes(), &body); err != nil {
		t.Fatal(err)
	}
	if !body.AgentMode || body.ThreadID != "thr_agent" {
		t.Fatalf("state = %+v", body)
	}
}

func TestRateLimit(t *testing.T) {
	handler := New(Options{
		Dispatcher: &fakeDispatcher{},
		Mode:       agentmode.New(),
		Logger:     slog.New(slog.NewTextHandler(io.Discard, nil)),
		Limiter:    rate.NewLimiter(rate.Every(time.Hour), 1),
	}).Handler()
	post(t, handler, "/v1/voice/utterance", UtteranceRequest{Text: "раз", Profile: "personal"})
	rec, body := post(t, handler, "/v1/voice/utterance", UtteranceRequest{Text: "два", Profile: "personal"})
	if rec.Code != http.StatusTooManyRequests || body["speech"] != SpeechRateLimited {
		t.Fatalf("status=%d body=%v", rec.Code, body)
	}
}

func newUpdatesHandler(d Dispatcher) http.Handler {
	return New(Options{
		Dispatcher:   d,
		Mode:         agentmode.New(),
		Logger:       slog.New(slog.NewTextHandler(io.Discard, nil)),
		PollInterval: time.Millisecond,
		MaxWait:      25 * time.Second,
		UpdatesWait:  10 * time.Second,
	}).Handler()
}

func TestUpdatesBootstrapReturnsCursorWithoutHistory(t *testing.T) {
	dispatcher := &fakeDispatcher{updates: []bb.UpdatesResult{{Cursor: 12}}}
	handler := newUpdatesHandler(dispatcher)
	rec, body := post(t, handler, "/v1/voice/updates", UpdatesRequest{})

	if rec.Code != http.StatusOK {
		t.Fatalf("status = %d", rec.Code)
	}
	if body["cursor"] != float64(12) {
		t.Fatalf("cursor = %v", body["cursor"])
	}
	items, ok := body["items"].([]any)
	if !ok || len(items) != 0 {
		t.Fatalf("items = %v", body["items"])
	}
	if len(dispatcher.updateCalls) != 1 {
		t.Fatalf("calls = %d", len(dispatcher.updateCalls))
	}
	call := dispatcher.updateCalls[0]
	if call.AfterSeq != nil {
		t.Fatalf("bootstrap sent afterSeq = %d", *call.AfterSeq)
	}
	if call.WaitSeconds == nil || *call.WaitSeconds != 10 {
		t.Fatalf("waitSeconds = %v, want the bridge cap 10", call.WaitSeconds)
	}
}

func TestUpdatesForwardsCursorAndReturnsItems(t *testing.T) {
	dispatcher := &fakeDispatcher{updates: []bb.UpdatesResult{{
		Cursor: 14,
		Items:  []bb.UpdateItem{{Seq: 13, Spoken: "Готово, ребёнок закончил.", CompletedAt: 1730000000123}},
	}}}
	handler := newUpdatesHandler(dispatcher)
	afterSeq, wait := int64(4), 5
	rec, body := post(t, handler, "/v1/voice/updates", UpdatesRequest{
		AfterSeq:    &afterSeq,
		WaitSeconds: &wait,
	})

	if rec.Code != http.StatusOK {
		t.Fatalf("status = %d", rec.Code)
	}
	items, ok := body["items"].([]any)
	if !ok || len(items) != 1 {
		t.Fatalf("items = %v", body["items"])
	}
	item := items[0].(map[string]any)
	if item["seq"] != float64(13) || item["spoken"] != "Готово, ребёнок закончил." {
		t.Fatalf("item = %v", item)
	}
	if item["completedAt"] != float64(1730000000123) {
		t.Fatalf("completedAt = %v", item["completedAt"])
	}
	call := dispatcher.updateCalls[0]
	if call.AfterSeq == nil || *call.AfterSeq != 4 {
		t.Fatalf("afterSeq = %v", call.AfterSeq)
	}
	if call.WaitSeconds == nil || *call.WaitSeconds != 5 {
		t.Fatalf("waitSeconds = %v", call.WaitSeconds)
	}
}

func TestUpdatesClampsWaitToTheBridgeCap(t *testing.T) {
	dispatcher := &fakeDispatcher{updates: []bb.UpdatesResult{{Cursor: 1}}}
	handler := newUpdatesHandler(dispatcher)
	wait := 100
	post(t, handler, "/v1/voice/updates", UpdatesRequest{WaitSeconds: &wait})
	call := dispatcher.updateCalls[0]
	if call.WaitSeconds == nil || *call.WaitSeconds != 10 {
		t.Fatalf("waitSeconds = %v, want 10", call.WaitSeconds)
	}
}

func TestUpdatesErrorMapsLikeResult(t *testing.T) {
	dispatcher := &fakeDispatcher{updatesErr: &bb.Error{
		Code:   "bridge_error",
		Speech: "Сервер агентов недоступен.",
	}}
	handler := newUpdatesHandler(dispatcher)
	rec, body := post(t, handler, "/v1/voice/updates", UpdatesRequest{})
	if rec.Code != http.StatusOK {
		t.Fatalf("status = %d", rec.Code)
	}
	if body["status"] != "failed" || body["speech"] != "Сервер агентов недоступен." {
		t.Fatalf("body = %v", body)
	}
}

func TestUpdatesRejectsNegativeAfterSeq(t *testing.T) {
	dispatcher := &fakeDispatcher{}
	handler := newUpdatesHandler(dispatcher)
	afterSeq := int64(-1)
	rec, body := post(t, handler, "/v1/voice/updates", UpdatesRequest{AfterSeq: &afterSeq})
	if rec.Code != http.StatusBadRequest || body["speech"] != SpeechBadRequest {
		t.Fatalf("status=%d body=%v", rec.Code, body)
	}
	if len(dispatcher.updateCalls) != 0 {
		t.Fatal("invalid request reached the bridge")
	}
}
