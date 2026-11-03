package queue_test

import (
	"context"
	_ "embed"
	"encoding/json"
	"net/http/httptest"
	"sort"
	"strconv"
	"strings"
	"testing"

	"ricardo.local/monorepo/packages/go/scheduler"
)

//go:embed testdata/voice_announce_python.json
var pythonFixture []byte

type fixture struct {
	Method  string `json:"method"`
	Request struct {
		TaskType       string `json:"taskType"`
		PayloadJSON    string `json:"payloadJson"`
		RunAtUnix      string `json:"runAtUnix"`
		Queue          string `json:"queue"`
		IdempotencyKey string `json:"idempotencyKey"`
	} `json:"request"`
	Payload         map[string]any    `json:"payload"`
	PayloadKeyOrder []string          `json:"payload_key_order"`
	PayloadTypes    map[string]string `json:"payload_types"`
}

func loadPythonFixture(t *testing.T) fixture {
	t.Helper()
	var decoded fixture
	if err := json.Unmarshal(pythonFixture, &decoded); err != nil {
		t.Fatalf("decode fixture: %v", err)
	}
	return decoded
}

// TestPythonProducedAnnouncementIsConsumedByGo feeds the payload_json produced
// by voice_proxy.enqueue_announce into the Go consumer.
func TestPythonProducedAnnouncementIsConsumedByGo(t *testing.T) {
	fixture := loadPythonFixture(t)
	calls := drive(t, driveOptions{
		events: []scheduler.Event{{
			ID:      "ev_fixture",
			Payload: fixture.Request.PayloadJSON,
		}},
		limit: 2,
	})
	if len(calls.announced) != 1 {
		t.Fatalf("announced = %#v", calls.announced)
	}
	if calls.announced[0] != fixture.Payload["text"] {
		t.Fatalf("announced %q, want %v", calls.announced[0], fixture.Payload["text"])
	}
	if len(calls.acks) != 1 || calls.acks[0].outcome != "ACK_OUTCOME_DELIVERED" {
		t.Fatalf("acks = %#v", calls.acks)
	}
}

// TestGoProducedAnnouncementMatchesThePythonContract drives the Go producer and
// asserts the request keys, the payload key order and the value types are the
// ones the Python consumer expects (the fixture records them).
func TestGoProducedAnnouncementMatchesThePythonContract(t *testing.T) {
	fixture := loadPythonFixture(t)
	schedulerFake := &fakeScheduler{}
	server := httptest.NewServer(schedulerFake)
	defer server.Close()
	producer := newProducer(t, server)
	created := int64(1_700_000_000)
	if !producer.Enqueue(context.Background(), "Готово.", "work", "thr_fixture", "done:42", &created) {
		t.Fatal("Enqueue returned false")
	}
	requests := schedulerFake.recorded()
	if len(requests) != 1 {
		t.Fatalf("request count = %d", len(requests))
	}
	body := requests[0].body

	for _, key := range []string{"taskType", "payloadJson", "runAtUnix", "queue", "idempotencyKey"} {
		if _, ok := body[key]; !ok {
			t.Errorf("request has no %s", key)
		}
	}
	if body["taskType"] != fixture.Request.TaskType || body["queue"] != fixture.Request.Queue {
		t.Errorf("request = %#v", body)
	}
	if body["idempotencyKey"] != fixture.Request.IdempotencyKey {
		t.Errorf("idempotencyKey = %v", body["idempotencyKey"])
	}
	if _, ok := body["runAtUnix"].(string); !ok {
		t.Errorf("runAtUnix = %#v, want a decimal string", body["runAtUnix"])
	}

	payloadRaw, _ := body["payloadJson"].(string)
	var payload map[string]json.RawMessage
	if err := json.Unmarshal([]byte(payloadRaw), &payload); err != nil {
		t.Fatalf("payloadJson: %v", err)
	}
	gotKeys := make([]string, 0, len(payload))
	for key := range payload {
		gotKeys = append(gotKeys, key)
	}
	sort.Strings(gotKeys)
	wantKeys := make([]string, 0, len(fixture.PayloadKeyOrder))
	wantKeys = append(wantKeys, fixture.PayloadKeyOrder...)
	sort.Strings(wantKeys)
	if strings.Join(gotKeys, ",") != strings.Join(wantKeys, ",") {
		t.Fatalf("payload keys = %v, want %v", gotKeys, wantKeys)
	}
	for key, wantType := range fixture.PayloadTypes {
		raw := string(payload[key])
		switch wantType {
		case "str":
			var value string
			if err := json.Unmarshal([]byte(raw), &value); err != nil {
				t.Errorf("%s is not a JSON string: %s", key, raw)
			}
		case "int":
			if strings.ContainsAny(raw, ".eE\"") {
				t.Errorf("%s is not a bare JSON integer: %s", key, raw)
			}
			var value int64
			if err := json.Unmarshal([]byte(raw), &value); err != nil {
				t.Errorf("%s is not an integer: %s", key, raw)
			}
		}
	}
	// The Go encoder writes the same key order as Python's dict literal.
	order := topLevelKeyOrder(t, payloadRaw)
	if len(order) != 5 ||
		order[0] != "text" || order[1] != "profile" || order[2] != "thread_id" ||
		order[3] != "seq" || order[4] != "created_unix" {
		t.Fatalf("payload key order = %v", order)
	}
}

// topLevelKeyOrder reads the object keys in wire order.
func topLevelKeyOrder(t *testing.T, raw string) []string {
	t.Helper()
	decoder := json.NewDecoder(strings.NewReader(raw))
	token, err := decoder.Token()
	if err != nil || token != json.Delim('{') {
		t.Fatalf("payload is not an object: %v", err)
	}
	var keys []string
	for decoder.More() {
		key, err := decoder.Token()
		if err != nil {
			t.Fatalf("read key: %v", err)
		}
		keys = append(keys, key.(string))
		var skip json.RawMessage
		if err := decoder.Decode(&skip); err != nil {
			t.Fatalf("skip value: %v", err)
		}
	}
	return keys
}

// TestFixtureRunAtIsDecimalSeconds guards the fixture itself against a
// hand-edited timestamp.
func TestFixtureRunAtIsDecimalSeconds(t *testing.T) {
	fixture := loadPythonFixture(t)
	if _, err := strconv.ParseInt(fixture.Request.RunAtUnix, 10, 64); err != nil {
		t.Fatalf("runAtUnix = %q: %v", fixture.Request.RunAtUnix, err)
	}
}
