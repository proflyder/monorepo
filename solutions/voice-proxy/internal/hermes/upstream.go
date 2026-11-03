// Package hermes is the upstream half of the voice proxy: a small
// OpenAI-compatible passthrough client plus the error normalisation HA's
// llama_cpp adapter requires.
//
// It is a port of _upstream(), error_body() and normalize_error() in
// voice_proxy.py. There is no authentication of our own: the client's
// Authorization header is forwarded verbatim, because the proxy is not an auth
// boundary (the nftables rule in front of the Pod is).
package hermes

import (
	"bytes"
	"context"
	"encoding/json"
	"errors"
	"fmt"
	"io"
	"net/http"
	"strings"
	"time"
)

// MaxBody is the request body cap from voice_proxy.py (10 MiB).
const MaxBody = 10 << 20

// DefaultTimeout is the upstream timeout for buffered and streaming requests.
const DefaultTimeout = 180 * time.Second

// ErrBodyTooLarge reports a request body over MaxBody.
var ErrBodyTooLarge = errors.New("request body too large")

// Client talks to the OpenAI-compatible upstream.
type Client struct {
	base      string
	timeout   time.Duration
	buffered  *http.Client
	streaming *http.Client
}

// New returns a client for baseURL (trailing slashes are ignored, like the
// Python module constant).
func New(baseURL string, timeout time.Duration) *Client {
	if timeout <= 0 {
		timeout = DefaultTimeout
	}
	transport := &http.Transport{Proxy: http.ProxyFromEnvironment}
	if base, ok := http.DefaultTransport.(*http.Transport); ok {
		transport = base.Clone()
	}
	// Waiting for the response headers is bounded exactly like Python's
	// urlopen(timeout): a stalled upstream fails instead of hanging.
	transport.ResponseHeaderTimeout = timeout
	return &Client{
		base:      strings.TrimSuffix(strings.TrimSpace(baseURL), "/"),
		timeout:   timeout,
		buffered:  &http.Client{Timeout: timeout},
		streaming: &http.Client{Transport: transport},
	}
}

// Base returns the normalised upstream base URL.
func (c *Client) Base() string { return c.base }

// Body is one upstream response.
type Body struct {
	Status      int
	Data        []byte
	ContentType string
}

// Do performs one buffered request. A non-2xx status is not an error: like
// urllib's HTTPError branch, the caller relays it and normalises the body.
func (c *Client) Do(ctx context.Context, method, path string, body []byte, headers http.Header) (Body, error) {
	req, err := c.request(ctx, method, path, body, headers)
	if err != nil {
		return Body{}, err
	}
	resp, err := c.buffered.Do(req)
	if err != nil {
		return Body{}, err
	}
	defer func() { _ = resp.Body.Close() }()
	data, err := io.ReadAll(resp.Body)
	if err != nil {
		return Body{}, err
	}
	return Body{
		Status:      resp.StatusCode,
		Data:        data,
		ContentType: headerOr(resp.Header, "Content-Type", "application/json"),
	}, nil
}

// Stream opens a request and leaves the response body to the caller, who must
// close it: closing the body also cancels the upstream turn.
//
// The response is returned even for 4xx/5xx so the caller can relay the status
// exactly like the Python HTTPError branch.
//
// The timeout is per read, like Python's socket timeout: a long turn that keeps
// producing frames is never cut, while a stalled one fails after the timeout.
func (c *Client) Stream(ctx context.Context, method, path string, body []byte, headers http.Header) (*http.Response, error) {
	reqCtx, cancel := context.WithCancel(ctx)
	req, err := c.request(reqCtx, method, path, body, headers)
	if err != nil {
		cancel()
		return nil, err
	}
	resp, err := c.streaming.Do(req)
	if err != nil {
		cancel()
		return nil, err
	}
	resp.Body = &idleBody{
		ReadCloser: resp.Body,
		cancel:     cancel,
		timeout:    c.timeout,
		timer:      time.AfterFunc(c.timeout, cancel),
	}
	return resp, nil
}

// idleBody cancels the request when no bytes arrive for the timeout, which is
// what a Python socket timeout does on each recv.
type idleBody struct {
	io.ReadCloser
	cancel  context.CancelFunc
	timeout time.Duration
	timer   *time.Timer
}

func (b *idleBody) Read(p []byte) (int, error) {
	b.timer.Reset(b.timeout)
	return b.ReadCloser.Read(p)
}

func (b *idleBody) Close() error {
	b.timer.Stop()
	b.cancel()
	return b.ReadCloser.Close()
}

func (c *Client) request(ctx context.Context, method, path string, body []byte, headers http.Header) (*http.Request, error) {
	var reader io.Reader
	if body != nil {
		reader = bytes.NewReader(body)
	}
	req, err := http.NewRequestWithContext(ctx, method, c.base+path, reader)
	if err != nil {
		return nil, err
	}
	// The client's Authorization is forwarded verbatim, including when it is
	// absent: Python sends the same empty header and the proxy is not an auth
	// boundary.
	req.Header.Set("Authorization", headerOr(headers, "Authorization", ""))
	req.Header.Set("Content-Type", headerOr(headers, "Content-Type", "application/json"))
	req.Header.Set("Accept", headerOr(headers, "Accept", "application/json"))
	return req, nil
}

func headerOr(h http.Header, name, fallback string) string {
	if values, ok := h[name]; ok && len(values) > 0 {
		return values[0]
	}
	return fallback
}

// ReadBody reads a request body with the 10 MiB cap. The cap is checked against
// Content-Length first so an oversized upload is rejected without buffering it.
func ReadBody(r *http.Request, max int64) ([]byte, error) {
	if max <= 0 {
		max = MaxBody
	}
	if r.ContentLength > max {
		return nil, ErrBodyTooLarge
	}
	data, err := io.ReadAll(io.LimitReader(r.Body, max+1))
	if err != nil {
		return nil, err
	}
	if int64(len(data)) > max {
		return nil, ErrBodyTooLarge
	}
	return data, nil
}

// ErrorBody is the OpenAI-shaped error payload HA's client can parse:
// the llama_cpp adapter calls .get on the "error" value, so it must be an
// object, including for startup failures.
func ErrorBody(message string) []byte {
	body, err := json.Marshal(map[string]any{
		"error": map[string]any{
			"message": message,
			"type":    "server_error",
			"code":    "upstream_error",
		},
	})
	if err != nil {
		return []byte(`{"error":{"message":"upstream error","type":"server_error","code":"upstream_error"}}`)
	}
	return body
}

// NormalizeError maps an upstream error response onto something HA can parse.
// Successful responses pass through untouched.
func NormalizeError(status int, body []byte, contentType string) ([]byte, string) {
	if status < 400 {
		return body, contentType
	}
	var payload any
	if err := json.Unmarshal(body, &payload); err == nil {
		if object, ok := payload.(map[string]any); ok {
			switch value := object["error"].(type) {
			case map[string]any:
				return body, "application/json"
			case string:
				return ErrorBody(value), "application/json"
			}
		}
	}
	return ErrorBody(fmt.Sprintf("Hermes upstream returned HTTP %d", status)), "application/json"
}
