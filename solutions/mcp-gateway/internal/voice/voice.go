// Package voice exposes the HTTP entry point of the voice proxy:
//
//	POST /v1/voice/utterance  route one spoken phrase
//	POST /v1/voice/result     long-poll the answer of a bb turn
//	POST /v1/voice/updates    poll dispatcher turns the owner did not start
//	GET  /v1/voice/state      debug view of the agent-conversation switch
//
// It is authenticated by a dedicated voice token, never by the MCP token, and
// is intentionally separate from /mcp: the voice proxy cannot call MCP tools
// and Hermes cannot use this route.
package voice

import (
	"context"
	"encoding/json"
	"fmt"
	"io"
	"log/slog"
	"net/http"
	"strings"
	"time"
	"unicode"

	"golang.org/x/time/rate"

	"github.com/proflyder/monorepo/solutions/mcp-gateway/internal/agentmode"
	"github.com/proflyder/monorepo/solutions/mcp-gateway/internal/bb"
	"github.com/proflyder/monorepo/solutions/mcp-gateway/internal/metrics"
)

// Profiles accepted from the proxy.
const (
	ProfilePersonal = "personal"
	ProfileWork     = "work"
)

// MaxTextRunes is the speech budget accepted from the proxy.
const MaxTextRunes = 2000

// DefaultMaxWait is the long-poll ceiling; the proxy must stay below the
// satellite's responding watchdog.
const DefaultMaxWait = 25 * time.Second

// DefaultUpdatesWait keeps one bridge call below the default 15 s SSH timeout
// (MCP_BB_TIMEOUT). The proxy simply calls again with the returned cursor, so
// a shorter plugin wait does not shorten the overall long-poll.
const DefaultUpdatesWait = 10 * time.Second

// UpdatesWaitFor derives the plugin wait from the SSH bridge timeout. One
// `updates` call must finish before the bridge times out, otherwise every
// long-poll returns status "failed" whenever MCP_BB_TIMEOUT is short. The
// returned value leaves one second of headroom and is capped at
// DefaultUpdatesWait.
func UpdatesWaitFor(bridgeTimeout time.Duration) time.Duration {
	wait := bridgeTimeout - time.Second
	if wait < time.Second {
		wait = time.Second
	}
	if wait > DefaultUpdatesWait {
		wait = DefaultUpdatesWait
	}
	return wait
}

// Short phrases spoken by the proxy path.
const (
	SpeechAgentSent    = "Передал агенту."
	SpeechWorkSent     = "Передал в рабочий тред."
	SpeechModeOff      = "Выключил режим агента."
	SpeechWorkMissing  = "Рабочий тред не настроен."
	SpeechRateLimited  = "Слишком часто, подожди немного."
	SpeechBadRequest   = "Не понял запрос."
	SpeechTurnFailed   = "Агент завершил работу с ошибкой."
	SpeechWaitingInput = "Агент ждёт ответа."
	SpeechTurnRunning  = "Агент ещё работает."
	SpeechDone         = "Готово."
)

// exitPhrases end agent conversation mode. They deliberately avoid the HA
// local intent vocabulary ("рабочий режим", "режим работы"), which belongs to
// the bedroom light and the Spotify/MA switches.
var exitPhrases = []string{
	"выйди из режима агента",
	"выключи режим агента",
	"закончи разговор с агентом",
}

// Dispatcher is the bb subset the voice route needs.
type Dispatcher interface {
	Send(ctx context.Context, req bb.SendRequest) (bb.SendResult, error)
	TurnResult(ctx context.Context, req bb.TurnResultRequest) (bb.TurnResultResult, error)
	Updates(ctx context.Context, req bb.UpdatesRequest) (bb.UpdatesResult, error)
}

// Options wires the voice handler.
type Options struct {
	Dispatcher Dispatcher
	// WorkThreadID is the permanent work dispatcher thread. When empty, the
	// work profile fails closed instead of falling back to Hermes.
	WorkThreadID string
	Mode         *agentmode.State
	Metrics      *metrics.Metrics
	Logger       *slog.Logger
	Now          func() time.Time
	Limiter      *rate.Limiter
	PollInterval time.Duration
	MaxWait      time.Duration
	// UpdatesWait caps one plugin `updates` call below the SSH bridge timeout.
	UpdatesWait time.Duration
}

// Handler serves the voice routes.
type Handler struct {
	opts Options
}

// New builds the handler with safe defaults.
func New(opts Options) *Handler {
	if opts.Logger == nil {
		opts.Logger = slog.New(slog.NewTextHandler(io.Discard, nil))
	}
	if opts.Now == nil {
		opts.Now = time.Now
	}
	if opts.Mode == nil {
		opts.Mode = agentmode.New()
	}
	if opts.Limiter == nil {
		// 30 utterances per minute, burst 10: enough for a conversation and
		// far below anything that could spawn or spam threads.
		opts.Limiter = rate.NewLimiter(rate.Every(2*time.Second), 10)
	}
	if opts.PollInterval <= 0 {
		opts.PollInterval = time.Second
	}
	if opts.MaxWait <= 0 {
		opts.MaxWait = DefaultMaxWait
	}
	if opts.UpdatesWait <= 0 {
		opts.UpdatesWait = DefaultUpdatesWait
	}
	return &Handler{opts: opts}
}

// Handler returns the route mux.
func (h *Handler) Handler() http.Handler {
	mux := http.NewServeMux()
	mux.HandleFunc("POST /v1/voice/utterance", h.handleUtterance)
	mux.HandleFunc("POST /v1/voice/result", h.handleResult)
	mux.HandleFunc("POST /v1/voice/updates", h.handleUpdates)
	mux.HandleFunc("GET /v1/voice/state", h.handleState)
	return mux
}

// --- contracts ------------------------------------------------------------

// UtteranceRequest is one recognised phrase plus the HA voice profile.
type UtteranceRequest struct {
	Text    string `json:"text"`
	Profile string `json:"profile"`
}

// UtteranceResponse tells the proxy where the phrase went. Route is one of
// "hermes", "bb" or "local" (handled here, do not forward).
type UtteranceResponse struct {
	Route    string `json:"route"`
	OK       *bool  `json:"ok,omitempty"`
	ThreadID string `json:"threadId,omitempty"`
	AfterSeq *int64 `json:"afterSeq,omitempty"`
	Speech   string `json:"speech,omitempty"`
}

// ResultRequest long-polls one turn.
type ResultRequest struct {
	ThreadID    string `json:"threadId"`
	AfterSeq    int64  `json:"afterSeq"`
	WaitSeconds int    `json:"waitSeconds"`
}

// ResultResponse is the turn outcome. CompletedSeq is the event sequence of
// the completed turn; the voice proxy uses it as the idempotency key of its
// answer queue, so it must be present on every finished turn.
type ResultResponse struct {
	Status       string `json:"status"`
	Speech       string `json:"speech,omitempty"`
	Text         string `json:"text,omitempty"`
	CompletedSeq *int64 `json:"completedSeq,omitempty"`
}

// StateResponse is the debug view of the agent-conversation switch.
type StateResponse struct {
	AgentMode bool   `json:"agentMode"`
	ThreadID  string `json:"threadId,omitempty"`
}

// UpdatesRequest long-polls the dispatcher thread. AfterSeq nil is the
// bootstrap call: the route returns the cursor and no history.
type UpdatesRequest struct {
	AfterSeq    *int64 `json:"afterSeq,omitempty"`
	WaitSeconds *int   `json:"waitSeconds,omitempty"`
}

// UpdatesResponse is the dispatcher-updates answer. Status is only set on a
// failure, mirroring the result route's {"status":"failed","speech":...}.
type UpdatesResponse struct {
	Cursor *int64          `json:"cursor,omitempty"`
	Items  []bb.UpdateItem `json:"items"`
	Status string          `json:"status,omitempty"`
	Speech string          `json:"speech,omitempty"`
}

// --- handlers -------------------------------------------------------------

func (h *Handler) handleUtterance(w http.ResponseWriter, r *http.Request) {
	var req UtteranceRequest
	if !decode(w, r, &req) {
		return
	}
	text := strings.TrimSpace(req.Text)
	if text == "" || len([]rune(text)) > MaxTextRunes {
		h.write(w, http.StatusBadRequest, UtteranceResponse{Route: "local", Speech: SpeechBadRequest})
		return
	}
	profile, ok := normalizeProfile(req.Profile)
	if !ok {
		h.write(w, http.StatusBadRequest, UtteranceResponse{Route: "local", Speech: SpeechBadRequest})
		return
	}
	if !h.opts.Limiter.Allow() {
		h.observe("utterance", profile, "rate_limited", "", len([]rune(text)))
		h.write(w, http.StatusTooManyRequests, UtteranceResponse{Route: "bb", Speech: SpeechRateLimited})
		return
	}

	if isExit(text) {
		h.opts.Mode.Exit()
		h.observe("exit", profile, "success", "", len([]rune(text)))
		h.write(w, http.StatusOK, UtteranceResponse{Route: "local", Speech: SpeechModeOff})
		return
	}

	if profile == ProfileWork {
		if strings.TrimSpace(h.opts.WorkThreadID) == "" {
			h.observe("work", profile, "failure", "", len([]rune(text)))
			h.write(w, http.StatusOK, UtteranceResponse{Route: "bb", Speech: SpeechWorkMissing})
			return
		}
		h.send(w, r, h.opts.WorkThreadID, text, profile, "work", SpeechWorkSent)
		return
	}

	// Agent conversation mode is a personal-profile feature. Work-profile
	// phrases always go to the work dispatcher; the mode stays on so it keeps
	// working when the profile switches back to personal.
	if threadID, active := h.opts.Mode.Active(); active {
		h.send(w, r, threadID, text, profile, "agent", SpeechAgentSent)
		return
	}

	h.observe("hermes", profile, "success", "", len([]rune(text)))
	h.write(w, http.StatusOK, UtteranceResponse{Route: "hermes"})
}

// send delivers one phrase to bb. Failures always stay on the bb route: a
// work-profile phrase must never fall through to Hermes.
func (h *Handler) send(w http.ResponseWriter, r *http.Request, threadID, text, profile, route, speech string) {
	result, err := h.opts.Dispatcher.Send(r.Context(), bb.SendRequest{
		ThreadID:       threadID,
		Text:           text,
		IdempotencyKey: newDeliveryKey(h.opts.Now()),
	})
	if err != nil {
		bridgeErr := bb.AsError(err)
		h.observe(route, profile, "failure", threadID, len([]rune(text)))
		failed := false
		h.write(w, http.StatusOK, UtteranceResponse{Route: "bb", OK: &failed, ThreadID: threadID, Speech: bridgeErr.Speech})
		return
	}
	h.observe(route, profile, "success", threadID, len([]rune(text)))
	ok := true
	afterSeq := result.AfterSeq
	h.write(w, http.StatusOK, UtteranceResponse{Route: "bb", OK: &ok, ThreadID: threadID, AfterSeq: &afterSeq, Speech: speech})
}

func (h *Handler) handleResult(w http.ResponseWriter, r *http.Request) {
	var req ResultRequest
	if !decode(w, r, &req) {
		return
	}
	if strings.TrimSpace(req.ThreadID) == "" || req.AfterSeq < 0 {
		h.write(w, http.StatusBadRequest, ResultResponse{Status: "failed", Speech: SpeechBadRequest})
		return
	}
	wait := req.WaitSeconds
	if wait <= 0 {
		wait = int(h.opts.MaxWait / time.Second)
	}
	maxWait := int(h.opts.MaxWait / time.Second)
	if wait > maxWait {
		wait = maxWait
	}
	deadline := h.opts.Now().Add(time.Duration(wait) * time.Second)

	for {
		result, err := h.opts.Dispatcher.TurnResult(r.Context(), bb.TurnResultRequest{
			ThreadID: req.ThreadID,
			AfterSeq: req.AfterSeq,
		})
		if err != nil {
			h.observe("result", "", "failure", req.ThreadID, 0)
			h.write(w, http.StatusOK, ResultResponse{Status: "failed", Speech: bb.SpeechForErr(err)})
			return
		}
		switch result.Status {
		case "running", "starting":
			if !h.opts.Now().Before(deadline) {
				h.observe("result", "", "running", req.ThreadID, 0)
				h.write(w, http.StatusOK, ResultResponse{Status: "running"})
				return
			}
			timer := time.NewTimer(h.opts.PollInterval)
			select {
			case <-r.Context().Done():
				timer.Stop()
				return
			case <-timer.C:
			}
		default:
			h.observe("result", "", result.Status, req.ThreadID, 0)
			h.write(w, http.StatusOK, ResultResponse{
				Status:       result.Status,
				Speech:       turnSpeech(result),
				Text:         result.Text,
				CompletedSeq: result.CompletedSeq,
			})
			return
		}
	}
}

func (h *Handler) handleUpdates(w http.ResponseWriter, r *http.Request) {
	var req UpdatesRequest
	if !decode(w, r, &req) {
		return
	}
	if req.AfterSeq != nil && *req.AfterSeq < 0 {
		h.write(w, http.StatusBadRequest, UpdatesResponse{
			Items:  []bb.UpdateItem{},
			Status: "failed",
			Speech: SpeechBadRequest,
		})
		return
	}

	wait := int(h.opts.MaxWait / time.Second)
	if req.WaitSeconds != nil && *req.WaitSeconds > 0 {
		wait = *req.WaitSeconds
	}
	if maxWait := int(h.opts.MaxWait / time.Second); wait > maxWait {
		wait = maxWait
	}
	// The plugin long-polls, but one SSH bridge call must stay under the
	// bridge timeout. A shorter answer is fine: the proxy uses the returned
	// cursor and calls again.
	if updatesWait := int(h.opts.UpdatesWait / time.Second); wait > updatesWait {
		wait = updatesWait
	}

	result, err := h.opts.Dispatcher.Updates(r.Context(), bb.UpdatesRequest{
		AfterSeq:    req.AfterSeq,
		WaitSeconds: &wait,
	})
	if err != nil {
		h.observe("updates", "", "failure", "", 0)
		h.write(w, http.StatusOK, UpdatesResponse{
			Items:  []bb.UpdateItem{},
			Status: "failed",
			Speech: bb.SpeechForErr(err),
		})
		return
	}
	items := result.Items
	if items == nil {
		items = []bb.UpdateItem{}
	}
	h.observe("updates", "", "ok", "", 0)
	cursor := result.Cursor
	h.write(w, http.StatusOK, UpdatesResponse{Cursor: &cursor, Items: items})
}

func (h *Handler) handleState(w http.ResponseWriter, _ *http.Request) {
	threadID, active := h.opts.Mode.Active()
	h.write(w, http.StatusOK, StateResponse{AgentMode: active, ThreadID: threadID})
}

func turnSpeech(result bb.TurnResultResult) string {
	switch result.Status {
	case "failed":
		return SpeechTurnFailed
	case "waiting_input":
		if result.Spoken != nil && strings.TrimSpace(*result.Spoken) != "" {
			return *result.Spoken
		}
		return SpeechWaitingInput
	default:
		if result.Spoken != nil && strings.TrimSpace(*result.Spoken) != "" {
			return *result.Spoken
		}
		return SpeechDone
	}
}

// --- helpers --------------------------------------------------------------

func decode(w http.ResponseWriter, r *http.Request, out any) bool {
	defer r.Body.Close()
	decoder := json.NewDecoder(io.LimitReader(r.Body, 16<<10))
	decoder.DisallowUnknownFields()
	if err := decoder.Decode(out); err != nil {
		w.Header().Set("Content-Type", "application/json")
		w.WriteHeader(http.StatusBadRequest)
		_ = json.NewEncoder(w).Encode(map[string]string{"error": "invalid request"})
		return false
	}
	return true
}

func (h *Handler) write(w http.ResponseWriter, status int, value any) {
	w.Header().Set("Content-Type", "application/json")
	w.WriteHeader(status)
	encoder := json.NewEncoder(w)
	encoder.SetEscapeHTML(false)
	_ = encoder.Encode(value)
}

func (h *Handler) observe(route, profile, outcome, threadID string, chars int) {
	if h.opts.Metrics != nil {
		h.opts.Metrics.ObserveVoice(route, outcome)
	}
	// Never log the spoken text: only its size and routing metadata.
	h.opts.Logger.Info("voice utterance",
		"event", "voice_utterance",
		"route", route,
		"profile", profile,
		"outcome", outcome,
		"thread_id", threadID,
		"chars", chars,
	)
}

// normalizeProfile maps the proxy's Russian or English value onto the two
// supported profiles. An unknown value is rejected (fail closed).
func normalizeProfile(raw string) (string, bool) {
	switch normalize(raw) {
	case "", "personal", "личный", "личный профиль":
		return ProfilePersonal, true
	case "work", "рабочий", "рабочий профиль":
		return ProfileWork, true
	default:
		return "", false
	}
}

// isExit matches a whole utterance, not a fragment, so a task that merely
// mentions agents is never mistaken for the exit command.
func isExit(text string) bool {
	normalized := normalize(text)
	for _, phrase := range exitPhrases {
		if normalized == phrase {
			return true
		}
	}
	return false
}

func normalize(value string) string {
	var b strings.Builder
	space := false
	for _, r := range strings.ToLower(strings.TrimSpace(value)) {
		if r == 'ё' {
			r = 'е'
		}
		if unicode.IsLetter(r) || unicode.IsDigit(r) {
			if space && b.Len() > 0 {
				b.WriteByte(' ')
			}
			space = false
			b.WriteRune(r)
			continue
		}
		space = true
	}
	return strings.TrimSpace(b.String())
}

// newDeliveryKey prevents an accidental duplicate within retries of one
// request while staying deterministic-enough for logs.
func newDeliveryKey(now time.Time) string {
	return fmt.Sprintf("voice-%d", now.UnixNano())
}
