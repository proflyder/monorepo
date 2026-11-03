package hermes_test

import (
	"context"
	"encoding/json"
	"io"
	"net/http"
	"net/http/httptest"
	"strings"
	"testing"
	"time"

	"ricardo.local/monorepo/solutions/voice-proxy/internal/hermes"
)

// TestHTTPErrorsNormalizeLegacyStringOrHTMLAndKeepValidError mirrors
// test_http_errors_normalize_legacy_string_or_html_and_keep_valid_error.
func TestHTTPErrorsNormalizeLegacyStringOrHTMLAndKeepValidError(t *testing.T) {
	for _, original := range [][]byte{
		[]byte(`{"error": "not ready"}`),
		[]byte(`<html>not ready</html>`),
		[]byte(`{"error": null}`),
	} {
		body, contentType := hermes.NormalizeError(502, original, "text/html")
		if contentType != "application/json" {
			t.Errorf("content type = %q, want application/json", contentType)
		}
		var payload map[string]any
		if err := json.Unmarshal(body, &payload); err != nil {
			t.Fatalf("body is not JSON: %v (%q)", err, body)
		}
		errorObject, ok := payload["error"].(map[string]any)
		if !ok {
			t.Fatalf("error is not an object: %v", payload["error"])
		}
		if message, _ := errorObject["message"].(string); message == "" {
			t.Errorf("error message is empty: %q", body)
		}
	}

	valid := []byte(`{"error":{"message":"limited","type":"rate_limit_error"}}`)
	body, contentType := hermes.NormalizeError(429, valid, "application/json")
	if string(body) != string(valid) || contentType != "application/json" {
		t.Fatalf("valid error was rewritten: %q, %q", body, contentType)
	}

	// A successful response is relayed untouched.
	okBody, okType := hermes.NormalizeError(200, []byte(`{"data":[]}`), "application/json")
	if string(okBody) != `{"data":[]}` || okType != "application/json" {
		t.Fatalf("success response was rewritten: %q, %q", okBody, okType)
	}
}

// TestErrorBodyIsAnObjectForHAParser pins the shape HA's llama_cpp adapter
// calls .get on.
func TestErrorBodyIsAnObjectForHAParser(t *testing.T) {
	var payload map[string]any
	if err := json.Unmarshal(hermes.ErrorBody("boom"), &payload); err != nil {
		t.Fatalf("decode: %v", err)
	}
	errorObject, ok := payload["error"].(map[string]any)
	if !ok {
		t.Fatalf("error is not an object: %v", payload["error"])
	}
	if errorObject["message"] != "boom" || errorObject["type"] != "server_error" ||
		errorObject["code"] != "upstream_error" {
		t.Fatalf("error = %#v", errorObject)
	}
}

func TestClientForwardsHeadersAndRelaysStatus(t *testing.T) {
	var got http.Header
	upstream := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		got = r.Header.Clone()
		w.Header().Set("Content-Type", "text/plain")
		w.WriteHeader(http.StatusTeapot)
		_, _ = w.Write([]byte("short and stout"))
	}))
	defer upstream.Close()

	client := hermes.New(upstream.URL+"/", time.Second)
	response, err := client.Do(context.Background(), http.MethodPost, "/v1/x", []byte(`{"a":1}`), nil)
	if err != nil {
		t.Fatalf("Do: %v", err)
	}
	if response.Status != http.StatusTeapot {
		t.Errorf("status = %d", response.Status)
	}
	if string(response.Data) != "short and stout" {
		t.Errorf("body = %q", response.Data)
	}
	if response.ContentType != "text/plain" {
		t.Errorf("content type = %q", response.ContentType)
	}
	if got.Get("Content-Type") != "application/json" {
		t.Errorf("Content-Type = %q, want the Python default", got.Get("Content-Type"))
	}
	if got.Get("Accept") != "application/json" {
		t.Errorf("Accept = %q, want the Python default", got.Get("Accept"))
	}
}

func TestReadBodyEnforcesTheCap(t *testing.T) {
	small := httptest.NewRequest(http.MethodPost, "/", strings.NewReader("abc"))
	if data, err := hermes.ReadBody(small, hermes.MaxBody); err != nil || string(data) != "abc" {
		t.Fatalf("small body = %q, %v", data, err)
	}

	over := httptest.NewRequest(http.MethodPost, "/", strings.NewReader("abcd"))
	if _, err := hermes.ReadBody(over, 3); err != hermes.ErrBodyTooLarge {
		t.Fatalf("err = %v, want ErrBodyTooLarge", err)
	}

	// A declared length over the cap is rejected before reading.
	declared := httptest.NewRequest(http.MethodPost, "/", strings.NewReader(""))
	declared.ContentLength = hermes.MaxBody + 1
	if _, err := hermes.ReadBody(declared, hermes.MaxBody); err != hermes.ErrBodyTooLarge {
		t.Fatalf("declared err = %v, want ErrBodyTooLarge", err)
	}
}

// TestStreamIdleTimeoutFailsAStalledBody pins the per-read timeout: a stream
// that stops producing frames is cancelled like a Python socket timeout.
func TestStreamIdleTimeoutFailsAStalledBody(t *testing.T) {
	upstream := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		w.Header().Set("Content-Type", "text/event-stream")
		w.WriteHeader(http.StatusOK)
		w.(http.Flusher).Flush()
		time.Sleep(500 * time.Millisecond)
	}))
	defer upstream.Close()

	client := hermes.New(upstream.URL, 100*time.Millisecond)
	resp, err := client.Stream(context.Background(), http.MethodGet, "/stream", nil, nil)
	if err != nil {
		t.Fatalf("Stream: %v", err)
	}
	defer func() { _ = resp.Body.Close() }()
	if _, err := io.ReadAll(resp.Body); err == nil {
		t.Fatal("a stalled stream was not cancelled")
	}
}

// TestStreamLongTurnKeepsGoing pins the other half: activity resets the
// timeout, so a turn longer than 180 s is not cut just for being long.
func TestStreamLongTurnKeepsGoing(t *testing.T) {
	const chunks = 12
	upstream := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		w.Header().Set("Content-Type", "text/event-stream")
		w.WriteHeader(http.StatusOK)
		flusher := w.(http.Flusher)
		for range chunks {
			_, _ = io.WriteString(w, "data: x\n\n")
			flusher.Flush()
			time.Sleep(30 * time.Millisecond)
		}
	}))
	defer upstream.Close()

	client := hermes.New(upstream.URL, 100*time.Millisecond)
	resp, err := client.Stream(context.Background(), http.MethodGet, "/stream", nil, nil)
	if err != nil {
		t.Fatalf("Stream: %v", err)
	}
	defer func() { _ = resp.Body.Close() }()
	data, err := io.ReadAll(resp.Body)
	if err != nil {
		t.Fatalf("a stream with steady traffic was cut: %v", err)
	}
	if strings.Count(string(data), "data: x") != chunks {
		t.Fatalf("received %q", data)
	}
}
