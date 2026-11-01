// Package alertmanager is a minimal read-only Alertmanager v2 API client.
package alertmanager

import (
	"context"
	"encoding/json"
	"fmt"
	"io"
	"net/http"
	"strings"
	"time"
)

// Alert is the subset of an Alertmanager alert the gateway reports.
type Alert struct {
	// Fingerprint is Alertmanager's stable identifier for the alert.
	Fingerprint string            `json:"fingerprint"`
	Labels      map[string]string `json:"labels"`
	Annotations map[string]string `json:"annotations"`
	StartsAt    time.Time         `json:"startsAt"`
	EndsAt      time.Time         `json:"endsAt"`
	Status      struct {
		State string `json:"state"`
	} `json:"status"`
}

// Summary returns a one-line human description.
func (a Alert) Summary() string {
	if text := a.Annotations["summary"]; text != "" {
		return text
	}
	if text := a.Annotations["description"]; text != "" {
		return text
	}
	if name := a.Labels["alertname"]; name != "" {
		return name
	}
	return "alert"
}

// Severity returns the alert severity label or an empty string.
func (a Alert) Severity() string { return a.Labels["severity"] }

// Client lists alerts from Alertmanager.
type Client struct {
	baseURL string
	http    *http.Client
}

// New builds a client. baseURL is the Alertmanager root, without /api/v2.
func New(baseURL string, hc *http.Client) *Client {
	if hc == nil {
		hc = &http.Client{Timeout: 5 * time.Second}
	}
	return &Client{baseURL: strings.TrimSuffix(baseURL, "/"), http: hc}
}

// Active returns active, unsilenced, uninhibited alerts, capped at limit.
func (c *Client) Active(ctx context.Context, limit int) ([]Alert, error) {
	if limit <= 0 {
		limit = 20
	}
	endpoint := c.baseURL + "/api/v2/alerts?active=true&silenced=false&inhibited=false"
	req, err := http.NewRequestWithContext(ctx, http.MethodGet, endpoint, nil)
	if err != nil {
		return nil, err
	}
	req.Header.Set("Accept", "application/json")
	resp, err := c.http.Do(req)
	if err != nil {
		return nil, fmt.Errorf("alertmanager query: %w", err)
	}
	defer resp.Body.Close()
	body, err := io.ReadAll(io.LimitReader(resp.Body, 1<<20))
	if err != nil {
		return nil, fmt.Errorf("alertmanager response: %w", err)
	}
	if resp.StatusCode != http.StatusOK {
		return nil, fmt.Errorf("alertmanager query returned HTTP %d", resp.StatusCode)
	}
	var alerts []Alert
	if err := json.Unmarshal(body, &alerts); err != nil {
		return nil, fmt.Errorf("alertmanager response is not JSON: %w", err)
	}
	if len(alerts) > limit {
		alerts = alerts[:limit]
	}
	return alerts, nil
}
