// Package testfake provides recording HTTP fakes for the voice-proxy tests.
//
// One fake exists per external dependency: Home Assistant, the Hermes upstream,
// the mcp-gateway voice API and the scheduler. Every fake records the requests
// it served, so a test can assert paths, headers and bodies without writing its
// own httptest handler.
package testfake

import (
	"encoding/json"
	"fmt"
	"io"
	"net/http"
	"net/http/httptest"
	"strings"
	"sync"
	"testing"
	"time"
)

// Serve starts an httptest server for handler and closes it when the test
// ends, so a test only names its fake and the URL.
func Serve(t testing.TB, handler http.Handler) *httptest.Server {
	t.Helper()
	server := httptest.NewServer(handler)
	t.Cleanup(server.Close)
	return server
}

// Call is one recorded request. Body is the decoded JSON object (nil when the
// body was not an object) and RawBody preserves the exact bytes.
type Call struct {
	Method  string
	Path    string
	Header  http.Header
	Body    map[string]any
	RawBody string
}

// Recorder collects calls in arrival order and is safe for concurrent use.
type Recorder struct {
	mu    sync.Mutex
	calls []Call
}

func (r *Recorder) record(req *http.Request) Call {
	raw, _ := io.ReadAll(req.Body)
	var body map[string]any
	_ = json.Unmarshal(raw, &body)
	call := Call{
		Method:  req.Method,
		Path:    req.URL.Path,
		Header:  req.Header.Clone(),
		Body:    body,
		RawBody: string(raw),
	}
	r.mu.Lock()
	r.calls = append(r.calls, call)
	r.mu.Unlock()
	return call
}

// Calls returns a copy of every recorded call.
func (r *Recorder) Calls() []Call {
	r.mu.Lock()
	defer r.mu.Unlock()
	return append([]Call(nil), r.calls...)
}

// CallsWithMethod returns the recorded calls for one HTTP method.
func (r *Recorder) CallsWithMethod(method string) []Call {
	var out []Call
	for _, call := range r.Calls() {
		if call.Method == method {
			out = append(out, call)
		}
	}
	return out
}

// CallsTo returns the recorded calls for one exact path.
func (r *Recorder) CallsTo(path string) []Call {
	var out []Call
	for _, call := range r.Calls() {
		if call.Path == path {
			out = append(out, call)
		}
	}
	return out
}

// Len is the number of recorded calls.
func (r *Recorder) Len() int {
	r.mu.Lock()
	defer r.mu.Unlock()
	return len(r.calls)
}

// Reset forgets every recorded call.
func (r *Recorder) Reset() {
	r.mu.Lock()
	defer r.mu.Unlock()
	r.calls = nil
}

// HA is a Home Assistant fake: GET /api/states/<entity> answers with the
// configured profile state and POST service calls are recorded.
type HA struct {
	Recorder

	mu                    sync.Mutex
	state                 string
	down                  bool
	failStartConversation bool
}

// NewHA builds a fake that reports the given input_select state.
func NewHA(state string) *HA { return &HA{state: state} }

// SetState changes the next GET answer.
func (h *HA) SetState(state string) {
	h.mu.Lock()
	defer h.mu.Unlock()
	h.state = state
}

// SetDown makes GET /api/states answer 503.
func (h *HA) SetDown(down bool) {
	h.mu.Lock()
	defer h.mu.Unlock()
	h.down = down
}

// SetFailStartConversation makes assist_satellite.start_conversation answer 503,
// so the announcement falls back to the script.
func (h *HA) SetFailStartConversation(fail bool) {
	h.mu.Lock()
	defer h.mu.Unlock()
	h.failStartConversation = fail
}

// StateCalls returns the recorded state reads, i.e. every GET.
func (h *HA) StateCalls() []Call { return h.CallsWithMethod(http.MethodGet) }

// ServiceCalls returns the recorded service calls, i.e. every POST.
func (h *HA) ServiceCalls() []Call { return h.CallsWithMethod(http.MethodPost) }

func (h *HA) ServeHTTP(w http.ResponseWriter, r *http.Request) {
	h.record(r)
	h.mu.Lock()
	state, down, failStart := h.state, h.down, h.failStartConversation
	h.mu.Unlock()

	if down {
		w.WriteHeader(http.StatusServiceUnavailable)
		return
	}
	if r.Method == http.MethodGet {
		w.Header().Set("Content-Type", "application/json")
		_ = json.NewEncoder(w).Encode(map[string]any{"entity_id": entityOf(r), "state": state})
		return
	}
	if failStart && strings.HasSuffix(r.URL.Path, "/api/services/assist_satellite/start_conversation") {
		w.WriteHeader(http.StatusServiceUnavailable)
		return
	}
	w.Header().Set("Content-Type", "application/json")
	_, _ = w.Write([]byte(`{"changed_states":[]}`))
}

// entityOf returns the last path segment, which is the entity id for a state
// read.
func entityOf(r *http.Request) string {
	path := strings.TrimSuffix(r.URL.Path, "/")
	if index := strings.LastIndex(path, "/"); index >= 0 {
		return path[index+1:]
	}
	return path
}

// Hermes is an OpenAI-compatible upstream fake. The default mode answers a
// plain JSON completion; the streaming modes reproduce a chunked reply with
// tool progress, reasoning, a gate and a Markdown tail.
type Hermes struct {
	Recorder

	mu       sync.Mutex
	mode     string
	status   int
	body     string
	down     bool
	gate     chan struct{}
	released bool
}

// Streaming modes understood by Hermes.
const (
	// ModeResponse answers the configured status and body as plain JSON.
	ModeResponse = "response"
	// ModeStream streams a role chunk, tool progress, reasoning, the first
	// sentence, then waits for Release before the tail and [DONE].
	ModeStream = "stream"
	// ModeTruncated streams the prefix and then closes before the tail.
	ModeTruncated = "truncated"
	// ModeHTTPError answers 429 with a JSON error object.
	ModeHTTPError = "http_error"
)

// NewHermes builds a fake upstream that answers a plain completion.
func NewHermes() *Hermes {
	return &Hermes{
		mode:   ModeResponse,
		status: http.StatusOK,
		body:   `{"choices":[{"message":{"role":"assistant","content":"upstream"}}]}`,
		gate:   make(chan struct{}),
	}
}

// SetMode selects the response behaviour.
func (h *Hermes) SetMode(mode string) {
	h.mu.Lock()
	defer h.mu.Unlock()
	h.mode = mode
}

// SetResponse sets the status and body used by ModeResponse.
func (h *Hermes) SetResponse(status int, body string) {
	h.mu.Lock()
	defer h.mu.Unlock()
	h.status, h.body = status, body
}

// SetDown makes the fake drop the connection, then serve normally again.
func (h *Hermes) SetDown(down bool) {
	h.mu.Lock()
	defer h.mu.Unlock()
	h.down = down
}

// Release opens the streaming gate. It is idempotent.
func (h *Hermes) Release() {
	h.mu.Lock()
	defer h.mu.Unlock()
	if !h.released {
		close(h.gate)
		h.released = true
	}
}

// Released reports whether Release was called, which proves the proxy did not
// buffer the whole answer.
func (h *Hermes) Released() bool {
	h.mu.Lock()
	defer h.mu.Unlock()
	return h.released
}

// Last returns the last decoded request body, or nil.
func (h *Hermes) Last() map[string]any {
	calls := h.Calls()
	if len(calls) == 0 {
		return nil
	}
	return calls[len(calls)-1].Body
}

// LastHeader returns the last request headers, or an empty header.
func (h *Hermes) LastHeader() http.Header {
	calls := h.Calls()
	if len(calls) == 0 {
		return http.Header{}
	}
	return calls[len(calls)-1].Header
}

func (h *Hermes) ServeHTTP(w http.ResponseWriter, r *http.Request) {
	h.record(r)
	h.mu.Lock()
	mode, status, body, down := h.mode, h.status, h.body, h.down
	h.mu.Unlock()

	if down {
		if hijacker, ok := w.(http.Hijacker); ok {
			if conn, _, err := hijacker.Hijack(); err == nil {
				_ = conn.Close()
			}
		}
		return
	}
	switch mode {
	case ModeHTTPError:
		w.Header().Set("Content-Type", "application/json")
		w.WriteHeader(http.StatusTooManyRequests)
		_, _ = io.WriteString(w, `{"error":{"message":"limited"}}`)
	case ModeStream, ModeTruncated:
		h.serveStream(w, r, mode)
	default:
		w.Header().Set("Content-Type", "application/json")
		w.WriteHeader(status)
		_, _ = io.WriteString(w, body)
	}
}

func (h *Hermes) serveStream(w http.ResponseWriter, r *http.Request, mode string) {
	w.Header().Set("Content-Type", "text/event-stream")
	w.WriteHeader(http.StatusOK)
	flusher := w.(http.Flusher)
	send := func(delta map[string]any, finish any) {
		chunk := map[string]any{
			"id":      "test",
			"object":  "chat.completion.chunk",
			"created": 1,
			"model":   "hermes-agent",
			"choices": []any{map[string]any{"index": 0, "delta": delta, "finish_reason": finish}},
		}
		encoded, _ := json.Marshal(chunk)
		_, _ = fmt.Fprintf(w, "data: %s\n\n", encoded)
		flusher.Flush()
	}
	send(map[string]any{"role": "assistant"}, nil)
	_, _ = io.WriteString(w, "event: hermes.tool.progress\ndata: {\"tool\":\"private\"}\n\n")
	flusher.Flush()
	send(map[string]any{"reasoning_content": "SECRET THINKING"}, nil)
	send(map[string]any{"content": "Первое предложение. "}, nil)

	h.mu.Lock()
	gate := h.gate
	h.mu.Unlock()
	select {
	case <-gate:
	case <-r.Context().Done():
		return
	}
	if mode == ModeTruncated {
		return
	}
	send(map[string]any{"content": "Источник: [Reuters](https://example.com)."}, nil)
	send(map[string]any{}, "stop")
	_, _ = io.WriteString(w, "data: [DONE]\n\n")
	flusher.Flush()
}

// Gateway is the mcp-gateway voice API fake: utterance, result and updates.
type Gateway struct {
	Recorder

	mu        sync.Mutex
	down      bool
	delay     time.Duration
	utterance map[string]any
	updates   map[string]any
	results   []map[string]any
}

// NewGateway builds a fake that answers route=hermes and running results.
func NewGateway() *Gateway {
	return &Gateway{utterance: map[string]any{"route": "hermes"}}
}

// SetUtterance sets the /v1/voice/utterance reply.
func (g *Gateway) SetUtterance(reply map[string]any) {
	g.mu.Lock()
	defer g.mu.Unlock()
	g.utterance = reply
}

// SetResults replaces the queued /v1/voice/result replies. Once the queue is
// empty the fake answers status=running.
func (g *Gateway) SetResults(results ...map[string]any) {
	g.mu.Lock()
	defer g.mu.Unlock()
	g.results = append([]map[string]any(nil), results...)
}

// SetUpdates sets the /v1/voice/updates reply. A nil reply answers 404.
func (g *Gateway) SetUpdates(reply map[string]any) {
	g.mu.Lock()
	defer g.mu.Unlock()
	g.updates = reply
}

// SetDown makes every route answer 503.
func (g *Gateway) SetDown(down bool) {
	g.mu.Lock()
	defer g.mu.Unlock()
	g.down = down
}

// SetDelay postpones every reply, so a client timeout can be exercised.
func (g *Gateway) SetDelay(delay time.Duration) {
	g.mu.Lock()
	defer g.mu.Unlock()
	g.delay = delay
}

func (g *Gateway) ServeHTTP(w http.ResponseWriter, r *http.Request) {
	g.record(r)
	g.mu.Lock()
	down, delay := g.down, g.delay
	g.mu.Unlock()

	if delay > 0 {
		select {
		case <-time.After(delay):
		case <-r.Context().Done():
			return
		}
	}
	if down {
		w.WriteHeader(http.StatusServiceUnavailable)
		_, _ = io.WriteString(w, `{"error":"down"}`)
		return
	}

	g.mu.Lock()
	var reply map[string]any
	switch r.URL.Path {
	case "/v1/voice/utterance":
		reply = g.utterance
	case "/v1/voice/result":
		reply = map[string]any{"status": "running"}
		if len(g.results) > 0 {
			reply = g.results[0]
			g.results = g.results[1:]
		}
	case "/v1/voice/updates":
		reply = g.updates
	}
	g.mu.Unlock()

	if reply == nil {
		w.WriteHeader(http.StatusNotFound)
		return
	}
	w.Header().Set("Content-Type", "application/json")
	_ = json.NewEncoder(w).Encode(reply)
}

// Scheduler is the scheduler EnqueueTask fake.
type Scheduler struct {
	Recorder

	mu   sync.Mutex
	fail bool
}

// NewScheduler builds a fake that accepts every task.
func NewScheduler() *Scheduler { return &Scheduler{} }

// SetFail makes EnqueueTask answer 503.
func (s *Scheduler) SetFail(fail bool) {
	s.mu.Lock()
	defer s.mu.Unlock()
	s.fail = fail
}

func (s *Scheduler) ServeHTTP(w http.ResponseWriter, r *http.Request) {
	s.record(r)
	s.mu.Lock()
	fail := s.fail
	s.mu.Unlock()
	if fail {
		w.WriteHeader(http.StatusServiceUnavailable)
		return
	}
	w.Header().Set("Content-Type", "application/json")
	_, _ = w.Write([]byte(`{"event":{"id":"ev_test"}}`))
}
