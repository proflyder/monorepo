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

const (
	statesPath          = "/api/states"
	maxStatesBodyBytes  = 8 << 20
	serviceCallBasePath = "/api/services/"
)

type serviceCallRequest struct {
	EntityID string `json:"entity_id"`
}

// CallService invokes <domain>.<service> on one entity. The control executor
// resolves the call from the catalog; no caller-supplied service is accepted.
func (c *Client) CallService(ctx context.Context, domain, service, entity string, data map[string]any) error {
	const op = "call service"
	if domain == "" || service == "" || entity == "" {
		return &httpapi.Error{Op: op, Err: errors.New("empty service call")}
	}
	body := map[string]any{}
	for k, v := range data {
		body[k] = v
	}
	body["entity_id"] = entity
	payload, err := json.Marshal(body)
	if err != nil {
		return &httpapi.Error{Op: op, Err: fmt.Errorf("encode request: %w", err)}
	}
	path := serviceCallBasePath + url.PathEscape(domain) + "/" + url.PathEscape(service)
	req, err := http.NewRequestWithContext(ctx, http.MethodPost, c.baseURL+path, bytes.NewReader(payload))
	if err != nil {
		return &httpapi.Error{Op: op, Err: errors.New("build request")}
	}
	req.Header.Set("Content-Type", "application/json")
	req.Header.Set("Authorization", "Bearer "+c.token)
	return c.send(req, op)
}

// Entity is one Home Assistant entity as discovered from /api/states.
type Entity struct {
	ID    string
	State string
	// Name is attributes.friendly_name ("" when absent).
	Name string
	// Icon is attributes.icon with any "mdi:" prefix removed ("" when absent).
	Icon string
	// Light attributes (zero values for other domains). Brightness is -1 when absent.
	ColorModes      []string
	MinKelvin       int
	MaxKelvin       int
	Brightness      int
	Kelvin          int
	Hue, Saturation float64
	HasHS           bool
	ColorMode       string
}

// Entities reads every entity with its state, friendly name and icon. Other
// attributes are discarded.
func (c *Client) Entities(ctx context.Context) ([]Entity, error) {
	const op = "read entities"
	req, err := http.NewRequestWithContext(ctx, http.MethodGet, c.baseURL+statesPath, nil)
	if err != nil {
		return nil, &httpapi.Error{Op: op, Err: errors.New("build request")}
	}
	req.Header.Set("Authorization", "Bearer "+c.token)
	resp, err := c.http.Do(req)
	if err != nil {
		return nil, &httpapi.Error{Op: op, Err: networkError(err, c.token)}
	}
	defer func() { _ = resp.Body.Close() }()
	if resp.StatusCode < 200 || resp.StatusCode >= 300 {
		_, _ = io.Copy(io.Discard, io.LimitReader(resp.Body, maxResponseBytes))
		return nil, &httpapi.Error{Op: op, StatusCode: resp.StatusCode, Message: httpapi.Sanitize(resp.Status, c.token)}
	}
	var states []struct {
		EntityID   string `json:"entity_id"`
		State      string `json:"state"`
		Attributes struct {
			FriendlyName string    `json:"friendly_name"`
			Icon         string    `json:"icon"`
			ColorModes   []string  `json:"supported_color_modes"`
			MinKelvin    *float64  `json:"min_color_temp_kelvin"`
			MaxKelvin    *float64  `json:"max_color_temp_kelvin"`
			Brightness   *float64  `json:"brightness"`
			Kelvin       *float64  `json:"color_temp_kelvin"`
			HS           []float64 `json:"hs_color"`
			ColorMode    string    `json:"color_mode"`
		} `json:"attributes"`
	}
	if err := json.NewDecoder(io.LimitReader(resp.Body, maxStatesBodyBytes)).Decode(&states); err != nil {
		return nil, &httpapi.Error{Op: op, Err: errors.New("decode states")}
	}
	out := make([]Entity, 0, len(states))
	for _, s := range states {
		a := s.Attributes
		e := Entity{
			ID:         s.EntityID,
			State:      s.State,
			Name:       a.FriendlyName,
			Icon:       strings.TrimPrefix(a.Icon, "mdi:"),
			ColorModes: a.ColorModes,
			MinKelvin:  intOf(a.MinKelvin),
			MaxKelvin:  intOf(a.MaxKelvin),
			Kelvin:     intOf(a.Kelvin),
			Brightness: -1,
			ColorMode:  a.ColorMode,
		}
		if a.Brightness != nil {
			e.Brightness = intOf(a.Brightness)
		}
		if len(a.HS) == 2 {
			e.Hue, e.Saturation, e.HasHS = a.HS[0], a.HS[1], true
		}
		out = append(out, e)
	}
	return out, nil
}

type templateRequest struct {
	Template string `json:"template"`
}

// RenderTemplate renders a Home Assistant template and returns the text. It is
// read-only discovery (areas, labels, hidden flags); the template is always
// built by this program, never taken from a client.
func (c *Client) RenderTemplate(ctx context.Context, template string) (string, error) {
	const op = "render template"
	payload, err := json.Marshal(templateRequest{Template: template})
	if err != nil {
		return "", &httpapi.Error{Op: op, Err: fmt.Errorf("encode request: %w", err)}
	}
	req, err := http.NewRequestWithContext(ctx, http.MethodPost, c.baseURL+"/api/template", bytes.NewReader(payload))
	if err != nil {
		return "", &httpapi.Error{Op: op, Err: errors.New("build request")}
	}
	req.Header.Set("Content-Type", "application/json")
	req.Header.Set("Authorization", "Bearer "+c.token)
	resp, err := c.http.Do(req)
	if err != nil {
		return "", &httpapi.Error{Op: op, Err: networkError(err, c.token)}
	}
	defer func() { _ = resp.Body.Close() }()
	if resp.StatusCode < 200 || resp.StatusCode >= 300 {
		_, _ = io.Copy(io.Discard, io.LimitReader(resp.Body, maxResponseBytes))
		return "", &httpapi.Error{Op: op, StatusCode: resp.StatusCode, Message: httpapi.Sanitize(resp.Status, c.token)}
	}
	raw, err := io.ReadAll(io.LimitReader(resp.Body, maxStatesBodyBytes))
	if err != nil {
		return "", &httpapi.Error{Op: op, Err: errors.New("read response")}
	}
	return string(raw), nil
}

func intOf(v *float64) int {
	if v == nil {
		return 0
	}
	return int(*v)
}
