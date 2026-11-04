package proxy

import (
	"net/http"
	"strconv"
	"time"

	"ricardo.local/monorepo/solutions/voice-proxy/internal/hermes"
	"ricardo.local/monorepo/solutions/voice-proxy/internal/sse"
)

// send relays a buffered response. The status/body pair goes through the same
// error normalisation as every non-streaming response.
func (h *Handler) send(w http.ResponseWriter, status int, body []byte, contentType string) {
	body, contentType = hermes.NormalizeError(status, body, contentType)
	w.Header().Set("Content-Type", contentType)
	w.Header().Set("Content-Length", strconv.Itoa(len(body)))
	w.WriteHeader(status)
	_, _ = w.Write(body)
}

// sendLocalCompletion returns a proxy-generated reply in the same shape as
// Hermes. A streaming request gets the role chunk, the finish chunk and
// [DONE]; anything else gets one chat.completion object.
func (h *Handler) sendLocalCompletion(w http.ResponseWriter, payload map[string]any, speech, completionID string) {
	if completionID == "" {
		completionID = DefaultCompletionID
	}
	if isStream(payload) {
		w.Header().Set("Content-Type", "text/event-stream; charset=utf-8")
		w.Header().Set("Cache-Control", "no-cache")
		w.Header().Set("Connection", "close")
		w.WriteHeader(http.StatusOK)
		events := sse.NewWriter(flushWriter{w: w, flusher: flusher(w)})
		finish := "stop"
		_ = events.WriteData(localChunk(payload, map[string]any{"role": "assistant", "content": speech}, nil, completionID))
		_ = events.WriteData(localChunk(payload, map[string]any{}, &finish, completionID))
		_ = events.WriteData("[DONE]")
		return
	}
	body, err := encodeJSON(map[string]any{
		"id":      completionID,
		"object":  "chat.completion",
		"created": time.Now().Unix(),
		"model":   modelName(payload),
		"choices": []any{map[string]any{
			"index": 0,
			"message": map[string]any{
				"role":    "assistant",
				"content": speech,
			},
			"finish_reason": "stop",
		}},
		"usage": map[string]any{
			"prompt_tokens":     0,
			"completion_tokens": 0,
			"total_tokens":      0,
		},
	})
	if err != nil {
		h.send(w, http.StatusInternalServerError, hermes.ErrorBody(upstreamUnavailable), "application/json")
		return
	}
	h.send(w, http.StatusOK, body, "application/json")
}

// localChunk builds one proxy-generated chat.completion.chunk.
func localChunk(payload map[string]any, delta map[string]any, finishReason *string, completionID string) map[string]any {
	var finish any
	if finishReason != nil {
		finish = *finishReason
	}
	return map[string]any{
		"id":      completionID,
		"object":  "chat.completion.chunk",
		"created": time.Now().Unix(),
		"model":   modelName(payload),
		"choices": []any{map[string]any{
			"index":         0,
			"delta":         delta,
			"finish_reason": finish,
		}},
	}
}

// errorFrame is the single SSE error record emitted after a mid-stream failure.
func errorFrame() map[string]any {
	return map[string]any{
		"error": map[string]any{
			"message": streamFailedMessage,
			"type":    "server_error",
		},
	}
}

// writeEvent writes one frame and flushes it.
func (h *Handler) writeEvent(w http.ResponseWriter, payload any) error {
	if err := sse.NewWriter(flushWriter{w: w, flusher: flusher(w)}).WriteData(payload); err != nil {
		return errClientGone
	}
	return nil
}

// flushWriter flushes the HTTP response after every frame so Piper can start
// speaking the first sentence before the upstream turn ends.
type flushWriter struct {
	w       http.ResponseWriter
	flusher http.Flusher
}

func (fw flushWriter) Write(p []byte) (int, error) {
	n, err := fw.w.Write(p)
	if fw.flusher != nil {
		fw.flusher.Flush()
	}
	return n, err
}

func flusher(w http.ResponseWriter) http.Flusher {
	if f, ok := w.(http.Flusher); ok {
		return f
	}
	return nil
}
