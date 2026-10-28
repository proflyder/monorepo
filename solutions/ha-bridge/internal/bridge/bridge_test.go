package bridge

import (
	"bytes"
	"context"
	"errors"
	"io"
	"log/slog"
	"net/http"
	"strings"
	"testing"
	"time"

	"ricardo.local/monorepo/solutions/ha-bridge/internal/config"
	"ricardo.local/monorepo/solutions/ha-bridge/internal/httpapi"
	"ricardo.local/monorepo/solutions/ha-bridge/internal/ledger"
	"ricardo.local/monorepo/solutions/ha-bridge/internal/scheduler"
)

type ackCall struct {
	eventID    string
	consumerID string
	outcome    string
	detail     string
}

type fakeScheduler struct {
	status      scheduler.Status
	statusErr   error
	statusCalls int

	events    []scheduler.Event
	listErr   error
	listCalls int
	listReq   scheduler.ListRequest

	acks   []ackCall
	ackErr error

	order *[]string
}

func (f *fakeScheduler) ListDueEvents(_ context.Context, req scheduler.ListRequest) ([]scheduler.Event, error) {
	f.listCalls++
	f.listReq = req
	if f.order != nil {
		*f.order = append(*f.order, "list")
	}
	return f.events, f.listErr
}

func (f *fakeScheduler) AckEvent(_ context.Context, eventID, consumerID, outcome, detail string) error {
	if f.order != nil {
		*f.order = append(*f.order, "ack")
	}
	f.acks = append(f.acks, ackCall{eventID: eventID, consumerID: consumerID, outcome: outcome, detail: detail})
	return f.ackErr
}

func (f *fakeScheduler) GetStatus(context.Context) (scheduler.Status, error) {
	f.statusCalls++
	return f.status, f.statusErr
}

type haCall struct {
	script    string
	variables map[string]any
}

type fakeHA struct {
	calls     []haCall
	err       error
	pingErr   error
	pingCalls int
	order     *[]string
}

func (f *fakeHA) CallScript(_ context.Context, script string, variables map[string]any) error {
	if f.order != nil {
		*f.order = append(*f.order, "ha")
	}
	f.calls = append(f.calls, haCall{script: script, variables: variables})
	return f.err
}

func (f *fakeHA) Ping(context.Context) error {
	f.pingCalls++
	return f.pingErr
}

type fakeReceipts struct {
	seen      map[string]bool
	recorded  []ledger.Receipt
	active    *ledger.ActiveAlarm
	recordErr error
	order     *[]string
}

func (f *fakeReceipts) Seen(eventID string) bool {
	if f.seen == nil {
		return false
	}
	return f.seen[eventID]
}

func (f *fakeReceipts) Record(receipt ledger.Receipt) error {
	f.mark(receipt)
	return f.recordErr
}

func (f *fakeReceipts) ActiveAlarm() (ledger.ActiveAlarm, bool) {
	if f.active == nil {
		return ledger.ActiveAlarm{}, false
	}
	return *f.active, true
}

func (f *fakeReceipts) RecordStart(receipt ledger.Receipt, active ledger.ActiveAlarm) error {
	f.mark(receipt)
	if f.recordErr == nil {
		marker := active
		f.active = &marker
	}
	return f.recordErr
}

func (f *fakeReceipts) RecordStop(receipt ledger.Receipt, expectSourceID string) (bool, error) {
	f.mark(receipt)
	cleared := f.active != nil && f.active.SourceID == expectSourceID
	if cleared && f.recordErr == nil {
		f.active = nil
	}
	return cleared, f.recordErr
}

func (f *fakeReceipts) mark(receipt ledger.Receipt) {
	if f.order != nil {
		*f.order = append(*f.order, "ledger")
	}
	f.recorded = append(f.recorded, receipt)
	if f.seen == nil {
		f.seen = map[string]bool{}
	}
	f.seen[receipt.EventID] = true
}

func testConfig() config.Config {
	return config.Config{
		HAURL:            "http://ha.example:8123",
		SchedulerURL:     "http://scheduler.example:8098",
		HAToken:          "ha-secret",
		SchedulerToken:   "scheduler-secret",
		ConsumerID:       "ha-bridge-home",
		Queue:            config.QueueHome,
		Execute:          true,
		BatchSize:        10,
		LeaseSeconds:     300,
		PollInterval:     time.Millisecond,
		MaxBackoff:       5 * time.Millisecond,
		HATimeout:        time.Second,
		SchedulerTimeout: time.Second,
		MaxAttempts:      10,
		AlarmScript:      "alarm_start",
		AnnounceScript:   "schedule_announce",
		StopScript:       "alarm_stop",
	}
}

func testLogger(t *testing.T) *slog.Logger {
	t.Helper()
	return slog.New(slog.NewTextHandler(io.Discard, &slog.HandlerOptions{Level: slog.LevelDebug}))
}

func testLoggerWithBuffer(t *testing.T) (*slog.Logger, *bytes.Buffer) {
	t.Helper()
	buffer := &bytes.Buffer{}
	return slog.New(slog.NewTextHandler(buffer, &slog.HandlerOptions{Level: slog.LevelDebug})), buffer
}

func alarmEvent() scheduler.Event {
	return scheduler.Event{
		ID:         "evt-alarm",
		SourceID:   "alarm-1",
		SourceKind: scheduler.SourceKindAlarm,
		DueUnix:    1700000000,
		Label:      "Wake up",
		Queue:      "home",
		Action:     scheduler.ActionFire,
	}
}

func TestDryRunDoesNotLeaseOrAcknowledge(t *testing.T) {
	cfg := testConfig()
	cfg.Execute = false

	sched := &fakeScheduler{status: scheduler.Status{Version: "test-1", PendingEvents: 2, EnabledSchedules: 1}}
	ha := &fakeHA{}
	receipts := &fakeReceipts{}
	logger, logs := testLoggerWithBuffer(t)

	runner := New(cfg, sched, ha, receipts, logger)
	if err := runner.Run(context.Background()); err != nil {
		t.Fatalf("Run: %v", err)
	}

	if sched.listCalls != 0 {
		t.Errorf("ListDueEvents called %d times in dry-run; it must never lease", sched.listCalls)
	}
	if sched.statusCalls != 1 {
		t.Errorf("GetStatus called %d times, want 1", sched.statusCalls)
	}
	if len(sched.acks) != 0 {
		t.Errorf("acks = %d, want 0", len(sched.acks))
	}
	if len(ha.calls) != 0 {
		t.Errorf("HA calls = %d, want 0", len(ha.calls))
	}
	if len(receipts.recorded) != 0 {
		t.Errorf("ledger writes = %d, want 0", len(receipts.recorded))
	}
	if ha.pingCalls != 1 {
		t.Errorf("HA ping = %d, want 1 readiness probe", ha.pingCalls)
	}
	if !strings.Contains(logs.String(), "dry-run") {
		t.Errorf("dry-run mode was not logged: %s", logs.String())
	}
}

func TestDryRunSkipsHAPingWithoutToken(t *testing.T) {
	cfg := testConfig()
	cfg.Execute = false
	cfg.HAToken = ""

	ha := &fakeHA{}
	r := New(cfg, &fakeScheduler{}, ha, nil, testLogger(t))
	if err := r.Run(context.Background()); err != nil {
		t.Fatalf("Run: %v", err)
	}
	if ha.pingCalls != 0 {
		t.Errorf("HA ping = %d, want 0 without a token", ha.pingCalls)
	}
}

func TestDryRunReportsSchedulerFailure(t *testing.T) {
	cfg := testConfig()
	cfg.Execute = false
	sched := &fakeScheduler{statusErr: errors.New("connection refused")}

	err := New(cfg, sched, nil, nil, testLogger(t)).Run(context.Background())
	if err == nil || !strings.Contains(err.Error(), "scheduler status") {
		t.Fatalf("err = %v, want a scheduler status error", err)
	}
	if sched.listCalls != 0 {
		t.Error("dry-run must not lease events even when the status check fails")
	}
}

func TestFireAlarmCallsAlarmScript(t *testing.T) {
	cfg := testConfig()
	order := []string{}
	sched := &fakeScheduler{events: []scheduler.Event{alarmEvent()}}
	ha := &fakeHA{order: &order}
	receipts := &fakeReceipts{order: &order}
	sched.order = &order

	r := New(cfg, sched, ha, receipts, testLogger(t))
	if err := r.cycle(context.Background()); err != nil {
		t.Fatalf("cycle: %v", err)
	}

	if sched.listReq.Queue != "home" {
		t.Errorf("lease queue = %q, want home", sched.listReq.Queue)
	}
	if len(ha.calls) != 1 {
		t.Fatalf("HA calls = %d, want 1", len(ha.calls))
	}
	if ha.calls[0].script != "alarm_start" {
		t.Errorf("script = %q, want alarm_start", ha.calls[0].script)
	}
	if ha.calls[0].variables["label"] != "Wake up" || ha.calls[0].variables["kind"] != "alarm" {
		t.Errorf("variables = %#v", ha.calls[0].variables)
	}

	if len(sched.acks) != 1 {
		t.Fatalf("acks = %d, want 1", len(sched.acks))
	}
	ack := sched.acks[0]
	if ack.eventID != "evt-alarm" || ack.consumerID != "ha-bridge-home" || ack.outcome != scheduler.AckDelivered {
		t.Errorf("ack = %+v", ack)
	}

	wantOrder := []string{"list", "ha", "ledger", "ack"}
	if strings.Join(order, ",") != strings.Join(wantOrder, ",") {
		t.Errorf("order = %v, want %v (receipt must be recorded before the ack)", order, wantOrder)
	}
}

func TestFireTimerAndReminderUseAnnounceScriptWithChime(t *testing.T) {
	cases := []struct {
		name     string
		event    scheduler.Event
		wantKind string
		wantMsg  string
	}{
		{
			name: "timer",
			event: scheduler.Event{
				ID: "evt-timer", SourceID: "timer-1", SourceKind: scheduler.SourceKindTimer,
				Label: "Пицца", Queue: "home", Action: scheduler.ActionFire,
			},
			wantKind: "timer",
			wantMsg:  "Пицца",
		},
		{
			name: "reminder without label",
			event: scheduler.Event{
				ID: "evt-reminder", SourceID: "reminder-1", SourceKind: scheduler.SourceKindReminder,
				Queue: "home", Action: scheduler.ActionFire,
			},
			wantKind: "reminder",
			wantMsg:  "Reminder",
		},
	}

	for _, tc := range cases {
		t.Run(tc.name, func(t *testing.T) {
			sched := &fakeScheduler{events: []scheduler.Event{tc.event}}
			ha := &fakeHA{}
			r := New(testConfig(), sched, ha, &fakeReceipts{}, testLogger(t))

			if err := r.cycle(context.Background()); err != nil {
				t.Fatalf("cycle: %v", err)
			}
			if len(ha.calls) != 1 {
				t.Fatalf("HA calls = %d, want 1", len(ha.calls))
			}
			call := ha.calls[0]
			if call.script != "schedule_announce" {
				t.Errorf("script = %q, want schedule_announce", call.script)
			}
			if call.variables["chime"] != true {
				t.Errorf("chime = %#v, want true", call.variables["chime"])
			}
			if call.variables["message"] != tc.wantMsg {
				t.Errorf("message = %#v, want %q", call.variables["message"], tc.wantMsg)
			}
			if call.variables["kind"] != tc.wantKind {
				t.Errorf("kind = %#v, want %q", call.variables["kind"], tc.wantKind)
			}
			if len(sched.acks) != 1 || sched.acks[0].outcome != scheduler.AckDelivered {
				t.Errorf("acks = %+v", sched.acks)
			}
		})
	}
}

func TestAlarmStopOnlySilencesTheActiveAlarm(t *testing.T) {
	fire := func(id, source string) scheduler.Event {
		return scheduler.Event{
			ID: id, SourceID: source, SourceKind: scheduler.SourceKindAlarm,
			Label: source, Queue: "home", Action: scheduler.ActionFire,
		}
	}
	stop := func(id, source string) scheduler.Event {
		event := fire(id, source)
		event.Action = scheduler.ActionStop
		return event
	}

	sched := &fakeScheduler{}
	ha := &fakeHA{}
	receipts := &fakeReceipts{}
	r := New(testConfig(), sched, ha, receipts, testLogger(t))

	runBatch := func(events ...scheduler.Event) {
		t.Helper()
		sched.events = events
		if err := r.cycle(context.Background()); err != nil {
			t.Fatalf("cycle: %v", err)
		}
	}

	runBatch(fire("evt-a-fire", "alarm-a"))
	if len(ha.calls) != 1 || ha.calls[0].script != "alarm_start" {
		t.Fatalf("alarm A fire calls = %+v", ha.calls)
	}
	if active, ok := receipts.ActiveAlarm(); !ok || active.SourceID != "alarm-a" {
		t.Fatalf("active alarm = %+v, %v; want alarm-a", active, ok)
	}

	runBatch(fire("evt-b-fire", "alarm-b"))
	if len(ha.calls) != 2 || ha.calls[1].script != "alarm_start" {
		t.Fatalf("alarm B fire calls = %+v", ha.calls)
	}

	// A stale STOP for alarm A must not silence the ringing alarm B.
	runBatch(stop("evt-a-stop", "alarm-a"))
	if len(ha.calls) != 2 {
		t.Fatalf("alarm A STOP called HA: %+v", ha.calls[len(ha.calls)-1:])
	}
	if last := sched.acks[len(sched.acks)-1]; last.outcome != scheduler.AckSkipped {
		t.Errorf("alarm A STOP outcome = %q, want SKIPPED", last.outcome)
	}
	if active, _ := receipts.ActiveAlarm(); active.SourceID != "alarm-b" {
		t.Fatalf("alarm A STOP cleared active alarm B: %+v", active)
	}

	// The STOP for the active alarm is allowed and clears the marker.
	runBatch(stop("evt-b-stop", "alarm-b"))
	if len(ha.calls) != 3 || ha.calls[2].script != "alarm_stop" {
		t.Fatalf("alarm B STOP calls = %+v", ha.calls)
	}
	if ha.calls[2].variables["reason"] != "scheduler_stop" {
		t.Errorf("stop reason = %#v", ha.calls[2].variables["reason"])
	}
	if strings.Contains(ha.calls[2].script, "snooze") {
		t.Errorf("STOP called %q: snooze must stay owned by the scheduler", ha.calls[2].script)
	}
	if last := sched.acks[len(sched.acks)-1]; last.outcome != scheduler.AckDelivered {
		t.Errorf("alarm B STOP outcome = %q, want DELIVERED", last.outcome)
	}
	if _, ok := receipts.ActiveAlarm(); ok {
		t.Error("active alarm marker was not cleared after the matching STOP")
	}
}

func TestStaleAlarmStopWithoutMarkerIsSkipped(t *testing.T) {
	sched := &fakeScheduler{events: []scheduler.Event{{
		ID: "evt-stale", SourceID: "alarm-gone", SourceKind: scheduler.SourceKindAlarm,
		Queue: "home", Action: scheduler.ActionStop,
	}}}
	ha := &fakeHA{}
	r := New(testConfig(), sched, ha, &fakeReceipts{}, testLogger(t))

	if err := r.cycle(context.Background()); err != nil {
		t.Fatalf("cycle: %v", err)
	}
	if len(ha.calls) != 0 {
		t.Fatalf("stale STOP called HA: %+v", ha.calls)
	}
	if len(sched.acks) != 1 || sched.acks[0].outcome != scheduler.AckSkipped {
		t.Fatalf("acks = %+v, want one SKIPPED ack", sched.acks)
	}
}

func TestAlarmStopWorksAfterLedgerRestart(t *testing.T) {
	active := ledger.ActiveAlarm{
		SourceID: "alarm-a", EventID: "evt-a-fire",
		OccurrenceUnix: 1700000000, FireDueUnix: 1700000060,
	}
	receipts := &fakeReceipts{active: &active}
	sched := &fakeScheduler{events: []scheduler.Event{{
		ID: "evt-a-stop", SourceID: "alarm-a", SourceKind: scheduler.SourceKindAlarm,
		OccurrenceUnix: 1700000000, DueUnix: 1700000120,
		Queue: "home", Action: scheduler.ActionStop,
	}}}
	ha := &fakeHA{}
	r := New(testConfig(), sched, ha, receipts, testLogger(t))

	if err := r.cycle(context.Background()); err != nil {
		t.Fatalf("cycle: %v", err)
	}
	if len(ha.calls) != 1 || ha.calls[0].script != "alarm_stop" {
		t.Fatalf("HA calls = %+v, want the restored marker to permit STOP", ha.calls)
	}
	if len(sched.acks) != 1 || sched.acks[0].outcome != scheduler.AckDelivered {
		t.Errorf("acks = %+v", sched.acks)
	}
}

func TestTimerAndReminderStopNeverUseGlobalAudioStop(t *testing.T) {
	for _, kind := range []scheduler.SourceKind{
		scheduler.SourceKindTimer,
		scheduler.SourceKindReminder,
	} {
		t.Run(string(kind), func(t *testing.T) {
			sched := &fakeScheduler{events: []scheduler.Event{{
				ID: "evt-stop", SourceID: "src-1", SourceKind: kind,
				Label: "Pizza", Queue: "home", Action: scheduler.ActionStop,
			}}}
			ha := &fakeHA{}
			r := New(testConfig(), sched, ha, &fakeReceipts{}, testLogger(t))

			if err := r.cycle(context.Background()); err != nil {
				t.Fatalf("cycle: %v", err)
			}
			if len(ha.calls) != 0 {
				t.Fatalf("HA calls = %+v; a one-shot announcement has no safe global stop", ha.calls)
			}
			if len(sched.acks) != 1 || sched.acks[0].outcome != scheduler.AckSkipped {
				t.Fatalf("acks = %+v, want one SKIPPED ack", sched.acks)
			}
		})
	}
}

func TestStopForTaskKindFailsClosed(t *testing.T) {
	sched := &fakeScheduler{events: []scheduler.Event{{
		ID: "evt-task-stop", SourceKind: scheduler.SourceKindTask, TaskType: "generic.ping",
		Queue: "home", Action: scheduler.ActionStop,
	}}}
	ha := &fakeHA{}
	r := New(testConfig(), sched, ha, &fakeReceipts{}, testLogger(t))

	if err := r.cycle(context.Background()); err != nil {
		t.Fatalf("cycle: %v", err)
	}
	if len(ha.calls) != 0 {
		t.Fatalf("HA calls = %+v; a task STOP must never run a script", ha.calls)
	}
	if len(sched.acks) != 1 || sched.acks[0].outcome != scheduler.AckFailed {
		t.Fatalf("acks = %+v, want a single FAILED ack", sched.acks)
	}
}

func TestUnsupportedEventsFailClosed(t *testing.T) {
	cases := []struct {
		name  string
		event scheduler.Event
	}{
		{
			name: "task fire",
			event: scheduler.Event{
				ID: "evt-task", SourceKind: scheduler.SourceKindTask, TaskType: "generic.ping",
				Queue: "home", Action: scheduler.ActionFire,
			},
		},
		{
			name: "scenario",
			event: scheduler.Event{
				ID: "evt-scenario", SourceKind: scheduler.SourceKindScenario,
				Queue: "home", Action: scheduler.ActionFire,
			},
		},
		{
			name: "unspecified kind",
			event: scheduler.Event{
				ID: "evt-unknown", SourceKind: scheduler.SourceKindUnspecified,
				Queue: "home", Action: scheduler.ActionFire,
			},
		},
		{
			name: "unknown action",
			event: scheduler.Event{
				ID: "evt-action", SourceKind: scheduler.SourceKindAlarm,
				Queue: "home", Action: scheduler.Action("EXPLODE"),
			},
		},
	}

	for _, tc := range cases {
		t.Run(tc.name, func(t *testing.T) {
			sched := &fakeScheduler{events: []scheduler.Event{tc.event}}
			ha := &fakeHA{}
			r := New(testConfig(), sched, ha, &fakeReceipts{}, testLogger(t))

			if err := r.cycle(context.Background()); err != nil {
				t.Fatalf("cycle: %v", err)
			}
			if len(ha.calls) != 0 {
				t.Errorf("HA calls = %+v, want none", ha.calls)
			}
			if len(sched.acks) != 1 || sched.acks[0].outcome != scheduler.AckFailed {
				t.Errorf("acks = %+v, want one FAILED ack", sched.acks)
			}
		})
	}
}

func TestEventFromAnotherQueueFailsClosed(t *testing.T) {
	event := alarmEvent()
	event.Queue = "tasks"

	sched := &fakeScheduler{events: []scheduler.Event{event}}
	ha := &fakeHA{}
	r := New(testConfig(), sched, ha, &fakeReceipts{}, testLogger(t))

	if err := r.cycle(context.Background()); err != nil {
		t.Fatalf("cycle: %v", err)
	}
	if len(ha.calls) != 0 {
		t.Errorf("HA calls = %+v, want none for a foreign queue", ha.calls)
	}
	if len(sched.acks) != 1 || sched.acks[0].outcome != scheduler.AckFailed {
		t.Errorf("acks = %+v, want one FAILED ack", sched.acks)
	}
}

func TestHAFailureNeverAcksSuccess(t *testing.T) {
	for _, tc := range []struct {
		name string
		err  error
		acks bool
	}{
		{"transient 500", &httpapi.Error{Op: "call script", StatusCode: http.StatusInternalServerError}, false},
		{"transient network", &httpapi.Error{Op: "call script", Err: errors.New("connection reset")}, false},
		{"permanent 404", &httpapi.Error{Op: "call script", StatusCode: http.StatusNotFound}, true},
	} {
		t.Run(tc.name, func(t *testing.T) {
			sched := &fakeScheduler{events: []scheduler.Event{alarmEvent()}}
			ha := &fakeHA{err: tc.err}
			receipts := &fakeReceipts{}
			r := New(testConfig(), sched, ha, receipts, testLogger(t))

			if err := r.cycle(context.Background()); err != nil {
				t.Fatalf("cycle: %v", err)
			}
			if len(receipts.recorded) != 0 {
				t.Errorf("receipt recorded despite HA failure: %+v", receipts.recorded)
			}
			if !tc.acks {
				if len(sched.acks) != 0 {
					t.Fatalf("acks = %+v; a transient HA failure must leave the event for lease expiry", sched.acks)
				}
				return
			}
			if len(sched.acks) != 1 {
				t.Fatalf("acks = %+v, want one FAILED ack", sched.acks)
			}
			if sched.acks[0].outcome == scheduler.AckDelivered {
				t.Fatal("a failed HA call must never be acknowledged as delivered")
			}
			if sched.acks[0].outcome != scheduler.AckFailed {
				t.Errorf("outcome = %q, want FAILED", sched.acks[0].outcome)
			}
		})
	}
}

func TestAuthFailureRetriesUntilAttemptLimit(t *testing.T) {
	authErr := &httpapi.Error{Op: "call script", StatusCode: http.StatusUnauthorized}

	t.Run("below limit", func(t *testing.T) {
		event := alarmEvent()
		event.Attempts = 3
		sched := &fakeScheduler{events: []scheduler.Event{event}}
		r := New(testConfig(), sched, &fakeHA{err: authErr}, &fakeReceipts{}, testLogger(t))

		if err := r.cycle(context.Background()); err != nil {
			t.Fatalf("cycle: %v", err)
		}
		if len(sched.acks) != 0 {
			t.Fatalf("acks = %+v; below the attempt limit the lease must expire instead", sched.acks)
		}
	})

	t.Run("one below limit", func(t *testing.T) {
		event := alarmEvent()
		event.Attempts = 9 // the server increments on lease, so 10 is reached next time
		sched := &fakeScheduler{events: []scheduler.Event{event}}
		r := New(testConfig(), sched, &fakeHA{err: authErr}, &fakeReceipts{}, testLogger(t))

		if err := r.cycle(context.Background()); err != nil {
			t.Fatalf("cycle: %v", err)
		}
		if len(sched.acks) != 0 {
			t.Fatalf("acks = %+v; attempts are already incremented by the server", sched.acks)
		}
	})

	t.Run("at limit", func(t *testing.T) {
		event := alarmEvent()
		event.Attempts = 10 // the server already incremented this when leasing
		sched := &fakeScheduler{events: []scheduler.Event{event}}
		r := New(testConfig(), sched, &fakeHA{err: authErr}, &fakeReceipts{}, testLogger(t))

		if err := r.cycle(context.Background()); err != nil {
			t.Fatalf("cycle: %v", err)
		}
		if len(sched.acks) != 1 || sched.acks[0].outcome != scheduler.AckFailed {
			t.Fatalf("acks = %+v, want one FAILED ack", sched.acks)
		}
	})
}

func TestDuplicateDeliveryIsAcknowledgedWithoutSideEffect(t *testing.T) {
	sched := &fakeScheduler{events: []scheduler.Event{alarmEvent()}}
	ha := &fakeHA{}
	receipts := &fakeReceipts{seen: map[string]bool{"evt-alarm": true}}
	r := New(testConfig(), sched, ha, receipts, testLogger(t))

	if err := r.cycle(context.Background()); err != nil {
		t.Fatalf("cycle: %v", err)
	}
	if len(ha.calls) != 0 {
		t.Errorf("HA was called again for a duplicate: %+v", ha.calls)
	}
	if len(sched.acks) != 1 || sched.acks[0].outcome != scheduler.AckDelivered {
		t.Errorf("acks = %+v, want one DELIVERED ack", sched.acks)
	}
}

func TestLedgerWriteFailureStillAcksAfterHASuccess(t *testing.T) {
	sched := &fakeScheduler{events: []scheduler.Event{alarmEvent()}}
	ha := &fakeHA{}
	receipts := &fakeReceipts{recordErr: errors.New("disk full")}
	logger, logs := testLoggerWithBuffer(t)
	r := New(testConfig(), sched, ha, receipts, logger)

	if err := r.cycle(context.Background()); err != nil {
		t.Fatalf("cycle: %v", err)
	}
	if len(ha.calls) != 1 {
		t.Fatalf("HA calls = %d, want 1", len(ha.calls))
	}
	if len(sched.acks) != 1 || sched.acks[0].outcome != scheduler.AckDelivered {
		t.Fatalf("acks = %+v; HA already accepted the call, so the event is delivered", sched.acks)
	}
	if !strings.Contains(logs.String(), "marker write failed") {
		t.Errorf("ledger failure was not logged: %s", logs.String())
	}
}

func TestAckFailureIsLogged(t *testing.T) {
	sched := &fakeScheduler{events: []scheduler.Event{alarmEvent()}, ackErr: errors.New("unavailable")}
	logger, logs := testLoggerWithBuffer(t)
	r := New(testConfig(), sched, &fakeHA{}, &fakeReceipts{}, logger)

	if err := r.cycle(context.Background()); err != nil {
		t.Fatalf("cycle: %v", err)
	}
	if !strings.Contains(logs.String(), "acknowledgement failed") {
		t.Errorf("ack failure was not logged: %s", logs.String())
	}
}

func TestStopForUnstartedAlarmDoesNotSilenceTheFire(t *testing.T) {
	fire := alarmEvent()
	fire.ID = "evt-fire"
	fire.DueUnix = 100

	stop := fire
	stop.ID = "evt-stop"
	stop.Action = scheduler.ActionStop

	// STOP is ordered before FIRE at the same due time. Nothing has rung yet,
	// so the stop must be skipped and the fire must still start the alarm.
	sched := &fakeScheduler{events: []scheduler.Event{fire, stop}}
	ha := &fakeHA{}
	r := New(testConfig(), sched, ha, &fakeReceipts{}, testLogger(t))

	if err := r.cycle(context.Background()); err != nil {
		t.Fatalf("cycle: %v", err)
	}
	if len(ha.calls) != 1 || ha.calls[0].script != "alarm_start" {
		t.Fatalf("HA calls = %+v, want only the FIRE", ha.calls)
	}
	if len(sched.acks) != 2 {
		t.Fatalf("acks = %+v, want SKIPPED then DELIVERED", sched.acks)
	}
	if sched.acks[0].outcome != scheduler.AckSkipped || sched.acks[1].outcome != scheduler.AckDelivered {
		t.Errorf("ack outcomes = %q/%q", sched.acks[0].outcome, sched.acks[1].outcome)
	}
}

func TestOrderEventsGroupsBySourceAndTime(t *testing.T) {
	events := []scheduler.Event{
		{ID: "b-2", SourceID: "b", DueUnix: 20},
		{ID: "a-2", SourceID: "a", DueUnix: 10, Action: scheduler.ActionFire},
		{ID: "a-1", SourceID: "a", DueUnix: 10, Action: scheduler.ActionStop},
		{ID: "a-0", SourceID: "a", DueUnix: 5},
	}
	ordered := orderEvents(events)
	got := []string{ordered[0].ID, ordered[1].ID, ordered[2].ID, ordered[3].ID}
	want := []string{"a-0", "a-1", "a-2", "b-2"}
	if strings.Join(got, ",") != strings.Join(want, ",") {
		t.Errorf("order = %v, want %v", got, want)
	}
}

func TestEmptyBatchDoesNothing(t *testing.T) {
	sched := &fakeScheduler{}
	ha := &fakeHA{}
	r := New(testConfig(), sched, ha, &fakeReceipts{}, testLogger(t))

	if err := r.cycle(context.Background()); err != nil {
		t.Fatalf("cycle: %v", err)
	}
	if len(ha.calls) != 0 || len(sched.acks) != 0 {
		t.Errorf("HA calls/acks = %d/%d, want 0/0", len(ha.calls), len(sched.acks))
	}
}

func TestEventWithoutIDIsIgnored(t *testing.T) {
	sched := &fakeScheduler{events: []scheduler.Event{{SourceKind: scheduler.SourceKindAlarm, Action: scheduler.ActionFire}}}
	ha := &fakeHA{}
	r := New(testConfig(), sched, ha, &fakeReceipts{}, testLogger(t))

	if err := r.cycle(context.Background()); err != nil {
		t.Fatalf("cycle: %v", err)
	}
	if len(ha.calls) != 0 || len(sched.acks) != 0 {
		t.Errorf("event without id caused HA call or ack: %+v %+v", ha.calls, sched.acks)
	}
}

func TestCancelledContextSkipsRemainingEvents(t *testing.T) {
	ctx, cancel := context.WithCancel(context.Background())
	cancel()

	sched := &fakeScheduler{events: []scheduler.Event{alarmEvent()}}
	ha := &fakeHA{}
	r := New(testConfig(), sched, ha, &fakeReceipts{}, testLogger(t))

	if err := r.cycle(ctx); err == nil {
		t.Fatal("cycle with a cancelled context must return the cancellation")
	}
	if len(ha.calls) != 0 || len(sched.acks) != 0 {
		t.Errorf("cancelled cycle still called HA or acked: %+v %+v", ha.calls, sched.acks)
	}
}

func TestCancelledHACallIsNotAcknowledged(t *testing.T) {
	ctx, cancel := context.WithCancel(context.Background())
	cancel()

	sched := &fakeScheduler{}
	ha := &fakeHA{err: context.Canceled}
	r := New(testConfig(), sched, ha, &fakeReceipts{}, testLogger(t))

	r.handle(ctx, alarmEvent())
	if len(sched.acks) != 0 {
		t.Errorf("acks = %+v; a cancelled call must leave the event unacknowledged", sched.acks)
	}
}

func TestBackoffGrowsAndShutdownIsGraceful(t *testing.T) {
	cfg := testConfig()
	cfg.PollInterval = 2 * time.Millisecond
	cfg.MaxBackoff = 100 * time.Millisecond

	sched := &fakeScheduler{listErr: errors.New("scheduler unavailable")}
	r := New(cfg, sched, &fakeHA{}, &fakeReceipts{}, testLogger(t))

	var waits []time.Duration
	r.sleep = func(_ context.Context, d time.Duration) error {
		waits = append(waits, d)
		if len(waits) == 3 {
			return context.Canceled
		}
		return nil
	}

	if err := r.Run(context.Background()); err != nil {
		t.Fatalf("Run: %v", err)
	}
	if sched.listCalls != 3 {
		t.Errorf("list calls = %d, want 3", sched.listCalls)
	}
	want := []time.Duration{2 * time.Millisecond, 4 * time.Millisecond, 8 * time.Millisecond}
	if len(waits) != len(want) {
		t.Fatalf("sleeps = %v, want %v", waits, want)
	}
	for i := range want {
		if waits[i] != want[i] {
			t.Errorf("sleep %d = %s, want %s", i+1, waits[i], want[i])
		}
	}
}

func TestBackoffSequence(t *testing.T) {
	cfg := testConfig()
	cfg.PollInterval = time.Second
	cfg.MaxBackoff = 5 * time.Second
	r := New(cfg, &fakeScheduler{}, &fakeHA{}, nil, testLogger(t))

	want := []time.Duration{1 * time.Second, 2 * time.Second, 4 * time.Second, 5 * time.Second, 5 * time.Second}
	for i, expected := range want {
		if got := r.backoff(i + 1); got != expected {
			t.Errorf("backoff(%d) = %s, want %s", i+1, got, expected)
		}
	}
}

func TestExecuteRequiresHAClient(t *testing.T) {
	err := New(testConfig(), &fakeScheduler{}, nil, nil, testLogger(t)).Run(context.Background())
	if err == nil || !strings.Contains(err.Error(), "Home Assistant client") {
		t.Fatalf("err = %v, want a missing client error", err)
	}
}

func TestStaleStopForNewerOccurrenceIsSkipped(t *testing.T) {
	active := ledger.ActiveAlarm{
		SourceID:       "alarm-a",
		EventID:        "evt-new-fire",
		OccurrenceUnix: 1700003600,
		FireDueUnix:    1700003600,
	}
	receipts := &fakeReceipts{active: &active}
	sched := &fakeScheduler{events: []scheduler.Event{{
		ID: "evt-old-stop", SourceID: "alarm-a", SourceKind: scheduler.SourceKindAlarm,
		// Older occurrence, later delivery: same source must still not match.
		OccurrenceUnix: 1700000000, DueUnix: 1700004000,
		Queue: "home", Action: scheduler.ActionStop,
	}}}
	ha := &fakeHA{}
	r := New(testConfig(), sched, ha, receipts, testLogger(t))

	if err := r.cycle(context.Background()); err != nil {
		t.Fatalf("cycle: %v", err)
	}
	if len(ha.calls) != 0 {
		t.Fatalf("stale occurrence STOP called HA: %+v", ha.calls)
	}
	if len(sched.acks) != 1 || sched.acks[0].outcome != scheduler.AckSkipped {
		t.Fatalf("acks = %+v, want one SKIPPED ack", sched.acks)
	}
	if active, _ := receipts.ActiveAlarm(); active.OccurrenceUnix != 1700003600 {
		t.Errorf("newer occurrence marker was cleared: %+v", active)
	}
}

func TestSnoozedRefireRejectsOlderStopButAllowsCurrentStop(t *testing.T) {
	occurrence := int64(1700000000)
	fireDue := int64(1700000600) // snooze delayed re-fire at t+10m

	sched := &fakeScheduler{}
	receipts := &fakeReceipts{active: &ledger.ActiveAlarm{
		SourceID:       "alarm-a",
		EventID:        "evt-refire",
		OccurrenceUnix: occurrence,
		FireDueUnix:    fireDue,
	}}
	ha := &fakeHA{}
	r := New(testConfig(), sched, ha, receipts, testLogger(t))

	runStop := func(id string, due int64) {
		t.Helper()
		sched.events = []scheduler.Event{{
			ID: id, SourceID: "alarm-a", SourceKind: scheduler.SourceKindAlarm,
			OccurrenceUnix: occurrence, DueUnix: due,
			Queue: "home", Action: scheduler.ActionStop,
		}}
		if err := r.cycle(context.Background()); err != nil {
			t.Fatalf("cycle: %v", err)
		}
	}

	// The old STOP from the first ring is older than the snoozed FIRE due.
	runStop("evt-old-stop", fireDue-300)
	if len(ha.calls) != 0 {
		t.Fatalf("old STOP after snooze called HA: %+v", ha.calls)
	}
	if last := sched.acks[len(sched.acks)-1]; last.outcome != scheduler.AckSkipped {
		t.Errorf("old STOP outcome = %q, want SKIPPED", last.outcome)
	}

	// A STOP at or after the re-fire due time is the current one.
	runStop("evt-current-stop", fireDue+30)
	if len(ha.calls) != 1 || ha.calls[0].script != "alarm_stop" {
		t.Fatalf("current STOP calls = %+v", ha.calls)
	}
	if last := sched.acks[len(sched.acks)-1]; last.outcome != scheduler.AckDelivered {
		t.Errorf("current STOP outcome = %q, want DELIVERED", last.outcome)
	}
}
