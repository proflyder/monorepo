package store

import (
	"context"
	"database/sql"
	"fmt"
	"time"

	ricardov1 "ricardo.local/monorepo/packages/proto/ricardo/v1"
	"ricardo.local/monorepo/solutions/scheduler/internal/planner"
)

const (
	tickBatchSize  = 200
	maxTickBatches = 10
)

// TickStats summarizes one atomic engine tick.
type TickStats struct {
	Fired          int
	Skipped        int
	Timers         int
	LeasesReleased int64
}

// Tick performs one atomic engine pass:
//  1. expired leases are released or failed;
//  2. expired ringing windows are cleared and due snoozes become active;
//  3. every due schedule emits at most one occurrence event and advances;
//  4. overdue occurrences older than the grace window emit a visible
//     SKIPPED event and are fast-forwarded;
//  5. every running timer that is due emits one event and becomes FIRED.
func (s *Store) Tick(ctx context.Context, now time.Time) (TickStats, error) {
	now = now.UTC()
	stats := TickStats{}
	tx, err := s.begin(ctx)
	if err != nil {
		return stats, err
	}
	defer func() { _ = tx.Rollback() }()

	released, err := s.releaseExpiredLeases(ctx, tx, now)
	if err != nil {
		return stats, err
	}
	stats.LeasesReleased = released

	if _, err := tx.ExecContext(ctx, `UPDATE schedules SET active_occurrence_unix=0,
		active_until_unix=0, snooze_until_unix=0, revision=revision+1, updated_unix=?
		WHERE active_until_unix>0 AND active_until_unix<=? AND deleted_unix=0`,
		now.Unix(), now.Unix()); err != nil {
		return stats, err
	}

	if err := s.activateSnoozes(ctx, tx, now); err != nil {
		return stats, err
	}

	for batch := 0; batch < maxTickBatches; batch++ {
		rows, err := tx.QueryContext(ctx, `SELECT `+scheduleCols+` FROM schedules
			WHERE enabled=1 AND next_due_unix>0 AND next_due_unix<=? AND deleted_unix=0
			ORDER BY next_due_unix LIMIT ?`, now.Unix(), tickBatchSize)
		if err != nil {
			return stats, err
		}
		batchSchedules := make([]*scheduleRecord, 0, tickBatchSize)
		for rows.Next() {
			rec, err := scanSchedule(rows.Scan)
			if err != nil {
				_ = rows.Close()
				return stats, err
			}
			batchSchedules = append(batchSchedules, rec)
		}
		if err := rows.Err(); err != nil {
			_ = rows.Close()
			return stats, err
		}
		_ = rows.Close()
		if len(batchSchedules) == 0 {
			break
		}
		for _, rec := range batchSchedules {
			if err := s.tickSchedule(ctx, tx, rec.schedule, now, &stats); err != nil {
				return stats, err
			}
		}
		if len(batchSchedules) < tickBatchSize {
			break
		}
	}

	for batch := 0; batch < maxTickBatches; batch++ {
		rows, err := tx.QueryContext(ctx, `SELECT `+timerCols+` FROM timers
			WHERE state=? AND due_unix>0 AND due_unix<=? ORDER BY due_unix LIMIT ?`,
			int32(ricardov1.TimerState_TIMER_STATE_RUNNING), now.Unix(), tickBatchSize)
		if err != nil {
			return stats, err
		}
		batchTimers := make([]*ricardov1.Timer, 0, tickBatchSize)
		for rows.Next() {
			tm, err := scanTimer(rows.Scan)
			if err != nil {
				_ = rows.Close()
				return stats, err
			}
			batchTimers = append(batchTimers, tm)
		}
		if err := rows.Err(); err != nil {
			_ = rows.Close()
			return stats, err
		}
		_ = rows.Close()
		if len(batchTimers) == 0 {
			break
		}
		for _, tm := range batchTimers {
			if _, _, err := s.insertEvent(ctx, tx, eventInsert{
				dedupeKey:   fmt.Sprintf("trigger:timer:%s:%d:0", tm.Id, tm.DueUnix),
				sourceID:    tm.Id,
				sourceKind:  ricardov1.SourceKind_SOURCE_KIND_TIMER,
				action:      ricardov1.EventAction_EVENT_ACTION_FIRE,
				dueUnix:     tm.DueUnix,
				occurrence:  0,
				label:       tm.Label,
				payloadJSON: "{}",
				state:       ricardov1.EventState_EVENT_STATE_PENDING,
				createdUnix: now.Unix(),
			}); err != nil {
				return stats, err
			}
			if _, err := tx.ExecContext(ctx, `UPDATE timers SET state=?, remaining_seconds=0,
				revision=revision+1, updated_unix=? WHERE id=? AND state=?`,
				int32(ricardov1.TimerState_TIMER_STATE_FIRED), now.Unix(), tm.Id,
				int32(ricardov1.TimerState_TIMER_STATE_RUNNING)); err != nil {
				return stats, err
			}
			stats.Timers++
		}
		if len(batchTimers) < tickBatchSize {
			break
		}
	}

	if err := tx.Commit(); err != nil {
		return stats, err
	}
	return stats, nil
}

func (s *Store) activateSnoozes(ctx context.Context, tx *sql.Tx, now time.Time) error {
	rows, err := tx.QueryContext(ctx, `SELECT DISTINCT source_id, occurrence_unix FROM events
		WHERE state=? AND action=? AND snooze_seq>0 AND due_unix<=?`,
		int32(ricardov1.EventState_EVENT_STATE_PENDING),
		int32(ricardov1.EventAction_EVENT_ACTION_FIRE), now.Unix())
	if err != nil {
		return err
	}
	type pending struct {
		id  string
		occ int64
	}
	items := make([]pending, 0)
	for rows.Next() {
		var p pending
		if err := rows.Scan(&p.id, &p.occ); err != nil {
			_ = rows.Close()
			return err
		}
		items = append(items, p)
	}
	if err := rows.Err(); err != nil {
		_ = rows.Close()
		return err
	}
	_ = rows.Close()
	for _, p := range items {
		if _, err := tx.ExecContext(ctx, `UPDATE schedules SET active_occurrence_unix=?,
			active_until_unix=?, snooze_until_unix=0, revision=revision+1, updated_unix=?
			WHERE id=? AND snooze_until_unix>0`,
			p.occ, now.Add(s.cfg.RingWindow).Unix(), now.Unix(), p.id); err != nil {
			return err
		}
	}
	return nil
}

func (s *Store) tickSchedule(ctx context.Context, tx *sql.Tx, sch *ricardov1.Schedule, now time.Time, stats *TickStats) error {
	rule := scheduleRule(sch)
	occ := sch.NextDueUnix
	if occ <= 0 {
		return nil
	}
	late := now.Unix() - occ
	if late > int64(s.cfg.MissedGrace.Seconds()) {
		if _, _, err := s.insertEvent(ctx, tx, eventInsert{
			dedupeKey:   fmt.Sprintf("trigger:%d:%s:%d:0", sourceKindOf(sch.Kind), sch.Id, occ),
			sourceID:    sch.Id,
			sourceKind:  sourceKindOf(sch.Kind),
			action:      ricardov1.EventAction_EVENT_ACTION_FIRE,
			dueUnix:     occ,
			occurrence:  occ,
			label:       sch.Label,
			payloadJSON: sch.PayloadJson,
			state:       ricardov1.EventState_EVENT_STATE_SKIPPED,
			detail:      fmt.Sprintf("missed_grace: %d seconds late", late),
			createdUnix: now.Unix(),
			ackedUnix:   now.Unix(),
			ackOutcome:  ricardov1.AckOutcome_ACK_OUTCOME_SKIPPED,
		}); err != nil {
			return err
		}
		stats.Skipped++
		next, exhausted, err := s.fastForward(rule, now)
		if err != nil {
			return err
		}
		return s.advanceSchedule(ctx, tx, sch, next, exhausted, now, false)
	}

	if _, _, err := s.insertEvent(ctx, tx, eventInsert{
		dedupeKey:   fmt.Sprintf("trigger:%d:%s:%d:0", sourceKindOf(sch.Kind), sch.Id, occ),
		sourceID:    sch.Id,
		sourceKind:  sourceKindOf(sch.Kind),
		action:      ricardov1.EventAction_EVENT_ACTION_FIRE,
		dueUnix:     occ,
		occurrence:  occ,
		label:       sch.Label,
		payloadJSON: sch.PayloadJson,
		state:       ricardov1.EventState_EVENT_STATE_PENDING,
		createdUnix: now.Unix(),
	}); err != nil {
		return err
	}
	stats.Fired++

	t, ok, err := rule.Next(time.Unix(occ, 0))
	if err != nil {
		return fmt.Errorf("advance schedule %s: %w", sch.Id, err)
	}
	exhausted := !ok
	next := int64(0)
	if ok {
		next = t.Unix()
	}
	if err := s.advanceSchedule(ctx, tx, sch, next, exhausted, now, true); err != nil {
		return err
	}
	return nil
}

// fastForward resumes a lagging schedule at the first occurrence after now.
// Every missed occurrence is coalesced into the single SKIPPED event already
// recorded for the earliest one; this keeps a years-old row from scanning
// forever or rolling back unrelated due timers.
func (s *Store) fastForward(rule planner.Rule, now time.Time) (int64, bool, error) {
	t, ok, err := rule.Next(now)
	if err != nil {
		return 0, false, err
	}
	if !ok {
		return 0, true, nil
	}
	return t.Unix(), false, nil
}

func (s *Store) advanceSchedule(ctx context.Context, tx *sql.Tx, sch *ricardov1.Schedule, next int64, exhausted bool, now time.Time, fired bool) error {
	enabled := 1
	if exhausted {
		enabled = 0
		next = 0
	}
	if fired {
		if _, err := tx.ExecContext(ctx, `UPDATE schedules SET next_due_unix=?, enabled=?,
			last_fired_unix=?, active_occurrence_unix=?, active_until_unix=?,
			snooze_until_unix=0, revision=revision+1, updated_unix=? WHERE id=?`,
			next, enabled, sch.NextDueUnix, sch.NextDueUnix,
			now.Add(s.cfg.RingWindow).Unix(), now.Unix(), sch.Id); err != nil {
			return err
		}
		return nil
	}
	if _, err := tx.ExecContext(ctx, `UPDATE schedules SET next_due_unix=?, enabled=?,
		revision=revision+1, updated_unix=? WHERE id=?`, next, enabled, now.Unix(), sch.Id); err != nil {
		return err
	}
	return nil
}
