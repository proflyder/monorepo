package store

import (
	"context"
	"errors"
	"strings"
	"testing"
	"time"

	ricardov1 "ricardo.local/monorepo/packages/proto/ricardo/v1"
)

func TestCreateGetListSchedule(t *testing.T) {
	st, _ := newTestStore(t)
	ctx := context.Background()

	sch := createAlarm(t, st, 7, 30, testBase)
	if sch.GetId() == "" || sch.GetRevision() != 1 {
		t.Fatalf("created schedule = %+v", sch)
	}
	wantNext := time.Date(2026, 1, 2, 7, 30, 0, 0, time.UTC).Unix()
	if sch.GetNextDueUnix() != wantNext {
		t.Fatalf("next due = %d, want %d", sch.GetNextDueUnix(), wantNext)
	}
	got, err := st.GetSchedule(ctx, sch.GetId())
	if err != nil || got.GetLabel() != "wake up" {
		t.Fatalf("GetSchedule = %+v, %v", got, err)
	}
	list, err := st.ListSchedules(ctx, false, ricardov1.ScheduleKind_SCHEDULE_KIND_ALARM)
	if err != nil || len(list) != 1 {
		t.Fatalf("ListSchedules = %d, %v", len(list), err)
	}
	if _, err := st.GetSchedule(ctx, "missing"); !errors.Is(err, ErrNotFound) {
		t.Fatalf("missing get error = %v", err)
	}
}

func TestCreateValidation(t *testing.T) {
	st, _ := newTestStore(t)
	now := testBase
	cases := map[string]*ricardov1.Schedule{
		"bad timezone": func() *ricardov1.Schedule {
			s := dailyAlarm(7, 0)
			s.Timezone = "Mars/Olympus"
			return s
		}(),
		"hour out of range": func() *ricardov1.Schedule {
			s := dailyAlarm(24, 0)
			return s
		}(),
		"custom without weekdays": func() *ricardov1.Schedule {
			s := dailyAlarm(7, 0)
			s.Repeat = ricardov1.RepeatKind_REPEAT_KIND_CUSTOM
			return s
		}(),
		"duplicate weekdays": func() *ricardov1.Schedule {
			s := dailyAlarm(7, 0)
			s.Repeat = ricardov1.RepeatKind_REPEAT_KIND_CUSTOM
			s.Weekdays = []int32{1, 1}
			return s
		}(),
		"reminder must be once": func() *ricardov1.Schedule {
			s := dailyAlarm(7, 0)
			s.Kind = ricardov1.ScheduleKind_SCHEDULE_KIND_REMINDER
			return s
		}(),
		"once in the past": func() *ricardov1.Schedule {
			s := dailyAlarm(7, 0)
			s.Repeat = ricardov1.RepeatKind_REPEAT_KIND_ONCE
			s.OnceDate = "2025-01-01"
			return s
		}(),
		"invalid json": func() *ricardov1.Schedule {
			s := dailyAlarm(7, 0)
			s.PayloadJson = "{not json"
			return s
		}(),
		"payload too large": func() *ricardov1.Schedule {
			s := dailyAlarm(7, 0)
			s.PayloadJson = `{"x":"` + strings.Repeat("a", 2000) + `"}`
			return s
		}(),
	}
	for name, in := range cases {
		t.Run(name, func(t *testing.T) {
			if _, err := st.CreateSchedule(context.Background(), in, "", now); !errors.Is(err, ErrInvalid) {
				t.Fatalf("CreateSchedule error = %v, want ErrInvalid", err)
			}
		})
	}
}

func TestIdempotentCreateAndConflict(t *testing.T) {
	st, _ := newTestStore(t)
	ctx := context.Background()

	first, err := st.CreateSchedule(ctx, dailyAlarm(7, 30), "key-1", testBase)
	if err != nil {
		t.Fatalf("create: %v", err)
	}
	second, err := st.CreateSchedule(ctx, dailyAlarm(7, 30), "key-1", testBase.Add(time.Minute))
	if err != nil {
		t.Fatalf("replay: %v", err)
	}
	if first.GetId() != second.GetId() {
		t.Fatalf("replay returned %s, want %s", second.GetId(), first.GetId())
	}
	conflicting := dailyAlarm(8, 0)
	if _, err := st.CreateSchedule(ctx, conflicting, "key-1", testBase); !errors.Is(err, ErrIdempotency) {
		t.Fatalf("conflict error = %v, want ErrIdempotency", err)
	}
	if n := eventCount(t, st); n != 0 {
		t.Fatalf("events = %d, want 0", n)
	}
}

func TestIdempotentStartTimer(t *testing.T) {
	st, _ := newTestStore(t)
	ctx := context.Background()
	first, err := st.StartTimer(ctx, "pasta", 300, "timer-key", testBase)
	if err != nil {
		t.Fatalf("start: %v", err)
	}
	second, err := st.StartTimer(ctx, "pasta", 300, "timer-key", testBase.Add(time.Minute))
	if err != nil || second.GetId() != first.GetId() {
		t.Fatalf("replay = %+v, %v", second, err)
	}
	if _, err := st.StartTimer(ctx, "other", 300, "timer-key", testBase); !errors.Is(err, ErrIdempotency) {
		t.Fatalf("conflict error = %v, want ErrIdempotency", err)
	}
}

func TestUpdateRevisionConflictAndTickBump(t *testing.T) {
	st, _ := newTestStore(t)
	ctx := context.Background()
	sch := createAlarm(t, st, 7, 30, testBase)

	edited := dailyAlarm(8, 0)
	edited.Id = sch.GetId()
	updated, err := st.UpdateSchedule(ctx, edited, 1, testBase.Add(time.Minute))
	if err != nil {
		t.Fatalf("update: %v", err)
	}
	if updated.GetRevision() != 2 || updated.GetAt().GetHour() != 8 {
		t.Fatalf("updated = %+v", updated)
	}
	if _, err := st.UpdateSchedule(ctx, edited, 1, testBase.Add(2*time.Minute)); !errors.Is(err, ErrConflict) {
		t.Fatalf("stale update error = %v, want ErrConflict", err)
	}

	// Engine-driven field changes must bump revision so a stale UI update
	// cannot silently overwrite newer state.
	due := time.Unix(updated.GetNextDueUnix(), 0)
	if _, err := st.Tick(ctx, due.Add(2*time.Second)); err != nil {
		t.Fatalf("tick: %v", err)
	}
	got, err := st.GetSchedule(ctx, sch.GetId())
	if err != nil {
		t.Fatalf("get: %v", err)
	}
	if got.GetRevision() != 3 {
		t.Fatalf("revision after tick = %d, want 3", got.GetRevision())
	}
	stale := dailyAlarm(9, 0)
	stale.Id = sch.GetId()
	if _, err := st.UpdateSchedule(ctx, stale, 2, due.Add(3*time.Second)); !errors.Is(err, ErrConflict) {
		t.Fatalf("stale-after-tick error = %v, want ErrConflict", err)
	}
}

func TestSetEnabledRecomputesFromNow(t *testing.T) {
	st, _ := newTestStore(t)
	ctx := context.Background()
	sch := createAlarm(t, st, 7, 30, testBase)

	disabled, err := st.SetScheduleEnabled(ctx, sch.GetId(), false, testBase)
	if err != nil || disabled.GetEnabled() || disabled.GetNextDueUnix() != 0 {
		t.Fatalf("disable = %+v, %v", disabled, err)
	}
	later := time.Date(2026, 2, 10, 12, 0, 0, 0, time.UTC)
	enabled, err := st.SetScheduleEnabled(ctx, sch.GetId(), true, later)
	if err != nil {
		t.Fatalf("enable: %v", err)
	}
	want := time.Date(2026, 2, 11, 7, 30, 0, 0, time.UTC).Unix()
	if enabled.GetNextDueUnix() != want {
		t.Fatalf("next after enable = %d, want %d", enabled.GetNextDueUnix(), want)
	}
	replayed, err := st.SetScheduleEnabled(ctx, sch.GetId(), true, later.Add(time.Hour))
	if err != nil || replayed.GetNextDueUnix() != want || replayed.GetRevision() != enabled.GetRevision() {
		t.Fatalf("enable replay = %+v, %v", replayed, err)
	}
}

func TestDeleteCancelsTriggers(t *testing.T) {
	st, _ := newTestStore(t)
	ctx := context.Background()
	sch := createAlarm(t, st, 7, 30, testBase)
	due := time.Unix(sch.GetNextDueUnix(), 0)
	if _, err := st.Tick(ctx, due.Add(time.Second)); err != nil {
		t.Fatalf("tick: %v", err)
	}
	deleted, err := st.DeleteSchedule(ctx, sch.GetId(), due.Add(2*time.Second))
	if err != nil || !deleted {
		t.Fatalf("delete = %v, %v", deleted, err)
	}
	if _, err := st.GetSchedule(ctx, sch.GetId()); !errors.Is(err, ErrNotFound) {
		t.Fatalf("get deleted error = %v, want ErrNotFound", err)
	}
	events := allEvents(t, st)
	if len(events) != 2 {
		t.Fatalf("events = %d, want FIRE skipped + STOP pending", len(events))
	}
	if events[0].GetState() != ricardov1.EventState_EVENT_STATE_SKIPPED {
		t.Fatalf("fire state = %v, want SKIPPED", events[0].GetState())
	}
	if events[1].GetAction() != ricardov1.EventAction_EVENT_ACTION_STOP {
		t.Fatalf("control action = %v, want STOP", events[1].GetAction())
	}
	if again, err := st.DeleteSchedule(ctx, sch.GetId(), due.Add(3*time.Second)); err != nil || again {
		t.Fatalf("second delete = %v, %v", again, err)
	}
}

func TestSnoozeAfterDeliveredAckThenStopRejectsSnooze(t *testing.T) {
	st, _ := newTestStore(t)
	ctx := context.Background()
	sch := createAlarm(t, st, 7, 30, testBase)
	due := time.Unix(sch.GetNextDueUnix(), 0)
	if _, err := st.Tick(ctx, due.Add(time.Second)); err != nil {
		t.Fatalf("tick: %v", err)
	}
	leased, err := st.ListDueEvents(ctx, "home-1", 60, 10, QueueHome, due.Add(2*time.Second))
	if err != nil || len(leased) != 1 {
		t.Fatalf("lease = %d, %v", len(leased), err)
	}
	if _, err := st.AckEvent(ctx, leased[0].GetId(), "home-1", ricardov1.AckOutcome_ACK_OUTCOME_DELIVERED, "", due.Add(3*time.Second)); err != nil {
		t.Fatalf("ack: %v", err)
	}

	// Snooze must still be possible after the executor acked delivery.
	snoozed, fire, err := st.SnoozeSchedule(ctx, sch.GetId(), 9, due.Add(4*time.Second))
	if err != nil {
		t.Fatalf("snooze after delivered: %v", err)
	}
	wantSnooze := due.Add(4*time.Second + 9*time.Minute).Unix()
	if snoozed.GetSnoozeUntilUnix() != wantSnooze || fire.GetDueUnix() != wantSnooze {
		t.Fatalf("snooze = %d fire = %d, want %d", snoozed.GetSnoozeUntilUnix(), fire.GetDueUnix(), wantSnooze)
	}
	if fire.GetSnoozeSeq() != 1 {
		t.Fatalf("snooze seq = %d, want 1", fire.GetSnoozeSeq())
	}
	// A STOP control event for the sounding occurrence exists.
	stopEvents := 0
	for _, ev := range allEvents(t, st) {
		if ev.GetAction() == ricardov1.EventAction_EVENT_ACTION_STOP && ev.GetDetail() == "snoozed for 9 minutes" {
			stopEvents++
		}
	}
	if stopEvents != 1 {
		t.Fatalf("snooze STOP events = %d, want 1", stopEvents)
	}

	// Stop while the snooze is still pending cancels it.
	stopped, err := st.StopSchedule(ctx, sch.GetId(), due.Add(5*time.Second))
	if err != nil {
		t.Fatalf("stop: %v", err)
	}
	if stopped.GetSnoozeUntilUnix() != 0 || stopped.GetActiveOccurrenceUnix() != 0 {
		t.Fatalf("stop did not clear active state: %+v", stopped)
	}
	if _, _, err := st.SnoozeSchedule(ctx, sch.GetId(), 5, due.Add(6*time.Second)); !errors.Is(err, ErrPrecondition) {
		t.Fatalf("snooze after stop error = %v, want ErrPrecondition", err)
	}
	// Repeated Stop is stable and does not create a second STOP event.
	before := eventCount(t, st)
	if _, err := st.StopSchedule(ctx, sch.GetId(), due.Add(7*time.Second)); err != nil {
		t.Fatalf("repeated stop: %v", err)
	}
	if after := eventCount(t, st); after != before {
		t.Fatalf("repeated stop created events: %d -> %d", before, after)
	}
}

func TestSnoozeRequiresRingingWindow(t *testing.T) {
	st, _ := newTestStore(t)
	ctx := context.Background()
	sch := createAlarm(t, st, 7, 30, testBase)
	if _, _, err := st.SnoozeSchedule(ctx, sch.GetId(), 9, testBase); !errors.Is(err, ErrPrecondition) {
		t.Fatalf("snooze idle error = %v, want ErrPrecondition", err)
	}
	if _, err := st.StopSchedule(ctx, sch.GetId(), testBase); !errors.Is(err, ErrPrecondition) {
		t.Fatalf("stop idle error = %v, want ErrPrecondition", err)
	}
	if _, _, err := st.SnoozeSchedule(ctx, sch.GetId(), 1441, testBase); !errors.Is(err, ErrInvalid) {
		t.Fatalf("snooze huge error = %v, want ErrInvalid", err)
	}
}

func TestOneShotCanSnoozeWhileDisabled(t *testing.T) {
	st, _ := newTestStore(t)
	ctx := context.Background()
	in := dailyAlarm(7, 30)
	in.Repeat = ricardov1.RepeatKind_REPEAT_KIND_ONCE
	// Pick the next UTC date.
	in.OnceDate = time.Now().UTC().AddDate(0, 0, 1).Format("2006-01-02")
	sch, err := st.CreateSchedule(ctx, in, "", testBase)
	if err != nil {
		t.Fatalf("create once: %v", err)
	}
	due := time.Unix(sch.GetNextDueUnix(), 0)
	if _, err := st.Tick(ctx, due.Add(time.Second)); err != nil {
		t.Fatalf("tick: %v", err)
	}
	got, err := st.GetSchedule(ctx, sch.GetId())
	if err != nil || got.GetEnabled() || got.GetNextDueUnix() != 0 {
		t.Fatalf("once after fire = %+v, %v", got, err)
	}
	leased, err := st.ListDueEvents(ctx, "home-1", 60, 10, QueueHome, due.Add(2*time.Second))
	if err != nil || len(leased) != 1 {
		t.Fatalf("lease = %d, %v", len(leased), err)
	}
	if _, err := st.AckEvent(ctx, leased[0].GetId(), "home-1", ricardov1.AckOutcome_ACK_OUTCOME_DELIVERED, "", due.Add(3*time.Second)); err != nil {
		t.Fatalf("ack: %v", err)
	}
	if _, _, err := st.SnoozeSchedule(ctx, sch.GetId(), 5, due.Add(4*time.Second)); err != nil {
		t.Fatalf("snooze exhausted once: %v", err)
	}
}

func TestReminderIsOneShotLocalDateTime(t *testing.T) {
	st, _ := newTestStore(t)
	ctx := context.Background()
	reminder := &ricardov1.Schedule{
		Label:    "call mom",
		Kind:     ricardov1.ScheduleKind_SCHEDULE_KIND_REMINDER,
		Timezone: "Europe/Berlin",
		At:       &ricardov1.WallTime{Hour: 18, Minute: 45},
		Repeat:   ricardov1.RepeatKind_REPEAT_KIND_ONCE,
		OnceDate: "2026-06-01",
		Enabled:  true,
	}
	created, err := st.CreateSchedule(ctx, reminder, "", testBase)
	if err != nil {
		t.Fatalf("create reminder: %v", err)
	}
	want := time.Date(2026, 6, 1, 18, 45, 0, 0, mustLoc(t, "Europe/Berlin")).Unix()
	if created.GetNextDueUnix() != want {
		t.Fatalf("next due = %d, want %d", created.GetNextDueUnix(), want)
	}
	if stats, err := st.Tick(ctx, time.Unix(want, 0).Add(time.Second)); err != nil || stats.Fired != 1 {
		t.Fatalf("tick = %+v, %v", stats, err)
	}
	got, err := st.GetSchedule(ctx, created.GetId())
	if err != nil || got.GetEnabled() || got.GetNextDueUnix() != 0 {
		t.Fatalf("reminder after fire = %+v, %v", got, err)
	}
	events := allEvents(t, st)
	if len(events) != 1 || events[0].GetTaskType() != "reminder.fire" || events[0].GetQueue() != QueueHome {
		t.Fatalf("reminder event = %+v", events)
	}
}

func mustLoc(t *testing.T, name string) *time.Location {
	t.Helper()
	loc, err := time.LoadLocation(name)
	if err != nil {
		t.Fatalf("load location: %v", err)
	}
	return loc
}
