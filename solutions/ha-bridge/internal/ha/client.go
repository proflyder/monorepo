// Package ha is a narrow Home Assistant REST client: it can call
// /api/services/script/turn_on and probe /api/. It deliberately has no
// schedule, entity-discovery or MCP surface. The scheduler owns schedule state;
// Home Assistant only performs the side effect.
package ha

import (
	"bytes"
	"context"
	"encoding/json"
	"errors"
	"fmt"
	"io"
	"net/http"
	"net/url"
	"strings"

	"ricardo.local/monorepo/solutions/ha-bridge/internal/httpapi"
)

const (
	scriptTurnOnPath = "/api/services/script/turn_on"
	pingPath         = "/api/"
	maxResponseBytes = 64 << 10
)

// Client calls script entities on Home Assistant.
type Client struct {
	baseURL string
	token   string
	http    *http.Client
}

// New returns a Home Assistant client. baseURL must not contain credentials.
func New(baseURL, token string, hc *http.Client) *Client {
	if hc == nil {
		hc = &http.Client{}
	}
	return &Client{
		baseURL: strings.TrimSuffix(strings.TrimSpace(baseURL), "/"),
		token:   token,
		http:    httpapi.WithoutRedirects(hc),
	}
}

// EntityID normalizes a configured script name into a Home Assistant entity id.
func EntityID(script string) string {
	script = strings.TrimSpace(script)
	if strings.Contains(script, ".") {
		return script
	}
	return "script." + script
}

type turnOnRequest struct {
	EntityID  string         `json:"entity_id"`
	Variables map[string]any `json:"variables,omitempty"`
}

// CallScript runs a script entity with variables. A 2xx response means Home
// Assistant accepted the service call, which is the only acknowledgement the
// bridge reports downstream. Non-2xx responses are returned as *httpapi.Error
// classified as transient or permanent.
func (c *Client) CallScript(ctx context.Context, script string, variables map[string]any) error {
	entity := EntityID(script)
	if entity == "" || entity == "script." {
		return &httpapi.Error{Op: "call script", Err: errors.New("empty script entity")}
	}
	payload, err := json.Marshal(turnOnRequest{EntityID: entity, Variables: variables})
	if err != nil {
		return &httpapi.Error{Op: "call script", Err: fmt.Errorf("encode request: %w", err)}
	}

	req, err := http.NewRequestWithContext(ctx, http.MethodPost, c.baseURL+scriptTurnOnPath, bytes.NewReader(payload))
	if err != nil {
		return &httpapi.Error{Op: "call script", Err: errors.New("build request")}
	}
	req.Header.Set("Content-Type", "application/json")
	req.Header.Set("Authorization", "Bearer "+c.token)
	return c.send(req, "call script")
}

// Ping verifies HTTP reachability and the token with the read-only /api/
// endpoint. Dry-run uses it only when an HA token is configured.
func (c *Client) Ping(ctx context.Context) error {
	req, err := http.NewRequestWithContext(ctx, http.MethodGet, c.baseURL+pingPath, nil)
	if err != nil {
		return &httpapi.Error{Op: "ping HA", Err: errors.New("build request")}
	}
	req.Header.Set("Authorization", "Bearer "+c.token)
	return c.send(req, "ping HA")
}

func (c *Client) send(req *http.Request, op string) error {
	resp, err := c.http.Do(req)
	if err != nil {
		return &httpapi.Error{Op: op, Err: networkError(err, c.token)}
	}
	defer func() { _ = resp.Body.Close() }()

	// The body is drained and discarded; it is never logged because it can
	// contain entity attributes or other deployment detail.
	_, _ = io.Copy(io.Discard, io.LimitReader(resp.Body, maxResponseBytes))

	if resp.StatusCode < 200 || resp.StatusCode >= 300 {
		return &httpapi.Error{
			Op:         op,
			StatusCode: resp.StatusCode,
			Message:    httpapi.Sanitize(resp.Status, c.token),
		}
	}
	return nil
}

func networkError(err error, token string) error {
	var urlErr *url.Error
	if errors.As(err, &urlErr) && urlErr.Err != nil {
		return errors.New(httpapi.Sanitize(urlErr.Err.Error(), token))
	}
	return errors.New(httpapi.Sanitize(err.Error(), token))
}
