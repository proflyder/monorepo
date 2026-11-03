package proxy

import (
	"bytes"
	"context"
	"encoding/json"
	"fmt"
	"io"
	"net/http"
	"net/http/httptest"
	"regexp"
	"strings"
	"sync"
	"sync/atomic"
	"testing"
	"time"

	"ricardo.local/monorepo/solutions/voice-proxy/internal/hermes"
	"ricardo.local/monorepo/solutions/voice-proxy/internal/sanitize"
	"ricardo.local/monorepo/solutions/voice-proxy/internal/sse"
)

// ---------------------------------------------------------------- helpers ---

func testPayload(t *testing.T, raw string) map[string]any {
	t.Helper()
	value, err := decodeJSON([]byte(raw))
	if err != nil {
		t.Fatalf("decode payload %q: %v", raw, err)
	}
	object, ok := value.(map[string]any)
	if !ok {
		t.Fatalf("payload %q is not an object", raw)
	}
	return object
}

func newProxy(t *testing.T, upstream string, logger Logger) *Handler {
	t.Helper()
	if logger == nil {
		logger = NopLogger{}
	}
	return New(Options{
		UpstreamURL:     upstream,
		ReasoningEffort: "low",
		Logger:          logger,
	})
}

func postChat(t *testing.T, address, body string) *http.Response {
	t.Helper()
	resp, err := http.Post(address+"/v1/chat/completions", "application/json", strings.NewReader(body))
	if err != nil {
		t.Fatalf("POST: %v", err)
	}
	return resp
}

func drain(t *testing.T, resp *http.Response) {
	t.Helper()
	defer resp.Body.Close()
	if _, err := io.Copy(io.Discard, resp.Body); err != nil {
		t.Fatalf("drain: %v", err)
	}
}

func nextEvent(t *testing.T, reader *sse.Reader) sse.Event {
	t.Helper()
	event, err := reader.Next()
	if err != nil {
		t.Fatalf("Next: %v", err)
	}
	return event
}

func allEvents(t *testing.T, reader *sse.Reader) []sse.Event {
	t.Helper()
	var events []sse.Event
	for {
		event, err := reader.Next()
		if err == io.EOF {
			return events
		}
		if err != nil {
			t.Fatalf("Next: %v", err)
		}
		events = append(events, event)
	}
}

func decodeEvent(t *testing.T, payload string) map[string]any {
	t.Helper()
	value, err := decodeJSON([]byte(payload))
	if err != nil {
		t.Fatalf("decode event %q: %v", payload, err)
	}
	object, ok := value.(map[string]any)
	if !ok {
		t.Fatalf("event %q is not an object", payload)
	}
	return object
}

func firstChoice(t *testing.T, chunk map[string]any) map[string]any {
	t.Helper()
	choices, ok := chunk["choices"].([]any)
	if !ok || len(choices) == 0 {
		t.Fatalf("chunk has no choices: %#v", chunk)
	}
	choice, ok := choices[0].(map[string]any)
	if !ok {
		t.Fatalf("choice is not an object: %#v", choices[0])
	}
	return choice
}

// --------------------------------------------------- CancelGuardTests ------

// TestCancelGuardExactNormalizedPhrasesMatch mirrors
// test_exact_normalized_cancel_phrases_match.
func TestCancelGuardExactNormalizedPhrasesMatch(t *testing.T) {
	handler := newProxy(t, "http://127.0.0.1:1", nil)
	for _, text := range []string{
		"отмена", "Отмена!", "  отмени  ", "  Отмена!  ", "Передумала.",
		"не надо", "НИЧЕГО", "отбой…", "забудь", "Неважно?",
	} {
		payload := testPayload(t, fmt.Sprintf(`{"messages":[{"role":"user","content":%q}]}`, text))
		if _, matched := handler.CancelShortCircuit(payload); !matched {
			t.Errorf("CancelShortCircuit(%q) did not match", text)
		}
	}
}

// TestCancelGuardExtraMeaningAndNonUserRolesDoNotMatch mirrors
// test_extra_meaning_and_non_user_roles_do_not_match.
func TestCancelGuardExtraMeaningAndNonUserRolesDoNotMatch(t *testing.T) {
	handler := newProxy(t, "http://127.0.0.1:1", nil)
	negatives := []string{
		`[{"role":"user","content":"отмена будильника"}]`,
		`[{"role":"user","content":"не надо музыку"}]`,
		`[{"role":"user","content":"не отмена"}]`,
		`[{"role":"user","content":"как отменить"}]`,
		`[{"role":"user","content":"«отмена»"}]`,
		`[{"role":"user","content":"\"отмена\""}]`,
		`[{"role":"user","content":"от-мена"}]`,
		`[{"role":"user","content":"не, надо"}]`,
		`[{"role":"user","content":""}]`,
		`[{"role":"assistant","content":"отмена"}]`,
		`[{"role":"tool","content":"отмена","tool_call_id":"1"}]`,
		`[{"role":"user","content":[{"type":"text","text":"отмена"}]}]`,
		`[{"role":"user","content":"привет"}]`,
	}
	for _, messages := range negatives {
		payload := testPayload(t, `{"messages":`+messages+`}`)
		if phrase, matched := handler.CancelShortCircuit(payload); matched {
			t.Errorf("CancelShortCircuit(%s) matched %q", messages, phrase)
		}
	}
}

// TestCancelGuardOnlyLastUserMessageIsConsidered mirrors
// test_only_last_user_message_is_considered.
func TestCancelGuardOnlyLastUserMessageIsConsidered(t *testing.T) {
	handler := newProxy(t, "http://127.0.0.1:1", nil)
	payload := testPayload(t, `{"messages":[
		{"role":"user","content":"отмена"},
		{"role":"assistant","content":"Что отменить?"},
		{"role":"user","content":"включи музыку"}]}`)
	if phrase, matched := handler.CancelShortCircuit(payload); matched {
		t.Fatalf("matched %q, want no match", phrase)
	}
}

// TestCancelGuardUnicodeFolding pins the casefold/NFKC behaviour the Python
// implementation gets from casefold(): fullwidth and non-breaking forms match.
func TestCancelGuardUnicodeFolding(t *testing.T) {
	handler := newProxy(t, "http://127.0.0.1:1", nil)
	for _, text := range []string{"ОТМЕНА", "Отмена\u00a0", "ОТМЕНА！", "отмена。"} {
		payload := testPayload(t, fmt.Sprintf(`{"messages":[{"role":"user","content":%q}]}`, text))
		_, matched := handler.CancelShortCircuit(payload)
		if text == "отмена。" {
			if matched {
				t.Errorf("%q matched, but 。 is not stripped by Python either", text)
			}
			continue
		}
		if !matched {
			t.Errorf("%q did not match", text)
		}
	}
}

// ----------------------------------------- CancelGuardTransportTests ------

// TestCancelGuardNonStreamCancelReturnsEmptyAssistantWithoutUpstream mirrors
// test_non_stream_cancel_returns_empty_assistant_without_upstream.
func TestCancelGuardNonStreamCancelReturnsEmptyAssistantWithoutUpstream(t *testing.T) {
	fake := &recordingUpstream{}
	upstream := httptest.NewServer(fake)
	defer upstream.Close()
	proxyServer := httptest.NewServer(newProxy(t, upstream.URL, nil))
	defer proxyServer.Close()

	resp := postChat(t, proxyServer.URL, `{"model":"hermes-agent","stream":false,"messages":[{"role":"user","content":"отмена"}]}`)
	defer resp.Body.Close()
	raw, err := io.ReadAll(resp.Body)
	if err != nil {
		t.Fatalf("read: %v", err)
	}
	body := decodeEvent(t, string(raw))
	if fake.count() != 0 {
		t.Fatalf("upstream was called %d times, want 0", fake.count())
	}
	choice := firstChoice(t, body)
	if choice["finish_reason"] != "stop" {
		t.Errorf("finish_reason = %v", choice["finish_reason"])
	}
	message := choice["message"].(map[string]any)
	if message["role"] != "assistant" {
		t.Errorf("role = %v", message["role"])
	}
	if strings.TrimSpace(fmt.Sprint(message["content"])) != "" {
		t.Errorf("content = %q, want whitespace only", message["content"])
	}
	if body["id"] != CancelCompletionID {
		t.Errorf("id = %v, want %v", body["id"], CancelCompletionID)
	}
	if body["object"] != "chat.completion" {
		t.Errorf("object = %v", body["object"])
	}
	usage := body["usage"].(map[string]any)
	for _, key := range []string{"prompt_tokens", "completion_tokens", "total_tokens"} {
		if usage[key].(json.Number).String() != "0" {
			t.Errorf("usage[%s] = %v, want 0", key, usage[key])
		}
	}
}

// TestCancelGuardStreamCancelHasRoleFinishDoneAndNoUpstream mirrors
// test_stream_cancel_has_role_finish_done_and_no_upstream.
func TestCancelGuardStreamCancelHasRoleFinishDoneAndNoUpstream(t *testing.T) {
	fake := &recordingUpstream{}
	upstream := httptest.NewServer(fake)
	defer upstream.Close()
	proxyServer := httptest.NewServer(newProxy(t, upstream.URL, nil))
	defer proxyServer.Close()

	resp := postChat(t, proxyServer.URL, `{"model":"hermes-agent","stream":true,"messages":[{"role":"user","content":"Отмена!"}]}`)
	defer resp.Body.Close()
	events := allEvents(t, sse.NewReader(resp.Body))
	if fake.count() != 0 {
		t.Fatalf("upstream was called %d times, want 0", fake.count())
	}
	if len(events) < 3 || events[len(events)-1].Data != "[DONE]" {
		t.Fatalf("events = %#v", events)
	}
	chunks := make([]map[string]any, 0, len(events)-1)
	for _, event := range events[:len(events)-1] {
		chunks = append(chunks, decodeEvent(t, event.Data))
	}
	if delta := firstChoice(t, chunks[0])["delta"].(map[string]any); delta["role"] != "assistant" {
		t.Errorf("first delta = %#v", delta)
	}
	if firstChoice(t, chunks[1])["finish_reason"] != "stop" {
		t.Errorf("second chunk = %#v", chunks[1])
	}
	var spoken strings.Builder
	for _, chunk := range chunks {
		choice := firstChoice(t, chunk)
		if choice["finish_reason"] != nil {
			continue
		}
		delta, _ := choice["delta"].(map[string]any)
		if content, ok := delta["content"].(string); ok {
			spoken.WriteString(content)
		}
	}
	if strings.TrimSpace(spoken.String()) != "" {
		t.Fatalf("spoken = %q, want whitespace only", spoken.String())
	}
}

// TestCancelGuardToolContinuationAndFreeTextStillReachUpstream mirrors
// test_tool_continuation_and_free_text_still_reach_upstream.
func TestCancelGuardToolContinuationAndFreeTextStillReachUpstream(t *testing.T) {
	fake := &recordingUpstream{}
	upstream := httptest.NewServer(fake)
	defer upstream.Close()
	proxyServer := httptest.NewServer(newProxy(t, upstream.URL, nil))
	defer proxyServer.Close()

	postChat(t, proxyServer.URL, `{"model":"hermes-agent","stream":false,"messages":[{"role":"tool","content":"отмена"}]}`).Body.Close()
	if fake.count() == 0 {
		t.Fatal("tool continuation did not reach the upstream")
	}
	fake.reset()
	postChat(t, proxyServer.URL, `{"model":"hermes-agent","stream":false,"messages":[{"role":"user","content":"включи музыку"}]}`).Body.Close()
	if fake.count() == 0 {
		t.Fatal("free text did not reach the upstream")
	}
}

// ------------------------------------------------- ProxyTransportTests -----

// TestProxyEarlyDeliveryCleanTailAndHAFinishOrder mirrors
// test_early_delivery_clean_tail_and_ha_finish_order.
func TestProxyEarlyDeliveryCleanTailAndHAFinishOrder(t *testing.T) {
	fake := newGatedUpstream(t)
	upstream := httptest.NewServer(fake)
	defer upstream.Close()
	proxyServer := httptest.NewServer(newProxy(t, upstream.URL, nil))
	defer proxyServer.Close()

	resp := postChat(t, proxyServer.URL, `{"model":"hermes-agent","stream":true,"messages":[{"role":"user","content":"hi"}]}`)
	defer resp.Body.Close()
	reader := sse.NewReader(resp.Body)

	first := decodeEvent(t, nextEvent(t, reader).Data)
	delta := firstChoice(t, first)["delta"].(map[string]any)
	if len(delta) != 1 || delta["role"] != "assistant" {
		t.Fatalf("first delta = %#v, want only the assistant role", delta)
	}

	early := decodeEvent(t, nextEvent(t, reader).Data)
	content := firstChoice(t, early)["delta"].(map[string]any)["content"]
	if content != "Первое предложение.\n\n" {
		t.Fatalf("early content = %q", content)
	}
	if fake.released() {
		t.Fatal("the proxy buffered the whole answer instead of streaming the first sentence")
	}

	fake.release()
	remaining := allEvents(t, reader)
	if len(remaining) == 0 || remaining[len(remaining)-1].Data != "[DONE]" {
		t.Fatalf("remaining = %#v", remaining)
	}

	received := fake.last()
	if stream, _ := received["stream"].(bool); !stream {
		t.Errorf("upstream stream = %v, want true", received["stream"])
	}
	options, _ := received["model_options"].(map[string]any)
	if options["reasoning_effort"] != "low" {
		t.Errorf("reasoning_effort = %v, want low", options["reasoning_effort"])
	}
	headers := fake.lastHeaders()
	if headers.Get("Accept") != "text/event-stream" {
		t.Errorf("Accept = %q, want text/event-stream", headers.Get("Accept"))
	}
	if headers.Get("Content-Type") != "application/json" {
		t.Errorf("Content-Type = %q, want application/json", headers.Get("Content-Type"))
	}

	chunks := []map[string]any{early}
	for _, event := range remaining[:len(remaining)-1] {
		chunks = append(chunks, decodeEvent(t, event.Data))
	}
	// Mirror HA's rule: a finish frame terminates reading before its delta is used.
	var spoken strings.Builder
	for _, chunk := range chunks {
		choice := firstChoice(t, chunk)
		if choice["finish_reason"] != nil {
			break
		}
		delta, _ := choice["delta"].(map[string]any)
		if text, ok := delta["content"].(string); ok {
			spoken.WriteString(text)
		}
	}
	if got, want := sanitize.SanitizeText(spoken.String()), "Первое предложение. Источник: Reuters."; got != want {
		t.Fatalf("spoken = %q, want %q", got, want)
	}
	rendered, _ := encodeJSON(chunks)
	if bytes.Contains(rendered, []byte("SECRET")) {
		t.Error("reasoning content leaked into the stream")
	}
	if bytes.Contains(rendered, []byte("private")) {
		t.Error("tool progress leaked into the stream")
	}
}

// TestProxyMidstreamDisconnectIsErrorWithoutSecondHTTPResponse mirrors
// test_midstream_disconnect_is_error_without_second_http_response.
func TestProxyMidstreamDisconnectIsErrorWithoutSecondHTTPResponse(t *testing.T) {
	fake := newGatedUpstream(t)
	fake.setMode("truncated")
	upstream := httptest.NewServer(fake)
	defer upstream.Close()
	proxyServer := httptest.NewServer(newProxy(t, upstream.URL, nil))
	defer proxyServer.Close()

	resp := postChat(t, proxyServer.URL, `{"model":"hermes-agent","stream":true,"messages":[{"role":"user","content":"hi"}]}`)
	defer resp.Body.Close()
	fake.release()
	raw, err := io.ReadAll(resp.Body)
	if err != nil {
		t.Fatalf("read: %v", err)
	}
	text := string(raw)
	if !strings.Contains(text, `"error"`) {
		t.Errorf("body = %q, want an error frame", text)
	}
	if strings.Contains(text, "[DONE]") {
		t.Errorf("body = %q, must not contain [DONE]", text)
	}
	if strings.Contains(text, "HTTP/") {
		t.Errorf("body = %q, must not contain a second HTTP response", text)
	}
}

// TestProxyHTTPErrorKeepsStatus mirrors test_http_error_keeps_status.
func TestProxyHTTPErrorKeepsStatus(t *testing.T) {
	fake := newGatedUpstream(t)
	fake.setMode("http_error")
	upstream := httptest.NewServer(fake)
	defer upstream.Close()
	proxyServer := httptest.NewServer(newProxy(t, upstream.URL, nil))
	defer proxyServer.Close()

	resp := postChat(t, proxyServer.URL, `{"model":"hermes-agent","stream":true,"messages":[{"role":"user","content":"hi"}]}`)
	defer resp.Body.Close()
	if resp.StatusCode != http.StatusTooManyRequests {
		t.Fatalf("status = %d, want 429", resp.StatusCode)
	}
	var body map[string]any
	if err := json.NewDecoder(resp.Body).Decode(&body); err != nil {
		t.Fatalf("decode: %v", err)
	}
	errorObject, ok := body["error"].(map[string]any)
	if !ok || errorObject["message"] != "limited" {
		t.Fatalf("error = %#v, want the upstream object preserved", body["error"])
	}
}

// ------------------------------------------------------ StreamingTests -----

// TestReasoningOverrideIsVoiceOnlyAndPreservesOptions mirrors
// test_reasoning_override_is_voice_only_and_preserves_options.
func TestReasoningOverrideIsVoiceOnlyAndPreservesOptions(t *testing.T) {
	original := testPayload(t, `{"stream":true,"model_options":{"reasoning_effort":"high","fast":false}}`)
	prepared := PrepareChatRequest(original, "low")
	options := prepared["model_options"].(map[string]any)
	if options["reasoning_effort"] != "low" || options["fast"] != false {
		t.Fatalf("model_options = %#v", options)
	}
	if original["model_options"].(map[string]any)["reasoning_effort"] != "high" {
		t.Fatal("PrepareChatRequest mutated the caller's payload")
	}
}

// ------------------------------------------------------------ new cases -----

// TestBufferedResponseIsSanitised pins the buffered relay: every choice content
// is sanitised and the payload shape is preserved.
func TestBufferedResponseIsSanitised(t *testing.T) {
	upstream := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		w.Header().Set("Content-Type", "application/json")
		_, _ = io.WriteString(w, `{"id":"x","choices":[{"message":{"role":"assistant","content":"A https://a.example"}},{"message":{"content":"B [x](https://b.example)"}}],"usage":{"n":1}}`)
	}))
	defer upstream.Close()
	server := httptest.NewServer(newProxy(t, upstream.URL, nil))
	defer server.Close()

	resp := postChat(t, server.URL, `{"model":"hermes-agent","messages":[{"role":"user","content":"hi"}]}`)
	defer resp.Body.Close()
	var body map[string]any
	if err := json.NewDecoder(resp.Body).Decode(&body); err != nil {
		t.Fatalf("decode: %v", err)
	}
	choices := body["choices"].([]any)
	first := choices[0].(map[string]any)["message"].(map[string]any)["content"]
	second := choices[1].(map[string]any)["message"].(map[string]any)["content"]
	if first != "A" || second != "B x" {
		t.Fatalf("contents = %q, %q", first, second)
	}
	if _, ok := body["usage"]; !ok {
		t.Fatal("the rest of the payload was dropped")
	}
}

// TestGetPassthroughRelaysStatusAndBody pins GET <any path>.
func TestGetPassthroughRelaysStatusAndBody(t *testing.T) {
	upstream := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		if r.Method != http.MethodGet || r.URL.RequestURI() != "/v1/models?x=1" {
			t.Errorf("upstream got %s %s", r.Method, r.URL.RequestURI())
		}
		if r.Header.Get("Authorization") != "Bearer client-token" {
			t.Errorf("Authorization = %q, want it forwarded", r.Header.Get("Authorization"))
		}
		w.Header().Set("Content-Type", "application/json")
		_, _ = io.WriteString(w, `{"data":[]}`)
	}))
	defer upstream.Close()
	server := httptest.NewServer(newProxy(t, upstream.URL, nil))
	defer server.Close()

	req, _ := http.NewRequest(http.MethodGet, server.URL+"/v1/models?x=1", nil)
	req.Header.Set("Authorization", "Bearer client-token")
	resp, err := http.DefaultClient.Do(req)
	if err != nil {
		t.Fatalf("GET: %v", err)
	}
	defer resp.Body.Close()
	body, _ := io.ReadAll(resp.Body)
	if resp.StatusCode != http.StatusOK || string(body) != `{"data":[]}` {
		t.Fatalf("status/body = %d %q", resp.StatusCode, body)
	}
}

// TestOtherPostPathsAreReencoded pins the de-facto Python behaviour: a POST to
// any other path is parsed and re-serialised before proxying.
func TestOtherPostPathsAreReencoded(t *testing.T) {
	received := make(chan string, 1)
	upstream := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		data, _ := io.ReadAll(r.Body)
		received <- string(data)
		w.Header().Set("Content-Type", "application/json")
		_, _ = io.WriteString(w, `{"ok":true}`)
	}))
	defer upstream.Close()
	server := httptest.NewServer(newProxy(t, upstream.URL, nil))
	defer server.Close()

	resp, err := http.Post(server.URL+"/v1/other", "application/json", strings.NewReader(`{ "b" : 2, "a" : 1 }`))
	if err != nil {
		t.Fatalf("POST: %v", err)
	}
	defer resp.Body.Close()
	if resp.StatusCode != http.StatusOK {
		t.Fatalf("status = %d", resp.StatusCode)
	}
	select {
	case body := <-received:
		if body != `{"a":1,"b":2}` {
			t.Fatalf("upstream body = %q, want a re-encoded object", body)
		}
	case <-time.After(2 * time.Second):
		t.Fatal("upstream never received the request")
	}
}

// TestNonJSONPostIs503 pins the inherited behaviour: a body that is not a JSON
// object cannot be re-serialised.
func TestNonJSONPostIs503(t *testing.T) {
	server := httptest.NewServer(newProxy(t, "http://127.0.0.1:1", nil))
	defer server.Close()

	for _, body := range []string{`not json`, `[1,2]`, `null`, `5`} {
		resp, err := http.Post(server.URL+"/v1/chat/completions", "application/json", strings.NewReader(body))
		if err != nil {
			t.Fatalf("POST %q: %v", body, err)
		}
		var payload map[string]any
		decodeErr := json.NewDecoder(resp.Body).Decode(&payload)
		resp.Body.Close()
		if resp.StatusCode != http.StatusServiceUnavailable {
			t.Errorf("POST %q status = %d, want 503", body, resp.StatusCode)
		}
		if decodeErr != nil {
			t.Errorf("POST %q body is not JSON: %v", body, decodeErr)
		}
		if _, ok := payload["error"].(map[string]any); !ok {
			t.Errorf("POST %q error = %#v, want an object", body, payload["error"])
		}
	}
}

// TestBodyLimitRejectsOversizedRequest pins the 10 MiB cap.
func TestBodyLimitRejectsOversizedRequest(t *testing.T) {
	server := httptest.NewServer(newProxy(t, "http://127.0.0.1:1", nil))
	defer server.Close()

	oversized := strings.Repeat("a", hermes.MaxBody+1)
	resp, err := http.Post(server.URL+"/v1/chat/completions", "application/json", strings.NewReader(oversized))
	if err != nil {
		t.Fatalf("POST: %v", err)
	}
	defer resp.Body.Close()
	if resp.StatusCode != http.StatusServiceUnavailable {
		t.Fatalf("status = %d, want 503", resp.StatusCode)
	}
	var body map[string]any
	if err := json.NewDecoder(resp.Body).Decode(&body); err != nil {
		t.Fatalf("decode: %v", err)
	}
	errorObject, ok := body["error"].(map[string]any)
	if !ok || errorObject["message"] == "" {
		t.Fatalf("error = %#v, want an OpenAI-shaped object", body["error"])
	}
}

// TestUnsupportedMethodIs501 mirrors http.server, which only implements GET and
// POST.
func TestUnsupportedMethodIs501(t *testing.T) {
	server := httptest.NewServer(newProxy(t, "http://127.0.0.1:1", nil))
	defer server.Close()
	req, _ := http.NewRequest(http.MethodPut, server.URL+"/v1/chat/completions", strings.NewReader("{}"))
	resp, err := http.DefaultClient.Do(req)
	if err != nil {
		t.Fatalf("PUT: %v", err)
	}
	defer resp.Body.Close()
	if resp.StatusCode != http.StatusNotImplemented {
		t.Fatalf("status = %d, want 501", resp.StatusCode)
	}
}

// TestClientDisconnectCancelsUpstream pins the context propagation: a closed HA
// connection must interrupt the Hermes turn instead of orphaning it.
func TestClientDisconnectCancelsUpstream(t *testing.T) {
	cancelled := make(chan struct{}, 1)
	upstream := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		w.Header().Set("Content-Type", "text/event-stream")
		w.WriteHeader(http.StatusOK)
		flusher := w.(http.Flusher)
		_, _ = io.WriteString(w, `data: {"id":"x","object":"chat.completion.chunk","choices":[{"index":0,"delta":{"role":"assistant"},"finish_reason":null}]}`+"\n\n")
		flusher.Flush()
		select {
		case <-r.Context().Done():
			cancelled <- struct{}{}
		case <-time.After(5 * time.Second):
		}
	}))
	defer upstream.Close()
	server := httptest.NewServer(newProxy(t, upstream.URL, nil))
	defer server.Close()

	ctx, cancel := context.WithCancel(context.Background())
	req, _ := http.NewRequestWithContext(ctx, http.MethodPost, server.URL+"/v1/chat/completions",
		strings.NewReader(`{"model":"hermes-agent","stream":true,"messages":[{"role":"user","content":"hi"}]}`))
	req.Header.Set("Content-Type", "application/json")
	resp, err := http.DefaultClient.Do(req)
	if err != nil {
		t.Fatalf("POST: %v", err)
	}
	reader := sse.NewReader(resp.Body)
	nextEvent(t, reader) // the first frame proves the relay is running
	cancel()
	resp.Body.Close()

	select {
	case <-cancelled:
	case <-time.After(5 * time.Second):
		t.Fatal("the upstream request was not cancelled after the client disconnected")
	}
}

// TestLogFormatKeepsTheSmokeGrepWorking pins the "voice-proxy:" prefix, the
// uppercase ERROR level and the absence of speech text.
func TestLogFormatKeepsTheSmokeGrepWorking(t *testing.T) {
	var logs bytes.Buffer
	fake := newGatedUpstream(t)
	upstream := httptest.NewServer(fake)
	defer upstream.Close()
	server := httptest.NewServer(newProxy(t, upstream.URL, NewStderrLogger(&logs)))
	defer server.Close()

	drain(t, postChat(t, server.URL, `{"model":"hermes-agent","stream":false,"messages":[{"role":"user","content":"отмена"}]}`))
	line := logs.String()
	if !strings.Contains(line, "voice-proxy: cancel guarded count=1 stream=false upstream=skipped") {
		t.Errorf("log = %q", line)
	}

	fake.setMode("truncated")
	fake.release()
	drain(t, postChat(t, server.URL, `{"model":"hermes-agent","stream":true,"messages":[{"role":"user","content":"hi"}]}`))
	line = logs.String()
	smoke := regexp.MustCompile(`\b(ERROR|CRITICAL|Traceback)\b`)
	if !smoke.MatchString(line) {
		t.Errorf("log = %q, want an uppercase ERROR token for a real failure", line)
	}
	if !strings.Contains(line, "voice-proxy: ERROR stream failed:") {
		t.Errorf("log = %q, want the stream failure recorded", line)
	}
	if strings.Contains(line, "отмена") {
		t.Errorf("log = %q, must never contain recognised speech", line)
	}
}

// ------------------------------------------------------------- fakes -------

// recordingUpstream records every POST body and answers with a plain JSON
// completion, like the Python test's Upstream.
type recordingUpstream struct {
	mu       sync.Mutex
	requests []map[string]any
}

func (u *recordingUpstream) ServeHTTP(w http.ResponseWriter, r *http.Request) {
	var body map[string]any
	_ = json.NewDecoder(r.Body).Decode(&body)
	u.mu.Lock()
	u.requests = append(u.requests, body)
	u.mu.Unlock()
	w.Header().Set("Content-Type", "application/json")
	_, _ = io.WriteString(w, `{"choices":[{"message":{"role":"assistant","content":"upstream"}}]}`)
}

func (u *recordingUpstream) count() int {
	u.mu.Lock()
	defer u.mu.Unlock()
	return len(u.requests)
}

func (u *recordingUpstream) reset() {
	u.mu.Lock()
	defer u.mu.Unlock()
	u.requests = nil
}

// gatedUpstream reproduces the Python ProxyTransportTests Upstream: a role
// chunk, tool progress, a reasoning delta, the first sentence, a gate, then the
// tail and [DONE].
type gatedUpstream struct {
	mu       sync.Mutex
	mode     string
	gate     chan struct{}
	closed   bool
	requests []map[string]any
	headers  []http.Header
}

func newGatedUpstream(t *testing.T) *gatedUpstream {
	t.Helper()
	return &gatedUpstream{mode: "normal", gate: make(chan struct{})}
}

func (u *gatedUpstream) setMode(mode string) {
	u.mu.Lock()
	defer u.mu.Unlock()
	u.mode = mode
}

func (u *gatedUpstream) release() {
	u.mu.Lock()
	defer u.mu.Unlock()
	if !u.closed {
		close(u.gate)
		u.closed = true
	}
}

func (u *gatedUpstream) released() bool {
	u.mu.Lock()
	defer u.mu.Unlock()
	return u.closed
}

func (u *gatedUpstream) lastHeaders() http.Header {
	u.mu.Lock()
	defer u.mu.Unlock()
	if len(u.headers) == 0 {
		return http.Header{}
	}
	return u.headers[len(u.headers)-1]
}

func (u *gatedUpstream) last() map[string]any {
	u.mu.Lock()
	defer u.mu.Unlock()
	if len(u.requests) == 0 {
		return nil
	}
	return u.requests[len(u.requests)-1]
}

func (u *gatedUpstream) ServeHTTP(w http.ResponseWriter, r *http.Request) {
	var body map[string]any
	_ = json.NewDecoder(r.Body).Decode(&body)
	u.mu.Lock()
	u.requests = append(u.requests, body)
	u.headers = append(u.headers, r.Header.Clone())
	mode := u.mode
	u.mu.Unlock()

	if mode == "http_error" {
		w.Header().Set("Content-Type", "application/json")
		w.WriteHeader(http.StatusTooManyRequests)
		_, _ = io.WriteString(w, `{"error":{"message":"limited"}}`)
		return
	}

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

	select {
	case <-u.gate:
	case <-r.Context().Done():
		return
	}
	if mode == "truncated" {
		return
	}
	send(map[string]any{"content": "Источник: [Reuters](https://example.com)."}, nil)
	send(map[string]any{}, "stop")
	_, _ = io.WriteString(w, "data: [DONE]\n\n")
	flusher.Flush()
}

// ------------------------------------------------ StartupRecoveryTests -----

// TestUnavailableUpstreamIsOpenAIErrorAndRecoversWithoutProxyRestart mirrors
// test_unavailable_upstream_is_openai_error_and_recovers_without_proxy_restart:
// the failure is not cached, so the next request reaches Hermes again.
func TestUnavailableUpstreamIsOpenAIErrorAndRecoversWithoutProxyRestart(t *testing.T) {
	flaky := &flakyUpstream{}
	flaky.down.Store(true)
	upstream := httptest.NewServer(flaky)
	defer upstream.Close()
	// One proxy process for the whole test: the failure must not be cached.
	server := httptest.NewServer(newProxy(t, upstream.URL, nil))
	defer server.Close()

	for _, method := range []string{http.MethodGet, http.MethodPost} {
		var body io.Reader
		if method == http.MethodPost {
			body = strings.NewReader(`{"stream": false}`)
		}
		req, err := http.NewRequest(method, server.URL+"/v1/models", body)
		if err != nil {
			t.Fatalf("build request: %v", err)
		}
		resp, err := http.DefaultClient.Do(req)
		if err != nil {
			t.Fatalf("%s: %v", method, err)
		}
		var payload map[string]any
		decodeErr := json.NewDecoder(resp.Body).Decode(&payload)
		resp.Body.Close()
		if resp.StatusCode != http.StatusServiceUnavailable {
			t.Errorf("%s status = %d, want 503", method, resp.StatusCode)
		}
		if decodeErr != nil {
			t.Errorf("%s body is not JSON: %v", method, decodeErr)
		}
		errorObject, ok := payload["error"].(map[string]any)
		if !ok {
			t.Errorf("%s error is not an object: %#v", method, payload["error"])
			continue
		}
		if message, _ := errorObject["message"].(string); message == "" {
			t.Errorf("%s error message is empty", method)
		}
	}

	flaky.down.Store(false)
	resp, err := http.Get(server.URL + "/v1/models")
	if err != nil {
		t.Fatalf("GET: %v", err)
	}
	defer resp.Body.Close()
	if resp.StatusCode != http.StatusOK {
		t.Fatalf("status = %d, want 200 after recovery", resp.StatusCode)
	}
}

// flakyUpstream can drop a connection without stopping its listener, which is
// how the Python test simulates a refused connection, then serve normally
// again.
type flakyUpstream struct {
	down atomic.Bool
}

func (f *flakyUpstream) ServeHTTP(w http.ResponseWriter, r *http.Request) {
	if f.down.Load() {
		if hijacker, ok := w.(http.Hijacker); ok {
			if conn, _, err := hijacker.Hijack(); err == nil {
				_ = conn.Close()
			}
		}
		return
	}
	w.Header().Set("Content-Type", "application/json")
	_, _ = io.WriteString(w, `{"data": []}`)
}
