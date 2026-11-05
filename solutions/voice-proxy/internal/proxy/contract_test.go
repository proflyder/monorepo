package proxy

import (
	"encoding/json"
	"io"
	"net/http"
	"net/http/httptest"
	"strings"
	"testing"

	"ricardo.local/monorepo/packages/go/contracttest"
)

// TestContractFixtures replays the proxy's external HTTP contract. The runner
// owns the transcript; this file provides the deterministic Hermes double.
func TestContractFixtures(t *testing.T) {
	contracttest.Run(t, "solutions/voice-proxy/internal/proxy/testdata/contract", newContractServer,
		contracttest.WithNormalizer(normalizeCreated),
		contracttest.WithHeaders("Cache-Control"))
}

func newContractServer(t *testing.T, _ string) http.Handler {
	t.Helper()
	upstream := httptest.NewServer(http.HandlerFunc(contractUpstream))
	t.Cleanup(upstream.Close)
	return New(Options{
		UpstreamURL:     upstream.URL,
		ReasoningEffort: "low",
		Logger:          NopLogger{},
	})
}

// contractUpstream is the deterministic Hermes double. It never touches the
// network and never changes between runs.
func contractUpstream(w http.ResponseWriter, r *http.Request) {
	switch {
	case r.Method == http.MethodGet:
		w.Header().Set("Content-Type", "application/json")
		_, _ = io.WriteString(w, `{"object":"list","data":[{"id":"hermes-agent","object":"model"}]}`)
	case r.Method == http.MethodPost && strings.HasPrefix(r.URL.Path, "/v1/chat/completions"):
		raw, _ := io.ReadAll(r.Body)
		var payload map[string]any
		_ = json.Unmarshal(raw, &payload)
		if payload["model"] == "boom" {
			w.Header().Set("Content-Type", "application/json")
			w.WriteHeader(http.StatusBadGateway)
			_, _ = io.WriteString(w, `{"error":{"message":"upstream exploded","type":"upstream_error"}}`)
			return
		}
		if stream, _ := payload["stream"].(bool); stream {
			w.Header().Set("Content-Type", "text/event-stream")
			_, _ = io.WriteString(w, strings.Join([]string{
				`data: {"id":"chatcmpl-contract","object":"chat.completion.chunk","created":1000,"model":"hermes-agent","choices":[{"index":0,"delta":{"role":"assistant"},"finish_reason":null}]}`,
				``,
				`data: {"id":"chatcmpl-contract","object":"chat.completion.chunk","created":1000,"model":"hermes-agent","choices":[{"index":0,"delta":{"content":"Привет."},"finish_reason":null}]}`,
				``,
				`data: {"id":"chatcmpl-contract","object":"chat.completion.chunk","created":1000,"model":"hermes-agent","choices":[{"index":0,"delta":{},"finish_reason":"stop"}]}`,
				``,
				`data: [DONE]`,
				``,
			}, "\n"))
			return
		}
		w.Header().Set("Content-Type", "application/json")
		_, _ = io.WriteString(w, `{"id":"chatcmpl-contract","object":"chat.completion","created":1000,"model":"hermes-agent","choices":[{"index":0,"message":{"role":"assistant","content":"Привет."},"finish_reason":"stop"}],"usage":{"prompt_tokens":1,"completion_tokens":2,"total_tokens":3}}`)
	default:
		http.NotFound(w, r)
	}
}

// normalizeCreated replaces the only non-deterministic field of a local
// completion: its wall-clock `created` timestamp. The handler has no clock
// seam, so the fixture records <any> for that number.
func normalizeCreated(_ string, value any) any {
	switch typed := value.(type) {
	case []any:
		out := make([]any, len(typed))
		for i, item := range typed {
			out[i] = normalizeCreated("", item)
		}
		return out
	case map[string]any:
		out := make(map[string]any, len(typed))
		for key, item := range typed {
			if key == "created" {
				out[key] = "<any>"
				continue
			}
			out[key] = normalizeCreated("", item)
		}
		return out
	default:
		return value
	}
}
