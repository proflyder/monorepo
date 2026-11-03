package gateway_test

import (
	"context"
	"encoding/json"
	"net/http"
	"net/http/httptest"
	"sync"
	"testing"
	"time"

	"ricardo.local/monorepo/solutions/voice-proxy/internal/gateway"
)

type request struct {
	path   string
	header string
	body   map[string]any
}

type recorder struct {
	mu       sync.Mutex
	requests []request
}

func (r *recorder) add(item request) {
	r.mu.Lock()
	defer r.mu.Unlock()
	r.requests = append(r.requests, item)
}

func (r *recorder) all() []request {
	r.mu.Lock()
	defer r.mu.Unlock()
	return append([]request(nil), r.requests...)
}

func fakeGateway(t *testing.T, reply map[string]any, delay time.Duration, out *recorder) *httptest.Server {
	t.Helper()
	return httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		var body map[string]any
		_ = json.NewDecoder(r.Body).Decode(&body)
		out.add(request{path: r.URL.Path, header: r.Header.Get("Authorization"), body: body})
		if delay > 0 {
			select {
			case <-time.After(delay):
			case <-r.Context().Done():
				return
			}
		}
		w.Header().Set("Content-Type", "application/json")
		_ = json.NewEncoder(w).Encode(reply)
	}))
}

// TestUtterancePostsTextAndProfile pins call_utterance.
func TestUtterancePostsTextAndProfile(t *testing.T) {
	var requests recorder
	server := fakeGateway(t, map[string]any{"route": "hermes"}, 0, &requests)
	defer server.Close()
	client := gateway.New(gateway.Config{URL: server.URL, Token: func() string { return "voice-token" }, Timeout: time.Second})

	reply, err := client.Utterance(context.Background(), "проверь", "work")
	if err != nil {
		t.Fatalf("Utterance: %v", err)
	}
	if reply["route"] != "hermes" {
		t.Fatalf("reply = %#v", reply)
	}
	if len(requests.all()) != 1 {
		t.Fatalf("requests = %d", len(requests.all()))
	}
	got := requests.all()[0]
	if got.path != "/v1/voice/utterance" {
		t.Errorf("path = %q", got.path)
	}
	if got.header != "Bearer voice-token" {
		t.Errorf("Authorization = %q", got.header)
	}
	if got.body["text"] != "проверь" || got.body["profile"] != "work" {
		t.Errorf("body = %#v", got.body)
	}
}

// TestResultRaisesWaitToAtLeastOne pins call_result's max(1, wait_seconds).
func TestResultRaisesWaitToAtLeastOne(t *testing.T) {
	var requests recorder
	server := fakeGateway(t, map[string]any{"status": "running"}, 0, &requests)
	defer server.Close()
	client := gateway.New(gateway.Config{URL: server.URL, Timeout: time.Second})

	if _, err := client.Result(context.Background(), "thr_example", 7, 0); err != nil {
		t.Fatalf("Result: %v", err)
	}
	body := requests.all()[0].body
	if body["threadId"] != "thr_example" || body["afterSeq"] != float64(7) || body["waitSeconds"] != float64(1) {
		t.Fatalf("body = %#v", body)
	}
}

// TestUpdatesOmitsTheCursorOnBootstrap pins call_updates: a nil afterSeq is
// absent, not zero.
func TestUpdatesOmitsTheCursorOnBootstrap(t *testing.T) {
	var requests recorder
	server := fakeGateway(t, map[string]any{"cursor": 5}, 0, &requests)
	defer server.Close()
	client := gateway.New(gateway.Config{URL: server.URL, Timeout: time.Second})

	if _, err := client.Updates(context.Background(), nil, 5); err != nil {
		t.Fatalf("Updates: %v", err)
	}
	if _, present := requests.all()[0].body["afterSeq"]; present {
		t.Fatalf("bootstrap body has afterSeq: %#v", requests.all()[0].body)
	}
	cursor := int64(5)
	if _, err := client.Updates(context.Background(), &cursor, 5); err != nil {
		t.Fatalf("Updates: %v", err)
	}
	if requests.all()[1].body["afterSeq"] != float64(5) {
		t.Fatalf("body = %#v", requests.all()[1].body)
	}
}

// TestTimeoutsMatchPython: the utterance timeout is the gateway timeout, and a
// long-poll timeout is the gateway timeout plus the wait.
func TestTimeoutsMatchPython(t *testing.T) {
	var requests recorder
	server := fakeGateway(t, map[string]any{"status": "running"}, 300*time.Millisecond, &requests)
	defer server.Close()
	client := gateway.New(gateway.Config{URL: server.URL, Timeout: 50 * time.Millisecond})

	if _, err := client.Utterance(context.Background(), "x", "work"); err == nil {
		t.Fatal("Utterance should time out after 50 ms")
	}
	// A 1 s wait adds a second to the timeout, so the 300 ms answer arrives.
	if _, err := client.Result(context.Background(), "thr", 1, 1); err != nil {
		t.Fatalf("Result with a wait-derived timeout: %v", err)
	}
	if _, err := client.Updates(context.Background(), nil, 1); err != nil {
		t.Fatalf("Updates with a wait-derived timeout: %v", err)
	}
}
