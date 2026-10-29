package control

import (
	"context"
	"errors"
	"testing"
	"time"

	"ricardo.local/monorepo/solutions/ha-bridge/internal/catalog"
	"ricardo.local/monorepo/solutions/ha-bridge/internal/httpapi"
	"ricardo.local/monorepo/solutions/ha-bridge/internal/scheduler"
)

type fakeSched struct {
	events []scheduler.Event
	acks   []ack
	req    scheduler.ListRequest
}
type ack struct{ id, outcome, detail string }

func (f *fakeSched) ListDueEvents(_ context.Context, r scheduler.ListRequest) ([]scheduler.Event, error) {
	f.req = r
	return f.events, nil
}
func (f *fakeSched) AckEvent(_ context.Context, id, _, outcome, detail string) error {
	f.acks = append(f.acks, ack{id, outcome, detail})
	return nil
}

type fakeHA struct {
	calls []catalog.ServiceCall
	err   error
}

func (f *fakeHA) CallService(_ context.Context, d, s, e string) error {
	f.calls = append(f.calls, catalog.ServiceCall{Domain: d, Service: s, Entity: e})
	return f.err
}

func testExecutor(t *testing.T, events []scheduler.Event, ha *fakeHA) (*Executor, *fakeSched) {
	t.Helper()
	cat, err := catalog.Parse([]byte(`{"version":1,"entries":[
	 {"id":"focus","kind":"scenario","title":"Focus","entity":"script.focus"},
	 {"id":"lamp","kind":"light","title":"Lamp","entity":"light.lamp"}]}`))
	if err != nil {
		t.Fatal(err)
	}
	sched := &fakeSched{events: events}
	cfg := Config{ConsumerID: "c", Queue: "control", BatchSize: 5, LeaseSeconds: 60, PollInterval: time.Second, MaxBackoff: time.Minute, MaxAttempts: 3}
	return New(cfg, cat, sched, ha, nil), sched
}

func task(id, payload string) scheduler.Event {
	return scheduler.Event{ID: id, SourceKind: scheduler.SourceKindTask, TaskType: catalog.TaskType, Queue: "control", Payload: payload}
}

func TestRunsScenarioAndDevice(t *testing.T) {
	ha := &fakeHA{}
	ex, sched := testExecutor(t, []scheduler.Event{
		task("e1", `{"entry":"focus","action":"run"}`),
		task("e2", `{"entry":"lamp","action":"turn_off"}`),
	}, ha)
	if err := ex.cycle(context.Background()); err != nil {
		t.Fatal(err)
	}
	if sched.req.Queue != "control" {
		t.Fatalf("leased queue %q", sched.req.Queue)
	}
	want := []catalog.ServiceCall{
		{Domain: "script", Service: "turn_on", Entity: "script.focus"},
		{Domain: "light", Service: "turn_off", Entity: "light.lamp"},
	}
	if len(ha.calls) != 2 || ha.calls[0] != want[0] || ha.calls[1] != want[1] {
		t.Fatalf("calls = %+v", ha.calls)
	}
	for _, a := range sched.acks {
		if a.outcome != scheduler.AckDelivered {
			t.Fatalf("ack %+v", a)
		}
	}
}

func TestRejectsBadCommandsWithoutCallingHA(t *testing.T) {
	cases := map[string]scheduler.Event{
		"bad json":       task("a", `nope`),
		"unknown entry":  task("b", `{"entry":"ghost","action":"run"}`),
		"wrong action":   task("c", `{"entry":"lamp","action":"run"}`),
		"toggle":         task("d", `{"entry":"lamp","action":"toggle"}`),
		"entity payload": task("e", `{"entry":"light.lamp","action":"turn_on"}`),
		"wrong type":     {ID: "f", SourceKind: scheduler.SourceKindTask, TaskType: "other", Queue: "control", Payload: `{"entry":"focus","action":"run"}`},
		"wrong queue":    {ID: "g", SourceKind: scheduler.SourceKindTask, TaskType: catalog.TaskType, Queue: "home", Payload: `{"entry":"focus","action":"run"}`},
		"alarm kind":     {ID: "h", SourceKind: scheduler.SourceKindAlarm, Queue: "control"},
	}
	for name, ev := range cases {
		ha := &fakeHA{}
		ex, sched := testExecutor(t, []scheduler.Event{ev}, ha)
		_ = ex.cycle(context.Background())
		if len(ha.calls) != 0 {
			t.Errorf("%s: HA was called: %+v", name, ha.calls)
		}
		if len(sched.acks) != 1 || sched.acks[0].outcome != scheduler.AckFailed {
			t.Errorf("%s: acks = %+v", name, sched.acks)
		}
	}
}

func TestHAFailureHandling(t *testing.T) {
	transient := &httpapi.Error{Op: "call service", StatusCode: 503}
	permanent := &httpapi.Error{Op: "call service", StatusCode: 404}
	auth := &httpapi.Error{Op: "call service", StatusCode: 401}

	run := func(err error, attempts int) []ack {
		ha := &fakeHA{err: err}
		ev := task("e", `{"entry":"focus","action":"run"}`)
		ev.Attempts = attempts
		ex, sched := testExecutor(t, []scheduler.Event{ev}, ha)
		_ = ex.cycle(context.Background())
		return sched.acks
	}
	if acks := run(transient, 1); len(acks) != 0 {
		t.Fatalf("transient must stay unacked, got %+v", acks)
	}
	if acks := run(permanent, 1); len(acks) != 1 || acks[0].outcome != scheduler.AckFailed {
		t.Fatalf("permanent: %+v", acks)
	}
	if acks := run(auth, 1); len(acks) != 0 {
		t.Fatalf("auth must retry first, got %+v", acks)
	}
	if acks := run(auth, 3); len(acks) != 1 || acks[0].outcome != scheduler.AckFailed {
		t.Fatalf("auth after limit: %+v", acks)
	}
	if acks := run(errors.New("plain"), 1); len(acks) != 0 {
		t.Fatalf("unclassified error must stay unacked, got %+v", acks)
	}
}
