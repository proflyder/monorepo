// Package loki is a minimal read-only Loki query client.
package loki

import (
	"context"
	"encoding/json"
	"fmt"
	"io"
	"net/http"
	"net/url"
	"sort"
	"strconv"
	"strings"
	"time"
)

// Line is one log line with its stream labels.
type Line struct {
	Timestamp time.Time         `json:"timestamp"`
	Labels    map[string]string `json:"labels,omitempty"`
	Text      string            `json:"text"`
}

// Client queries the Loki HTTP API.
type Client struct {
	baseURL string
	http    *http.Client
}

// New builds a client. baseURL is the Loki root, for example
// http://loki.example:3100.
func New(baseURL string, hc *http.Client) *Client {
	if hc == nil {
		hc = &http.Client{Timeout: 5 * time.Second}
	}
	return &Client{baseURL: strings.TrimSuffix(baseURL, "/"), http: hc}
}

type queryResponse struct {
	Status string `json:"status"`
	Error  string `json:"error"`
	Data   struct {
		ResultType string `json:"resultType"`
		Result     []struct {
			Stream map[string]string `json:"stream"`
			Values [][]string        `json:"values"`
		} `json:"result"`
	} `json:"data"`
}

// Query runs a range query for the last `since` and returns the newest lines
// first, capped at limit.
func (c *Client) Query(ctx context.Context, selector string, since time.Duration, limit int) ([]Line, error) {
	if strings.TrimSpace(selector) == "" {
		return nil, fmt.Errorf("loki selector is required")
	}
	if since <= 0 {
		since = 15 * time.Minute
	}
	if limit <= 0 {
		limit = 50
	}
	end := time.Now()
	start := end.Add(-since)
	params := url.Values{
		"query":     {selector},
		"start":     {strconv.FormatInt(start.UnixNano(), 10)},
		"end":       {strconv.FormatInt(end.UnixNano(), 10)},
		"limit":     {strconv.Itoa(limit)},
		"direction": {"backward"},
	}
	endpoint := c.baseURL + "/loki/api/v1/query_range?" + params.Encode()
	req, err := http.NewRequestWithContext(ctx, http.MethodGet, endpoint, nil)
	if err != nil {
		return nil, err
	}
	req.Header.Set("Accept", "application/json")
	resp, err := c.http.Do(req)
	if err != nil {
		return nil, fmt.Errorf("loki query: %w", err)
	}
	defer resp.Body.Close()
	body, err := io.ReadAll(io.LimitReader(resp.Body, 2<<20))
	if err != nil {
		return nil, fmt.Errorf("loki response: %w", err)
	}
	if resp.StatusCode != http.StatusOK {
		return nil, fmt.Errorf("loki query returned HTTP %d", resp.StatusCode)
	}
	var decoded queryResponse
	if err := json.Unmarshal(body, &decoded); err != nil {
		return nil, fmt.Errorf("loki response is not JSON: %w", err)
	}
	if decoded.Status != "success" {
		return nil, fmt.Errorf("loki query failed: %s", decoded.Error)
	}
	var lines []Line
	for _, stream := range decoded.Data.Result {
		for _, value := range stream.Values {
			if len(value) != 2 {
				continue
			}
			nanos, err := strconv.ParseInt(value[0], 10, 64)
			if err != nil {
				continue
			}
			lines = append(lines, Line{
				Timestamp: time.Unix(0, nanos).UTC(),
				Labels:    stream.Stream,
				Text:      value[1],
			})
		}
	}
	sort.SliceStable(lines, func(i, j int) bool { return lines[i].Timestamp.After(lines[j].Timestamp) })
	if len(lines) > limit {
		lines = lines[:limit]
	}
	return lines, nil
}
