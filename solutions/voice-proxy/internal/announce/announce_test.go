package announce_test

import (
	"context"
	"encoding/json"
	"net/http"
	"net/http/httptest"
	"sync"
	"testing"
	"time"

	"ricardo.local/monorepo/solutions/voice-proxy/internal/announce"
)

type call struct {
	path string
	body map[string]any
}

type fakeHA struct {
	mu                    sync.Mutex
	calls                 []call
	startConversationFail bool
}

func (h *fakeHA) ServeHTTP(w http.ResponseWriter, r *http.Request) {
	var body map[string]any
	_ = json.NewDecoder(r.Body).Decode(&body)
	h.mu.Lock()
	h.calls = append(h.calls, call{path: r.URL.Path, body: body})
	fail := h.startConversationFail && r.URL.Path == "/api/services/assist_satellite/start_conversation"
	h.mu.Unlock()
	if fail {
		w.WriteHeader(http.StatusServiceUnavailable)
		return
	}
	w.Header().Set("Content-Type", "application/json")
	_, _ = w.Write([]byte(`{"changed_states":[]}`))
}

func (h *fakeHA) recorded() []call {
	h.mu.Lock()
	defer h.mu.Unlock()
	return append([]call(nil), h.calls...)
}

func newAnnouncer(t *testing.T, server *httptest.Server) *announce.Announcer {
	t.Helper()
	return announce.New(announce.Config{
		URL:       server.URL,
		Token:     func() string { return "ha-token" },
		Script:    "script.schedule_announce",
		Satellite: "assist_satellite.station",
		Timeout:   time.Second,
	})
}

// TestLateStatementStillUsesTheAnnouncementScript mirrors
// test_late_statement_still_uses_the_announcement_script.
func TestLateStatementStillUsesTheAnnouncementScript(t *testing.T) {
	ha := &fakeHA{}
	server := httptest.NewServer(ha)
	defer server.Close()
	announced, listening := newAnnouncer(t, server).Announce(context.Background(), "Готово.")

	if !announced || listening {
		t.Fatalf("announced=%t listening=%t", announced, listening)
	}
	calls := ha.recorded()
	if len(calls) != 1 {
		t.Fatalf("calls = %#v", calls)
	}
	if calls[0].path != "/api/services/script/turn_on" {
		t.Errorf("path = %q", calls[0].path)
	}
	if calls[0].body["entity_id"] != "script.schedule_announce" {
		t.Errorf("entity_id = %v", calls[0].body["entity_id"])
	}
	variables, _ := calls[0].body["variables"].(map[string]any)
	if variables["message"] != "Готово." || variables["chime"] != false {
		t.Errorf("variables = %#v", variables)
	}
}

// TestLateQuestionStartsConversationAndKeepsMicrophoneOpen mirrors
// test_late_question_starts_conversation_and_keeps_microphone_open.
func TestLateQuestionStartsConversationAndKeepsMicrophoneOpen(t *testing.T) {
	ha := &fakeHA{}
	server := httptest.NewServer(ha)
	defer server.Close()
	announced, listening := newAnnouncer(t, server).Announce(context.Background(), "Готово. Подтверждаете?")

	if !announced || !listening {
		t.Fatalf("announced=%t listening=%t", announced, listening)
	}
	calls := ha.recorded()
	if len(calls) != 1 {
		t.Fatalf("calls = %#v", calls)
	}
	if calls[0].path != "/api/services/assist_satellite/start_conversation" {
		t.Errorf("path = %q", calls[0].path)
	}
	want := map[string]any{
		"entity_id":     "assist_satellite.station",
		"start_message": "Готово. Подтверждаете?",
		"preannounce":   false,
	}
	for key, value := range want {
		if calls[0].body[key] != value {
			t.Errorf("%s = %#v, want %#v", key, calls[0].body[key], value)
		}
	}
}

// TestFailedStartConversationFallsBackToScriptAnnounce mirrors
// test_failed_start_conversation_falls_back_to_script_announce.
func TestFailedStartConversationFallsBackToScriptAnnounce(t *testing.T) {
	ha := &fakeHA{startConversationFail: true}
	server := httptest.NewServer(ha)
	defer server.Close()
	announced, listening := newAnnouncer(t, server).Announce(context.Background(), "Подтверждаете?")

	if !announced || listening {
		t.Fatalf("announced=%t listening=%t", announced, listening)
	}
	calls := ha.recorded()
	if len(calls) != 2 {
		t.Fatalf("calls = %#v", calls)
	}
	if calls[0].path != "/api/services/assist_satellite/start_conversation" {
		t.Errorf("first path = %q", calls[0].path)
	}
	if calls[1].path != "/api/services/script/turn_on" {
		t.Errorf("second path = %q", calls[1].path)
	}
	variables, _ := calls[1].body["variables"].(map[string]any)
	if variables["message"] != "Подтверждаете?" {
		t.Errorf("fallback message = %#v", variables)
	}
}

// TestEmptyTextAnnouncesNothing pins announce("").
func TestEmptyTextAnnouncesNothing(t *testing.T) {
	ha := &fakeHA{}
	server := httptest.NewServer(ha)
	defer server.Close()
	announced, listening := newAnnouncer(t, server).Announce(context.Background(), "")
	if announced || listening {
		t.Fatalf("announced=%t listening=%t", announced, listening)
	}
	if calls := ha.recorded(); len(calls) != 0 {
		t.Fatalf("calls = %#v", calls)
	}
}

// TestFullwidthQuestionAlsoUsesStartConversation pins the "？" marker.
func TestFullwidthQuestionAlsoUsesStartConversation(t *testing.T) {
	ha := &fakeHA{}
	server := httptest.NewServer(ha)
	defer server.Close()
	if announced, listening := newAnnouncer(t, server).Announce(context.Background(), "Готово？"); !announced || !listening {
		t.Fatalf("announced=%t listening=%t", announced, listening)
	}
}
