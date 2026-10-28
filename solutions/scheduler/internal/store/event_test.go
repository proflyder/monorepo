package store

import (
	"context"
	"errors"
	"testing"
	"time"

	ricardov1 "ricardo.local/monorepo/packages/proto/ricardo/v1"
)

func fireOne(t *testing.T, st *Store, now time.Time) *ricardov1.DueEvent {
	t.Helper()
	sch := createAlarm(t, st, 7, 30, now)
	due := time.Unix(sch.GetNextDueUnix(), 0)
	if _, err := st.Tick(context.Background(), due.Add(time.Second)); err != nil {
		t.Fatalf("tick: %v", err)
	}
	events := allEvents(t, st)
	if len(events) != 1 {
		t.Fatalf("events = %d, want 1", len(events))
	}
	return events[0]
}

func TestAckRequiresActiveLease(t *testing.T) {
	st, _ := newTestStore(t)
	ctx := context.Background()
	ev := fireOne(t, st, testBase)
	at := time.Unix(ev.GetDueUnix(), 0).Add(2 * time.Second)

	if _, err := st.AckEvent(ctx, ev.GetId(), "home-1", ricardov1.AckOutcome_ACK_OUTCOME_DELIVERED, "", at); !errors.Is(err, ErrPrecondition) {
		t.Fatalf("ack unleased error = %v, want ErrPrecondition", err)
	}
	status, err := st.Status(ctx, at)
	if err != nil {
		t.Fatalf("status: %v", err)
	}
	if len(status.Consumers) != 0 {
		t.Fatalf("invalid ack created consumer liveness: %+v", status.Consumers)
	}

	leased, err := st.ListDueEvents(ctx, "home-1", 60, 10, QueueHome, at)
	if err != nil || len(leased) != 1 {
		t.Fatalf("lease = %d, %v", len(leased), err)
	}
	if _, err := st.AckEvent(ctx, ev.GetId(), "home-2", ricardov1.AckOutcome_ACK_OUTCOME_DELIVERED, "", at.Add(time.Second)); !errors.Is(err, ErrPrecondition) {
		t.Fatalf("wrong consumer ack error = %v, want ErrPrecondition", err)
	}
	acked, err := st.AckEvent(ctx, ev.GetId(), "home-1", ricardov1.AckOutcome_ACK_OUTCOME_DELIVERED, "ok", at.Add(2*time.Second))
	if err != nil {
		t.Fatalf("ack: %v", err)
	}
	if acked.GetState() != ricardov1.EventState_EVENT_STATE_DELIVERED || acked.GetAckOutcome() != ricardov1.AckOutcome_ACK_OUTCOME_DELIVERED {
		t.Fatalf("acked = %+v", acked)
	}
	// Terminal replay is idempotent and does not create a new delivery.
	replay, err := st.AckEvent(ctx, ev.GetId(), "home-1", ricardov1.AckOutcome_ACK_OUTCOME_DELIVERED, "", at.Add(3*time.Second))
	if err != nil || replay.GetState() != ricardov1.EventState_EVENT_STATE_DELIVERED {
		t.Fatalf("replay = %+v, %v", replay, err)
	}
}

func TestExpiredLeaseRedeliveryKeepsID(t *testing.T) {
	st, _ := newTestStore(t)
	ctx := context.Background()
	ev := fireOne(t, st, testBase)
	at := time.Unix(ev.GetDueUnix(), 0).Add(2 * time.Second)

	leased, err := st.ListDueEvents(ctx, "home-1", 5, 10, QueueHome, at)
	if err != nil || len(leased) != 1 || leased[0].GetId() != ev.GetId() {
		t.Fatalf("lease = %+v, %v", leased, err)
	}
	if _, err := st.AckEvent(ctx, ev.GetId(), "home-1", ricardov1.AckOutcome_ACK_OUTCOME_DELIVERED, "", at.Add(6*time.Second)); !errors.Is(err, ErrPrecondition) {
		t.Fatalf("expired lease ack error = %v, want ErrPrecondition", err)
	}
	again, err := st.ListDueEvents(ctx, "home-1", 60, 10, QueueHome, at.Add(7*time.Second))
	if err != nil || len(again) != 1 {
		t.Fatalf("redelivery = %d, %v", len(again), err)
	}
	if again[0].GetId() != ev.GetId() || again[0].GetAttempts() != 2 {
		t.Fatalf("redelivered = %+v, want same id attempts=2", again[0])
	}
}

func TestLeaseAttemptsExhausted(t *testing.T) {
	st, _ := newTestStore(t)
	ctx := context.Background()
	ev := fireOne(t, st, testBase)
	at := time.Unix(ev.GetDueUnix(), 0).Add(2 * time.Second)
	if _, err := st.ListDueEvents(ctx, "home-1", 5, 10, QueueHome, at); err != nil {
		t.Fatalf("first lease: %v", err)
	}
	if _, err := st.ListDueEvents(ctx, "home-1", 5, 10, QueueHome, at.Add(6*time.Second)); err != nil {
		t.Fatalf("second lease: %v", err)
	}
	if _, err := st.Tick(ctx, at.Add(20*time.Second)); err != nil {
		t.Fatalf("tick: %v", err)
	}
	events := allEvents(t, st)
	if len(events) != 1 || events[0].GetState() != ricardov1.EventState_EVENT_STATE_FAILED {
		t.Fatalf("events = %+v, want one FAILED", events)
	}
	leased, err := st.ListDueEvents(ctx, "home-1", 5, 10, QueueHome, at.Add(21*time.Second))
	if err != nil || len(leased) != 0 {
		t.Fatalf("lease after exhaustion = %d, %v", len(leased), err)
	}
}

func TestListDueEventsQueueFilter(t *testing.T) {
	st, _ := newTestStore(t)
	ctx := context.Background()
	at := testBase
	ev := fireOne(t, st, at)
	task, err := st.EnqueueTask(ctx, "notify.push", `{"text":"hi"}`, "", 0, "", at)
	if err != nil {
		t.Fatalf("enqueue: %v", err)
	}
	dueAt := time.Unix(ev.GetDueUnix(), 0).Add(2 * time.Second)

	home, err := st.ListDueEvents(ctx, "home-1", 60, 10, QueueHome, dueAt)
	if err != nil {
		t.Fatalf("home lease: %v", err)
	}
	if len(home) != 1 || home[0].GetQueue() != QueueHome {
		t.Fatalf("home events = %+v", home)
	}
	tasks, err := st.ListDueEvents(ctx, "worker-1", 60, 10, QueueTasks, dueAt)
	if err != nil {
		t.Fatalf("tasks lease: %v", err)
	}
	if len(tasks) != 1 || tasks[0].GetId() != task.GetId() {
		t.Fatalf("task events = %+v", tasks)
	}
	all, err := st.ListDueEvents(ctx, "admin", 60, 10, "", dueAt)
	if err != nil || len(all) != 0 {
		t.Fatalf("all-queue lease after leases = %d, %v", len(all), err)
	}
}

func TestPurgeOnlyTerminalEvents(t *testing.T) {
	st, _ := newTestStore(t)
	ctx := context.Background()
	ev := fireOne(t, st, testBase)
	at := time.Unix(ev.GetDueUnix(), 0).Add(2 * time.Second)
	leased, err := st.ListDueEvents(ctx, "home-1", 60, 10, QueueHome, at)
	if err != nil {
		t.Fatalf("lease: %v", err)
	}
	if _, err := st.AckEvent(ctx, leased[0].GetId(), "home-1", ricardov1.AckOutcome_ACK_OUTCOME_DELIVERED, "", at.Add(time.Second)); err != nil {
		t.Fatalf("ack: %v", err)
	}
	pending, err := st.EnqueueTask(ctx, "notify.push", "{}", "", 0, "", at.Add(2*time.Second))
	if err != nil {
		t.Fatalf("enqueue: %v", err)
	}
	n, err := st.Purge(ctx, at.Add(31*24*time.Hour))
	if err != nil || n != 1 {
		t.Fatalf("purge = %d, %v", n, err)
	}
	events := allEvents(t, st)
	if len(events) != 1 || events[0].GetId() != pending.GetId() {
		t.Fatalf("events after purge = %+v", events)
	}
}

func TestStatusConsumersAndQueues(t *testing.T) {
	st, _ := newTestStore(t)
	ctx := context.Background()
	ev := fireOne(t, st, testBase)
	at := time.Unix(ev.GetDueUnix(), 0).Add(2 * time.Second)
	if _, err := st.ListDueEvents(ctx, "home-1", 60, 10, QueueHome, at); err != nil {
		t.Fatalf("lease: %v", err)
	}
	status, err := st.Status(ctx, at)
	if err != nil {
		t.Fatalf("status: %v", err)
	}
	if len(status.Consumers) != 1 || status.Consumers[0].GetConsumerId() != "home-1" || !status.Consumers[0].GetAlive() {
		t.Fatalf("consumers = %+v", status.Consumers)
	}
	if status.LeasedEvents != 1 || status.PendingEvents != 0 {
		t.Fatalf("counters = %+v", status)
	}
	var homeQueue *ricardov1.QueueInfo
	for _, q := range status.Queues {
		if q.GetName() == QueueHome {
			homeQueue = q
		}
	}
	if homeQueue == nil || homeQueue.GetLeased() != 1 {
		t.Fatalf("home queue = %+v (all %+v)", homeQueue, status.Queues)
	}
	if len(status.Queues) != 3 {
		t.Fatalf("built-in queues discoverable = %+v, want home/tasks/scenario", status.Queues)
	}
	// Consumer liveness expires.
	stale, err := st.Status(ctx, at.Add(2*time.Minute))
	if err != nil {
		t.Fatalf("status: %v", err)
	}
	if stale.Consumers[0].GetAlive() {
		t.Fatalf("consumer reported alive after liveness window")
	}
}
