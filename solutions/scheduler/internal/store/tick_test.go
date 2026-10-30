package store

import (
	"context"
	"errors"
	"strings"
	"testing"
	"time"

	ricardov1 "ricardo.local/monorepo/packages/proto/ricardo/v1"
)

func TestTickDedupeAndRestart(t *testing.T) {
	st, path := newTestStore(t)
	ctx := context.Background()
	sch := createAlarm(t, st, 7, 30, testBase)
	due := time.Unix(sch.GetNextDueUnix(), 0)

	stats, err := st.Tick(ctx, due.Add(time.Second))
	if err != nil {
		t.Fatalf("tick: %v", err)
	}
	if stats.Fired != 1 {
		t.Fatalf("fired = %d, want 1", stats.Fired)
	}
	if got := eventCount(t, st); got != 1 {
		t.Fatalf("events = %d, want 1", got)
	}
	// A second tick at the same instant must not duplicate the occurrence.
	if stats, err = st.Tick(ctx, due.Add(2*time.Second)); err != nil || stats.Fired != 0 {
		t.Fatalf("second tick = %+v, %v", stats, err)
	}
	if got := eventCount(t, st); got != 1 {
		t.Fatalf("events after second tick = %d, want 1", got)
	}

	st = reopen(t, st, path)
	if stats, err = st.Tick(ctx, due.Add(3*time.Second)); err != nil || stats.Fired != 0 {
		t.Fatalf("tick after restart = %+v, %v", stats, err)
	}
	if got := eventCount(t, st); got != 1 {
		t.Fatalf("events after restart = %d, want 1", got)
	}
}

func TestMissedOccurrenceIsSkippedExplicitly(t *testing.T) {
	st, _ := newTestStore(t)
	ctx := context.Background()
	sch := createAlarm(t, st, 7, 30, testBase)
	due := time.Unix(sch.GetNextDueUnix(), 0)

	late := due.Add(11 * time.Minute) // grace is 10 minutes
	stats, err := st.Tick(ctx, late)
	if err != nil {
		t.Fatalf("tick: %v", err)
	}
	if stats.Skipped != 1 || stats.Fired != 0 {
		t.Fatalf("stats = %+v, want one skipped", stats)
	}
	events := allEvents(t, st)
	if len(events) != 1 {
		t.Fatalf("events = %d, want 1", len(events))
	}
	ev := events[0]
	if ev.GetState() != ricardov1.EventState_EVENT_STATE_SKIPPED {
		t.Fatalf("state = %v, want SKIPPED", ev.GetState())
	}
	if ev.GetAckOutcome() != ricardov1.AckOutcome_ACK_OUTCOME_SKIPPED || ev.GetAckedUnix() == 0 {
		t.Fatalf("skipped event ack fields = %+v", ev)
	}
	if !strings.Contains(ev.GetDetail(), "missed_grace") {
		t.Fatalf("detail = %q, want missed_grace", ev.GetDetail())
	}
	got, err := st.GetSchedule(ctx, sch.GetId())
	if err != nil {
		t.Fatalf("get: %v", err)
	}
	if got.GetNextDueUnix() <= late.Unix() {
		t.Fatalf("next due %d not advanced past %d", got.GetNextDueUnix(), late.Unix())
	}
	if got.GetLastFiredUnix() != 0 {
		t.Fatalf("last fired = %d, want 0 for a skip", got.GetLastFiredUnix())
	}
}

func TestTickWithYearsOldScheduleDoesNotPoisonTimers(t *testing.T) {
	st, _ := newTestStore(t)
	ctx := context.Background()
	sch := createAlarm(t, st, 7, 30, testBase)
	due := time.Unix(sch.GetNextDueUnix(), 0)
	now := due.AddDate(5, 0, 0)

	tm, err := st.StartTimer(ctx, "tea", 60, "", now.Add(-2*time.Minute))
	if err != nil {
		t.Fatalf("start timer: %v", err)
	}
	stats, err := st.Tick(ctx, now)
	if err != nil {
		t.Fatalf("tick with stale schedule: %v", err)
	}
	if stats.Skipped != 1 || stats.Timers != 1 {
		t.Fatalf("stats = %+v, want 1 skipped + 1 timer", stats)
	}
	got, err := st.GetSchedule(ctx, sch.GetId())
	if err != nil {
		t.Fatalf("get: %v", err)
	}
	if got.GetNextDueUnix() <= now.Unix() {
		t.Fatalf("schedule did not coalesce past now: %d", got.GetNextDueUnix())
	}
	events := allEvents(t, st)
	var timerEvent *ricardov1.DueEvent
	for _, ev := range events {
		if ev.GetSourceKind() == ricardov1.SourceKind_SOURCE_KIND_TIMER {
			timerEvent = ev
		}
	}
	if timerEvent == nil {
		t.Fatalf("timer event missing")
	}
	if timerEvent.GetSourceId() != tm.GetId() {
		t.Fatalf("timer event source = %s, want %s", timerEvent.GetSourceId(), tm.GetId())
	}
}

func TestActiveWindowExpires(t *testing.T) {
	st, _ := newTestStore(t)
	ctx := context.Background()
	sch := createAlarm(t, st, 7, 30, testBase)
	due := time.Unix(sch.GetNextDueUnix(), 0)
	if _, err := st.Tick(ctx, due.Add(time.Second)); err != nil {
		t.Fatalf("tick: %v", err)
	}
	active, err := st.GetSchedule(ctx, sch.GetId())
	if err != nil {
		t.Fatalf("get: %v", err)
	}
	if active.GetActiveOccurrenceUnix() != due.Unix() {
		t.Fatalf("active occurrence = %d, want %d", active.GetActiveOccurrenceUnix(), due.Unix())
	}
	if _, err := st.Tick(ctx, due.Add(16*time.Minute)); err != nil {
		t.Fatalf("expire tick: %v", err)
	}
	expired, err := st.GetSchedule(ctx, sch.GetId())
	if err != nil {
		t.Fatalf("get: %v", err)
	}
	if expired.GetActiveOccurrenceUnix() != 0 || expired.GetActiveUntilUnix() != 0 {
		t.Fatalf("window not cleared: %+v", expired)
	}
	if expired.GetRevision() <= active.GetRevision() {
		t.Fatalf("revision did not advance on tick change: %d -> %d", active.GetRevision(), expired.GetRevision())
	}
}

func TestSnoozeEventActivatesWindow(t *testing.T) {
	st, _ := newTestStore(t)
	ctx := context.Background()
	sch := createAlarm(t, st, 7, 30, testBase)
	due := time.Unix(sch.GetNextDueUnix(), 0)
	if _, err := st.Tick(ctx, due.Add(time.Second)); err != nil {
		t.Fatalf("tick: %v", err)
	}
	snoozed, fire, err := st.SnoozeSchedule(ctx, sch.GetId(), 9, due.Add(2*time.Second))
	if err != nil {
		t.Fatalf("snooze: %v", err)
	}
	at := time.Unix(snoozed.GetSnoozeUntilUnix(), 0).Add(time.Second)
	if _, err := st.Tick(ctx, at); err != nil {
		t.Fatalf("activation tick: %v", err)
	}
	activated, err := st.GetSchedule(ctx, sch.GetId())
	if err != nil {
		t.Fatalf("get: %v", err)
	}
	if activated.GetSnoozeUntilUnix() != 0 {
		t.Fatalf("snooze_until = %d, want cleared", activated.GetSnoozeUntilUnix())
	}
	if activated.GetActiveUntilUnix() <= at.Unix() {
		t.Fatalf("active_until = %d not extended", activated.GetActiveUntilUnix())
	}
	events, err := st.ListDueEvents(ctx, "home-1", 60, 10, QueueHome, at.Add(time.Second))
	if err != nil {
		t.Fatalf("lease: %v", err)
	}
	found := false
	for _, ev := range events {
		if ev.GetId() == fire.GetId() && ev.GetAction() == ricardov1.EventAction_EVENT_ACTION_FIRE {
			found = true
		}
	}
	if !found {
		t.Fatalf("snoozed FIRE event not leased")
	}
}

func TestScenarioEventsStayOutOfHomeQueue(t *testing.T) {
	st, _ := newTestStore(t)
	ctx := context.Background()
	in := dailyAlarm(9, 0)
	in.Kind = ricardov1.ScheduleKind_SCHEDULE_KIND_SCENARIO
	in.PayloadJson = `{"action":"lights_on"}`
	sch, err := st.CreateSchedule(ctx, in, "", testBase)
	if err != nil {
		t.Fatalf("create scenario: %v", err)
	}
	due := time.Unix(sch.GetNextDueUnix(), 0)
	if _, err := st.Tick(ctx, due.Add(time.Second)); err != nil {
		t.Fatalf("tick: %v", err)
	}
	home, err := st.ListDueEvents(ctx, "home-1", 60, 10, QueueHome, due.Add(2*time.Second))
	if err != nil || len(home) != 0 {
		t.Fatalf("home leased scenario event: %d, %v", len(home), err)
	}
	events := allEvents(t, st)
	if len(events) != 1 || events[0].GetQueue() != QueueScenario || events[0].GetTaskType() != "scenario.fire" {
		t.Fatalf("scenario event = %+v", events)
	}
}

func findEvent(t *testing.T, events []*ricardov1.DueEvent, sourceID string, action ricardov1.EventAction) *ricardov1.DueEvent {
	t.Helper()
	var found *ricardov1.DueEvent
	for _, ev := range events {
		if ev.GetSourceId() != sourceID || ev.GetAction() != action {
			continue
		}
		if found != nil {
			t.Fatalf("duplicate %v event for %s", action, sourceID)
		}
		found = ev
	}
	return found
}

func findEventDetail(t *testing.T, events []*ricardov1.DueEvent, sourceID string, action ricardov1.EventAction, detail string) *ricardov1.DueEvent {
	t.Helper()
	for _, ev := range events {
		if ev.GetSourceId() == sourceID && ev.GetAction() == action && ev.GetDetail() == detail {
			return ev
		}
	}
	return nil
}

func TestRingWindowExpiryEmitsOneDurableStop(t *testing.T) {
	st, path := newTestStore(t)
	ctx := context.Background()
	sch := createAlarm(t, st, 7, 30, testBase)
	due := time.Unix(sch.GetNextDueUnix(), 0)
	if _, err := st.Tick(ctx, due.Add(time.Second)); err != nil {
		t.Fatalf("fire tick: %v", err)
	}

	expiry := due.Add(15 * time.Minute).Add(time.Second)
	stats, err := st.Tick(ctx, expiry)
	if err != nil {
		t.Fatalf("expiry tick: %v", err)
	}
	if stats.Expired != 1 || stats.Fired != 0 {
		t.Fatalf("expiry stats = %+v, want one expired", stats)
	}

	events := allEvents(t, st)
	stop := findEvent(t, events, sch.GetId(), ricardov1.EventAction_EVENT_ACTION_STOP)
	if stop == nil {
		t.Fatalf("no STOP event after expiry: %+v", events)
	}
	if stop.GetOccurrenceUnix() != due.Unix() {
		t.Fatalf("stop occurrence = %d, want %d", stop.GetOccurrenceUnix(), due.Unix())
	}
	if stop.GetDetail() != DetailRingWindowExpired {
		t.Fatalf("stop detail = %q", stop.GetDetail())
	}
	if stop.GetQueue() != QueueHome || stop.GetTaskType() != "alarm.stop" {
		t.Fatalf("stop routing = queue %q task %q", stop.GetQueue(), stop.GetTaskType())
	}
	if stop.GetState() != ricardov1.EventState_EVENT_STATE_PENDING {
		t.Fatalf("stop state = %v, want PENDING", stop.GetState())
	}
	// The FIRE of the closed occurrence must not be startable afterwards.
	fire := findEvent(t, events, sch.GetId(), ricardov1.EventAction_EVENT_ACTION_FIRE)
	if fire == nil || fire.GetState() != ricardov1.EventState_EVENT_STATE_SKIPPED {
		t.Fatalf("fire after expiry = %+v", fire)
	}

	// Repeated ticks and a process restart must not duplicate the STOP.
	if stats, err := st.Tick(ctx, expiry.Add(time.Second)); err != nil || stats.Expired != 0 {
		t.Fatalf("second expiry tick = %+v, %v", stats, err)
	}
	st = reopen(t, st, path)
	if stats, err := st.Tick(ctx, expiry.Add(2*time.Second)); err != nil || stats.Expired != 0 {
		t.Fatalf("tick after restart = %+v, %v", stats, err)
	}
	if got := len(allEvents(t, st)); got != 2 {
		t.Fatalf("events = %d, want exactly FIRE(skipped) + STOP", got)
	}
}

func TestExpiryCancelsOnlyTheExpiredOccurrenceFire(t *testing.T) {
	st, _ := newTestStore(t)
	ctx := context.Background()
	alarm := createAlarm(t, st, 7, 30, testBase)
	other := createAlarm(t, st, 7, 30, testBase)
	due := time.Unix(alarm.GetNextDueUnix(), 0)
	if _, err := st.Tick(ctx, due.Add(time.Second)); err != nil {
		t.Fatalf("fire tick: %v", err)
	}
	// The second alarm snoozes: its own occurrence window and delayed FIRE
	// must survive the first alarm's expiry.
	snoozed, snoozeFire, err := st.SnoozeSchedule(ctx, other.GetId(), 9, due.Add(2*time.Second))
	if err != nil {
		t.Fatalf("snooze: %v", err)
	}

	if _, err := st.Tick(ctx, due.Add(15*time.Minute).Add(time.Second)); err != nil {
		t.Fatalf("expiry tick: %v", err)
	}
	events := allEvents(t, st)
	stop := findEvent(t, events, alarm.GetId(), ricardov1.EventAction_EVENT_ACTION_STOP)
	if stop == nil || stop.GetOccurrenceUnix() != due.Unix() {
		t.Fatalf("missing stop for expired alarm: %+v", events)
	}
	if expiredStop := findEventDetail(t, events, other.GetId(), ricardov1.EventAction_EVENT_ACTION_STOP, DetailRingWindowExpired); expiredStop != nil {
		t.Fatalf("expiry emitted a STOP for a still-open snoozed occurrence")
	}
	var kept *ricardov1.DueEvent
	for _, ev := range events {
		if ev.GetId() == snoozeFire.GetId() {
			kept = ev
		}
	}
	if kept == nil || kept.GetState() != ricardov1.EventState_EVENT_STATE_PENDING {
		t.Fatalf("snoozed FIRE was cancelled by another occurrence's expiry: %+v", kept)
	}
	still, err := st.GetSchedule(ctx, other.GetId())
	if err != nil {
		t.Fatalf("get other: %v", err)
	}
	if still.GetActiveUntilUnix() <= due.Add(15*time.Minute).Unix() {
		t.Fatalf("snoozed window was closed early: until=%d snooze=%d",
			still.GetActiveUntilUnix(), snoozed.GetSnoozeUntilUnix())
	}
}

func TestExpiryStopSurvivesLeaseExpiryAndAck(t *testing.T) {
	st, _ := newTestStore(t)
	ctx := context.Background()
	sch := createAlarm(t, st, 7, 30, testBase)
	due := time.Unix(sch.GetNextDueUnix(), 0)
	if _, err := st.Tick(ctx, due.Add(time.Second)); err != nil {
		t.Fatalf("fire tick: %v", err)
	}
	expiry := due.Add(15 * time.Minute).Add(time.Second)
	if _, err := st.Tick(ctx, expiry); err != nil {
		t.Fatalf("expiry tick: %v", err)
	}
	leased, err := st.ListDueEvents(ctx, "home-1", 5, 10, QueueHome, expiry.Add(time.Second))
	if err != nil || len(leased) != 1 || leased[0].GetAction() != ricardov1.EventAction_EVENT_ACTION_STOP {
		t.Fatalf("lease STOP = %+v, %v", leased, err)
	}
	stopID := leased[0].GetId()
	// The lease expires without an ack: the same logical STOP is redelivered.
	again, err := st.ListDueEvents(ctx, "home-1", 60, 10, QueueHome, expiry.Add(7*time.Second))
	if err != nil || len(again) != 1 || again[0].GetId() != stopID || again[0].GetAttempts() != 2 {
		t.Fatalf("STOP redelivery = %+v, %v", again, err)
	}
	if _, err := st.AckEvent(ctx, stopID, "home-1", ricardov1.AckOutcome_ACK_OUTCOME_DELIVERED, "ok", expiry.Add(8*time.Second)); err != nil {
		t.Fatalf("ack STOP: %v", err)
	}
	if stats, err := st.Tick(ctx, expiry.Add(9*time.Second)); err != nil || stats.Expired != 0 {
		t.Fatalf("tick after ack = %+v, %v", stats, err)
	}
	if got := findEvent(t, allEvents(t, st), sch.GetId(), ricardov1.EventAction_EVENT_ACTION_STOP); got == nil {
		t.Fatalf("STOP disappeared")
	}
}

func TestStopAndSnoozeAfterExpiryKeepPreconditionSemantics(t *testing.T) {
	st, _ := newTestStore(t)
	ctx := context.Background()
	sch := createAlarm(t, st, 7, 30, testBase)
	due := time.Unix(sch.GetNextDueUnix(), 0)
	if _, err := st.Tick(ctx, due.Add(time.Second)); err != nil {
		t.Fatalf("fire tick: %v", err)
	}
	expiry := due.Add(15 * time.Minute).Add(time.Second)
	if _, err := st.Tick(ctx, expiry); err != nil {
		t.Fatalf("expiry tick: %v", err)
	}
	// The scheduler already stopped this occurrence; a voice stop shortly
	// afterwards is an idempotent success, not a new event.
	if _, err := st.StopSchedule(ctx, sch.GetId(), expiry.Add(time.Minute)); err != nil {
		t.Fatalf("stop after expiry = %v, want idempotent replay", err)
	}
	if got := findEvent(t, allEvents(t, st), sch.GetId(), ricardov1.EventAction_EVENT_ACTION_STOP); got == nil {
		t.Fatalf("STOP missing")
	}
	// Snoozing a closed occurrence stays a precondition failure.
	if _, _, err := st.SnoozeSchedule(ctx, sch.GetId(), 9, expiry.Add(time.Minute)); !errors.Is(err, ErrPrecondition) {
		t.Fatalf("snooze after expiry = %v, want ErrPrecondition", err)
	}
	// Long after the window even an idempotent stop is no longer eligible.
	if _, err := st.StopSchedule(ctx, sch.GetId(), expiry.Add(20*time.Minute)); !errors.Is(err, ErrPrecondition) {
		t.Fatalf("late stop = %v, want ErrPrecondition", err)
	}
}

func TestExpiryDoesNotSilenceTheNextOccurrence(t *testing.T) {
	st, _ := newTestStore(t)
	ctx := context.Background()
	sch := createAlarm(t, st, 7, 30, testBase)
	first := time.Unix(sch.GetNextDueUnix(), 0)
	if _, err := st.Tick(ctx, first.Add(time.Second)); err != nil {
		t.Fatalf("first fire: %v", err)
	}
	if _, err := st.Tick(ctx, first.Add(15*time.Minute).Add(time.Second)); err != nil {
		t.Fatalf("expiry tick: %v", err)
	}
	stop := findEvent(t, allEvents(t, st), sch.GetId(), ricardov1.EventAction_EVENT_ACTION_STOP)
	if stop == nil || stop.GetOccurrenceUnix() != first.Unix() {
		t.Fatalf("expiry stop = %+v", stop)
	}
	// The next daily occurrence fires with its own identity and window.
	next := first.Add(24 * time.Hour)
	stats, err := st.Tick(ctx, next.Add(time.Second))
	if err != nil || stats.Fired != 1 {
		t.Fatalf("next fire = %+v, %v", stats, err)
	}
	live, err := st.GetSchedule(ctx, sch.GetId())
	if err != nil {
		t.Fatalf("get: %v", err)
	}
	if live.GetActiveOccurrenceUnix() != next.Unix() || live.GetActiveUntilUnix() == 0 {
		t.Fatalf("next occurrence window = %+v", live)
	}
	var nextFire *ricardov1.DueEvent
	for _, ev := range allEvents(t, st) {
		if ev.GetAction() == ricardov1.EventAction_EVENT_ACTION_FIRE && ev.GetState() == ricardov1.EventState_EVENT_STATE_PENDING {
			nextFire = ev
		}
	}
	if nextFire == nil || nextFire.GetOccurrenceUnix() != next.Unix() {
		t.Fatalf("next FIRE = %+v", nextFire)
	}
	if nextFire.GetOccurrenceUnix() == stop.GetOccurrenceUnix() {
		t.Fatalf("next FIRE reuses the expired occurrence identity")
	}
}

func TestExpiryOfReminderCancelsFireWithoutGlobalStop(t *testing.T) {
	st, _ := newTestStore(t)
	ctx := context.Background()
	reminder := &ricardov1.Schedule{
		Label:    "call mom",
		Kind:     ricardov1.ScheduleKind_SCHEDULE_KIND_REMINDER,
		Timezone: "UTC",
		At:       &ricardov1.WallTime{Hour: 12, Minute: 1},
		Repeat:   ricardov1.RepeatKind_REPEAT_KIND_ONCE,
		OnceDate: "2026-01-01",
		Enabled:  true,
	}
	sch, err := st.CreateSchedule(ctx, reminder, "", testBase)
	if err != nil {
		t.Fatalf("create: %v", err)
	}
	due := time.Unix(sch.GetNextDueUnix(), 0)
	if _, err := st.Tick(ctx, due.Add(time.Second)); err != nil {
		t.Fatalf("fire: %v", err)
	}
	if _, err := st.Tick(ctx, due.Add(15*time.Minute).Add(time.Second)); err != nil {
		t.Fatalf("expiry: %v", err)
	}
	events := allEvents(t, st)
	if findEvent(t, events, sch.GetId(), ricardov1.EventAction_EVENT_ACTION_STOP) != nil {
		t.Fatalf("reminder expiry emitted a global STOP")
	}
	fire := findEvent(t, events, sch.GetId(), ricardov1.EventAction_EVENT_ACTION_FIRE)
	if fire == nil || fire.GetState() != ricardov1.EventState_EVENT_STATE_SKIPPED {
		t.Fatalf("reminder FIRE after expiry = %+v", fire)
	}
	closed, err := st.GetSchedule(ctx, sch.GetId())
	if err != nil || closed.GetActiveOccurrenceUnix() != 0 {
		t.Fatalf("reminder window = %+v, %v", closed, err)
	}
}
