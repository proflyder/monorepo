package scheduler

import (
	"context"
	"encoding/json"
	"fmt"
	"net/http"
	"net/http/httptest"
	"strings"
	"sync/atomic"
	"testing"
	"time"

	"ricardo.local/monorepo/solutions/ha-bridge/internal/httpapi"
)

const testToken = "scheduler-secret-token"

type recordedRequest struct {
	method string
	path   string
	body   map[string]any
	header http.Header
}

func newTestClient(t *testing.T, handler func(http.ResponseWriter, *http.Request)) (*Client, *[]recordedRequest) {
	t.Helper()
	var seen []recordedRequest
	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		var body map[string]any
		_ = json.NewDecoder(r.Body).Decode(&body)
		seen = append(seen, recordedRequest{
			method: r.Method,
			path:   r.URL.Path,
			body:   body,
			header: r.Header.Clone(),
		})
		handler(w, r)
	}))
	t.Cleanup(server.Close)
	return New(server.URL, testToken, server.Client()), &seen
}

func TestListDueEventsRequestShape(t *testing.T) {
	client, seen := newTestClient(t, func(w http.ResponseWriter, r *http.Request) {
		fmt.Fprint(w, `{"events":[
			{"id":"evt-1","sourceId":"alarm-1","sourceKind":"SOURCE_KIND_ALARM","dueUnix":"1700000000","occurrenceUnix":"1699999900","label":"Wake up","state":"EVENT_STATE_LEASED","attempts":"1","snoozeSeq":"0","leaseUntilUnix":"1700000120","action":"EVENT_ACTION_FIRE","queue":"home","taskType":"alarm.fire"},
			{"id":"evt-2","sourceId":"alarm-1","sourceKind":2,"dueUnix":1700000001,"label":"","state":2,"attempts":0,"leaseUntilUnix":1700000120,"action":2,"queue":"home"},
			{"id":"evt-3","sourceId":"task-1","sourceKind":5,"dueUnix":"1700000002","action":1,"queue":"home","taskType":"generic.ping"}
		]}`)
	})

	events, err := client.ListDueEvents(context.Background(), ListRequest{
		ConsumerID:   "ha-bridge-home",
		Queue:        "home",
		LeaseSeconds: 300,
		Limit:        10,
	})
	if err != nil {
		t.Fatalf("ListDueEvents: %v", err)
	}

	if len(*seen) != 1 {
		t.Fatalf("requests = %d, want 1", len(*seen))
	}
	req := (*seen)[0]
	if req.method != http.MethodPost {
		t.Errorf("method = %s, want POST", req.method)
	}
	if req.path != ServicePath+"ListDueEvents" {
		t.Errorf("path = %s, want %s", req.path, ServicePath+"ListDueEvents")
	}
	if got := req.header.Get("Authorization"); got != "Bearer "+testToken {
		t.Errorf("Authorization = %q", got)
	}
	if got := req.header.Get("Content-Type"); got != "application/json" {
		t.Errorf("Content-Type = %q", got)
	}
	if got := req.header.Get("Connect-Protocol-Version"); got != "1" {
		t.Errorf("Connect-Protocol-Version = %q, want 1", got)
	}
	if got := req.body["consumerId"]; got != "ha-bridge-home" {
		t.Errorf("consumerId = %v", got)
	}
	if got := req.body["queue"]; got != "home" {
		t.Errorf("queue = %v, want home (an HA executor must never request all queues)", got)
	}
	if got := req.body["leaseSeconds"]; got != float64(300) {
		t.Errorf("leaseSeconds = %v, want 300", got)
	}
	if got := req.body["limit"]; got != float64(10) {
		t.Errorf("limit = %v, want 10", got)
	}

	if len(events) != 3 {
		t.Fatalf("events = %d, want 3", len(events))
	}
	alarm := events[0]
	if alarm.SourceKind != SourceKindAlarm || alarm.Action != ActionFire {
		t.Errorf("event 1 kind/action = %q/%q", alarm.SourceKind, alarm.Action)
	}
	if alarm.DueUnix != 1700000000 || alarm.OccurrenceUnix != 1699999900 {
		t.Errorf("event 1 times = %d/%d", alarm.DueUnix, alarm.OccurrenceUnix)
	}
	if alarm.Attempts != 1 || alarm.LeaseUntilUnix != 1700000120 || alarm.Queue != "home" {
		t.Errorf("event 1 metadata = attempts %d lease %d queue %q", alarm.Attempts, alarm.LeaseUntilUnix, alarm.Queue)
	}
	if alarm.TaskType != "alarm.fire" {
		t.Errorf("taskType = %q", alarm.TaskType)
	}

	reminder := events[1]
	if reminder.SourceKind != SourceKindReminder || reminder.Action != ActionStop {
		t.Errorf("numeric enums decoded as %q/%q, want reminder/STOP", reminder.SourceKind, reminder.Action)
	}

	task := events[2]
	if task.SourceKind != SourceKindTask {
		t.Errorf("task source kind = %q, want task", task.SourceKind)
	}
	if task.Action != ActionFire {
		t.Errorf("task action = %q, want FIRE", task.Action)
	}
}

func TestListDueEventsLegacyUnspecifiedActionIsFire(t *testing.T) {
	client, _ := newTestClient(t, func(w http.ResponseWriter, r *http.Request) {
		fmt.Fprint(w, `{"events":[{"id":"evt-legacy","sourceKind":"SOURCE_KIND_ALARM","dueUnix":"1"}]}`)
	})
	events, err := client.ListDueEvents(context.Background(), ListRequest{ConsumerID: "c", Queue: "home", LeaseSeconds: 60, Limit: 1})
	if err != nil {
		t.Fatalf("ListDueEvents: %v", err)
	}
	if len(events) != 1 || events[0].Action != ActionFire {
		t.Fatalf("legacy action = %+v, want FIRE", events)
	}
}

func TestAckEventRequestShape(t *testing.T) {
	client, seen := newTestClient(t, func(w http.ResponseWriter, r *http.Request) {
		fmt.Fprint(w, `{"event":{"id":"evt-1","state":"EVENT_STATE_DELIVERED"}}`)
	})

	err := client.AckEvent(context.Background(), "evt-1", "ha-bridge-home", AckDelivered, "home assistant accepted alarm_start")
	if err != nil {
		t.Fatalf("AckEvent: %v", err)
	}

	req := (*seen)[0]
	if req.path != ServicePath+"AckEvent" {
		t.Errorf("path = %s", req.path)
	}
	if req.body["eventId"] != "evt-1" || req.body["consumerId"] != "ha-bridge-home" {
		t.Errorf("body = %v", req.body)
	}
	if req.body["outcome"] != AckDelivered {
		t.Errorf("outcome = %v, want %s", req.body["outcome"], AckDelivered)
	}
	if detail, ok := req.body["detail"].(string); !ok || !strings.Contains(detail, "alarm_start") {
		t.Errorf("detail = %v", req.body["detail"])
	}
}

func TestAckEventRejectsEmptyIDAndBadOutcome(t *testing.T) {
	client, _ := newTestClient(t, func(w http.ResponseWriter, r *http.Request) {
		t.Error("no HTTP request should be sent for invalid acks")
	})
	if err := client.AckEvent(context.Background(), "", "c", AckDelivered, ""); err == nil {
		t.Error("empty event id must be rejected")
	}
	if err := client.AckEvent(context.Background(), "evt-1", "c", "ACK_OUTCOME_UNSPECIFIED", ""); err == nil {
		t.Error("invalid outcome must be rejected locally")
	}
}

func TestGetStatusParsesQueues(t *testing.T) {
	client, seen := newTestClient(t, func(w http.ResponseWriter, r *http.Request) {
		fmt.Fprint(w, `{
			"version":"1.2.3","schemaVersion":2,"nowUnix":"1700000000","lastTickUnix":"1699999999",
			"pendingEvents":3,"leasedEvents":1,"enabledSchedules":2,"runningTimers":1,
			"scenarioExecutionSupported":false,
			"queues":[{"name":"home","pending":2,"leased":1},{"name":"tasks","pending":1}],
			"consumers":[{"consumerId":"ha-bridge-home","lastSeenUnix":"1700000000","lastAckUnix":"1699999950","alive":true}]
		}`)
	})

	status, err := client.GetStatus(context.Background())
	if err != nil {
		t.Fatalf("GetStatus: %v", err)
	}
	if status.Version != "1.2.3" || status.SchemaVersion != 2 || status.PendingEvents != 3 {
		t.Errorf("status = %+v", status)
	}
	if len(status.Queues) != 2 || status.Queues[0].Name != "home" || status.Queues[1].Pending != 1 {
		t.Errorf("queues = %+v", status.Queues)
	}
	if len(status.Consumers) != 1 || !status.Consumers[0].Alive {
		t.Errorf("consumers = %+v", status.Consumers)
	}
	if (*seen)[0].path != ServicePath+"GetStatus" {
		t.Errorf("path = %s", (*seen)[0].path)
	}
}

func TestErrorClassificationFromHTTP(t *testing.T) {
	cases := []struct {
		name      string
		status    int
		body      string
		transient bool
		code      string
	}{
		{"unavailable", http.StatusServiceUnavailable, `{"code":"unavailable","message":"try later"}`, true, "unavailable"},
		{"invalid", http.StatusBadRequest, `{"code":"invalid_argument","message":"bad lease"}`, false, "invalid_argument"},
		{"unauthenticated", http.StatusUnauthorized, `{"code":"unauthenticated","message":"bad token"}`, false, "unauthenticated"},
	}
	for _, tc := range cases {
		client, _ := newTestClient(t, func(w http.ResponseWriter, r *http.Request) {
			w.WriteHeader(tc.status)
			fmt.Fprint(w, tc.body)
		})
		_, err := client.GetStatus(context.Background())
		if err == nil {
			t.Fatalf("%s: expected an error", tc.name)
		}
		if got := httpapi.IsTransient(err); got != tc.transient {
			t.Errorf("%s: transient = %v, want %v", tc.name, got, tc.transient)
		}
		if !strings.Contains(err.Error(), "code="+tc.code) {
			t.Errorf("%s: error = %q", tc.name, err.Error())
		}
	}
}

func TestErrorNeverLeaksTokenOrURL(t *testing.T) {
	client, _ := newTestClient(t, func(w http.ResponseWriter, r *http.Request) {
		w.WriteHeader(http.StatusUnauthorized)
		fmt.Fprintf(w, `{"code":"unauthenticated","message":"token %s rejected"}`, testToken)
	})

	_, err := client.GetStatus(context.Background())
	if err == nil {
		t.Fatal("expected an error")
	}
	message := err.Error()
	if strings.Contains(message, testToken) {
		t.Errorf("error leaked the bearer token: %q", message)
	}
	if strings.Contains(message, "http://") {
		t.Errorf("error leaked the request URL: %q", message)
	}
}

func TestNetworkErrorOmitsURL(t *testing.T) {
	server := httptest.NewServer(http.HandlerFunc(func(http.ResponseWriter, *http.Request) {}))
	url := server.URL
	server.Close()

	client := New(url, testToken, &http.Client{Timeout: time.Second})
	_, err := client.GetStatus(context.Background())
	if err == nil {
		t.Fatal("expected an error for a closed server")
	}
	if strings.Contains(err.Error(), url) {
		t.Errorf("error leaked the URL: %q", err.Error())
	}
	if strings.Contains(err.Error(), testToken) {
		t.Errorf("error leaked the token: %q", err.Error())
	}
	if !httpapi.IsTransient(err) {
		t.Error("a network failure must be transient")
	}
}

func TestRedirectIsNotFollowed(t *testing.T) {
	var followed atomic.Int32
	destination := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		followed.Add(1)
		fmt.Fprint(w, `{"version":"stolen"}`)
	}))
	defer destination.Close()

	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		http.Redirect(w, r, destination.URL, http.StatusFound)
	}))
	defer server.Close()

	client := New(server.URL, testToken, server.Client())
	_, err := client.GetStatus(context.Background())
	if err == nil {
		t.Fatal("a 3xx RPC response must be an error")
	}
	if followed.Load() != 0 {
		t.Fatal("redirect target was requested; the bearer token must not be forwarded")
	}
	if httpapi.IsTransient(err) {
		t.Error("a redirect must not be treated as a transient RPC error")
	}
}

func TestClientToleratesFullServiceURL(t *testing.T) {
	var paths []string
	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		paths = append(paths, r.URL.Path)
		fmt.Fprint(w, `{}`)
	}))
	defer server.Close()

	client := New(server.URL+ServicePath[:len(ServicePath)-1], testToken, server.Client())
	if _, err := client.GetStatus(context.Background()); err != nil {
		t.Fatalf("GetStatus: %v", err)
	}
	if len(paths) != 1 || paths[0] != ServicePath+"GetStatus" {
		t.Errorf("paths = %v", paths)
	}
}

func TestTruncateBoundsAckDetail(t *testing.T) {
	got := truncate(strings.Repeat("x", 2000), maxAckDetail)
	if len(got) != maxAckDetail {
		t.Errorf("len = %d, want %d", len(got), maxAckDetail)
	}
}

func TestDecodeHelpers(t *testing.T) {
	var invalid protoInt64
	if err := invalid.UnmarshalJSON([]byte(`"not-a-number"`)); err == nil {
		t.Error("invalid int64 must fail")
	}
	var value protoInt64
	if err := value.UnmarshalJSON([]byte(`null`)); err != nil || value != 0 {
		t.Errorf("null int64 = %d, %v", value, err)
	}
	if got := enumValue(protoEnum("9"), "SOURCE_KIND_", map[int]string{1: "alarm"}); got != "enum-9" {
		t.Errorf("unknown enum = %q, want enum-9", got)
	}
	if got := sourceKindOf(protoEnum("SOURCE_KIND_UNSPECIFIED")); got != SourceKindUnspecified {
		t.Errorf("unspecified kind = %q", got)
	}
	if got := actionOf(protoEnum("EVENT_ACTION_BOGUS")); got != Action("BOGUS") {
		t.Errorf("unknown action = %q, want BOGUS so the bridge can fail it", got)
	}
}
