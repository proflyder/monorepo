// Package checks executes the typed external probes declared in the service
// catalog.
//
// The package is project-agnostic: it receives catalog.Check values, a Home
// Assistant base URL and token from the composition root, and never reads
// configuration itself. Bounded timeouts, no body retention and a fixed
// result shape keep the voice path safe.
package checks

import (
	"context"
	"encoding/json"
	"fmt"
	"io"
	"net"
	"net/http"
	"strconv"
	"strings"
	"time"

	"ricardo.local/monorepo/solutions/mcp-gateway/internal/catalog"
)

// DefaultTimeout bounds one external check.
const DefaultTimeout = 3 * time.Second

// maxBody caps Home Assistant responses; /api/states can be large.
const maxBody = 4 << 20

// Result is one executed check.
type Result struct {
	Service string   `json:"service"`
	Type    string   `json:"type"`
	Label   string   `json:"label"`
	OK      bool     `json:"ok"`
	Skipped bool     `json:"skipped,omitempty"`
	Value   string   `json:"value,omitempty"`
	Error   string   `json:"error,omitempty"`
	Details []string `json:"details,omitempty"`
}

// Runner executes checks against the configured Home Assistant endpoint.
type Runner struct {
	baseURL string
	token   string
	client  *http.Client
	timeout time.Duration
}

// New builds a runner. An empty baseURL or token disables all ha_* checks;
// they are reported as skipped rather than failed.
func New(baseURL, token string, client *http.Client) *Runner {
	if client == nil {
		client = &http.Client{Timeout: DefaultTimeout}
	}
	return &Runner{
		baseURL: strings.TrimSuffix(strings.TrimSpace(baseURL), "/"),
		token:   strings.TrimSpace(token),
		client:  client,
		timeout: DefaultTimeout,
	}
}

// WithTimeout overrides the per-check timeout.
func (r *Runner) WithTimeout(d time.Duration) *Runner {
	if d > 0 {
		r.timeout = d
	}
	return r
}

// RunAll executes every declared check of every service in parallel and
// returns results in catalog order.
func (r *Runner) RunAll(ctx context.Context, services []catalog.Service) []Result {
	type task struct {
		index   int
		service string
		check   catalog.Check
	}
	var tasks []task
	for _, service := range services {
		for _, check := range service.Checks {
			tasks = append(tasks, task{index: len(tasks), service: service.ID, check: check})
		}
	}
	results := make([]Result, len(tasks))
	if len(tasks) == 0 {
		return results
	}
	done := make(chan struct{}, len(tasks))
	for _, t := range tasks {
		t := t
		go func() {
			results[t.index] = r.Run(ctx, t.service, t.check)
			done <- struct{}{}
		}()
	}
	for range tasks {
		<-done
	}
	return results
}

// Run executes one check.
func (r *Runner) Run(ctx context.Context, serviceID string, check catalog.Check) Result {
	result := Result{Service: serviceID, Type: check.Type, Label: check.DisplayLabel()}
	callCtx, cancel := context.WithTimeout(ctx, r.timeout)
	defer cancel()

	switch check.Type {
	case catalog.CheckTCP:
		r.runTCP(callCtx, check, &result)
	case catalog.CheckHTTP:
		r.runHTTP(callCtx, check, &result)
	case catalog.CheckHAState:
		r.runHAState(callCtx, check, &result)
	case catalog.CheckHATemplate:
		r.runHATemplate(callCtx, check, &result)
	case catalog.CheckHAUnavailable:
		r.runHAUnavailable(callCtx, check, &result)
	default:
		result.Error = "неизвестный тип проверки: " + check.Type
	}
	return result
}

func (r *Runner) runTCP(ctx context.Context, check catalog.Check, result *Result) {
	target := net.JoinHostPort(check.Host, strconv.Itoa(check.Port))
	dialer := net.Dialer{Timeout: r.timeout}
	conn, err := dialer.DialContext(ctx, "tcp", target)
	if err != nil {
		result.Error = err.Error()
		return
	}
	_ = conn.Close()
	result.OK = true
	result.Value = target
}

func (r *Runner) runHTTP(ctx context.Context, check catalog.Check, result *Result) {
	request, err := http.NewRequestWithContext(ctx, http.MethodGet, check.URL, nil)
	if err != nil {
		result.Error = err.Error()
		return
	}
	if check.Auth == catalog.AuthHA {
		if r.token == "" {
			result.Skipped = true
			result.OK = true
			result.Value = "Home Assistant не настроен"
			return
		}
		request.Header.Set("Authorization", "Bearer "+r.token)
	}
	resp, err := r.client.Do(request)
	if err != nil {
		result.Error = err.Error()
		return
	}
	defer resp.Body.Close()
	_, _ = io.Copy(io.Discard, io.LimitReader(resp.Body, 64<<10))
	expected := check.ExpectStatus
	if expected == 0 {
		expected = http.StatusOK
	}
	result.Value = "HTTP " + strconv.Itoa(resp.StatusCode)
	if resp.StatusCode != expected {
		result.Error = fmt.Sprintf("ожидался HTTP %d, получен %d", expected, resp.StatusCode)
		return
	}
	result.OK = true
}

type haState struct {
	EntityID   string `json:"entity_id"`
	State      string `json:"state"`
	Attributes struct {
		FriendlyName string `json:"friendly_name"`
	} `json:"attributes"`
}

func (r *Runner) requireHA(result *Result) bool {
	if r.baseURL == "" || r.token == "" {
		result.Skipped = true
		result.OK = true
		result.Value = "Home Assistant не настроен"
		return false
	}
	return true
}

func (r *Runner) runHAState(ctx context.Context, check catalog.Check, result *Result) {
	if !r.requireHA(result) {
		return
	}
	var state haState
	if err := r.getJSON(ctx, "/api/states/"+check.EntityID, &state); err != nil {
		result.Error = err.Error()
		return
	}
	bad := check.BadStates
	if len(bad) == 0 {
		bad = []string{"unavailable", "unknown"}
	}
	result.Value = state.State
	for _, candidate := range bad {
		if strings.EqualFold(strings.TrimSpace(candidate), state.State) {
			result.Error = "состояние " + state.State
			return
		}
	}
	result.OK = true
}

func (r *Runner) runHATemplate(ctx context.Context, check catalog.Check, result *Result) {
	if !r.requireHA(result) {
		return
	}
	body, err := r.postTemplate(ctx, check.Template)
	if err != nil {
		result.Error = err.Error()
		return
	}
	value := strings.TrimSpace(body)
	if len(value) > 200 {
		value = value[:200] + "…"
	}
	result.Value = value
	if value == "" {
		if check.Required {
			result.Error = "шаблон вернул пустое значение"
			return
		}
		result.OK = true
		result.Value = "нет данных"
		return
	}
	result.OK = true
}

func (r *Runner) runHAUnavailable(ctx context.Context, check catalog.Check, result *Result) {
	if !r.requireHA(result) {
		return
	}
	var states []haState
	if err := r.getJSON(ctx, "/api/states", &states); err != nil {
		result.Error = err.Error()
		return
	}
	domains := make(map[string]bool, len(check.Domains))
	for _, domain := range check.Domains {
		domains[strings.ToLower(strings.TrimSpace(domain))] = true
	}
	count := 0
	for _, state := range states {
		if len(domains) > 0 {
			domain, _, _ := strings.Cut(state.EntityID, ".")
			if !domains[strings.ToLower(domain)] {
				continue
			}
		}
		if state.State != "unavailable" {
			continue
		}
		count++
		if len(result.Details) < 10 {
			// Prefer the friendly name, but keep the entity id so the report is
			// actionable; fall back to the id alone when HA has no name.
			entity := state.EntityID
			if name := strings.TrimSpace(state.Attributes.FriendlyName); name != "" {
				entity = name + " (" + state.EntityID + ")"
			}
			result.Details = append(result.Details, entity)
		}
	}
	result.Value = strconv.Itoa(count)
	if count > 0 {
		result.Error = fmt.Sprintf("недоступно сущностей: %d", count)
		return
	}
	result.OK = true
}

func (r *Runner) getJSON(ctx context.Context, path string, out any) error {
	request, err := http.NewRequestWithContext(ctx, http.MethodGet, r.baseURL+path, nil)
	if err != nil {
		return err
	}
	request.Header.Set("Authorization", "Bearer "+r.token)
	request.Header.Set("Accept", "application/json")
	resp, err := r.client.Do(request)
	if err != nil {
		return err
	}
	defer resp.Body.Close()
	body, err := io.ReadAll(io.LimitReader(resp.Body, maxBody))
	if err != nil {
		return err
	}
	if resp.StatusCode != http.StatusOK {
		return fmt.Errorf("Home Assistant %s вернул HTTP %d", path, resp.StatusCode)
	}
	if err := json.Unmarshal(body, out); err != nil {
		return fmt.Errorf("ответ Home Assistant не JSON: %w", err)
	}
	return nil
}

func (r *Runner) postTemplate(ctx context.Context, template string) (string, error) {
	payload, err := json.Marshal(map[string]string{"template": template})
	if err != nil {
		return "", err
	}
	request, err := http.NewRequestWithContext(ctx, http.MethodPost, r.baseURL+"/api/template", strings.NewReader(string(payload)))
	if err != nil {
		return "", err
	}
	request.Header.Set("Authorization", "Bearer "+r.token)
	request.Header.Set("Content-Type", "application/json")
	resp, err := r.client.Do(request)
	if err != nil {
		return "", err
	}
	defer resp.Body.Close()
	body, err := io.ReadAll(io.LimitReader(resp.Body, 64<<10))
	if err != nil {
		return "", err
	}
	if resp.StatusCode != http.StatusOK {
		return "", fmt.Errorf("Home Assistant /api/template вернул HTTP %d", resp.StatusCode)
	}
	text := strings.TrimSpace(string(body))
	// The endpoint normally returns text/plain, but accept a JSON string too.
	var quoted string
	if json.Unmarshal(body, &quoted) == nil {
		text = quoted
	}
	return text, nil
}
