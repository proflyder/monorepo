package queue_test

import (
	"context"
	"encoding/json"
	"io"
	"net/http"
	"net/http/httptest"
	"sync"
	"testing"
	"time"

	"ricardo.local/monorepo/packages/go/scheduler"
	"ricardo.local/monorepo/solutions/voice-proxy/internal/queue"
)

type schedulerCall struct {
	path string
	body map[string]any
}

type fakeScheduler struct {
	mu    sync.Mutex
	calls []schedulerCall
	fail  bool
}

func (s *fakeScheduler) ServeHTTP(w http.ResponseWriter, r *http.Request) {
	raw, _ := io.ReadAll(r.Body)
	var body map[string]any
	_ = json.Unmarshal(raw, &body)
	s.mu.Lock()
	s.calls = append(s.calls, schedulerCall{path: r.URL.Path, body: body})
	fail := s.fail
	s.mu.Unlock()
	if fail {
		w.WriteHeader(http.StatusServiceUnavailable)
		return
	}
	w.Header().Set("Content-Type", "application/json")
	_, _ = w.Write([]byte(`{"event":{"id":"ev_test"}}`))
}

func (s *fakeScheduler) recorded() []schedulerCall {
	s.mu.Lock()
	defer s.mu.Unlock()
	return append([]schedulerCall(nil), s.calls...)
}

func newProducer(t *testing.T, server *httptest.Server) *queue.Producer {
	t.Helper()
	client := scheduler.New(server.URL, "test-scheduler-token", &http.Client{Timeout: time.Second})
	return queue.NewProducer(client, func() bool { return true })
}

// TestEnqueuePostsADurableTaskWithTheIdempotencyKey mirrors
// test_enqueue_posts_a_durable_task_with_the_idempotency_key.
func TestEnqueuePostsADurableTaskWithTheIdempotencyKey(t *testing.T) {
	server := httptest.NewServer(&fakeScheduler{})
	defer server.Close()
	producer := newProducer(t, server)
	created := int64(1_700_000_000)
	if !producer.Enqueue(context.Background(), "Готово.", "work", "thr_example", "after:7", &created) {
		t.Fatal("Enqueue returned false")
	}
	calls := server.Config.Handler.(*fakeScheduler).recorded()
	if len(calls) != 1 {
		t.Fatalf("calls = %#v", calls)
	}
	got := calls[0]
	if got.path != "/ricardo.v1.SchedulerService/EnqueueTask" {
		t.Errorf("path = %q", got.path)
	}
	if got.body["taskType"] != "voice.announce" || got.body["queue"] != "voice-work" {
		t.Errorf("body = %#v", got.body)
	}
	if got.body["idempotencyKey"] != "voice:thr_example:after:7" {
		t.Errorf("idempotencyKey = %v", got.body["idempotencyKey"])
	}
	if _, ok := got.body["runAtUnix"].(string); !ok {
		t.Errorf("runAtUnix = %#v, want a decimal string", got.body["runAtUnix"])
	}
	payloadRaw, _ := got.body["payloadJson"].(string)
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

// TestPersonalProfileUsesItsOwnQueue mirrors
// test_personal_profile_uses_its_own_queue.
func TestPersonalProfileUsesItsOwnQueue(t *testing.T) {
	server := httptest.NewServer(&fakeScheduler{})
	defer server.Close()
	producer := newProducer(t, server)
	if !producer.Enqueue(context.Background(), "Ответ.", "personal", "thr_p", "done:12", nil) {
		t.Fatal("Enqueue returned false")
	}
	got := server.Config.Handler.(*fakeScheduler).recorded()[0].body
	if got["queue"] != "voice-personal" || got["idempotencyKey"] != "voice:thr_p:done:12" {
		t.Fatalf("body = %#v", got)
	}
}

// TestSchedulerErrorReturnsFalse mirrors test_scheduler_error_returns_false.
func TestSchedulerErrorReturnsFalse(t *testing.T) {
	server := httptest.NewServer(&fakeScheduler{fail: true})
	defer server.Close()
	producer := newProducer(t, server)
	if producer.Enqueue(context.Background(), "Готово.", "work", "thr_example", "after:1", nil) {
		t.Fatal("Enqueue must return false when the scheduler is down")
	}
}

// TestQueueProducerRefusesUnknownProfilesAndEmptyText pins enqueue_announce's
// guard clauses: an unusable task must never be written.
func TestQueueProducerRefusesUnknownProfilesAndEmptyText(t *testing.T) {
	server := httptest.NewServer(&fakeScheduler{})
	defer server.Close()
	producer := newProducer(t, server)
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
	if calls := server.Config.Handler.(*fakeScheduler).recorded(); len(calls) != 0 {
		t.Fatalf("calls = %#v", calls)
	}
}
