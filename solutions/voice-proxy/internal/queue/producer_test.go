package queue_test

import (
	"context"
	"encoding/json"
	"net/http"
	"testing"
	"time"

	"github.com/proflyder/monorepo/packages/go/scheduler"
	"github.com/proflyder/monorepo/solutions/voice-proxy/internal/queue"
	"github.com/proflyder/monorepo/solutions/voice-proxy/internal/testfake"
)

func newProducer(t *testing.T, address string) *queue.Producer {
	t.Helper()
	client := scheduler.New(address, "test-scheduler-token", &http.Client{Timeout: time.Second})
	return queue.NewProducer(client, func() bool { return true })
}

// TestEnqueuePostsADurableTaskWithTheIdempotencyKey: the producer writes one
// task with the shared key.
func TestEnqueuePostsADurableTaskWithTheIdempotencyKey(t *testing.T) {
	fake := testfake.NewScheduler()
	server := testfake.Serve(t, fake)
	producer := newProducer(t, server.URL)
	created := int64(1_700_000_000)
	if !producer.Enqueue(context.Background(), "Готово.", "work", "thr_example", "after:7", &created) {
		t.Fatal("Enqueue returned false")
	}
	calls := fake.Calls()
	if len(calls) != 1 {
		t.Fatalf("calls = %#v", calls)
	}
	got := calls[0]
	if got.Path != "/ricardo.v1.SchedulerService/EnqueueTask" {
		t.Errorf("path = %q", got.Path)
	}
	if got.Body["taskType"] != "voice.announce" || got.Body["queue"] != "voice-work" {
		t.Errorf("body = %#v", got.Body)
	}
	if got.Body["idempotencyKey"] != "voice:thr_example:after:7" {
		t.Errorf("idempotencyKey = %v", got.Body["idempotencyKey"])
	}
	if _, ok := got.Body["runAtUnix"].(string); !ok {
		t.Errorf("runAtUnix = %#v, want a decimal string", got.Body["runAtUnix"])
	}
	payloadRaw, _ := got.Body["payloadJson"].(string)
	var payload map[string]any
	if err := json.Unmarshal([]byte(payloadRaw), &payload); err != nil {
		t.Fatalf("payloadJson: %v", err)
	}
	want := map[string]any{
		"text": "Готово.", "profile": "work", "thread_id": "thr_example",
		"seq": "after:7", "created_unix": float64(1_700_000_000),
	}
	if len(payload) != len(want) {
		t.Fatalf("payload = %#v", payload)
	}
	for key, value := range want {
		if payload[key] != value {
			t.Errorf("payload[%s] = %#v, want %#v", key, payload[key], value)
		}
	}
}

// TestPersonalProfileUsesItsOwnQueue: each profile has a separate queue name.
func TestPersonalProfileUsesItsOwnQueue(t *testing.T) {
	fake := testfake.NewScheduler()
	server := testfake.Serve(t, fake)
	producer := newProducer(t, server.URL)
	if !producer.Enqueue(context.Background(), "Ответ.", "personal", "thr_p", "done:12", nil) {
		t.Fatal("Enqueue returned false")
	}
	got := fake.Calls()[0].Body
	if got["queue"] != "voice-personal" || got["idempotencyKey"] != "voice:thr_p:done:12" {
		t.Fatalf("body = %#v", got)
	}
}

// TestSchedulerErrorReturnsFalse: a scheduler outage falls back to a direct
// announcement.
func TestSchedulerErrorReturnsFalse(t *testing.T) {
	fake := testfake.NewScheduler()
	fake.SetFail(true)
	server := testfake.Serve(t, fake)
	producer := newProducer(t, server.URL)
	if producer.Enqueue(context.Background(), "Готово.", "work", "thr_example", "after:1", nil) {
		t.Fatal("Enqueue must return false when the scheduler is down")
	}
}

// TestQueueProducerRefusesUnknownProfilesAndEmptyText: an unusable task must
// never be written.
func TestQueueProducerRefusesUnknownProfilesAndEmptyText(t *testing.T) {
	fake := testfake.NewScheduler()
	server := testfake.Serve(t, fake)
	producer := newProducer(t, server.URL)
	for _, tc := range []struct {
		text    string
		profile string
	}{
		{"", "work"},
		{"Готово.", "guest"},
	} {
		if producer.Enqueue(context.Background(), tc.text, tc.profile, "thr", "1", nil) {
			t.Errorf("Enqueue(%q, %q) = true", tc.text, tc.profile)
		}
	}
	if calls := fake.Calls(); len(calls) != 0 {
		t.Fatalf("calls = %#v", calls)
	}
}
