// Package prometheus is a minimal read-only Prometheus HTTP API client.
//
// It implements only the instant query endpoint the gateway needs, which keeps
// the solution free of the Prometheus Go module tree.
package prometheus

import (
	"context"
	"encoding/json"
	"fmt"
	"io"
	"net/http"
	"net/url"
	"strconv"
	"strings"
	"time"
)

// Sample is one series returned by an instant query.
type Sample struct {
	Labels map[string]string `json:"labels,omitempty"`
	Value  float64           `json:"value"`
	Raw    string            `json:"raw"`
}

// Client queries a Prometheus-compatible API.
type Client struct {
	baseURL string
	http    *http.Client
}

// New builds a client. baseURL is the API root, for example
// http://prometheus.example:9090.
func New(baseURL string, hc *http.Client) *Client {
	if hc == nil {
		hc = &http.Client{Timeout: 5 * time.Second}
	}
	return &Client{baseURL: strings.TrimSuffix(baseURL, "/"), http: hc}
}

type apiResponse struct {
	Status    string `json:"status"`
	ErrorType string `json:"errorType"`
	Error     string `json:"error"`
	Data      struct {
		ResultType string `json:"resultType"`
		Result     []struct {
			Metric map[string]string `json:"metric"`
			Value  []any             `json:"value"`
		} `json:"result"`
	} `json:"data"`
}

// Query runs an instant query and returns at most limit samples.
func (c *Client) Query(ctx context.Context, promql string, limit int) ([]Sample, error) {
	if strings.TrimSpace(promql) == "" {
		return nil, fmt.Errorf("promql is required")
	}
	if limit <= 0 {
		limit = 20
	}
	endpoint := c.baseURL + "/api/v1/query?" + url.Values{"query": {promql}}.Encode()
	req, err := http.NewRequestWithContext(ctx, http.MethodGet, endpoint, nil)
	if err != nil {
		return nil, err
	}
	req.Header.Set("Accept", "application/json")
	resp, err := c.http.Do(req)
	if err != nil {
		return nil, fmt.Errorf("prometheus query: %w", err)
	}
	defer resp.Body.Close()
	body, err := io.ReadAll(io.LimitReader(resp.Body, 1<<20))
	if err != nil {
		return nil, fmt.Errorf("prometheus response: %w", err)
	}
	if resp.StatusCode != http.StatusOK {
		return nil, fmt.Errorf("prometheus query returned HTTP %d", resp.StatusCode)
	}
	var decoded apiResponse
	if err := json.Unmarshal(body, &decoded); err != nil {
		return nil, fmt.Errorf("prometheus response is not JSON: %w", err)
	}
	if decoded.Status != "success" {
		return nil, fmt.Errorf("prometheus query failed: %s %s", decoded.ErrorType, decoded.Error)
	}
	samples := make([]Sample, 0, len(decoded.Data.Result))
	for _, item := range decoded.Data.Result {
		if len(samples) >= limit {
			break
		}
		sample := Sample{Labels: item.Metric}
		if len(item.Value) == 2 {
			sample.Raw, _ = item.Value[1].(string)
			sample.Value, _ = strconv.ParseFloat(sample.Raw, 64)
		}
		samples = append(samples, sample)
	}
	return samples, nil
}
