// Package gateway is the mcp-gateway voice client: it asks where an utterance
// belongs (/v1/voice/utterance) and long-polls the routed turn result and the
// dispatcher updates.
//
// The request bodies, paths and timeouts are copied from call_utterance(),
// call_result(), call_updates() and _post_json() in voice_proxy.py:
//
//   - utterance: 5 s (HERMES_VOICE_GATEWAY_TIMEOUT);
//   - result/updates: gateway timeout + the long-poll wait, because the server
//     only answers when the wait expires;
//   - a non-object JSON response is treated as an empty object.
package gateway

import (
	"bytes"
	"context"
	"encoding/json"
	"errors"
	"io"
	"net/http"
	"strings"
	"time"
)

// Paths of the voice API.
const (
	UtterancePath = "/v1/voice/utterance"
	ResultPath    = "/v1/voice/result"
	UpdatesPath   = "/v1/voice/updates"
)

// maxResponseBytes bounds one gateway response.
const maxResponseBytes = 8 << 20

// Config configures a Client.
type Config struct {
	// URL is the gateway base (no trailing slash).
	URL string
	// Token is resolved on every request, matching voice_token().
	Token func() string
	// Timeout is HERMES_VOICE_GATEWAY_TIMEOUT.
	Timeout time.Duration
	// HTTP is the shared client; nil builds a plain one.
	HTTP *http.Client
}

// Client calls the mcp-gateway voice routes.
type Client struct {
	url     string
	token   func() string
	timeout time.Duration
	http    *http.Client
}

// New builds a Client.
func New(cfg Config) *Client {
	timeout := cfg.Timeout
	if timeout <= 0 {
		timeout = 5 * time.Second
	}
	hc := cfg.HTTP
	if hc == nil {
		hc = &http.Client{}
	}
	token := cfg.Token
	if token == nil {
		token = func() string { return "" }
	}
	return &Client{
		url:     strings.TrimSuffix(strings.TrimSpace(cfg.URL), "/"),
		token:   token,
		timeout: timeout,
		http:    hc,
	}
}

// Utterance asks the gateway where one phrase belongs.
func (c *Client) Utterance(ctx context.Context, text, profile string) (map[string]any, error) {
	return c.post(ctx, UtterancePath, map[string]any{
		"text":    text,
		"profile": profile,
	}, c.timeout)
}

// Result long-polls the answer of a routed turn. waitSeconds is raised to at
// least 1 and added to the request timeout, like call_result().
func (c *Client) Result(ctx context.Context, threadID string, afterSeq int64, waitSeconds int) (map[string]any, error) {
	wait := waitSeconds
	if wait < 1 {
		wait = 1
	}
	return c.post(ctx, ResultPath, map[string]any{
		"threadId":    threadID,
		"afterSeq":    afterSeq,
		"waitSeconds": wait,
	}, c.timeout+time.Duration(wait)*time.Second)
}

// Updates long-polls the dispatcher updates. A nil afterSeq bootstraps without
// a cursor; the gateway then returns the current cursor and no history.
func (c *Client) Updates(ctx context.Context, afterSeq *int64, waitSeconds int) (map[string]any, error) {
	wait := waitSeconds
	if wait < 1 {
		wait = 1
	}
	body := map[string]any{"waitSeconds": wait}
	if afterSeq != nil {
		body["afterSeq"] = *afterSeq
	}
	return c.post(ctx, UpdatesPath, body, c.timeout+time.Duration(wait)*time.Second)
}

func (c *Client) post(ctx context.Context, path string, body map[string]any, timeout time.Duration) (map[string]any, error) {
	if c.url == "" {
		return map[string]any{}, errors.New("gateway URL is not configured")
	}
	payload, err := json.Marshal(body)
	if err != nil {
		return map[string]any{}, err
	}
	reqCtx, cancel := context.WithTimeout(ctx, timeout)
	defer cancel()
	req, err := http.NewRequestWithContext(reqCtx, http.MethodPost, c.url+path, bytes.NewReader(payload))
	if err != nil {
		return map[string]any{}, err
	}
	req.Header.Set("Authorization", "Bearer "+c.token())
	req.Header.Set("Content-Type", "application/json")
	req.Header.Set("Accept", "application/json")
	resp, err := c.http.Do(req)
	if err != nil {
		return map[string]any{}, err
	}
	defer func() { _ = resp.Body.Close() }()
	if resp.StatusCode < 200 || resp.StatusCode >= 300 {
		return map[string]any{}, errors.New("gateway returned " + resp.Status)
	}
	// _post_json returns an empty object for a non-object JSON value.
	var value any
	if err := json.NewDecoder(io.LimitReader(resp.Body, maxResponseBytes)).Decode(&value); err != nil {
		return map[string]any{}, err
	}
	object, ok := value.(map[string]any)
	if !ok {
		return map[string]any{}, nil
	}
	return object, nil
}
