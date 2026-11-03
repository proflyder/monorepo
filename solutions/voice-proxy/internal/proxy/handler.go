// Package proxy is the HTTP surface of the voice proxy: GET passthrough and
// the POST /v1/chat/completions pipeline.
//
// The pipeline order is part of the contract and is copied from
// voice_proxy.py: cancel guard first (before any profile read or routing call),
// then the optional voice router, then the reasoning_effort injection, then
// either the streaming relay or the buffered sanitising relay.
package proxy

import (
	"context"
	"encoding/json"
	"errors"
	"net/http"
	"strings"
	"time"

	"ricardo.local/monorepo/solutions/voice-proxy/internal/hermes"
	"ricardo.local/monorepo/solutions/voice-proxy/internal/sanitize"
)

const (
	// DefaultCompletionID is the id of a proxy-generated local reply.
	DefaultCompletionID = "chatcmpl-ricardo-local"
	// CancelCompletionID marks the local reply that answers a cancel phrase.
	CancelCompletionID = "chatcmpl-ricardo-cancel"
	// CancelSilence is never spoken and never reaches Hermes. HA's llama_cpp
	// adapter drops an assistant message with empty content and then raises
	// "Unable to get response"; a whitespace-only body keeps it valid while the
	// pipeline's tts_input.strip() check still skips TTS.
	CancelSilence = " "
	// DefaultModel is used when the client did not send a model name.
	DefaultModel = "hermes-agent"

	upstreamUnavailable = "Hermes upstream is unavailable"
	streamFailedMessage = "Voice upstream stream failed"
)

// DefaultCancelPhrases are the exact local cancel phrases registered by the HA
// custom_sentences StationCancel intent. The guard is a last-resort path for
// continued-conversation mode and must not be broadened into fuzzy matching.
var DefaultCancelPhrases = []string{
	"отмена",
	"отмени",
	"передумал",
	"передумала",
	"не надо",
	"ничего",
	"отбой",
	"забудь",
	"неважно",
}

// RouteResult is the decision of the optional voice router.
type RouteResult struct {
	// Handled means the utterance must not reach Hermes.
	Handled bool
	// Speech is the already-shaped local reply.
	Speech string
	// CompletionID overrides DefaultCompletionID when non-empty.
	CompletionID string
	// Announce, when non-nil, runs after the local reply was written; it starts
	// the background result poller for a routed bb turn.
	Announce func()
}

// Router resolves profile routing for the last user utterance. It is the
// extension point for the routing step: a nil Router keeps the plain
// sanitiser behaviour, exactly like routing_enabled() returning false.
type Router interface {
	Route(ctx context.Context, payload map[string]any, utterance string) RouteResult
}

// Options configures a Handler.
type Options struct {
	UpstreamURL     string
	ReasoningEffort string
	MaxBody         int64
	Timeout         time.Duration
	Logger          Logger
	CancelPhrases   []string
	Router          Router
}

// Handler serves the proxy HTTP surface.
type Handler struct {
	upstream        *hermes.Client
	reasoningEffort string
	maxBody         int64
	logger          Logger
	cancelPhrases   map[string]struct{}
	router          Router
	cancelCount     int64
}

// New builds a Handler.
func New(opts Options) *Handler {
	if opts.ReasoningEffort == "" {
		opts.ReasoningEffort = "low"
	}
	if opts.MaxBody <= 0 {
		opts.MaxBody = hermes.MaxBody
	}
	if opts.Logger == nil {
		opts.Logger = NopLogger{}
	}
	phrases := opts.CancelPhrases
	if phrases == nil {
		phrases = DefaultCancelPhrases
	}
	set := make(map[string]struct{}, len(phrases))
	for _, phrase := range phrases {
		set[sanitize.NormalizeUtterance(phrase)] = struct{}{}
	}
	return &Handler{
		upstream:        hermes.New(opts.UpstreamURL, opts.Timeout),
		reasoningEffort: opts.ReasoningEffort,
		maxBody:         opts.MaxBody,
		logger:          opts.Logger,
		cancelPhrases:   set,
		router:          opts.Router,
	}
}

// UpstreamURL is the normalised upstream base, used by -healthcheck and -status.
func (h *Handler) UpstreamURL() string { return h.upstream.Base() }

// CancelCount reports how many cancel phrases were answered locally.
func (h *Handler) CancelCount() int64 { return h.cancelCount }

// ServeHTTP dispatches like BaseHTTPRequestHandler: only GET and POST exist, so
// HEAD, PUT and friends are 501, not 405.
func (h *Handler) ServeHTTP(w http.ResponseWriter, r *http.Request) {
	// Python's http.server is HTTP/1.0 and closes every connection; force the
	// same semantics so HA's OpenAI client never sees a chunked tail.
	w.Header().Set("Connection", "close")
	switch r.Method {
	case http.MethodGet:
		h.handleGet(w, r)
	case http.MethodPost:
		h.handlePost(w, r)
	default:
		http.Error(w, "Unsupported method ("+r.Method+")", http.StatusNotImplemented)
	}
}

func (h *Handler) handleGet(w http.ResponseWriter, r *http.Request) {
	response, err := h.upstream.Do(r.Context(), http.MethodGet, r.URL.RequestURI(), nil, r.Header)
	if err != nil {
		h.send(w, http.StatusServiceUnavailable, hermes.ErrorBody(upstreamUnavailable), "application/json")
		return
	}
	h.send(w, response.Status, response.Data, response.ContentType)
}

// postState carries the per-request stream bookkeeping.
type postState struct {
	streamStarted bool
}

var errClientGone = errors.New("client disconnected")

func (h *Handler) handlePost(w http.ResponseWriter, r *http.Request) {
	state := &postState{}
	if err := h.processPost(w, r, state); err != nil {
		if errors.Is(err, errClientGone) {
			// The client closed the connection: closing the upstream body already
			// interrupted Hermes' turn, and there is nobody left to answer.
			return
		}
		if state.streamStarted {
			// Never append a second HTTP response or flush an untrusted tail.
			h.logger.Errorf("stream failed: %s", err)
			_ = h.writeEvent(w, errorFrame())
			return
		}
		h.send(w, http.StatusServiceUnavailable, hermes.ErrorBody(upstreamUnavailable), "application/json")
	}
}

func (h *Handler) processPost(w http.ResponseWriter, r *http.Request, state *postState) error {
	raw, err := hermes.ReadBody(r, h.maxBody)
	if err != nil {
		return err
	}
	payload, err := decodeObject(raw)
	if err != nil {
		return err
	}
	path := r.URL.RequestURI()
	isChat := strings.HasPrefix(path, "/v1/chat/completions")

	if isChat {
		// The cancel guard runs before any profile read or routing call.
		if _, matched := h.CancelShortCircuit(payload); matched {
			h.sendCancelCompletion(w, payload)
			return nil
		}
		if h.router != nil {
			if utterance, ok := LastUserText(payload); ok {
				result := h.router.Route(r.Context(), payload, utterance)
				if result.Handled && result.Speech != "" {
					h.sendLocalCompletion(w, payload, result.Speech, result.CompletionID)
					if result.Announce != nil {
						result.Announce()
					}
					return nil
				}
			}
		}
		payload = PrepareChatRequest(payload, h.reasoningEffort)
		if stream, ok := payload["stream"].(bool); ok && stream {
			return h.streamChat(w, r, payload, state)
		}
	}

	body, err := encodeJSON(payload)
	if err != nil {
		return err
	}
	response, err := h.upstream.Do(r.Context(), http.MethodPost, path, body, r.Header)
	if err != nil {
		return err
	}
	if isChat && response.Status == http.StatusOK {
		if object, err := decodeObject(response.Data); err == nil {
			sanitize.SanitizeChatPayload(object)
			if encoded, err := encodeJSON(object); err == nil {
				response.Data = encoded
			}
		}
	}
	h.send(w, response.Status, response.Data, response.ContentType)
	return nil
}

func (h *Handler) sendCancelCompletion(w http.ResponseWriter, payload map[string]any) {
	h.cancelCount++
	// Structural evidence only: never log the recognised text or payload.
	h.logger.Infof("cancel guarded count=%d stream=%t upstream=skipped", h.cancelCount, truthy(payload["stream"]))
	h.sendLocalCompletion(w, payload, CancelSilence, CancelCompletionID)
}

// ErrNotObject reports a request body that is not a JSON object; Python raises
// the same way when it calls .get on a non-dict value.
var ErrNotObject = errors.New("request body is not a JSON object")

func decodeObject(raw []byte) (map[string]any, error) {
	if len(raw) == 0 {
		// json.loads(raw or b"{}"): an absent body is an empty object.
		return map[string]any{}, nil
	}
	value, err := decodeJSON(raw)
	if err != nil {
		return nil, err
	}
	object, ok := value.(map[string]any)
	if !ok {
		return nil, ErrNotObject
	}
	return object, nil
}

// isStream mirrors Python's `payload.get("stream") is True`: only the JSON
// literal true selects the streaming reply shape.
func isStream(payload map[string]any) bool {
	stream, ok := payload["stream"].(bool)
	return ok && stream
}

// truthy mirrors Python's bool(value), used only for the structural cancel log.
func truthy(value any) bool {
	switch typed := value.(type) {
	case nil:
		return false
	case bool:
		return typed
	case string:
		return typed != ""
	case json.Number:
		number, err := typed.Float64()
		return err == nil && number != 0
	case float64:
		return typed != 0
	case []any:
		return len(typed) > 0
	case map[string]any:
		return len(typed) > 0
	}
	return true
}

func modelName(payload map[string]any) string {
	if model, ok := payload["model"].(string); ok && model != "" {
		return model
	}
	return DefaultModel
}

// PrepareChatRequest injects the voice reasoning effort without mutating the
// caller's payload. Hermes accepts reasoning through model_options, not
// max_tokens or HA options.
func PrepareChatRequest(payload map[string]any, effort string) map[string]any {
	out := make(map[string]any, len(payload)+1)
	for key, value := range payload {
		out[key] = value
	}
	options := map[string]any{}
	if existing, ok := payload["model_options"].(map[string]any); ok {
		for key, value := range existing {
			options[key] = value
		}
	}
	options["reasoning_effort"] = effort
	out["model_options"] = options
	return out
}

// LastUserText returns the text of the final message when it is a plain user
// turn: no tool result, no assistant text, no multi-part content.
func LastUserText(payload map[string]any) (string, bool) {
	messages, ok := payload["messages"].([]any)
	if !ok || len(messages) == 0 {
		return "", false
	}
	last, ok := messages[len(messages)-1].(map[string]any)
	if !ok {
		return "", false
	}
	if role, _ := last["role"].(string); role != "user" {
		return "", false
	}
	content, ok := last["content"].(string)
	if !ok || sanitize.TrimSpace(content) == "" {
		return "", false
	}
	return content, true
}

// CancelShortCircuit returns the matched cancel phrase for the last user
// message. Only the final message is considered; tool results, assistant text,
// quoted history and messages with extra meaning are forwarded upstream.
func (h *Handler) CancelShortCircuit(payload map[string]any) (string, bool) {
	messages, ok := payload["messages"].([]any)
	if !ok || len(messages) == 0 {
		return "", false
	}
	last, ok := messages[len(messages)-1].(map[string]any)
	if !ok {
		return "", false
	}
	if role, _ := last["role"].(string); role != "user" {
		return "", false
	}
	content, ok := last["content"].(string)
	if !ok {
		return "", false
	}
	normalized := sanitize.NormalizeUtterance(content)
	if _, matched := h.cancelPhrases[normalized]; !matched {
		return "", false
	}
	return normalized, true
}
