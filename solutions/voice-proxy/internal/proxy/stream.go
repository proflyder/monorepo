package proxy

import (
	"errors"
	"io"
	"net/http"
	"strings"
	"time"

	"ricardo.local/monorepo/solutions/voice-proxy/internal/sanitize"
	"ricardo.local/monorepo/solutions/voice-proxy/internal/sse"
)

var (
	errUpstreamNotSSE      = errors.New("upstream did not return SSE")
	errUpstreamEndedEarly  = errors.New("upstream ended without finish_reason")
	errUpstreamStreamError = errors.New("upstream stream error")
	errUnsuccessfulFinish  = errors.New("unsuccessful upstream finish")
	errStreamDisconnected  = errors.New("upstream disconnected before DONE")
)

// streamChat relays /v1/chat/completions as Server-Sent Events.
//
// Everything that produces speech is whitelisted: only content deltas and the
// assistant role reach HA. Reasoning content, tool calls, tool progress and
// status events never do. A finish frame is always written after the tail it
// belongs to, because HA stops reading as soon as it sees a finish_reason.
func (h *Handler) streamChat(w http.ResponseWriter, r *http.Request, payload map[string]any, state *postState) error {
	body, err := encodeJSON(payload)
	if err != nil {
		return err
	}
	// The streaming request always asks Hermes for SSE, whatever the client
	// sent, exactly like _stream_chat in voice_proxy.py.
	headers := r.Header.Clone()
	headers.Set("Content-Type", "application/json")
	headers.Set("Accept", "text/event-stream")
	started := time.Now()
	resp, err := h.upstream.Stream(r.Context(), http.MethodPost, r.URL.RequestURI(), body, headers)
	if err != nil {
		// Python's urlopen raises here before any header is sent, so the outer
		// handler answers 503 with the OpenAI-shaped error.
		return err
	}
	defer func() { _ = resp.Body.Close() }()

	if resp.StatusCode >= 400 {
		data, readErr := io.ReadAll(io.LimitReader(resp.Body, h.maxBody))
		if readErr != nil {
			return readErr
		}
		// The upstream error body is relayed as JSON; _send normalises it.
		h.send(w, resp.StatusCode, data, "application/json")
		return nil
	}
	if !strings.HasPrefix(resp.Header.Get("Content-Type"), "text/event-stream") {
		return errUpstreamNotSSE
	}

	w.Header().Set("Content-Type", "text/event-stream; charset=utf-8")
	w.Header().Set("Cache-Control", "no-cache")
	w.Header().Set("Connection", "close")
	w.WriteHeader(http.StatusOK)
	state.streamStarted = true
	events := sse.NewWriter(flushWriter{w: w, flusher: flusher(w)})

	sanitizers := map[int]*sanitize.StreamingSanitizer{}
	finished := map[int]bool{}
	firstContent := -1.0
	reader := sse.NewReader(resp.Body)

	for {
		event, readErr := reader.Next()
		if errors.Is(readErr, io.EOF) {
			break
		}
		if readErr != nil {
			return readErr
		}
		if event.Event == sse.KeepaliveEvent {
			// Preserve liveness and notice a disconnected HA client while the
			// model is still thinking.
			if err := events.WriteKeepalive(); err != nil {
				return errClientGone
			}
			continue
		}
		if event.Data == "[DONE]" {
			if len(sanitizers) == 0 || len(finished) != len(sanitizers) {
				return errUpstreamEndedEarly
			}
			if err := events.WriteData("[DONE]"); err != nil {
				return errClientGone
			}
			h.logger.Infof("stream complete first_text=%.3fs total=%.3fs reasoning=%s",
				firstContentOrZero(firstContent), time.Since(started).Seconds(), h.reasoningEffort)
			return nil
		}
		if event.Event != "message" && event.Event != "" {
			// Hermes tool progress, approvals and status are not speech.
			continue
		}
		value, err := decodeJSON([]byte(event.Data))
		if err != nil {
			return err
		}
		chunk, ok := value.(map[string]any)
		if !ok {
			return errUpstreamStreamError
		}
		if object, _ := chunk["object"].(string); object != "chat.completion.chunk" {
			if _, hasError := chunk["error"]; hasError {
				return errUpstreamStreamError
			}
			continue
		}

		var choices, finishChoices []any
		rawChoices, _ := chunk["choices"].([]any)
		for _, item := range rawChoices {
			choice, ok := item.(map[string]any)
			if !ok {
				continue
			}
			index := intValue(choice["index"], 0)
			if finished[index] {
				continue
			}
			sanitizer, ok := sanitizers[index]
			if !ok {
				sanitizer = &sanitize.StreamingSanitizer{}
				sanitizers[index] = sanitizer
			}
			delta, _ := choice["delta"].(map[string]any)
			reason, _ := choice["finish_reason"].(string)
			hasFinish := choice["finish_reason"] != nil
			if hasFinish && reason != "stop" && reason != "length" {
				return errUnsuccessfulFinish
			}
			content, _ := delta["content"].(string)
			text, feedErr := sanitizer.Feed(content, hasFinish)
			if feedErr != nil {
				return feedErr
			}
			if text != "" && !hasFinish {
				// Piper's sentence_stream waits for the next capitalized word.
				// A paragraph boundary flushes this complete sentence immediately.
				text += "\n\n"
			}
			safeDelta := map[string]any{}
			if text != "" {
				safeDelta["content"] = text
			}
			if role, _ := delta["role"].(string); role == "assistant" {
				safeDelta["role"] = "assistant"
			}
			if text != "" && firstContent < 0 {
				firstContent = time.Since(started).Seconds()
			}
			if hasFinish {
				finished[index] = true
				finishChoices = append(finishChoices, map[string]any{
					"index":         index,
					"delta":         map[string]any{},
					"finish_reason": reason,
				})
			}
			if len(safeDelta) > 0 {
				choices = append(choices, map[string]any{
					"index":         index,
					"delta":         safeDelta,
					"finish_reason": nil,
				})
			}
		}
		if len(choices) > 0 {
			// Whitelist wire fields; never forward reasoning or tool-call deltas.
			if err := events.WriteData(whitelisted(chunk, choices)); err != nil {
				return errClientGone
			}
		}
		if len(finishChoices) > 0 {
			// HA stops reading as soon as finish_reason is set, so the tail above
			// was already flushed by the previous frame.
			if err := events.WriteData(whitelisted(chunk, finishChoices)); err != nil {
				return errClientGone
			}
		}
	}
	return errStreamDisconnected
}

// whitelisted copies the chunk header fields the client already knows about and
// replaces the choices with the sanitised ones.
func whitelisted(chunk map[string]any, choices []any) map[string]any {
	out := map[string]any{"choices": choices}
	for _, key := range []string{"id", "object", "created", "model"} {
		if value, ok := chunk[key]; ok {
			out[key] = value
		}
	}
	return out
}

func firstContentOrZero(value float64) float64 {
	if value < 0 {
		return 0
	}
	return value
}
