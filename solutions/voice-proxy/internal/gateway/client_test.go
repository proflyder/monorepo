package gateway_test

import (
	"context"
	"testing"
	"time"

	"github.com/proflyder/monorepo/solutions/voice-proxy/internal/gateway"
	"github.com/proflyder/monorepo/solutions/voice-proxy/internal/testfake"
)

// TestUtterancePostsTextAndProfile: the utterance route carries the text, the
// profile and the bearer token.
func TestUtterancePostsTextAndProfile(t *testing.T) {
	fake := testfake.NewGateway()
	fake.SetUtterance(map[string]any{"route": "hermes"})
	server := testfake.Serve(t, fake)
	client := gateway.New(gateway.Config{URL: server.URL, Token: func() string { return "voice-token" }, Timeout: time.Second})

	reply, err := client.Utterance(context.Background(), "проверь", "work")
	if err != nil {
		t.Fatalf("Utterance: %v", err)
	}
	if reply["route"] != "hermes" {
		t.Fatalf("reply = %#v", reply)
	}
	calls := fake.Calls()
	if len(calls) != 1 {
		t.Fatalf("requests = %d", len(calls))
	}
	if calls[0].Path != "/v1/voice/utterance" {
		t.Errorf("path = %q", calls[0].Path)
	}
	if got := calls[0].Header.Get("Authorization"); got != "Bearer voice-token" {
		t.Errorf("Authorization = %q", got)
	}
	if calls[0].Body["text"] != "проверь" || calls[0].Body["profile"] != "work" {
		t.Errorf("body = %#v", calls[0].Body)
	}
}

// TestResultRaisesWaitToAtLeastOne pins max(1, wait_seconds).
func TestResultRaisesWaitToAtLeastOne(t *testing.T) {
	fake := testfake.NewGateway()
	server := testfake.Serve(t, fake)
	client := gateway.New(gateway.Config{URL: server.URL, Timeout: time.Second})

	if _, err := client.Result(context.Background(), "thr_example", 7, 0); err != nil {
		t.Fatalf("Result: %v", err)
	}
	body := fake.Calls()[0].Body
	if body["threadId"] != "thr_example" || body["afterSeq"] != float64(7) || body["waitSeconds"] != float64(1) {
		t.Fatalf("body = %#v", body)
	}
}

// TestUpdatesOmitsTheCursorOnBootstrap: a nil afterSeq is absent, not zero.
func TestUpdatesOmitsTheCursorOnBootstrap(t *testing.T) {
	fake := testfake.NewGateway()
	fake.SetUpdates(map[string]any{"cursor": 5})
	server := testfake.Serve(t, fake)
	client := gateway.New(gateway.Config{URL: server.URL, Timeout: time.Second})

	if _, err := client.Updates(context.Background(), nil, 5); err != nil {
		t.Fatalf("Updates: %v", err)
	}
	if _, present := fake.Calls()[0].Body["afterSeq"]; present {
		t.Fatalf("bootstrap body has afterSeq: %#v", fake.Calls()[0].Body)
	}
	cursor := int64(5)
	if _, err := client.Updates(context.Background(), &cursor, 5); err != nil {
		t.Fatalf("Updates: %v", err)
	}
	if fake.Calls()[1].Body["afterSeq"] != float64(5) {
		t.Fatalf("body = %#v", fake.Calls()[1].Body)
	}
}

// TestTimeoutsMatchTheRequestContract: the utterance timeout is the gateway
// timeout, and a long-poll timeout is the gateway timeout plus the wait.
func TestTimeoutsMatchTheRequestContract(t *testing.T) {
	fake := testfake.NewGateway()
	fake.SetDelay(300 * time.Millisecond)
	fake.SetUpdates(map[string]any{"status": "running"})
	server := testfake.Serve(t, fake)
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
