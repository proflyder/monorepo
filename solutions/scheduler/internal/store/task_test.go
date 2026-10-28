package store

import (
	"context"
	"errors"
	"strings"
	"testing"
	"time"

	ricardov1 "ricardo.local/monorepo/packages/proto/ricardo/v1"
)

func TestEnqueueTaskDefaultsAndReplay(t *testing.T) {
	st, _ := newTestStore(t)
	ctx := context.Background()
	ev, err := st.EnqueueTask(ctx, "notify.push", `{"text":"hi"}`, "", 0, "task-key", testBase)
	if err != nil {
		t.Fatalf("enqueue: %v", err)
	}
	if ev.GetSourceKind() != ricardov1.SourceKind_SOURCE_KIND_TASK || ev.GetTaskType() != "notify.push" {
		t.Fatalf("event = %+v", ev)
	}
	if ev.GetQueue() != QueueTasks {
		t.Fatalf("queue = %q, want %q", ev.GetQueue(), QueueTasks)
	}
	if ev.GetDueUnix() != testBase.Unix() || ev.GetState() != ricardov1.EventState_EVENT_STATE_PENDING {
		t.Fatalf("due/state = %d/%v", ev.GetDueUnix(), ev.GetState())
	}
	replay, err := st.EnqueueTask(ctx, "notify.push", `{"text":"hi"}`, "", 0, "task-key", testBase.Add(time.Hour))
	if err != nil || replay.GetId() != ev.GetId() {
		t.Fatalf("replay = %+v, %v", replay, err)
	}
	if _, err := st.EnqueueTask(ctx, "notify.push", `{"text":"different"}`, "", 0, "task-key", testBase); !errors.Is(err, ErrIdempotency) {
		t.Fatalf("conflict error = %v, want ErrIdempotency", err)
	}
}

func TestEnqueueTaskDelayedDispatch(t *testing.T) {
	st, _ := newTestStore(t)
	ctx := context.Background()
	runAt := testBase.Add(time.Hour)
	ev, err := st.EnqueueTask(ctx, "notify.push", "{}", "reminders", runAt.Unix(), "", testBase)
	if err != nil {
		t.Fatalf("enqueue: %v", err)
	}
	if ev.GetDueUnix() != runAt.Unix() {
		t.Fatalf("due = %d, want %d", ev.GetDueUnix(), runAt.Unix())
	}
	early, err := st.ListDueEvents(ctx, "worker-1", 60, 10, "reminders", testBase.Add(time.Minute))
	if err != nil || len(early) != 0 {
		t.Fatalf("early lease = %d, %v", len(early), err)
	}
	onTime, err := st.ListDueEvents(ctx, "worker-1", 60, 10, "reminders", runAt.Add(time.Second))
	if err != nil || len(onTime) != 1 || onTime[0].GetId() != ev.GetId() {
		t.Fatalf("on-time lease = %+v, %v", onTime, err)
	}
}

func TestCancelTask(t *testing.T) {
	st, _ := newTestStore(t)
	ctx := context.Background()
	ev, err := st.EnqueueTask(ctx, "notify.push", "{}", "", 0, "", testBase)
	if err != nil {
		t.Fatalf("enqueue: %v", err)
	}
	cancelled, err := st.CancelTask(ctx, ev.GetId(), testBase.Add(time.Second))
	if err != nil {
		t.Fatalf("cancel: %v", err)
	}
	if cancelled.GetState() != ricardov1.EventState_EVENT_STATE_SKIPPED || cancelled.GetDetail() != "cancelled" {
		t.Fatalf("cancelled = %+v", cancelled)
	}
	lease, err := st.ListDueEvents(ctx, "worker-1", 60, 10, QueueTasks, testBase.Add(2*time.Second))
	if err != nil || len(lease) != 0 {
		t.Fatalf("cancelled task leased: %d, %v", len(lease), err)
	}
	// Replay is stable.
	if replay, err := st.CancelTask(ctx, ev.GetId(), testBase.Add(3*time.Second)); err != nil || replay.GetState() != ricardov1.EventState_EVENT_STATE_SKIPPED {
		t.Fatalf("cancel replay = %+v, %v", replay, err)
	}
}

func TestCancelTaskRejectsLeasedAndNonTask(t *testing.T) {
	st, _ := newTestStore(t)
	ctx := context.Background()
	ev, err := st.EnqueueTask(ctx, "notify.push", "{}", "", 0, "", testBase)
	if err != nil {
		t.Fatalf("enqueue: %v", err)
	}
	if _, err := st.ListDueEvents(ctx, "worker-1", 60, 10, QueueTasks, testBase.Add(time.Second)); err != nil {
		t.Fatalf("lease: %v", err)
	}
	if _, err := st.CancelTask(ctx, ev.GetId(), testBase.Add(2*time.Second)); !errors.Is(err, ErrPrecondition) {
		t.Fatalf("cancel leased error = %v, want ErrPrecondition", err)
	}
	alarm := createAlarm(t, st, 7, 30, testBase.Add(3*time.Second))
	if _, err := st.Tick(ctx, time.Unix(alarm.GetNextDueUnix(), 0).Add(time.Second)); err != nil {
		t.Fatalf("tick: %v", err)
	}
	var alarmEvent *ricardov1.DueEvent
	for _, ev := range allEvents(t, st) {
		if ev.GetSourceKind() == ricardov1.SourceKind_SOURCE_KIND_ALARM {
			alarmEvent = ev
		}
	}
	if alarmEvent == nil {
		t.Fatalf("alarm event missing")
	}
	if _, err := st.CancelTask(ctx, alarmEvent.GetId(), testBase.Add(4*time.Second)); !errors.Is(err, ErrPrecondition) {
		t.Fatalf("cancel alarm event error = %v, want ErrPrecondition", err)
	}
}

func TestTaskValidation(t *testing.T) {
	st, _ := newTestStore(t)
	ctx := context.Background()
	hugePayload := `{"x":"` + strings.Repeat("a", 2000) + `"}`
	cases := []struct {
		name     string
		taskType string
		payload  string
		queue    string
		runAt    int64
	}{
		{"empty type", "", "{}", "", 0},
		{"uppercase type", "Notify.Push", "{}", "", 0},
		{"bad queue", "notify.push", "{}", "Home Queue", 0},
		{"invalid json", "notify.push", "{oops", "", 0},
		{"huge payload", "notify.push", hugePayload, "", 0},
		{"negative run_at", "notify.push", "{}", "", -1},
		{"too far", "notify.push", "{}", "", testBase.AddDate(11, 0, 0).Unix()},
	}
	for _, tc := range cases {
		t.Run(tc.name, func(t *testing.T) {
			if _, err := st.EnqueueTask(ctx, tc.taskType, tc.payload, tc.queue, tc.runAt, "", testBase); !errors.Is(err, ErrInvalid) {
				t.Fatalf("error = %v, want ErrInvalid", err)
			}
		})
	}
}

func TestTimerFireCarriesTaskTypeAndQueue(t *testing.T) {
	st, _ := newTestStore(t)
	ctx := context.Background()
	tm, err := st.StartTimer(ctx, "tea", 60, "", testBase)
	if err != nil {
		t.Fatalf("start: %v", err)
	}
	if _, err := st.Tick(ctx, testBase.Add(time.Minute)); err != nil {
		t.Fatalf("tick: %v", err)
	}
	events := allEvents(t, st)
	if len(events) != 1 {
		t.Fatalf("events = %d", len(events))
	}
	ev := events[0]
	if ev.GetTaskType() != "timer.fire" || ev.GetQueue() != QueueHome {
		t.Fatalf("timer event task_type/queue = %q/%q", ev.GetTaskType(), ev.GetQueue())
	}
	if ev.GetSourceId() != tm.GetId() || ev.GetSourceKind() != ricardov1.SourceKind_SOURCE_KIND_TIMER {
		t.Fatalf("timer source = %+v", ev)
	}
}
