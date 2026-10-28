package store

import (
	"context"
	"errors"
	"testing"
	"time"

	ricardov1 "ricardo.local/monorepo/packages/proto/ricardo/v1"
)

func TestTimerTransitionsAndRestartPersistence(t *testing.T) {
	st, path := newTestStore(t)
	ctx := context.Background()
	t0 := testBase

	tm, err := st.StartTimer(ctx, "tea", 60, "", t0)
	if err != nil {
		t.Fatalf("start: %v", err)
	}
	if tm.GetState() != ricardov1.TimerState_TIMER_STATE_RUNNING || tm.GetDueUnix() != t0.Add(time.Minute).Unix() {
		t.Fatalf("started = %+v", tm)
	}
	paused, err := st.PauseTimer(ctx, tm.GetId(), t0.Add(20*time.Second))
	if err != nil {
		t.Fatalf("pause: %v", err)
	}
	if paused.GetState() != ricardov1.TimerState_TIMER_STATE_PAUSED ||
		paused.GetRemainingSeconds() != 40 || paused.GetDueUnix() != 0 {
		t.Fatalf("paused = %+v", paused)
	}

	// Pausing a paused timer replays the current state.
	if again, err := st.PauseTimer(ctx, tm.GetId(), t0.Add(30*time.Second)); err != nil || again.GetRemainingSeconds() != 40 {
		t.Fatalf("pause replay = %+v, %v", again, err)
	}

	st = reopen(t, st, path)
	restored, err := st.GetTimer(ctx, tm.GetId())
	if err != nil || restored.GetRemainingSeconds() != 40 || restored.GetState() != ricardov1.TimerState_TIMER_STATE_PAUSED {
		t.Fatalf("restored = %+v, %v", restored, err)
	}

	resumeAt := t0.Add(time.Hour)
	resumed, err := st.ResumeTimer(ctx, tm.GetId(), resumeAt)
	if err != nil {
		t.Fatalf("resume: %v", err)
	}
	wantDue := resumeAt.Add(40 * time.Second).Unix()
	if resumed.GetDueUnix() != wantDue || resumed.GetState() != ricardov1.TimerState_TIMER_STATE_RUNNING {
		t.Fatalf("resumed = %+v, want due %d", resumed, wantDue)
	}
	if stats, err := st.Tick(ctx, resumeAt.Add(39*time.Second)); err != nil || stats.Timers != 0 {
		t.Fatalf("early tick = %+v, %v", stats, err)
	}
	if stats, err := st.Tick(ctx, time.Unix(wantDue, 0)); err != nil || stats.Timers != 1 {
		t.Fatalf("due tick = %+v, %v", stats, err)
	}
	if stats, err := st.Tick(ctx, time.Unix(wantDue, 0).Add(time.Second)); err != nil || stats.Timers != 0 {
		t.Fatalf("duplicate tick = %+v, %v", stats, err)
	}
	fired, err := st.GetTimer(ctx, tm.GetId())
	if err != nil || fired.GetState() != ricardov1.TimerState_TIMER_STATE_FIRED {
		t.Fatalf("fired = %+v, %v", fired, err)
	}
	if _, err := st.PauseTimer(ctx, tm.GetId(), time.Unix(wantDue, 0).Add(2*time.Second)); !errors.Is(err, ErrPrecondition) {
		t.Fatalf("pause fired error = %v, want ErrPrecondition", err)
	}

	// Cancelling a fired timer sends STOP so an executor can silence it.
	cancelled, err := st.CancelTimer(ctx, tm.GetId(), time.Unix(wantDue, 0).Add(3*time.Second))
	if err != nil {
		t.Fatalf("cancel fired: %v", err)
	}
	if cancelled.GetState() != ricardov1.TimerState_TIMER_STATE_CANCELLED {
		t.Fatalf("state = %v, want CANCELLED", cancelled.GetState())
	}
	events := allEvents(t, st)
	foundStop := false
	for _, ev := range events {
		if ev.GetSourceId() == tm.GetId() && ev.GetAction() == ricardov1.EventAction_EVENT_ACTION_STOP {
			foundStop = true
		}
	}
	if !foundStop {
		t.Fatalf("no STOP control event for fired timer")
	}
	// Cancel replay is stable.
	if _, err := st.CancelTimer(ctx, tm.GetId(), time.Unix(wantDue, 0).Add(4*time.Second)); err != nil {
		t.Fatalf("cancel replay: %v", err)
	}
}

func TestTimerStartValidationAndLimit(t *testing.T) {
	st, _ := newTestStore(t)
	ctx := context.Background()
	for _, d := range []int64{-1, 0, 3601} {
		if _, err := st.StartTimer(ctx, "bad", d, "", testBase); !errors.Is(err, ErrInvalid) {
			t.Fatalf("duration %d error = %v, want ErrInvalid", d, err)
		}
	}
	if _, err := st.StartTimer(ctx, "one", 60, "", testBase); err != nil {
		t.Fatalf("start: %v", err)
	}
	if _, err := st.StartTimer(ctx, "two", 60, "", testBase); !errors.Is(err, ErrLimitExceeded) {
		t.Fatalf("second timer error = %v, want ErrLimitExceeded", err)
	}
}

func TestTimerCancelRunningSkipsNoEvent(t *testing.T) {
	st, _ := newTestStore(t)
	ctx := context.Background()
	tm, err := st.StartTimer(ctx, "quick", 30, "", testBase)
	if err != nil {
		t.Fatalf("start: %v", err)
	}
	cancelled, err := st.CancelTimer(ctx, tm.GetId(), testBase.Add(time.Second))
	if err != nil || cancelled.GetState() != ricardov1.TimerState_TIMER_STATE_CANCELLED {
		t.Fatalf("cancel = %+v, %v", cancelled, err)
	}
	if stats, err := st.Tick(ctx, testBase.Add(2*time.Minute)); err != nil || stats.Timers != 0 {
		t.Fatalf("tick after cancel = %+v, %v", stats, err)
	}
	if n := eventCount(t, st); n != 0 {
		t.Fatalf("events = %d, want 0", n)
	}
}
