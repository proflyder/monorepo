// Package announce speaks a late answer through Home Assistant.
//
// The choice matters for the dialog:
//
//   - a statement uses the configured announcement script and leaves the
//     satellite alone;
//   - a question uses assist_satellite.start_conversation, which speaks the
//     text and then keeps listening for the answer;
//   - when start_conversation fails, the script is still tried, so the answer
//     is never lost silently.
package announce

import (
	"context"
	"net/http"
	"strings"
	"time"

	"ricardo.local/monorepo/packages/go/ha"
	"ricardo.local/monorepo/solutions/voice-proxy/internal/sanitize"
)

// Announcer speaks text through Home Assistant.
type Announcer struct {
	url       string
	client    *ha.Client
	script    string
	satellite string
	timeout   time.Duration
}

// Config configures an Announcer.
type Config struct {
	// URL is the Home Assistant base URL.
	URL string
	// Token is resolved on every request, so a rotated mounted secret needs no
	// restart.
	Token func() string
	// Script is HERMES_VOICE_ANNOUNCE_SCRIPT.
	Script string
	// Satellite is HERMES_VOICE_SATELLITE.
	Satellite string
	// Timeout is HERMES_HA_TIMEOUT.
	Timeout time.Duration
	// HTTP is the shared HTTP client; nil builds a plain one.
	HTTP *http.Client
}

// New builds an Announcer. The Home Assistant client is created once; only the
// token is re-read per request.
func New(cfg Config) *Announcer {
	timeout := cfg.Timeout
	if timeout <= 0 {
		timeout = 3 * time.Second
	}
	return &Announcer{
		url:       strings.TrimSuffix(strings.TrimSpace(cfg.URL), "/"),
		client:    ha.NewWithToken(cfg.URL, cfg.Token, cfg.HTTP),
		script:    cfg.Script,
		satellite: cfg.Satellite,
		timeout:   timeout,
	}
}

// Announce speaks text and reports (announced, listening). A question that was
// accepted by start_conversation keeps the microphone open (listening = true).
func (a *Announcer) Announce(ctx context.Context, text string) (bool, bool) {
	if text == "" {
		return false, false
	}
	if sanitize.EndsWithQuestion(text) && a.StartConversation(ctx, text) {
		return true, true
	}
	return a.ViaScript(ctx, text), false
}

// ViaScript runs the announcement script with {message, chime:false}.
func (a *Announcer) ViaScript(ctx context.Context, text string) bool {
	if a.url == "" || a.script == "" {
		return false
	}
	callCtx, cancel := context.WithTimeout(ctx, a.timeout)
	defer cancel()
	return a.client.CallScript(callCtx, a.script, map[string]any{
		"message": text,
		"chime":   false,
	}) == nil
}

// StartConversation speaks a question and leaves the satellite microphone open
// for the answer.
func (a *Announcer) StartConversation(ctx context.Context, text string) bool {
	if a.url == "" || a.satellite == "" {
		return false
	}
	callCtx, cancel := context.WithTimeout(ctx, a.timeout)
	defer cancel()
	return a.client.CallService(callCtx, "assist_satellite", "start_conversation", a.satellite,
		map[string]any{"start_message": text, "preannounce": false}) == nil
}
