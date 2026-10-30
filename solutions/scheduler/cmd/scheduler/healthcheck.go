// Healthcheck for the scheduler container.
//
// The probe must prove that the authenticated API answers and that the tick
// loop is healthy. It must not put the bearer token on a command line: the
// token is read from SCHEDULER_TOKEN_FILE by the normal config loader and only
// lives in this process's memory. The probe therefore runs the binary itself
// as `scheduler -healthcheck`, with no secret arguments.
package main

import (
	"bytes"
	"context"
	"encoding/json"
	"fmt"
	"io"
	"net"
	"net/http"
	"os"
	"strings"
	"time"

	"ricardo.local/monorepo/solutions/scheduler/internal/config"
)

// healthcheckTimeout bounds a single probe. Kubernetes gives the probe a
// slightly larger timeout; keeping the client lower avoids overlapping calls.
const healthcheckTimeout = 4 * time.Second

type statusResponse struct {
	Ready         bool   `json:"ready"`
	LastTickError string `json:"lastTickError"`
}

// runHealthcheck returns a process exit code.
func runHealthcheck() int {
	cfg, err := config.Load()
	if err != nil {
		fmt.Fprintln(os.Stderr, "healthcheck: configuration:", err)
		return 1
	}
	addr, err := healthcheckAddress(cfg.Listen)
	if err != nil {
		fmt.Fprintln(os.Stderr, "healthcheck:", err)
		return 1
	}
	ctx, cancel := context.WithTimeout(context.Background(), healthcheckTimeout)
	defer cancel()
	if err := checkReady(ctx, "http://"+addr, cfg.Token, http.DefaultClient); err != nil {
		fmt.Fprintln(os.Stderr, "healthcheck:", err)
		return 1
	}
	return 0
}

// runStatus prints the raw authenticated GetStatus response.
//
// The container image is distroless: there is no shell, curl or wget to query
// the API from a deploy script. `scheduler -status` reads the bearer from
// SCHEDULER_TOKEN_FILE exactly like the healthcheck and writes the JSON body to
// stdout, so operators and CI never need a shell inside the pod.
func runStatus(stdout, stderr io.Writer) int {
	cfg, err := config.Load()
	if err != nil {
		fmt.Fprintln(stderr, "status: configuration:", err)
		return 1
	}
	addr, err := healthcheckAddress(cfg.Listen)
	if err != nil {
		fmt.Fprintln(stderr, "status:", err)
		return 1
	}
	ctx, cancel := context.WithTimeout(context.Background(), healthcheckTimeout)
	defer cancel()
	body, err := getStatusBody(ctx, "http://"+addr, cfg.Token, http.DefaultClient)
	if err != nil {
		fmt.Fprintln(stderr, "status:", err)
		return 1
	}
	if _, err := stdout.Write(body); err != nil {
		return 1
	}
	return 0
}

// getStatusBody performs the authenticated GetStatus call and returns the body.
func getStatusBody(ctx context.Context, baseURL, token string, hc *http.Client) ([]byte, error) {
	payload, err := json.Marshal(struct{}{})
	if err != nil {
		return nil, err
	}
	req, err := http.NewRequestWithContext(
		ctx,
		http.MethodPost,
		strings.TrimSuffix(baseURL, "/")+"/ricardo.v1.SchedulerService/GetStatus",
		bytes.NewReader(payload),
	)
	if err != nil {
		return nil, err
	}
	req.Header.Set("Content-Type", "application/json")
	req.Header.Set("Authorization", "Bearer "+token)

	resp, err := hc.Do(req)
	if err != nil {
		return nil, fmt.Errorf("GetStatus request failed: %w", err)
	}
	defer resp.Body.Close()
	body, err := io.ReadAll(io.LimitReader(resp.Body, 64*1024))
	if err != nil {
		return nil, fmt.Errorf("GetStatus response: %w", err)
	}
	if resp.StatusCode != http.StatusOK {
		return nil, fmt.Errorf("GetStatus returned HTTP %d", resp.StatusCode)
	}
	return body, nil
}

// healthcheckAddress turns the configured listen address into a loopback
// address. An empty host (all interfaces) becomes 127.0.0.1.
func healthcheckAddress(listen string) (string, error) {
	host, port, err := net.SplitHostPort(listen)
	if err != nil {
		return "", fmt.Errorf("invalid listen address: %w", err)
	}
	if host == "" || host == "0.0.0.0" || host == "::" {
		host = "127.0.0.1"
	}
	return net.JoinHostPort(host, port), nil
}

// checkReady performs the authenticated GetStatus call and evaluates readiness.
func checkReady(ctx context.Context, baseURL, token string, hc *http.Client) error {
	payload, err := json.Marshal(struct{}{})
	if err != nil {
		return err
	}
	req, err := http.NewRequestWithContext(
		ctx,
		http.MethodPost,
		strings.TrimSuffix(baseURL, "/")+"/ricardo.v1.SchedulerService/GetStatus",
		bytes.NewReader(payload),
	)
	if err != nil {
		return err
	}
	req.Header.Set("Content-Type", "application/json")
	req.Header.Set("Authorization", "Bearer "+token)

	resp, err := hc.Do(req)
	if err != nil {
		return fmt.Errorf("GetStatus request failed: %w", err)
	}
	defer resp.Body.Close()
	body, err := io.ReadAll(io.LimitReader(resp.Body, 64*1024))
	if err != nil {
		return fmt.Errorf("GetStatus response: %w", err)
	}
	if resp.StatusCode != http.StatusOK {
		return fmt.Errorf("GetStatus returned HTTP %d", resp.StatusCode)
	}
	var status statusResponse
	if err := json.Unmarshal(body, &status); err != nil {
		return fmt.Errorf("GetStatus response is not JSON: %w", err)
	}
	if !status.Ready {
		reason := status.LastTickError
		if reason == "" {
			reason = "tick not confirmed"
		}
		return fmt.Errorf("scheduler is not ready: %s", reason)
	}
	return nil
}
