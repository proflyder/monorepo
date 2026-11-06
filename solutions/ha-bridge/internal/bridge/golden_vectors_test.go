package bridge

import (
	"context"
	"encoding/json"
	"flag"
	"fmt"
	"net/http"
	"os"
	"path/filepath"
	"testing"

	"github.com/proflyder/monorepo/packages/go/httpapi"
	"github.com/proflyder/monorepo/packages/go/scheduler"
	"github.com/proflyder/monorepo/solutions/ha-bridge/internal/ledger"
)

// updateGolden rewrites testdata/events/*.json from the Go bridge. The files are
// the shared golden table: the Rust executor must produce the same script,
// variables and ack outcome for the same wire event.
var updateGolden = flag.Bool("update", false, "rewrite ha-bridge golden vectors")

// goldenVector is one wire event plus the observable result of handling it.
// The event is a protojson DueEvent object, so the Rust test feeds it through
// the real wire conversion before asking the handler.
type goldenVector struct {
	Name        string              `json:"name"`
	Event       map[string]any      `json:"event"`
	ActiveAlarm *ledger.ActiveAlarm `json:"activeAlarm,omitempty"`
	Seen        bool                `json:"seen,omitempty"`
	HAError     string              `json:"haError,omitempty"`
	Expected    goldenExpected      `json:"expected"`
}

type goldenExpected struct {
	Outcome   string         `json:"outcome"`
	Detail    string         `json:"detail,omitempty"`
	Script    string         `json:"script,omitempty"`
	Variables map[string]any `json:"variables,omitempty"`
}

type goldenCase struct {
	name    string
	event   scheduler.Event
	active  *ledger.ActiveAlarm
	seen    bool
	haError error
	// wireAction overrides the serialized enum so a legacy UNSPECIFIED value
	// is exercised even though the client normalizes it to FIRE before the
	// bridge sees the event.
	wireAction string
}

func goldenCaseVectors() map[string][]goldenCase {
	alarmFire := func(id, source, label string) scheduler.Event {
		return scheduler.Event{
			ID: id, SourceID: source, SourceKind: scheduler.SourceKindAlarm,
			Label: label, Queue: configQueueHome(), Action: scheduler.ActionFire,
		}
	}
	stopOf := func(ev scheduler.Event, occurrence, due int64) scheduler.Event {
		ev.Action = scheduler.ActionStop
		ev.OccurrenceUnix = occurrence
		ev.DueUnix = due
		return ev
	}
	return map[string][]goldenCase{
		"fire": {
			{
				name:  "alarm fire uses alarm_start",
				event: alarmFire("evt-alarm", "alarm-1", "Wake up"),
			},
			{
				name: "legacy unspecified action is fire",
				event: func() scheduler.Event {
					ev := alarmFire("evt-legacy", "alarm-1", "Wake up")
					ev.Action = scheduler.ActionFire
					return ev
				}(),
				wireAction: "EVENT_ACTION_UNSPECIFIED",
			},
			{
				name: "timer fire announces the label",
				event: scheduler.Event{
					ID: "evt-timer", SourceID: "timer-1", SourceKind: scheduler.SourceKindTimer,
					Label: "Пицца", Queue: configQueueHome(), Action: scheduler.ActionFire,
				},
			},
			{
				name: "reminder without a label announces the kind",
				event: scheduler.Event{
					ID: "evt-reminder", SourceID: "reminder-1", SourceKind: scheduler.SourceKindReminder,
					Queue: configQueueHome(), Action: scheduler.ActionFire,
				},
			},
		},
		"stop": {
			{
				name:   "stop silences the active alarm",
				event:  stopOf(alarmFire("evt-a-stop", "alarm-a", "Wake up"), 1700000000, 1700000120),
				active: &ledger.ActiveAlarm{SourceID: "alarm-a", EventID: "evt-a-fire", OccurrenceUnix: 1700000000, FireDueUnix: 1700000060, SinceUnix: 1700000060},
			},
			{
				name:  "stop without an active marker is skipped",
				event: stopOf(alarmFire("evt-stale-stop", "alarm-gone", "Gone"), 0, 0),
			},
			{
				name:   "stop for an older occurrence is skipped",
				event:  stopOf(alarmFire("evt-old-stop", "alarm-a", "Wake up"), 1600000000, 1700004000),
				active: &ledger.ActiveAlarm{SourceID: "alarm-a", EventID: "evt-new-fire", OccurrenceUnix: 1700003600, FireDueUnix: 1700003600, SinceUnix: 1700003600},
			},
			{
				name:   "stop before the snoozed re-fire due is skipped",
				event:  stopOf(alarmFire("evt-early-stop", "alarm-a", "Wake up"), 1700000000, 1700000300),
				active: &ledger.ActiveAlarm{SourceID: "alarm-a", EventID: "evt-refire", OccurrenceUnix: 1700000000, FireDueUnix: 1700000600, SinceUnix: 1700000600},
			},
			{
				name: "timer stop is skipped without a global stop",
				event: scheduler.Event{
					ID: "evt-timer-stop", SourceID: "timer-1", SourceKind: scheduler.SourceKindTimer,
					Label: "Пицца", Queue: configQueueHome(), Action: scheduler.ActionStop,
				},
			},
			{
				name: "reminder stop is skipped without a global stop",
				event: scheduler.Event{
					ID: "evt-reminder-stop", SourceID: "reminder-1", SourceKind: scheduler.SourceKindReminder,
					Label: "Позвонить", Queue: configQueueHome(), Action: scheduler.ActionStop,
				},
			},
		},
		"rejected": {
			{
				name: "generic task fire is rejected",
				event: scheduler.Event{
					ID: "evt-task", SourceKind: scheduler.SourceKindTask, TaskType: "generic.ping",
					Queue: configQueueHome(), Action: scheduler.ActionFire,
				},
			},
			{
				name: "generic task stop is rejected",
				event: scheduler.Event{
					ID: "evt-task-stop", SourceKind: scheduler.SourceKindTask, TaskType: "generic.ping",
					Queue: configQueueHome(), Action: scheduler.ActionStop,
				},
			},
			{
				name: "scenario fire is rejected",
				event: scheduler.Event{
					ID: "evt-scenario", SourceID: "scenario-1", SourceKind: scheduler.SourceKindScenario,
					Queue: configQueueHome(), Action: scheduler.ActionFire,
				},
			},
			{
				name: "unspecified kind is rejected",
				event: scheduler.Event{
					ID: "evt-unknown", Queue: configQueueHome(), Action: scheduler.ActionFire,
				},
			},
			{
				// The wire value 99 is normalized by the client to `ENUM-99`;
				// the handler must fail it closed without guessing FIRE.
				name: "unknown action is rejected",
				event: scheduler.Event{
					ID: "evt-action", SourceID: "alarm-1", SourceKind: scheduler.SourceKindAlarm,
					Queue: configQueueHome(), Action: scheduler.Action("ENUM-99"),
				},
			},
			{
				name: "unknown source kind is rejected",
				event: scheduler.Event{
					ID: "evt-source", SourceID: "src-1", SourceKind: scheduler.SourceKind("enum-77"),
					Queue: configQueueHome(), Action: scheduler.ActionFire,
				},
			},
			{
				name: "foreign queue is rejected",
				event: func() scheduler.Event {
					ev := alarmFire("evt-foreign", "alarm-1", "Wake up")
					ev.Queue = configQueueTasks()
					return ev
				}(),
			},
			{
				name: "event without id is ignored",
				event: scheduler.Event{
					SourceKind: scheduler.SourceKindAlarm, Queue: configQueueHome(), Action: scheduler.ActionFire,
				},
			},
		},
		"delivery": {
			{
				name:  "duplicate delivery is acknowledged without a side effect",
				event: alarmFire("evt-alarm", "alarm-1", "Wake up"),
				seen:  true,
			},
			{
				name:    "permanent HA rejection fails the event",
				event:   alarmFire("evt-alarm", "alarm-1", "Wake up"),
				haError: &httpapi.Error{Op: "call script", StatusCode: http.StatusNotFound},
			},
			{
				name:    "transient HA failure leaves the event for retry",
				event:   alarmFire("evt-alarm", "alarm-1", "Wake up"),
				haError: &httpapi.Error{Op: "call script", StatusCode: http.StatusInternalServerError},
			},
			{
				name: "unauthorized below the attempt limit is retried",
				event: func() scheduler.Event {
					ev := alarmFire("evt-alarm", "alarm-1", "Wake up")
					ev.Attempts = 9
					return ev
				}(),
				haError: &httpapi.Error{Op: "call script", StatusCode: http.StatusUnauthorized},
			},
			{
				name: "unauthorized at the attempt limit is failed",
				event: func() scheduler.Event {
					ev := alarmFire("evt-alarm", "alarm-1", "Wake up")
					ev.Attempts = 10
					return ev
				}(),
				haError: &httpapi.Error{Op: "call script", StatusCode: http.StatusUnauthorized},
			},
			{
				name: "forbidden at the attempt limit is failed",
				event: func() scheduler.Event {
					ev := alarmFire("evt-alarm", "alarm-1", "Wake up")
					ev.Attempts = 10
					return ev
				}(),
				haError: &httpapi.Error{Op: "call script", StatusCode: http.StatusForbidden},
			},
			{
				name:    "permanent rejection ignores the attempt limit",
				event:   alarmFire("evt-alarm", "alarm-1", "Wake up"),
				haError: &httpapi.Error{Op: "call script", StatusCode: http.StatusBadRequest},
			},
		},
	}
}

func configQueueHome() string  { return "home" }
func configQueueTasks() string { return "tasks" }

// observe runs one event through the real bridge cycle and returns the
// externally visible result. Retry means "no terminal ack": the lease expires
// and the scheduler re-delivers the event.
func observe(t *testing.T, tc goldenCase) goldenVector {
	t.Helper()
	cfg := testConfig()
	sched := &fakeScheduler{events: []scheduler.Event{tc.event}}
	ha := &fakeHA{err: tc.haError}
	receipts := &fakeReceipts{}
	if tc.seen {
		receipts.seen = map[string]bool{tc.event.ID: true}
	}
	if tc.active != nil {
		active := *tc.active
		receipts.active = &active
	}

	runner := New(cfg, sched, ha, receipts, testLogger(t))
	if err := runner.cycle(context.Background()); err != nil {
		t.Fatalf("%s: cycle: %v", tc.name, err)
	}

	expected := goldenExpected{Outcome: "RETRY"}
	if len(sched.acks) > 0 {
		expected.Outcome = sched.acks[0].outcome
		expected.Detail = sched.acks[0].detail
	}
	if len(ha.calls) > 0 {
		expected.Script = ha.calls[0].script
		expected.Variables = ha.calls[0].variables
	}

	return goldenVector{
		Name:        tc.name,
		Event:       wireEvent(tc.event, tc.wireAction),
		ActiveAlarm: tc.active,
		Seen:        tc.seen,
		HAError:     haErrorName(tc.haError),
		Expected:    expected,
	}
}

// wireEvent renders the scheduler event as a protojson DueEvent object. Enum
// names and quoted int64 values keep the fixture identical to the wire shape
// the Rust client decodes.
func wireEvent(ev scheduler.Event, wireAction string) map[string]any {
	out := map[string]any{"id": ev.ID}
	if ev.SourceID != "" {
		out["sourceId"] = ev.SourceID
	}
	if name := sourceKindName(ev.SourceKind); name != nil {
		out["sourceKind"] = name
	}
	if ev.TaskType != "" {
		out["taskType"] = ev.TaskType
	}
	if ev.Queue != "" {
		out["queue"] = ev.Queue
	}
	if ev.Label != "" {
		out["label"] = ev.Label
	}
	if ev.DueUnix != 0 {
		out["dueUnix"] = fmt.Sprintf("%d", ev.DueUnix)
	}
	if ev.OccurrenceUnix != 0 {
		out["occurrenceUnix"] = fmt.Sprintf("%d", ev.OccurrenceUnix)
	}
	if ev.Attempts != 0 {
		out["attempts"] = ev.Attempts
	}
	if wireAction != "" {
		out["action"] = wireAction
	} else if name := actionName(ev.Action); name != nil {
		out["action"] = name
	}
	return out
}

func sourceKindName(kind scheduler.SourceKind) any {
	switch kind {
	case scheduler.SourceKindUnspecified:
		return nil
	case scheduler.SourceKindAlarm:
		return "SOURCE_KIND_ALARM"
	case scheduler.SourceKindReminder:
		return "SOURCE_KIND_REMINDER"
	case scheduler.SourceKindTimer:
		return "SOURCE_KIND_TIMER"
	case scheduler.SourceKindScenario:
		return "SOURCE_KIND_SCENARIO"
	case scheduler.SourceKindTask:
		return "SOURCE_KIND_TASK"
	case scheduler.SourceKind("enum-77"):
		// Unknown wire value: Go normalizes it to `enum-<n>`; the golden event
		// carries the raw number so the Rust side sees the same value.
		return 77
	default:
		return nil
	}
}

func actionName(action scheduler.Action) any {
	switch action {
	case scheduler.ActionFire, "":
		return "EVENT_ACTION_FIRE"
	case scheduler.ActionStop:
		return "EVENT_ACTION_STOP"
	default:
		return 99
	}
}

func haErrorName(err error) string {
	if err == nil {
		return ""
	}
	var apiErr *httpapi.Error
	if ok := asHTTPAPIError(err, &apiErr); ok && apiErr.StatusCode != 0 {
		switch apiErr.StatusCode {
		case http.StatusNotFound, http.StatusBadRequest:
			return fmt.Sprintf("permanent-%d", apiErr.StatusCode)
		case http.StatusUnauthorized, http.StatusForbidden:
			return fmt.Sprintf("permanent-%d", apiErr.StatusCode)
		default:
			return fmt.Sprintf("transient-%d", apiErr.StatusCode)
		}
	}
	return "transient-network"
}

func asHTTPAPIError(err error, target **httpapi.Error) bool {
	apiErr, ok := err.(*httpapi.Error)
	if ok {
		*target = apiErr
	}
	return ok
}

func goldenDir(t *testing.T) string {
	t.Helper()
	if root := os.Getenv("TEST_SRCDIR"); root != "" {
		workspace := os.Getenv("TEST_WORKSPACE")
		for _, dir := range []string{
			filepath.Join(root, workspace, "solutions/ha-bridge/testdata/events"),
			filepath.Join(root, "solutions/ha-bridge/testdata/events"),
		} {
			if info, err := os.Stat(dir); err == nil && info.IsDir() {
				return dir
			}
		}
	}
	candidates := []string{"../../testdata/events", "solutions/ha-bridge/testdata/events"}
	for _, dir := range candidates {
		if info, err := os.Stat(dir); err == nil && info.IsDir() {
			return dir
		}
	}
	return candidates[0]
}

func encodeGolden(t *testing.T, cases map[string][]goldenCase) map[string][]byte {
	t.Helper()
	files := make(map[string][]byte, len(cases))
	for file, list := range cases {
		vectors := make([]goldenVector, 0, len(list))
		for _, tc := range list {
			vectors = append(vectors, observe(t, tc))
		}
		data, err := json.MarshalIndent(vectors, "", "  ")
		if err != nil {
			t.Fatalf("%s: encode: %v", file, err)
		}
		files[file] = append(data, '\n')
	}
	return files
}

func TestGoldenVectors(t *testing.T) {
	files := encodeGolden(t, goldenCaseVectors())
	dir := goldenDir(t)
	if *updateGolden {
		if err := os.MkdirAll(dir, 0o755); err != nil {
			t.Fatalf("create %s: %v", dir, err)
		}
		for name, data := range files {
			path := filepath.Join(dir, name+".json")
			if err := os.WriteFile(path, data, 0o644); err != nil {
				t.Fatalf("write %s: %v", path, err)
			}
			t.Logf("wrote %s", path)
		}
		return
	}
	for name, want := range files {
		path := filepath.Join(dir, name+".json")
		got, err := os.ReadFile(path)
		if err != nil {
			t.Fatalf("read %s (run go test -run TestGoldenVectors -update): %v", path, err)
		}
		if string(got) != string(want) {
			t.Errorf("%s does not match the Go bridge; run go test -run TestGoldenVectors -update\nwant:\n%s\ngot:\n%s", path, want, got)
		}
	}
}
