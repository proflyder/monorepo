package router_test

import (
	"context"
	"encoding/json"
	"net/http"
	"net/http/httptest"
	"strings"
	"sync"
	"testing"
	"time"

	"ricardo.local/monorepo/solutions/voice-proxy/internal/announce"
	"ricardo.local/monorepo/solutions/voice-proxy/internal/gateway"
	"ricardo.local/monorepo/solutions/voice-proxy/internal/profile"
	"ricardo.local/monorepo/solutions/voice-proxy/internal/proxy"
	"ricardo.local/monorepo/solutions/voice-proxy/internal/result"
	"ricardo.local/monorepo/solutions/voice-proxy/internal/router"
	"ricardo.local/monorepo/solutions/voice-proxy/internal/sse"
)

type haCall struct {
	path string
	body map[string]any
}

type pipeline struct {
	t *testing.T

	mu                    sync.Mutex
	haState               string
	haDown                bool
	haCalls               []string
	announceCalls         []haCall
	utterances            []map[string]any
	results               []map[string]any
	resultCalls           []map[string]any
	gatewayDown           bool
	gatewayReply          map[string]any
	hermesCalls           []map[string]any
	startConversationFail bool

	profileCache  time.Duration
	resultTimeout time.Duration
	resultRetry   time.Duration

	haServer       *httptest.Server
	gatewayServer  *httptest.Server
	upstreamServer *httptest.Server
	proxyServer    *httptest.Server
	proxyServers   []*httptest.Server

	announcer *result.Announcer
	router    *router.Router
}

func newPipeline(t *testing.T, profileCache time.Duration) *pipeline {
	t.Helper()
	p := &pipeline{
		t:             t,
		haState:       "Личный",
		gatewayReply:  map[string]any{"route": "hermes"},
		profileCache:  profileCache,
		resultTimeout: 5 * time.Second,
		resultRetry:   50 * time.Millisecond,
	}
	p.haServer = httptest.NewServer(http.HandlerFunc(p.serveHA))
	p.gatewayServer = httptest.NewServer(http.HandlerFunc(p.serveGateway))
	p.upstreamServer = httptest.NewServer(http.HandlerFunc(p.serveUpstream))
	p.buildRouter()
	p.proxyServer = p.spawnProxy(true)
	t.Cleanup(func() {
		p.announcer.Wait(2 * time.Second)
		for _, server := range p.proxyServers {
			server.Close()
		}
		p.upstreamServer.Close()
		p.gatewayServer.Close()
		p.haServer.Close()
	})
	return p
}

func (p *pipeline) buildRouter() {
	reader := profile.New(profile.Config{
		URL:     p.haServer.URL,
		Token:   func() string { return "test-ha-token" },
		Entity:  "input_select.ricardo_voice_profile",
		Cache:   p.profileCache,
		Timeout: time.Second,
	})
	gateways := gateway.New(gateway.Config{
		URL:     p.gatewayServer.URL,
		Token:   func() string { return "test-voice-token" },
		Timeout: time.Second,
	})
	speaker := announce.New(announce.Config{
		URL:       p.haServer.URL,
		Token:     func() string { return "test-ha-token" },
		Script:    "script.schedule_announce",
		Satellite: "assist_satellite.iandeks_stantsiia_midi_assist_satellite",
		Timeout:   time.Second,
	})
	p.announcer = result.New(result.Config{
		Context:      context.Background(),
		Client:       gateways,
		Announce:     speaker,
		QueueEnabled: func() bool { return false },
		MaxChars:     200,
		Timeout:      p.resultTimeout,
		Wait:         25,
		Retry:        p.resultRetry,
	})
	p.router = router.New(router.Config{Profile: reader, Gateway: gateways, Results: p.announcer})
}

func (p *pipeline) newProxyServer(routing bool) *httptest.Server {
	var voiceRouter proxy.Router
	if routing {
		voiceRouter = p.router
	}
	return httptest.NewServer(proxy.New(proxy.Options{
		UpstreamURL:     p.upstreamServer.URL,
		ReasoningEffort: "low",
		Logger:          proxy.NopLogger{},
		Router:          voiceRouter,
	}))
}

// spawnProxy starts a proxy server and tracks it for cleanup.
func (p *pipeline) spawnProxy(routing bool) *httptest.Server {
	server := p.newProxyServer(routing)
	p.proxyServers = append(p.proxyServers, server)
	return server
}

func (p *pipeline) set(fn func()) {
	p.mu.Lock()
	defer p.mu.Unlock()
	fn()
}

func (p *pipeline) serveHA(w http.ResponseWriter, r *http.Request) {
	p.mu.Lock()
	if r.Method == http.MethodGet {
		p.haCalls = append(p.haCalls, r.URL.Path)
	}
	if r.Method == http.MethodPost {
		var body map[string]any
		_ = json.NewDecoder(r.Body).Decode(&body)
		p.announceCalls = append(p.announceCalls, haCall{path: r.URL.Path, body: body})
		if p.startConversationFail && strings.HasSuffix(r.URL.Path, "/api/services/assist_satellite/start_conversation") {
			p.mu.Unlock()
			w.WriteHeader(http.StatusServiceUnavailable)
			return
		}
	}
	down := p.haDown
	state := p.haState
	p.mu.Unlock()

	if r.Method == http.MethodGet {
		if down {
			w.WriteHeader(http.StatusServiceUnavailable)
			_, _ = w.Write([]byte(`{"error":"down"}`))
			return
		}
		w.Header().Set("Content-Type", "application/json")
		_ = json.NewEncoder(w).Encode(map[string]any{
			"entity_id": "input_select.ricardo_voice_profile", "state": state,
		})
		return
	}
	w.Header().Set("Content-Type", "application/json")
	_, _ = w.Write([]byte(`{"changed_states":[]}`))
}

func (p *pipeline) serveGateway(w http.ResponseWriter, r *http.Request) {
	var body map[string]any
	_ = json.NewDecoder(r.Body).Decode(&body)

	p.mu.Lock()
	if p.gatewayDown {
		p.mu.Unlock()
		w.WriteHeader(http.StatusServiceUnavailable)
		_, _ = w.Write([]byte(`{"error":"down"}`))
		return
	}
	if strings.HasSuffix(r.URL.Path, "/v1/voice/utterance") {
		p.utterances = append(p.utterances, body)
		reply := p.gatewayReply
		p.mu.Unlock()
		w.Header().Set("Content-Type", "application/json")
		_ = json.NewEncoder(w).Encode(reply)
		return
	}
	if strings.HasSuffix(r.URL.Path, "/v1/voice/result") {
		p.resultCalls = append(p.resultCalls, body)
		reply := map[string]any{"status": "running"}
		if len(p.results) > 0 {
			reply = p.results[0]
			p.results = p.results[1:]
		}
		p.mu.Unlock()
		w.Header().Set("Content-Type", "application/json")
		_ = json.NewEncoder(w).Encode(reply)
		return
	}
	p.mu.Unlock()
	w.WriteHeader(http.StatusNotFound)
}

func (p *pipeline) serveUpstream(w http.ResponseWriter, r *http.Request) {
	var body map[string]any
	_ = json.NewDecoder(r.Body).Decode(&body)
	p.mu.Lock()
	p.hermesCalls = append(p.hermesCalls, body)
	p.mu.Unlock()
	w.Header().Set("Content-Type", "application/json")
	_, _ = w.Write([]byte(`{"choices":[{"message":{"role":"assistant","content":"Ответ Hermes."},"finish_reason":"stop"}]}`))
}

func (p *pipeline) open(address, text string, stream bool, role string) *http.Response {
	p.t.Helper()
	payload := map[string]any{
		"model": "hermes-agent", "stream": stream,
		"messages": []any{map[string]any{"role": role, "content": text}},
	}
	raw, _ := json.Marshal(payload)
	resp, err := http.Post(address+"/v1/chat/completions", "application/json", strings.NewReader(string(raw)))
	if err != nil {
		p.t.Fatalf("POST: %v", err)
	}
	return resp
}

// ask returns (spoken text, finish_seen) exactly as HA would read it.
func (p *pipeline) ask(text string, stream bool, role string) (string, bool) {
	p.t.Helper()
	return p.askAt(p.proxyServer.URL, text, stream, role)
}

func (p *pipeline) askAt(address, text string, stream bool, role string) (string, bool) {
	p.t.Helper()
	resp := p.open(address, text, stream, role)
	defer resp.Body.Close()
	if !stream {
		var body map[string]any
		if err := json.NewDecoder(resp.Body).Decode(&body); err != nil {
			p.t.Fatalf("decode: %v", err)
		}
		choices := body["choices"].([]any)
		choice := choices[0].(map[string]any)
		message := choice["message"].(map[string]any)
		content, _ := message["content"].(string)
		return content, choice["finish_reason"] == "stop"
	}
	reader := sse.NewReader(resp.Body)
	spoken := ""
	finished := false
	for {
		event, err := reader.Next()
		if err != nil {
			break
		}
		if event.Data == "[DONE]" {
			continue
		}
		var chunk map[string]any
		if err := json.Unmarshal([]byte(event.Data), &chunk); err != nil {
			p.t.Fatalf("decode event: %v", err)
		}
		choice := chunk["choices"].([]any)[0].(map[string]any)
		if choice["finish_reason"] != nil {
			finished = true
			break
		}
		delta, _ := choice["delta"].(map[string]any)
		if content, ok := delta["content"].(string); ok {
			spoken += content
		}
	}
	return spoken, finished
}

func (p *pipeline) waitUntil(predicate func() bool, timeout time.Duration, message string) {
	p.t.Helper()
	deadline := time.Now().Add(timeout)
	for time.Now().Before(deadline) {
		p.announcer.Wait(20 * time.Millisecond)
		if predicate() {
			return
		}
		time.Sleep(5 * time.Millisecond)
	}
	p.t.Fatalf("%s", message)
}

func (p *pipeline) announceCount() int {
	p.mu.Lock()
	defer p.mu.Unlock()
	return len(p.announceCalls)
}

func (p *pipeline) announceAt(index int) haCall {
	p.mu.Lock()
	defer p.mu.Unlock()
	return p.announceCalls[index]
}

func (p *pipeline) hermesCount() int {
	p.mu.Lock()
	defer p.mu.Unlock()
	return len(p.hermesCalls)
}

func (p *pipeline) haCount() int {
	p.mu.Lock()
	defer p.mu.Unlock()
	return len(p.haCalls)
}

// --------------------------------------------------------------- routing ---

// TestPersonalProfileHermesRouteForwardsUnchanged mirrors
// test_personal_profile_hermes_route_forwards_unchanged.
func TestPersonalProfileHermesRouteForwardsUnchanged(t *testing.T) {
	p := newPipeline(t, 2*time.Second)
	spoken, finished := p.ask("что такое фотосинтез", false, "user")
	if spoken != "Ответ Hermes." || !finished {
		t.Fatalf("spoken=%q finished=%t", spoken, finished)
	}
	p.mu.Lock()
	defer p.mu.Unlock()
	if len(p.utterances) != 1 || p.utterances[0]["profile"] != "personal" {
		t.Fatalf("utterances = %#v", p.utterances)
	}
	if len(p.hermesCalls) != 1 {
		t.Fatalf("hermes calls = %d", len(p.hermesCalls))
	}
}

// TestWorkProfileBBRouteAnswersWithoutHermesAndAnnouncesLater mirrors
// test_work_profile_bb_route_answers_without_hermes_and_announces_later.
func TestWorkProfileBBRouteAnswersWithoutHermesAndAnnouncesLater(t *testing.T) {
	p := newPipeline(t, 2*time.Second)
	p.set(func() {
		p.haState = "Рабочий"
		p.gatewayReply = map[string]any{"route": "bb", "ok": true, "threadId": "thr_example", "afterSeq": 7, "speech": "Передал в рабочий тред."}
		p.results = []map[string]any{{"status": "done", "speech": "**Готово**: отчёт [тут](https://example.com/x)."}}
	})
	spoken, finished := p.ask("сделай отчёт", false, "user")
	if spoken != "Передал в рабочий тред." || !finished {
		t.Fatalf("spoken=%q finished=%t", spoken, finished)
	}
	if p.hermesCount() != 0 {
		t.Fatal("Hermes was called")
	}
	p.waitUntil(func() bool { return p.announceCount() >= 1 }, time.Second, "announce was not sent")
	p.mu.Lock()
	defer p.mu.Unlock()
	if p.utterances[0]["profile"] != "work" {
		t.Errorf("profile = %v", p.utterances[0]["profile"])
	}
	if len(p.resultCalls) != 1 || p.resultCalls[0]["threadId"] != "thr_example" || p.resultCalls[0]["afterSeq"] != float64(7) {
		t.Fatalf("result calls = %#v", p.resultCalls)
	}
	call := p.announceCalls[0]
	if call.path != "/api/services/script/turn_on" {
		t.Errorf("path = %q", call.path)
	}
	if call.body["entity_id"] != "script.schedule_announce" {
		t.Errorf("entity_id = %v", call.body["entity_id"])
	}
	variables := call.body["variables"].(map[string]any)
	if variables["message"] != "Готово: отчёт тут." || variables["chime"] != false {
		t.Errorf("variables = %#v", variables)
	}
}

// TestLateQuestionStartsConversationAndKeepsMicrophoneOpen mirrors
// test_late_question_starts_conversation_and_keeps_microphone_open.
func TestLateQuestionStartsConversationAndKeepsMicrophoneOpen(t *testing.T) {
	p := newPipeline(t, 2*time.Second)
	p.set(func() {
		p.haState = "Рабочий"
		p.gatewayReply = map[string]any{"route": "bb", "ok": true, "threadId": "thr_example", "afterSeq": 2, "speech": "Передал в рабочий тред."}
		p.results = []map[string]any{{"status": "done", "speech": "Готово. Подтверждаете?"}}
	})
	p.ask("проверь", false, "user")
	p.waitUntil(func() bool { return p.announceCount() >= 1 }, time.Second, "announce was not sent")
	call := p.announceAt(0)
	if call.path != "/api/services/assist_satellite/start_conversation" {
		t.Fatalf("path = %q", call.path)
	}
	want := map[string]any{
		"entity_id":     "assist_satellite.iandeks_stantsiia_midi_assist_satellite",
		"start_message": "Готово. Подтверждаете?",
		"preannounce":   false,
	}
	for key, value := range want {
		if call.body[key] != value {
			t.Errorf("%s = %#v, want %#v", key, call.body[key], value)
		}
	}
}

// TestLateStatementStillUsesTheAnnouncementScript mirrors
// test_late_statement_still_uses_the_announcement_script.
func TestLateStatementStillUsesTheAnnouncementScript(t *testing.T) {
	p := newPipeline(t, 2*time.Second)
	p.set(func() {
		p.haState = "Рабочий"
		p.gatewayReply = map[string]any{"route": "bb", "ok": true, "threadId": "thr_example", "afterSeq": 2, "speech": "Передал в рабочий тред."}
		p.results = []map[string]any{{"status": "done", "speech": "Готово."}}
	})
	p.ask("проверь", false, "user")
	p.waitUntil(func() bool { return p.announceCount() >= 1 }, time.Second, "announce was not sent")
	call := p.announceAt(0)
	if call.path != "/api/services/script/turn_on" {
		t.Fatalf("path = %q", call.path)
	}
	variables := call.body["variables"].(map[string]any)
	if variables["message"] != "Готово." {
		t.Fatalf("variables = %#v", variables)
	}
}

// TestFailedStartConversationFallsBackToScriptAnnounce mirrors
// test_failed_start_conversation_falls_back_to_script_announce.
func TestFailedStartConversationFallsBackToScriptAnnounce(t *testing.T) {
	p := newPipeline(t, 2*time.Second)
	p.set(func() {
		p.startConversationFail = true
		p.haState = "Рабочий"
		p.gatewayReply = map[string]any{"route": "bb", "ok": true, "threadId": "thr_example", "afterSeq": 2, "speech": "Передал в рабочий тред."}
		p.results = []map[string]any{{"status": "done", "speech": "Подтверждаете?"}}
	})
	p.ask("проверь", false, "user")
	p.waitUntil(func() bool { return p.announceCount() >= 2 }, time.Second, "fallback announce was not sent")
	first := p.announceAt(0)
	second := p.announceAt(1)
	if first.path != "/api/services/assist_satellite/start_conversation" {
		t.Errorf("first path = %q", first.path)
	}
	if second.path != "/api/services/script/turn_on" {
		t.Errorf("second path = %q", second.path)
	}
	if second.body["variables"].(map[string]any)["message"] != "Подтверждаете?" {
		t.Errorf("second body = %#v", second.body)
	}
}

// TestBBRouteStreamsTheAckAndNeverCallsHermes mirrors
// test_bb_route_streams_the_ack_and_never_calls_hermes.
func TestBBRouteStreamsTheAckAndNeverCallsHermes(t *testing.T) {
	p := newPipeline(t, 2*time.Second)
	p.set(func() {
		p.haState = "Рабочий"
		p.gatewayReply = map[string]any{"route": "bb", "ok": true, "threadId": "thr_example", "afterSeq": 3, "speech": "Передал в рабочий тред."}
		p.results = []map[string]any{{"status": "done", "speech": "Готово."}}
	})
	spoken, finished := p.ask("проверь сборку", true, "user")
	if spoken != "Передал в рабочий тред." || !finished {
		t.Fatalf("spoken=%q finished=%t", spoken, finished)
	}
	if p.hermesCount() != 0 {
		t.Fatal("Hermes was called")
	}
	p.waitUntil(func() bool { return p.announceCount() >= 1 }, time.Second, "announce was not sent")
	if p.announceAt(0).body["variables"].(map[string]any)["message"] != "Готово." {
		t.Fatalf("announce = %#v", p.announceAt(0))
	}
}

// TestLocalRouteAnswersWithItsOwnPhrase mirrors
// test_local_route_answers_with_its_own_phrase.
func TestLocalRouteAnswersWithItsOwnPhrase(t *testing.T) {
	p := newPipeline(t, 2*time.Second)
	p.set(func() {
		p.haState = "Рабочий"
		p.gatewayReply = map[string]any{"route": "local", "speech": "Выключил режим агента."}
	})
	spoken, finished := p.ask("выйди из режима агента", false, "user")
	if spoken != "Выключил режим агента." || !finished {
		t.Fatalf("spoken=%q finished=%t", spoken, finished)
	}
	if p.hermesCount() != 0 {
		t.Fatal("Hermes was called")
	}
	if p.announceCount() != 0 {
		t.Fatal("a local answer must not be announced later")
	}
}

// TestAckNeverEndsWithAQuestion mirrors test_ack_never_ends_with_a_question.
func TestAckNeverEndsWithAQuestion(t *testing.T) {
	p := newPipeline(t, 2*time.Second)
	p.set(func() {
		p.haState = "Рабочий"
		p.gatewayReply = map[string]any{"route": "bb", "ok": true, "threadId": "thr_example", "afterSeq": 1, "speech": "Передал в рабочий тред?"}
		p.results = []map[string]any{{"status": "done", "speech": "Готово."}}
	})
	spoken, _ := p.ask("проверь", false, "user")
	if !strings.HasSuffix(spoken, ".") || strings.Contains(spoken, "?") {
		t.Fatalf("ack = %q", spoken)
	}
}

// ----------------------------------------------------------- fail-closed ---

// TestGatewayErrorForWorkStaysLocal mirrors test_gateway_error_for_work_stays_local.
func TestGatewayErrorForWorkStaysLocal(t *testing.T) {
	p := newPipeline(t, 2*time.Second)
	p.set(func() {
		p.haState = "Рабочий"
		p.gatewayReply = map[string]any{"route": "bb", "ok": false, "speech": "Рабочий тред не настроен."}
	})
	spoken, _ := p.ask("проверь", false, "user")
	if spoken != "Рабочий тред не настроен." {
		t.Fatalf("spoken = %q", spoken)
	}
	if p.hermesCount() != 0 || p.announceCount() != 0 {
		t.Fatal("a gateway failure must neither reach Hermes nor announce")
	}
}

// TestUnknownRouteIsNotForwardedToHermes mirrors
// test_unknown_route_is_not_forwarded_to_hermes.
func TestUnknownRouteIsNotForwardedToHermes(t *testing.T) {
	p := newPipeline(t, 2*time.Second)
	p.set(func() {
		p.haState = "Рабочий"
		p.gatewayReply = map[string]any{"route": "something-else", "speech": "Странно."}
	})
	spoken, _ := p.ask("проверь", false, "user")
	if spoken != "Сервер агентов недоступен." {
		t.Fatalf("spoken = %q", spoken)
	}
	if p.hermesCount() != 0 {
		t.Fatal("an unknown route reached Hermes")
	}
}

// TestHADownFailsClosedForEveryProfile mirrors
// test_ha_down_fails_closed_for_every_profile.
func TestHADownFailsClosedForEveryProfile(t *testing.T) {
	p := newPipeline(t, 2*time.Second)
	p.set(func() { p.haDown = true })
	spoken, _ := p.ask("проверь", false, "user")
	if spoken != "Не могу подтвердить профиль." {
		t.Fatalf("spoken = %q", spoken)
	}
	p.mu.Lock()
	defer p.mu.Unlock()
	if len(p.utterances) != 0 || len(p.hermesCalls) != 0 {
		t.Fatalf("utterances=%#v hermes=%#v", p.utterances, p.hermesCalls)
	}
}

// TestUnknownProfileValueFailsClosed mirrors test_unknown_profile_value_fails_closed.
func TestUnknownProfileValueFailsClosed(t *testing.T) {
	p := newPipeline(t, 2*time.Second)
	p.set(func() { p.haState = "Непонятно" })
	spoken, _ := p.ask("проверь", false, "user")
	if spoken != "Не могу подтвердить профиль." {
		t.Fatalf("spoken = %q", spoken)
	}
	if p.hermesCount() != 0 {
		t.Fatal("an unknown profile reached Hermes")
	}
}

// TestFailClosedPhraseIsStreamedToo mirrors test_fail_closed_phrase_is_streamed_too.
func TestFailClosedPhraseIsStreamedToo(t *testing.T) {
	p := newPipeline(t, 2*time.Second)
	p.set(func() { p.haDown = true })
	spoken, finished := p.ask("проверь", true, "user")
	if spoken != "Не могу подтвердить профиль." || !finished {
		t.Fatalf("spoken=%q finished=%t", spoken, finished)
	}
	if p.hermesCount() != 0 {
		t.Fatal("a fail-closed answer reached Hermes")
	}
}

// TestGatewayDownInWorkProfileNeverUsesHermes mirrors
// test_gateway_down_in_work_profile_never_uses_hermes.
func TestGatewayDownInWorkProfileNeverUsesHermes(t *testing.T) {
	p := newPipeline(t, 2*time.Second)
	p.set(func() {
		p.haState = "Рабочий"
		p.gatewayDown = true
	})
	spoken, _ := p.ask("проверь", false, "user")
	if spoken != "Сервер агентов недоступен." {
		t.Fatalf("spoken = %q", spoken)
	}
	if p.hermesCount() != 0 {
		t.Fatal("a gateway outage reached Hermes in the work profile")
	}
}

// TestGatewayDownInPersonalProfileKeepsHermes mirrors
// test_gateway_down_in_personal_profile_keeps_hermes.
func TestGatewayDownInPersonalProfileKeepsHermes(t *testing.T) {
	p := newPipeline(t, 2*time.Second)
	p.set(func() {
		p.haState = "Личный"
		p.gatewayDown = true
	})
	spoken, _ := p.ask("расскажи новости", false, "user")
	if spoken != "Ответ Hermes." {
		t.Fatalf("spoken = %q", spoken)
	}
	if p.hermesCount() != 1 {
		t.Fatalf("hermes calls = %d", p.hermesCount())
	}
}

// TestRoutingDisabledKeepsPreviousBehaviour mirrors
// test_routing_disabled_keeps_previous_behaviour.
func TestRoutingDisabledKeepsPreviousBehaviour(t *testing.T) {
	p := newPipeline(t, 2*time.Second)
	plain := p.spawnProxy(false)

	spoken, _ := p.askAt(plain.URL, "расскажи новости", false, "user")
	if spoken != "Ответ Hermes." {
		t.Fatalf("spoken = %q", spoken)
	}
	p.mu.Lock()
	defer p.mu.Unlock()
	if len(p.haCalls) != 0 || len(p.utterances) != 0 || len(p.hermesCalls) != 1 {
		t.Fatalf("ha=%#v utterances=%#v hermes=%#v", p.haCalls, p.utterances, p.hermesCalls)
	}
}

// TestNonUserLastMessageIsNotRouted mirrors test_non_user_last_message_is_not_routed.
func TestNonUserLastMessageIsNotRouted(t *testing.T) {
	p := newPipeline(t, 2*time.Second)
	spoken, _ := p.ask("отмена", false, "tool")
	if spoken != "Ответ Hermes." {
		t.Fatalf("spoken = %q", spoken)
	}
	p.mu.Lock()
	defer p.mu.Unlock()
	if len(p.haCalls) != 0 || len(p.utterances) != 0 {
		t.Fatalf("ha=%#v utterances=%#v", p.haCalls, p.utterances)
	}
}

// TestCancelGuardRunsBeforeProfileReadAndRouting mirrors
// test_cancel_guard_runs_before_profile_read_and_routing.
func TestCancelGuardRunsBeforeProfileReadAndRouting(t *testing.T) {
	p := newPipeline(t, 2*time.Second)
	spoken, finished := p.ask("Отмена!", false, "user")
	if strings.TrimSpace(spoken) != "" || !finished {
		t.Fatalf("spoken=%q finished=%t", spoken, finished)
	}
	p.mu.Lock()
	defer p.mu.Unlock()
	if len(p.haCalls) != 0 || len(p.utterances) != 0 || len(p.hermesCalls) != 0 {
		t.Fatalf("ha=%#v utterances=%#v hermes=%#v", p.haCalls, p.utterances, p.hermesCalls)
	}
}

// TestFreshProfileIsCachedBetweenRequests mirrors
// test_fresh_profile_is_cached_between_requests.
func TestFreshProfileIsCachedBetweenRequests(t *testing.T) {
	p := newPipeline(t, 2*time.Second)
	p.ask("первый", false, "user")
	p.ask("второй", false, "user")
	if p.haCount() != 1 {
		t.Fatalf("HA calls = %d, want 1", p.haCount())
	}
	p.mu.Lock()
	defer p.mu.Unlock()
	if len(p.utterances) != 2 {
		t.Fatalf("utterances = %d, want 2", len(p.utterances))
	}
}

// TestStaleCacheIsNeverUsedAfterAFailedRead mirrors
// test_stale_cache_is_never_used_after_a_failed_read.
func TestStaleCacheIsNeverUsedAfterAFailedRead(t *testing.T) {
	p := newPipeline(t, 0)
	p.ask("первый", false, "user")
	p.set(func() { p.haDown = true })
	spoken, _ := p.ask("второй", false, "user")
	if spoken != "Не могу подтвердить профиль." {
		t.Fatalf("spoken = %q", spoken)
	}
	p.mu.Lock()
	defer p.mu.Unlock()
	if len(p.utterances) != 1 {
		t.Fatalf("utterances = %d, want 1", len(p.utterances))
	}
}

// TestResultLongPollWaitsForTheFinalAnswer mirrors
// test_result_long_poll_waits_for_the_final_answer.
func TestResultLongPollWaitsForTheFinalAnswer(t *testing.T) {
	p := newPipeline(t, 2*time.Second)
	p.set(func() {
		p.haState = "Рабочий"
		p.gatewayReply = map[string]any{"route": "bb", "ok": true, "threadId": "thr_example", "afterSeq": 4, "speech": "Передал в рабочий тред."}
		p.results = []map[string]any{
			{"status": "running"},
			{"status": "waiting_input", "speech": "Нужно уточнение."},
		}
	})
	p.ask("проверь", false, "user")
	p.waitUntil(func() bool { return p.announceCount() >= 1 }, time.Second, "announce was not sent")
	p.mu.Lock()
	defer p.mu.Unlock()
	if len(p.resultCalls) != 2 {
		t.Fatalf("result calls = %d, want 2", len(p.resultCalls))
	}
	if p.announceCalls[0].body["variables"].(map[string]any)["message"] != "Нужно уточнение." {
		t.Fatalf("announce = %#v", p.announceCalls[0])
	}
}

// TestResultTimeoutDoesNotSpeakAnything mirrors
// test_result_timeout_does_not_speak_anything.
func TestResultTimeoutDoesNotSpeakAnything(t *testing.T) {
	p := newPipeline(t, 2*time.Second)
	p.resultTimeout = 200 * time.Millisecond
	p.resultRetry = 50 * time.Millisecond
	p.buildRouter()
	p.proxyServer = p.spawnProxy(true)
	p.set(func() {
		p.haState = "Рабочий"
		p.gatewayReply = map[string]any{"route": "bb", "ok": true, "threadId": "thr_example", "afterSeq": 4, "speech": "Передал в рабочий тред."}
	})
	p.ask("проверь", false, "user")
	p.waitUntil(func() bool { return p.announcer.Running() == 0 }, 2*time.Second, "announcer did not finish")
	if p.announceCount() != 0 {
		t.Fatalf("announced on timeout")
	}
}

// TestRouteCounterRecordsOutcomesWithoutText mirrors
// test_route_counter_records_outcomes_without_text.
func TestRouteCounterRecordsOutcomesWithoutText(t *testing.T) {
	p := newPipeline(t, 2*time.Second)
	p.set(func() { p.gatewayReply = map[string]any{"route": "hermes"} })
	p.ask("первый", false, "user")
	if got := p.router.Count("hermes"); got != 1 {
		t.Fatalf("hermes counter = %d, want 1", got)
	}
}
