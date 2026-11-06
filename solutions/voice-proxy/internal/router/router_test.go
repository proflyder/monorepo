package router_test

import (
	"context"
	"encoding/json"
	"net/http"
	"net/http/httptest"
	"strings"
	"testing"
	"time"

	"github.com/proflyder/monorepo/solutions/voice-proxy/internal/announce"
	"github.com/proflyder/monorepo/solutions/voice-proxy/internal/gateway"
	"github.com/proflyder/monorepo/solutions/voice-proxy/internal/profile"
	"github.com/proflyder/monorepo/solutions/voice-proxy/internal/proxy"
	"github.com/proflyder/monorepo/solutions/voice-proxy/internal/result"
	"github.com/proflyder/monorepo/solutions/voice-proxy/internal/router"
	"github.com/proflyder/monorepo/solutions/voice-proxy/internal/sse"
	"github.com/proflyder/monorepo/solutions/voice-proxy/internal/testfake"
)

type pipeline struct {
	t *testing.T

	ha      *testfake.HA
	gateway *testfake.Gateway
	hermes  *testfake.Hermes

	profileCache  time.Duration
	resultTimeout time.Duration
	resultRetry   time.Duration

	haServer       *httptest.Server
	gatewayServer  *httptest.Server
	upstreamServer *httptest.Server
	proxyServer    *httptest.Server

	announcer *result.Announcer
	router    *router.Router
}

func newPipeline(t *testing.T, profileCache time.Duration) *pipeline {
	t.Helper()
	p := &pipeline{
		t:             t,
		ha:            testfake.NewHA("Личный"),
		gateway:       testfake.NewGateway(),
		hermes:        testfake.NewHermes(),
		profileCache:  profileCache,
		resultTimeout: 5 * time.Second,
		resultRetry:   50 * time.Millisecond,
	}
	p.hermes.SetResponse(http.StatusOK, `{"choices":[{"message":{"role":"assistant","content":"Ответ Hermes."},"finish_reason":"stop"}]}`)
	p.haServer = testfake.Serve(t, p.ha)
	p.gatewayServer = testfake.Serve(t, p.gateway)
	p.upstreamServer = testfake.Serve(t, p.hermes)
	p.buildRouter()
	p.proxyServer = p.spawnProxy(true)
	t.Cleanup(func() { p.announcer.Wait(2 * time.Second) })
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
	return testfake.Serve(p.t, proxy.New(proxy.Options{
		UpstreamURL:     p.upstreamServer.URL,
		ReasoningEffort: "low",
		Logger:          proxy.NopLogger{},
		Router:          voiceRouter,
	}))
}

// spawnProxy starts a proxy server; its cleanup is registered by testfake.
func (p *pipeline) spawnProxy(routing bool) *httptest.Server {
	return p.newProxyServer(routing)
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

func (p *pipeline) announceCount() int { return len(p.ha.ServiceCalls()) }

func (p *pipeline) announceAt(index int) testfake.Call { return p.ha.ServiceCalls()[index] }

func (p *pipeline) hermesCount() int { return p.hermes.Len() }

func (p *pipeline) haCount() int { return len(p.ha.StateCalls()) }

// --------------------------------------------------------------- routing ---

// TestPersonalProfileHermesRouteForwardsUnchanged: a personal utterance is
// routed to Hermes untouched.
func TestPersonalProfileHermesRouteForwardsUnchanged(t *testing.T) {
	p := newPipeline(t, 2*time.Second)
	spoken, finished := p.ask("что такое фотосинтез", false, "user")
	if spoken != "Ответ Hermes." || !finished {
		t.Fatalf("spoken=%q finished=%t", spoken, finished)
	}
	utterances := p.gateway.CallsTo("/v1/voice/utterance")
	if len(utterances) != 1 || utterances[0].Body["profile"] != "personal" {
		t.Fatalf("utterances = %#v", utterances)
	}
	if p.hermesCount() != 1 {
		t.Fatalf("hermes calls = %d", p.hermesCount())
	}
}

// TestWorkProfileBBRouteAnswersWithoutHermesAndAnnouncesLater: a routed bb turn
// speaks the ack locally and announces the final answer later.
func TestWorkProfileBBRouteAnswersWithoutHermesAndAnnouncesLater(t *testing.T) {
	p := newPipeline(t, 2*time.Second)
	p.ha.SetState("Рабочий")
	p.gateway.SetUtterance(map[string]any{"route": "bb", "ok": true, "threadId": "thr_example", "afterSeq": 7, "speech": "Передал в рабочий тред."})
	p.gateway.SetResults(map[string]any{"status": "done", "speech": "**Готово**: отчёт [тут](https://example.com/x)."})
	spoken, finished := p.ask("сделай отчёт", false, "user")
	if spoken != "Передал в рабочий тред." || !finished {
		t.Fatalf("spoken=%q finished=%t", spoken, finished)
	}
	if p.hermesCount() != 0 {
		t.Fatal("Hermes was called")
	}
	p.waitUntil(func() bool { return p.announceCount() >= 1 }, time.Second, "announce was not sent")
	if p.gateway.CallsTo("/v1/voice/utterance")[0].Body["profile"] != "work" {
		t.Errorf("utterances = %#v", p.gateway.CallsTo("/v1/voice/utterance"))
	}
	results := p.gateway.CallsTo("/v1/voice/result")
	if len(results) != 1 || results[0].Body["threadId"] != "thr_example" || results[0].Body["afterSeq"] != float64(7) {
		t.Fatalf("result calls = %#v", results)
	}
	call := p.announceAt(0)
	if call.Path != "/api/services/script/turn_on" {
		t.Errorf("path = %q", call.Path)
	}
	if call.Body["entity_id"] != "script.schedule_announce" {
		t.Errorf("entity_id = %v", call.Body["entity_id"])
	}
	variables := call.Body["variables"].(map[string]any)
	if variables["message"] != "Готово: отчёт тут." || variables["chime"] != false {
		t.Errorf("variables = %#v", variables)
	}
}

// TestLateQuestionStartsConversationAndKeepsMicrophoneOpen: a late question
// keeps the dialogue open.
func TestLateQuestionStartsConversationAndKeepsMicrophoneOpen(t *testing.T) {
	p := newPipeline(t, 2*time.Second)
	p.ha.SetState("Рабочий")
	p.gateway.SetUtterance(map[string]any{"route": "bb", "ok": true, "threadId": "thr_example", "afterSeq": 2, "speech": "Передал в рабочий тред."})
	p.gateway.SetResults(map[string]any{"status": "done", "speech": "Готово. Подтверждаете?"})
	p.ask("проверь", false, "user")
	p.waitUntil(func() bool { return p.announceCount() >= 1 }, time.Second, "announce was not sent")
	call := p.announceAt(0)
	if call.Path != "/api/services/assist_satellite/start_conversation" {
		t.Fatalf("path = %q", call.Path)
	}
	want := map[string]any{
		"entity_id":     "assist_satellite.iandeks_stantsiia_midi_assist_satellite",
		"start_message": "Готово. Подтверждаете?",
		"preannounce":   false,
	}
	for key, value := range want {
		if call.Body[key] != value {
			t.Errorf("%s = %#v, want %#v", key, call.Body[key], value)
		}
	}
}

// TestLateStatementStillUsesTheAnnouncementScript: a late statement uses the
// script.
func TestLateStatementStillUsesTheAnnouncementScript(t *testing.T) {
	p := newPipeline(t, 2*time.Second)
	p.ha.SetState("Рабочий")
	p.gateway.SetUtterance(map[string]any{"route": "bb", "ok": true, "threadId": "thr_example", "afterSeq": 2, "speech": "Передал в рабочий тред."})
	p.gateway.SetResults(map[string]any{"status": "done", "speech": "Готово."})
	p.ask("проверь", false, "user")
	p.waitUntil(func() bool { return p.announceCount() >= 1 }, time.Second, "announce was not sent")
	call := p.announceAt(0)
	if call.Path != "/api/services/script/turn_on" {
		t.Fatalf("path = %q", call.Path)
	}
	variables := call.Body["variables"].(map[string]any)
	if variables["message"] != "Готово." {
		t.Fatalf("variables = %#v", variables)
	}
}

// TestFailedStartConversationFallsBackToScriptAnnounce: a rejected
// start_conversation falls back to the script.
func TestFailedStartConversationFallsBackToScriptAnnounce(t *testing.T) {
	p := newPipeline(t, 2*time.Second)
	p.ha.SetFailStartConversation(true)
	p.ha.SetState("Рабочий")
	p.gateway.SetUtterance(map[string]any{"route": "bb", "ok": true, "threadId": "thr_example", "afterSeq": 2, "speech": "Передал в рабочий тред."})
	p.gateway.SetResults(map[string]any{"status": "done", "speech": "Подтверждаете?"})
	p.ask("проверь", false, "user")
	p.waitUntil(func() bool { return p.announceCount() >= 2 }, time.Second, "fallback announce was not sent")
	first := p.announceAt(0)
	second := p.announceAt(1)
	if first.Path != "/api/services/assist_satellite/start_conversation" {
		t.Errorf("first path = %q", first.Path)
	}
	if second.Path != "/api/services/script/turn_on" {
		t.Errorf("second path = %q", second.Path)
	}
	if second.Body["variables"].(map[string]any)["message"] != "Подтверждаете?" {
		t.Errorf("second body = %#v", second.Body)
	}
}

// TestBBRouteStreamsTheAckAndNeverCallsHermes: a streaming ack has the same
// content and never reaches Hermes.
func TestBBRouteStreamsTheAckAndNeverCallsHermes(t *testing.T) {
	p := newPipeline(t, 2*time.Second)
	p.ha.SetState("Рабочий")
	p.gateway.SetUtterance(map[string]any{"route": "bb", "ok": true, "threadId": "thr_example", "afterSeq": 3, "speech": "Передал в рабочий тред."})
	p.gateway.SetResults(map[string]any{"status": "done", "speech": "Готово."})
	spoken, finished := p.ask("проверь сборку", true, "user")
	if spoken != "Передал в рабочий тред." || !finished {
		t.Fatalf("spoken=%q finished=%t", spoken, finished)
	}
	if p.hermesCount() != 0 {
		t.Fatal("Hermes was called")
	}
	p.waitUntil(func() bool { return p.announceCount() >= 1 }, time.Second, "announce was not sent")
	if p.announceAt(0).Body["variables"].(map[string]any)["message"] != "Готово." {
		t.Fatalf("announce = %#v", p.announceAt(0))
	}
}

// TestLocalRouteAnswersWithItsOwnPhrase: a local route speaks and stops.
func TestLocalRouteAnswersWithItsOwnPhrase(t *testing.T) {
	p := newPipeline(t, 2*time.Second)
	p.ha.SetState("Рабочий")
	p.gateway.SetUtterance(map[string]any{"route": "local", "speech": "Выключил режим агента."})
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

// TestAckNeverEndsWithAQuestion: HA would continue the dialog on a question.
func TestAckNeverEndsWithAQuestion(t *testing.T) {
	p := newPipeline(t, 2*time.Second)
	p.ha.SetState("Рабочий")
	p.gateway.SetUtterance(map[string]any{"route": "bb", "ok": true, "threadId": "thr_example", "afterSeq": 1, "speech": "Передал в рабочий тред?"})
	p.gateway.SetResults(map[string]any{"status": "done", "speech": "Готово."})
	spoken, _ := p.ask("проверь", false, "user")
	if !strings.HasSuffix(spoken, ".") || strings.Contains(spoken, "?") {
		t.Fatalf("ack = %q", spoken)
	}
}

// ----------------------------------------------------------- fail-closed ---

// TestGatewayErrorForWorkStaysLocal: a gateway failure in the work profile is
// spoken locally and never forwarded.
func TestGatewayErrorForWorkStaysLocal(t *testing.T) {
	p := newPipeline(t, 2*time.Second)
	p.ha.SetState("Рабочий")
	p.gateway.SetUtterance(map[string]any{"route": "bb", "ok": false, "speech": "Рабочий тред не настроен."})
	spoken, _ := p.ask("проверь", false, "user")
	if spoken != "Рабочий тред не настроен." {
		t.Fatalf("spoken = %q", spoken)
	}
	if p.hermesCount() != 0 || p.announceCount() != 0 {
		t.Fatal("a gateway failure must neither reach Hermes nor announce")
	}
}

// TestUnknownRouteIsNotForwardedToHermes: an unknown route is a gateway failure.
func TestUnknownRouteIsNotForwardedToHermes(t *testing.T) {
	p := newPipeline(t, 2*time.Second)
	p.ha.SetState("Рабочий")
	p.gateway.SetUtterance(map[string]any{"route": "something-else", "speech": "Странно."})
	spoken, _ := p.ask("проверь", false, "user")
	if spoken != "Сервер агентов недоступен." {
		t.Fatalf("spoken = %q", spoken)
	}
	if p.hermesCount() != 0 {
		t.Fatal("an unknown route reached Hermes")
	}
}

// TestHADownFailsClosedForEveryProfile: an unreadable profile never guesses.
func TestHADownFailsClosedForEveryProfile(t *testing.T) {
	p := newPipeline(t, 2*time.Second)
	p.ha.SetDown(true)
	spoken, _ := p.ask("проверь", false, "user")
	if spoken != "Не могу подтвердить профиль." {
		t.Fatalf("spoken = %q", spoken)
	}
	if len(p.gateway.CallsTo("/v1/voice/utterance")) != 0 || p.hermesCount() != 0 {
		t.Fatalf("utterances=%#v hermes=%d", p.gateway.CallsTo("/v1/voice/utterance"), p.hermesCount())
	}
}

// TestUnknownProfileValueFailsClosed: an unmapped state is unknown, not a guess.
func TestUnknownProfileValueFailsClosed(t *testing.T) {
	p := newPipeline(t, 2*time.Second)
	p.ha.SetState("Непонятно")
	spoken, _ := p.ask("проверь", false, "user")
	if spoken != "Не могу подтвердить профиль." {
		t.Fatalf("spoken = %q", spoken)
	}
	if p.hermesCount() != 0 {
		t.Fatal("an unknown profile reached Hermes")
	}
}

// TestFailClosedPhraseIsStreamedToo: the fail-closed phrase works in streaming
// mode.
func TestFailClosedPhraseIsStreamedToo(t *testing.T) {
	p := newPipeline(t, 2*time.Second)
	p.ha.SetDown(true)
	spoken, finished := p.ask("проверь", true, "user")
	if spoken != "Не могу подтвердить профиль." || !finished {
		t.Fatalf("spoken=%q finished=%t", spoken, finished)
	}
	if p.hermesCount() != 0 {
		t.Fatal("a fail-closed answer reached Hermes")
	}
}

// TestGatewayDownInWorkProfileNeverUsesHermes: work is fail-closed.
func TestGatewayDownInWorkProfileNeverUsesHermes(t *testing.T) {
	p := newPipeline(t, 2*time.Second)
	p.ha.SetState("Рабочий")
	p.gateway.SetDown(true)
	spoken, _ := p.ask("проверь", false, "user")
	if spoken != "Сервер агентов недоступен." {
		t.Fatalf("spoken = %q", spoken)
	}
	if p.hermesCount() != 0 {
		t.Fatal("a gateway outage reached Hermes in the work profile")
	}
}

// TestGatewayDownInPersonalProfileKeepsHermes: personal keeps the previous
// behaviour.
func TestGatewayDownInPersonalProfileKeepsHermes(t *testing.T) {
	p := newPipeline(t, 2*time.Second)
	p.ha.SetState("Личный")
	p.gateway.SetDown(true)
	spoken, _ := p.ask("расскажи новости", false, "user")
	if spoken != "Ответ Hermes." {
		t.Fatalf("spoken = %q", spoken)
	}
	if p.hermesCount() != 1 {
		t.Fatalf("hermes calls = %d", p.hermesCount())
	}
}

// TestRoutingDisabledKeepsPreviousBehaviour: without a router the proxy only
// sanitises.
func TestRoutingDisabledKeepsPreviousBehaviour(t *testing.T) {
	p := newPipeline(t, 2*time.Second)
	plain := p.spawnProxy(false)

	spoken, _ := p.askAt(plain.URL, "расскажи новости", false, "user")
	if spoken != "Ответ Hermes." {
		t.Fatalf("spoken = %q", spoken)
	}
	if p.haCount() != 0 || len(p.gateway.CallsTo("/v1/voice/utterance")) != 0 || p.hermesCount() != 1 {
		t.Fatalf("ha=%d utterances=%#v hermes=%d", p.haCount(), p.gateway.CallsTo("/v1/voice/utterance"), p.hermesCount())
	}
}

// TestNonUserLastMessageIsNotRouted: a tool result is forwarded unchanged.
func TestNonUserLastMessageIsNotRouted(t *testing.T) {
	p := newPipeline(t, 2*time.Second)
	spoken, _ := p.ask("отмена", false, "tool")
	if spoken != "Ответ Hermes." {
		t.Fatalf("spoken = %q", spoken)
	}
	if p.haCount() != 0 || len(p.gateway.CallsTo("/v1/voice/utterance")) != 0 {
		t.Fatalf("ha=%d utterances=%#v", p.haCount(), p.gateway.CallsTo("/v1/voice/utterance"))
	}
}

// TestCancelGuardRunsBeforeProfileReadAndRouting: the local reply opens no
// upstream turn.
func TestCancelGuardRunsBeforeProfileReadAndRouting(t *testing.T) {
	p := newPipeline(t, 2*time.Second)
	spoken, finished := p.ask("Отмена!", false, "user")
	if strings.TrimSpace(spoken) != "" || !finished {
		t.Fatalf("spoken=%q finished=%t", spoken, finished)
	}
	if p.haCount() != 0 || len(p.gateway.CallsTo("/v1/voice/utterance")) != 0 || p.hermesCount() != 0 {
		t.Fatalf("ha=%d utterances=%#v hermes=%d", p.haCount(), p.gateway.CallsTo("/v1/voice/utterance"), p.hermesCount())
	}
}

// TestFreshProfileIsCachedBetweenRequests: the second turn reuses the cache.
func TestFreshProfileIsCachedBetweenRequests(t *testing.T) {
	p := newPipeline(t, 2*time.Second)
	p.ask("первый", false, "user")
	p.ask("второй", false, "user")
	if p.haCount() != 1 {
		t.Fatalf("HA calls = %d, want 1", p.haCount())
	}
	if utterances := p.gateway.CallsTo("/v1/voice/utterance"); len(utterances) != 2 {
		t.Fatalf("utterances = %d, want 2", len(utterances))
	}
}

// TestStaleCacheIsNeverUsedAfterAFailedRead: a failed read makes the profile
// unknown.
func TestStaleCacheIsNeverUsedAfterAFailedRead(t *testing.T) {
	p := newPipeline(t, 0)
	p.ask("первый", false, "user")
	p.ha.SetDown(true)
	spoken, _ := p.ask("второй", false, "user")
	if spoken != "Не могу подтвердить профиль." {
		t.Fatalf("spoken = %q", spoken)
	}
	if utterances := p.gateway.CallsTo("/v1/voice/utterance"); len(utterances) != 1 {
		t.Fatalf("utterances = %d, want 1", len(utterances))
	}
}

// TestResultLongPollWaitsForTheFinalAnswer: a running answer is polled until it
// finishes.
func TestResultLongPollWaitsForTheFinalAnswer(t *testing.T) {
	p := newPipeline(t, 2*time.Second)
	p.ha.SetState("Рабочий")
	p.gateway.SetUtterance(map[string]any{"route": "bb", "ok": true, "threadId": "thr_example", "afterSeq": 4, "speech": "Передал в рабочий тред."})
	p.gateway.SetResults(
		map[string]any{"status": "running"},
		map[string]any{"status": "waiting_input", "speech": "Нужно уточнение."},
	)
	p.ask("проверь", false, "user")
	p.waitUntil(func() bool { return p.announceCount() >= 1 }, time.Second, "announce was not sent")
	if results := p.gateway.CallsTo("/v1/voice/result"); len(results) != 2 {
		t.Fatalf("result calls = %d, want 2", len(results))
	}
	if p.announceAt(0).Body["variables"].(map[string]any)["message"] != "Нужно уточнение." {
		t.Fatalf("announce = %#v", p.announceAt(0))
	}
}

// TestResultTimeoutDoesNotSpeakAnything: a timed-out turn speaks nothing.
func TestResultTimeoutDoesNotSpeakAnything(t *testing.T) {
	p := newPipeline(t, 2*time.Second)
	p.resultTimeout = 200 * time.Millisecond
	p.resultRetry = 50 * time.Millisecond
	p.buildRouter()
	p.proxyServer = p.spawnProxy(true)
	p.ha.SetState("Рабочий")
	p.gateway.SetUtterance(map[string]any{"route": "bb", "ok": true, "threadId": "thr_example", "afterSeq": 4, "speech": "Передал в рабочий тред."})
	p.ask("проверь", false, "user")
	p.waitUntil(func() bool { return p.announcer.Running() == 0 }, 2*time.Second, "announcer did not finish")
	if p.announceCount() != 0 {
		t.Fatalf("announced on timeout")
	}
}

// TestRouteCounterRecordsOutcomesWithoutText: the counter carries only the
// structural outcome.
func TestRouteCounterRecordsOutcomesWithoutText(t *testing.T) {
	p := newPipeline(t, 2*time.Second)
	p.gateway.SetUtterance(map[string]any{"route": "hermes"})
	p.ask("первый", false, "user")
	if got := p.router.Count("hermes"); got != 1 {
		t.Fatalf("hermes counter = %d, want 1", got)
	}
}
