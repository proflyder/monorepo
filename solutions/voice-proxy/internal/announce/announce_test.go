package announce_test

import (
	"context"
	"testing"
	"time"

	"github.com/proflyder/monorepo/solutions/voice-proxy/internal/announce"
	"github.com/proflyder/monorepo/solutions/voice-proxy/internal/testfake"
)

func newAnnouncer(t *testing.T, address string) *announce.Announcer {
	t.Helper()
	return announce.New(announce.Config{
		URL:       address,
		Token:     func() string { return "ha-token" },
		Script:    "script.schedule_announce",
		Satellite: "assist_satellite.station",
		Timeout:   time.Second,
	})
}

// TestLateStatementStillUsesTheAnnouncementScript: a statement runs the
// announcement script and leaves the satellite alone.
func TestLateStatementStillUsesTheAnnouncementScript(t *testing.T) {
	ha := testfake.NewHA("")
	server := testfake.Serve(t, ha)
	announced, listening := newAnnouncer(t, server.URL).Announce(context.Background(), "Готово.")

	if !announced || listening {
		t.Fatalf("announced=%t listening=%t", announced, listening)
	}
	calls := ha.ServiceCalls()
	if len(calls) != 1 {
		t.Fatalf("calls = %#v", calls)
	}
	if calls[0].Path != "/api/services/script/turn_on" {
		t.Errorf("path = %q", calls[0].Path)
	}
	if calls[0].Body["entity_id"] != "script.schedule_announce" {
		t.Errorf("entity_id = %v", calls[0].Body["entity_id"])
	}
	variables, _ := calls[0].Body["variables"].(map[string]any)
	if variables["message"] != "Готово." || variables["chime"] != false {
		t.Errorf("variables = %#v", variables)
	}
}

// TestLateQuestionStartsConversationAndKeepsMicrophoneOpen: a question speaks
// through start_conversation and keeps listening.
func TestLateQuestionStartsConversationAndKeepsMicrophoneOpen(t *testing.T) {
	ha := testfake.NewHA("")
	server := testfake.Serve(t, ha)
	announced, listening := newAnnouncer(t, server.URL).Announce(context.Background(), "Готово. Подтверждаете?")

	if !announced || !listening {
		t.Fatalf("announced=%t listening=%t", announced, listening)
	}
	calls := ha.ServiceCalls()
	if len(calls) != 1 {
		t.Fatalf("calls = %#v", calls)
	}
	if calls[0].Path != "/api/services/assist_satellite/start_conversation" {
		t.Errorf("path = %q", calls[0].Path)
	}
	want := map[string]any{
		"entity_id":     "assist_satellite.station",
		"start_message": "Готово. Подтверждаете?",
		"preannounce":   false,
	}
	for key, value := range want {
		if calls[0].Body[key] != value {
			t.Errorf("%s = %#v, want %#v", key, calls[0].Body[key], value)
		}
	}
}

// TestFailedStartConversationFallsBackToScriptAnnounce: a rejected
// start_conversation must not lose the answer.
func TestFailedStartConversationFallsBackToScriptAnnounce(t *testing.T) {
	ha := testfake.NewHA("")
	ha.SetFailStartConversation(true)
	server := testfake.Serve(t, ha)
	announced, listening := newAnnouncer(t, server.URL).Announce(context.Background(), "Подтверждаете?")

	if !announced || listening {
		t.Fatalf("announced=%t listening=%t", announced, listening)
	}
	calls := ha.ServiceCalls()
	if len(calls) != 2 {
		t.Fatalf("calls = %#v", calls)
	}
	if calls[0].Path != "/api/services/assist_satellite/start_conversation" {
		t.Errorf("first path = %q", calls[0].Path)
	}
	if calls[1].Path != "/api/services/script/turn_on" {
		t.Errorf("second path = %q", calls[1].Path)
	}
	variables, _ := calls[1].Body["variables"].(map[string]any)
	if variables["message"] != "Подтверждаете?" {
		t.Errorf("fallback message = %#v", variables)
	}
}

// TestEmptyTextAnnouncesNothing pins announce("").
func TestEmptyTextAnnouncesNothing(t *testing.T) {
	ha := testfake.NewHA("")
	server := testfake.Serve(t, ha)
	announced, listening := newAnnouncer(t, server.URL).Announce(context.Background(), "")
	if announced || listening {
		t.Fatalf("announced=%t listening=%t", announced, listening)
	}
	if calls := ha.Calls(); len(calls) != 0 {
		t.Fatalf("calls = %#v", calls)
	}
}

// TestFullwidthQuestionAlsoUsesStartConversation pins the "？" marker.
func TestFullwidthQuestionAlsoUsesStartConversation(t *testing.T) {
	ha := testfake.NewHA("")
	server := testfake.Serve(t, ha)
	if announced, listening := newAnnouncer(t, server.URL).Announce(context.Background(), "Готово？"); !announced || !listening {
		t.Fatalf("announced=%t listening=%t", announced, listening)
	}
}
