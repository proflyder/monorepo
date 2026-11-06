package queue_test

import (
	"context"
	"strconv"
	"testing"

	"github.com/proflyder/monorepo/packages/go/scheduler"
	"github.com/proflyder/monorepo/solutions/voice-proxy/internal/testfake"
)

// goldenPayload is the on-wire voice.announce payload. Its keys, order and value
// types are a compatibility contract between producers and consumers, so it is
// pinned literally rather than derived from the encoder under test.
const goldenPayload = `{"text":"Готово.","profile":"work","thread_id":"thr_example","seq":"done:42","created_unix":1700000000}`

// TestAnnouncementPayloadGolden pins the producer's task fields and the exact
// payload bytes.
func TestAnnouncementPayloadGolden(t *testing.T) {
	fake := testfake.NewScheduler()
	server := testfake.Serve(t, fake)
	created := int64(1_700_000_000)
	if !newProducer(t, server.URL).Enqueue(context.Background(), "Готово.", "work", "thr_example", "done:42", &created) {
		t.Fatal("Enqueue returned false")
	}
	calls := fake.Calls()
	if len(calls) != 1 {
		t.Fatalf("calls = %#v", calls)
	}
	body := calls[0].Body
	if body["taskType"] != "voice.announce" {
		t.Errorf("taskType = %v", body["taskType"])
	}
	if body["queue"] != "voice-work" {
		t.Errorf("queue = %v", body["queue"])
	}
	if body["idempotencyKey"] != "voice:thr_example:done:42" {
		t.Errorf("idempotencyKey = %v", body["idempotencyKey"])
	}
	runAt, ok := body["runAtUnix"].(string)
	if !ok {
		t.Fatalf("runAtUnix = %#v, want a decimal string", body["runAtUnix"])
	}
	if _, err := strconv.ParseInt(runAt, 10, 64); err != nil {
		t.Fatalf("runAtUnix = %q: %v", runAt, err)
	}
	if payload, _ := body["payloadJson"].(string); payload != goldenPayload {
		t.Fatalf("payloadJson = %s\nwant %s", payload, goldenPayload)
	}
}

// TestConsumerAcceptsTheGoldenPayload feeds the pinned payload back into the
// consumer, so producer and consumer cannot drift apart.
func TestConsumerAcceptsTheGoldenPayload(t *testing.T) {
	calls := drive(t, driveOptions{
		events: []scheduler.Event{{ID: "ev_golden", Payload: goldenPayload}},
		limit:  2,
	})
	if len(calls.announced) != 1 || calls.announced[0] != "Готово." {
		t.Fatalf("announced = %#v", calls.announced)
	}
	if len(calls.acks) != 1 || calls.acks[0].outcome != "ACK_OUTCOME_DELIVERED" {
		t.Fatalf("acks = %#v", calls.acks)
	}
}
