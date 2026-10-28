package store

import (
	"context"
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
