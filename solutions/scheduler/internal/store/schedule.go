package store

import (
	"context"
	"database/sql"
	"encoding/json"
	"errors"
	"fmt"
	"strings"
	"time"

	ricardov1 "ricardo.local/monorepo/packages/proto/ricardo/v1"
	"ricardo.local/monorepo/solutions/scheduler/internal/idgen"
	"ricardo.local/monorepo/solutions/scheduler/internal/planner"
)

const scheduleCols = `id, kind, label, timezone, at_hour, at_minute, at_second, once_date,
	repeat_kind, weekdays, next_due_unix, enabled, payload_json, revision,
	created_unix, updated_unix, deleted_unix, last_fired_unix, snooze_until_unix,
	snooze_count, active_occurrence_unix, active_until_unix`

type scheduleRecord struct {
	schedule *ricardov1.Schedule
	deleted  int64
}

func scanSchedule(scan func(dest ...any) error) (*scheduleRecord, error) {
	var (
		rec           scheduleRecord
		s             ricardov1.Schedule
		kind, repeat  int32
		atH, atM, atS int32
		enabled       int
		weekdays      string
		deleted       int64
	)
	if err := scan(
		&s.Id, &kind, &s.Label, &s.Timezone, &atH, &atM, &atS, &s.OnceDate,
		&repeat, &weekdays, &s.NextDueUnix, &enabled, &s.PayloadJson, &s.Revision,
		&s.CreatedUnix, &s.UpdatedUnix, &deleted, &s.LastFiredUnix,
		&s.SnoozeUntilUnix, &s.SnoozeCount, &s.ActiveOccurrenceUnix, &s.ActiveUntilUnix,
	); err != nil {
		return nil, err
	}
	s.Kind = ricardov1.ScheduleKind(kind)
	s.Repeat = ricardov1.RepeatKind(repeat)
	s.At = &ricardov1.WallTime{Hour: atH, Minute: atM, Second: atS}
	s.Enabled = enabled != 0
	s.Weekdays = weekdaysFromText(weekdays)
	rec.schedule = &s
	rec.deleted = deleted
	return &rec, nil
}

func scheduleRule(s *ricardov1.Schedule) planner.Rule {
	at := s.GetAt()
	return planner.Rule{
		Timezone: s.GetTimezone(),
		Hour:     int(at.GetHour()),
		Minute:   int(at.GetMinute()),
		Second:   int(at.GetSecond()),
		Repeat:   s.GetRepeat(),
		Weekdays: s.GetWeekdays(),
		OnceDate: s.GetOnceDate(),
	}
}

// prepareSchedule validates and normalizes client input. It never trusts
// server-derived fields present in the request.
func (s *Store) prepareSchedule(in *ricardov1.Schedule) (*ricardov1.Schedule, error) {
	if in == nil {
		return nil, fmt.Errorf("%w: schedule is required", ErrInvalid)
	}
	out := &ricardov1.Schedule{
		Id:          strings.TrimSpace(in.GetId()),
		Label:       in.GetLabel(),
		Kind:        in.GetKind(),
		Timezone:    strings.TrimSpace(in.GetTimezone()),
		OnceDate:    strings.TrimSpace(in.GetOnceDate()),
		Repeat:      in.GetRepeat(),
		Weekdays:    append([]int32(nil), in.GetWeekdays()...),
		Enabled:     in.GetEnabled(),
		PayloadJson: in.GetPayloadJson(),
	}
	if at := in.GetAt(); at != nil {
		out.At = &ricardov1.WallTime{Hour: at.GetHour(), Minute: at.GetMinute(), Second: at.GetSecond()}
	} else {
		out.At = &ricardov1.WallTime{}
	}
	if len(out.Label) > 200 {
		return nil, fmt.Errorf("%w: label exceeds 200 bytes", ErrInvalid)
	}
	switch out.Kind {
	case ricardov1.ScheduleKind_SCHEDULE_KIND_ALARM,
		ricardov1.ScheduleKind_SCHEDULE_KIND_REMINDER,
		ricardov1.ScheduleKind_SCHEDULE_KIND_SCENARIO:
	default:
		return nil, fmt.Errorf("%w: unsupported schedule kind", ErrInvalid)
	}
	if out.Timezone == "" {
		out.Timezone = s.cfg.DefaultTimezone
	}
	if out.Kind == ricardov1.ScheduleKind_SCHEDULE_KIND_REMINDER &&
		out.Repeat != ricardov1.RepeatKind_REPEAT_KIND_ONCE {
		return nil, fmt.Errorf("%w: reminders must use REPEAT_KIND_ONCE", ErrInvalid)
	}
	if out.PayloadJson == "" {
		out.PayloadJson = "{}"
	}
	if len(out.PayloadJson) > s.cfg.MaxPayloadBytes {
		return nil, fmt.Errorf("%w: payload exceeds %d bytes", ErrInvalid, s.cfg.MaxPayloadBytes)
	}
	if !json.Valid([]byte(out.PayloadJson)) {
		return nil, fmt.Errorf("%w: payload_json must be valid JSON", ErrInvalid)
	}
	if err := scheduleRule(out).Validate(); err != nil {
		return nil, fmt.Errorf("%w: %v", ErrInvalid, err)
	}
	return out, nil
}

func nextOccurrence(s *ricardov1.Schedule, after time.Time) (int64, error) {
	next, ok, err := scheduleRule(s).Next(after)
	if err != nil {
		return 0, fmt.Errorf("%w: %v", ErrInvalid, err)
	}
	if !ok {
		return 0, nil
	}
	return next.Unix(), nil
}

func canonicalSchedule(s *ricardov1.Schedule) string {
	return fmt.Sprintf("schedule|%s|%d|%s|%s|%d|%d|%d|%d|%s|%t|%s",
		s.GetLabel(), s.GetKind(), s.GetTimezone(), s.GetOnceDate(),
		int(s.GetAt().GetHour()), int(s.GetAt().GetMinute()), int(s.GetAt().GetSecond()),
		int(s.GetRepeat()), weekdaysToText(s.GetWeekdays()), s.GetEnabled(), s.GetPayloadJson())
}

func validIdempotencyKey(key string) error {
	if key == "" {
		return nil
	}
	if len(key) > 200 {
		return fmt.Errorf("%w: idempotency key exceeds 200 bytes", ErrInvalid)
	}
	return nil
}

// CreateSchedule creates a durable schedule. A non-empty idempotency key
// replays the original resource when the canonical request matches, and fails
// with ErrIdempotency when it does not.
func (s *Store) CreateSchedule(ctx context.Context, in *ricardov1.Schedule, idempotencyKey string, now time.Time) (*ricardov1.Schedule, error) {
	prepared, err := s.prepareSchedule(in)
	if err != nil {
		return nil, err
	}
	if err := validIdempotencyKey(idempotencyKey); err != nil {
		return nil, err
	}
	now = now.UTC()
	id, err := idgen.NewResourceID(now)
	if err != nil {
		return nil, err
	}
	prepared.Id = id
	prepared.Revision = 1
	prepared.CreatedUnix = now.Unix()
	prepared.UpdatedUnix = now.Unix()
	prepared.NextDueUnix = 0
	if prepared.Enabled {
		next, err := nextOccurrence(prepared, now)
		if err != nil {
			return nil, err
		}
		if next == 0 {
			return nil, fmt.Errorf("%w: one-shot date is in the past or does not exist", ErrInvalid)
		}
		prepared.NextDueUnix = next
	}

	hash := idgen.HashRequest(canonicalSchedule(prepared))

	tx, err := s.begin(ctx)
	if err != nil {
		return nil, err
	}
	defer func() { _ = tx.Rollback() }()

	if idempotencyKey != "" {
		var (
			kind, resourceID, existingHash string
		)
		err := tx.QueryRowContext(ctx,
			`SELECT resource_kind, resource_id, request_hash FROM idempotency WHERE key = ?`,
			idempotencyKey).Scan(&kind, &resourceID, &existingHash)
		switch {
		case err == nil:
			if kind != "schedule" || existingHash != hash {
				return nil, fmt.Errorf("%w: idempotency key reused with a different request", ErrIdempotency)
			}
			rec, err := getScheduleTx(ctx, tx, resourceID)
			if err != nil {
				return nil, fmt.Errorf("%w: idempotent schedule no longer exists", ErrIdempotency)
			}
			if err := tx.Commit(); err != nil {
				return nil, err
			}
			return rec.schedule, nil
		case !errors.Is(err, sql.ErrNoRows):
			return nil, err
		}
	}

	if _, err := tx.ExecContext(ctx, `INSERT INTO schedules (`+scheduleCols+`) VALUES (?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?)`,
		prepared.Id, int32(prepared.Kind), prepared.Label, prepared.Timezone,
		int32(prepared.At.GetHour()), int32(prepared.At.GetMinute()), int32(prepared.At.GetSecond()),
		prepared.OnceDate, int32(prepared.Repeat), weekdaysToText(prepared.Weekdays),
		prepared.NextDueUnix, boolToInt(prepared.Enabled), prepared.PayloadJson,
		prepared.Revision, prepared.CreatedUnix, prepared.UpdatedUnix, int64(0),
		prepared.LastFiredUnix, prepared.SnoozeUntilUnix, prepared.SnoozeCount,
		prepared.ActiveOccurrenceUnix, prepared.ActiveUntilUnix,
	); err != nil {
		return nil, fmt.Errorf("insert schedule: %w", err)
	}
	if idempotencyKey != "" {
		if _, err := tx.ExecContext(ctx,
			`INSERT INTO idempotency (key, resource_kind, resource_id, request_hash, created_unix) VALUES (?,?,?,?,?)`,
			idempotencyKey, "schedule", prepared.Id, hash, now.Unix()); err != nil {
			return nil, fmt.Errorf("%w: idempotency key already exists", ErrIdempotency)
		}
	}
	if err := tx.Commit(); err != nil {
		return nil, err
	}
	return prepared, nil
}

func getScheduleTx(ctx context.Context, tx *sql.Tx, id string) (*scheduleRecord, error) {
	row := tx.QueryRowContext(ctx, `SELECT `+scheduleCols+` FROM schedules WHERE id = ? AND deleted_unix = 0`, id)
	rec, err := scanSchedule(row.Scan)
	if errors.Is(err, sql.ErrNoRows) {
		return nil, ErrNotFound
	}
	if err != nil {
		return nil, err
	}
	return rec, nil
}

// GetSchedule returns a live schedule.
func (s *Store) GetSchedule(ctx context.Context, id string) (*ricardov1.Schedule, error) {
	if strings.TrimSpace(id) == "" {
		return nil, fmt.Errorf("%w: id is required", ErrInvalid)
	}
	rec, err := scanSchedule(s.db.QueryRowContext(ctx,
		`SELECT `+scheduleCols+` FROM schedules WHERE id = ? AND deleted_unix = 0`, id).Scan)
	if errors.Is(err, sql.ErrNoRows) {
		return nil, ErrNotFound
	}
	if err != nil {
		return nil, err
	}
	return rec.schedule, nil
}

// ListSchedules returns live schedules ordered by creation time.
func (s *Store) ListSchedules(ctx context.Context, includeDisabled bool, kind ricardov1.ScheduleKind) ([]*ricardov1.Schedule, error) {
	query := `SELECT ` + scheduleCols + ` FROM schedules WHERE deleted_unix = 0`
	args := make([]any, 0, 2)
	if !includeDisabled {
		query += ` AND enabled = 1`
	}
	if kind != ricardov1.ScheduleKind_SCHEDULE_KIND_UNSPECIFIED {
		query += ` AND kind = ?`
		args = append(args, int32(kind))
	}
	query += ` ORDER BY created_unix, id`
	rows, err := s.db.QueryContext(ctx, query, args...)
	if err != nil {
		return nil, err
	}
	defer func() { _ = rows.Close() }()
	out := make([]*ricardov1.Schedule, 0)
	for rows.Next() {
		rec, err := scanSchedule(rows.Scan)
		if err != nil {
			return nil, err
		}
		out = append(out, rec.schedule)
	}
	return out, rows.Err()
}

// UpdateSchedule replaces a schedule's rule. expectedRevision of 0 disables
// the optimistic concurrency check. Server-derived fields are recomputed.
func (s *Store) UpdateSchedule(ctx context.Context, in *ricardov1.Schedule, expectedRevision int64, now time.Time) (*ricardov1.Schedule, error) {
	if strings.TrimSpace(in.GetId()) == "" {
		return nil, fmt.Errorf("%w: id is required", ErrInvalid)
	}
	prepared, err := s.prepareSchedule(in)
	if err != nil {
		return nil, err
	}
	now = now.UTC()
	tx, err := s.begin(ctx)
	if err != nil {
		return nil, err
	}
	defer func() { _ = tx.Rollback() }()

	existing, err := getScheduleTx(ctx, tx, prepared.Id)
	if err != nil {
		return nil, err
	}
	if expectedRevision != 0 && expectedRevision != existing.schedule.Revision {
		return nil, fmt.Errorf("%w: current revision %d, expected %d", ErrConflict, existing.schedule.Revision, expectedRevision)
	}
	prepared.Revision = existing.schedule.Revision + 1
	prepared.CreatedUnix = existing.schedule.CreatedUnix
	prepared.UpdatedUnix = now.Unix()
	prepared.LastFiredUnix = existing.schedule.LastFiredUnix
	prepared.NextDueUnix = 0
	if prepared.Enabled {
		next, err := nextOccurrence(prepared, now)
		if err != nil {
			return nil, err
		}
		if next == 0 {
			return nil, fmt.Errorf("%w: one-shot date is in the past or does not exist", ErrInvalid)
		}
		prepared.NextDueUnix = next
	}
	// Editing a rule stops any active delivery window; sound control is a
	// separate StopSchedule call.
	prepared.SnoozeUntilUnix = 0
	prepared.SnoozeCount = 0
	prepared.ActiveOccurrenceUnix = 0
	prepared.ActiveUntilUnix = 0

	if _, err := tx.ExecContext(ctx, `UPDATE schedules SET
		kind=?, label=?, timezone=?, at_hour=?, at_minute=?, at_second=?, once_date=?,
		repeat_kind=?, weekdays=?, next_due_unix=?, enabled=?, payload_json=?,
		revision=?, updated_unix=?, last_fired_unix=?, snooze_until_unix=?,
		snooze_count=?, active_occurrence_unix=?, active_until_unix=?
		WHERE id=? AND deleted_unix=0`,
		int32(prepared.Kind), prepared.Label, prepared.Timezone,
		int32(prepared.At.GetHour()), int32(prepared.At.GetMinute()), int32(prepared.At.GetSecond()),
		prepared.OnceDate, int32(prepared.Repeat), weekdaysToText(prepared.Weekdays),
		prepared.NextDueUnix, boolToInt(prepared.Enabled), prepared.PayloadJson,
		prepared.Revision, prepared.UpdatedUnix, prepared.LastFiredUnix,
		prepared.SnoozeUntilUnix, prepared.SnoozeCount,
		prepared.ActiveOccurrenceUnix, prepared.ActiveUntilUnix, prepared.Id,
	); err != nil {
		return nil, fmt.Errorf("update schedule: %w", err)
	}
	if err := tx.Commit(); err != nil {
		return nil, err
	}
	return prepared, nil
}

// SetScheduleEnabled toggles a schedule. Enabling recomputes the next
// occurrence after now instead of replaying a stale one.
func (s *Store) SetScheduleEnabled(ctx context.Context, id string, enabled bool, now time.Time) (*ricardov1.Schedule, error) {
	now = now.UTC()
	tx, err := s.begin(ctx)
	if err != nil {
		return nil, err
	}
	defer func() { _ = tx.Rollback() }()
	rec, err := getScheduleTx(ctx, tx, id)
	if err != nil {
		return nil, err
	}
	cur := rec.schedule
	if cur.Enabled == enabled {
		if err := tx.Commit(); err != nil {
			return nil, err
		}
		return cur, nil
	}
	next := int64(0)
	if enabled {
		next, err = nextOccurrence(cur, now)
		if err != nil {
			return nil, err
		}
		if next == 0 {
			return nil, fmt.Errorf("%w: one-shot date is in the past or does not exist", ErrInvalid)
		}
	}
	cur.Enabled = enabled
	cur.NextDueUnix = next
	cur.Revision++
	cur.UpdatedUnix = now.Unix()
	if enabled {
		cur.SnoozeUntilUnix = 0
		cur.SnoozeCount = 0
		cur.ActiveOccurrenceUnix = 0
		cur.ActiveUntilUnix = 0
	}
	if _, err := tx.ExecContext(ctx, `UPDATE schedules SET enabled=?, next_due_unix=?, revision=?, updated_unix=?,
		snooze_until_unix=?, snooze_count=?, active_occurrence_unix=?, active_until_unix=? WHERE id=?`,
		boolToInt(cur.Enabled), cur.NextDueUnix, cur.Revision, cur.UpdatedUnix,
		cur.SnoozeUntilUnix, cur.SnoozeCount, cur.ActiveOccurrenceUnix, cur.ActiveUntilUnix, cur.Id,
	); err != nil {
		return nil, err
	}
	if err := tx.Commit(); err != nil {
		return nil, err
	}
	return cur, nil
}

// DeleteSchedule soft-deletes a schedule, stops pending triggers and queues a
// STOP control event when an occurrence is active.
func (s *Store) DeleteSchedule(ctx context.Context, id string, now time.Time) (bool, error) {
	now = now.UTC()
	tx, err := s.begin(ctx)
	if err != nil {
		return false, err
	}
	defer func() { _ = tx.Rollback() }()
	row := tx.QueryRowContext(ctx, `SELECT `+scheduleCols+` FROM schedules WHERE id = ?`, id)
	rec, err := scanSchedule(row.Scan)
	if errors.Is(err, sql.ErrNoRows) {
		return false, ErrNotFound
	}
	if err != nil {
		return false, err
	}
	if rec.deleted != 0 {
		// Already deleted: idempotent replay.
		if err := tx.Commit(); err != nil {
			return false, err
		}
		return false, nil
	}
	sch := rec.schedule
	occ, active := s.activeOccurrence(sch, now)
	if active {
		if _, _, err := s.insertEvent(ctx, tx, eventInsert{
			dedupeKey:   fmt.Sprintf("control:stop:%s:%d:0", sch.Id, occ),
			sourceID:    sch.Id,
			sourceKind:  sourceKindOf(sch.Kind),
			action:      ricardov1.EventAction_EVENT_ACTION_STOP,
			dueUnix:     now.Unix(),
			occurrence:  occ,
			label:       sch.Label,
			payloadJSON: sch.PayloadJson,
			state:       ricardov1.EventState_EVENT_STATE_PENDING,
			detail:      "deleted",
			createdUnix: now.Unix(),
		}); err != nil {
			return false, err
		}
	}
	if err := s.skipFireEvents(ctx, tx, sch.Id, now, "deleted"); err != nil {
		return false, err
	}
	if _, err := tx.ExecContext(ctx, `UPDATE schedules SET deleted_unix=?, enabled=0, next_due_unix=0,
		revision=revision+1, updated_unix=?, active_occurrence_unix=0, active_until_unix=0,
		snooze_until_unix=0 WHERE id=?`,
		now.Unix(), now.Unix(), sch.Id); err != nil {
		return false, err
	}
	if err := tx.Commit(); err != nil {
		return false, err
	}
	return true, nil
}

// SnoozeSchedule queues an immediate STOP control event and a delayed FIRE
// occurrence. Snooze eligibility is based on the active ringing window, not on
// transport lease state, so it still works after a DELIVERED ack.
func (s *Store) SnoozeSchedule(ctx context.Context, id string, minutes int32, now time.Time) (*ricardov1.Schedule, *ricardov1.DueEvent, error) {
	if minutes < 0 || minutes > 1440 {
		return nil, nil, fmt.Errorf("%w: snooze minutes must be 0..1440", ErrInvalid)
	}
	if minutes == 0 {
		minutes = int32(s.cfg.DefaultSnoozeMinutes)
	}
	now = now.UTC()
	tx, err := s.begin(ctx)
	if err != nil {
		return nil, nil, err
	}
	defer func() { _ = tx.Rollback() }()
	rec, err := getScheduleTx(ctx, tx, id)
	if err != nil {
		return nil, nil, err
	}
	sch := rec.schedule
	occ, active := s.activeOccurrence(sch, now)
	if !active {
		return nil, nil, fmt.Errorf("%w: schedule is not ringing", ErrPrecondition)
	}

	var maxSeq sql.NullInt64
	if err := tx.QueryRowContext(ctx, `SELECT MAX(snooze_seq) FROM events WHERE source_id = ? AND occurrence_unix = ?`, sch.Id, occ).Scan(&maxSeq); err != nil {
		return nil, nil, err
	}
	seq := int32(sch.SnoozeCount + 1)
	if maxSeq.Valid && maxSeq.Int64 >= int64(seq) {
		seq = int32(maxSeq.Int64 + 1)
	}
	snoozeDue := now.Add(time.Duration(minutes) * time.Minute)

	if _, _, err := s.insertEvent(ctx, tx, eventInsert{
		dedupeKey:   fmt.Sprintf("control:stop:%s:%d:%d", sch.Id, occ, seq),
		sourceID:    sch.Id,
		sourceKind:  sourceKindOf(sch.Kind),
		action:      ricardov1.EventAction_EVENT_ACTION_STOP,
		dueUnix:     now.Unix(),
		occurrence:  occ,
		label:       sch.Label,
		payloadJSON: sch.PayloadJson,
		state:       ricardov1.EventState_EVENT_STATE_PENDING,
		detail:      fmt.Sprintf("snoozed for %d minutes", minutes),
		createdUnix: now.Unix(),
		snoozeSeq:   seq,
	}); err != nil {
		return nil, nil, err
	}
	if err := s.skipFireEvents(ctx, tx, sch.Id, now, "snoozed"); err != nil {
		return nil, nil, err
	}
	fire, _, err := s.insertEvent(ctx, tx, eventInsert{
		dedupeKey:   fmt.Sprintf("trigger:%d:%s:%d:%d", sourceKindOf(sch.Kind), sch.Id, occ, seq),
		sourceID:    sch.Id,
		sourceKind:  sourceKindOf(sch.Kind),
		action:      ricardov1.EventAction_EVENT_ACTION_FIRE,
		dueUnix:     snoozeDue.Unix(),
		occurrence:  occ,
		label:       sch.Label,
		payloadJSON: sch.PayloadJson,
		state:       ricardov1.EventState_EVENT_STATE_PENDING,
		createdUnix: now.Unix(),
		snoozeSeq:   seq,
	})
	if err != nil {
		return nil, nil, err
	}
	sch.SnoozeUntilUnix = snoozeDue.Unix()
	sch.SnoozeCount = int64(seq)
	sch.ActiveOccurrenceUnix = occ
	sch.ActiveUntilUnix = snoozeDue.Add(s.cfg.RingWindow).Unix()
	sch.Revision++
	sch.UpdatedUnix = now.Unix()
	if _, err := tx.ExecContext(ctx, `UPDATE schedules SET snooze_until_unix=?, snooze_count=?,
		active_occurrence_unix=?, active_until_unix=?, revision=?, updated_unix=? WHERE id=?`,
		sch.SnoozeUntilUnix, sch.SnoozeCount, sch.ActiveOccurrenceUnix, sch.ActiveUntilUnix,
		sch.Revision, sch.UpdatedUnix, sch.Id); err != nil {
		return nil, nil, err
	}
	if err := tx.Commit(); err != nil {
		return nil, nil, err
	}
	return sch, fire, nil
}

// StopSchedule queues an immediate STOP control event, cancels pending
// triggers and clears the active window. Recurrence is untouched.
func (s *Store) StopSchedule(ctx context.Context, id string, now time.Time) (*ricardov1.Schedule, error) {
	now = now.UTC()
	tx, err := s.begin(ctx)
	if err != nil {
		return nil, err
	}
	defer func() { _ = tx.Rollback() }()
	rec, err := getScheduleTx(ctx, tx, id)
	if err != nil {
		return nil, err
	}
	sch := rec.schedule
	occ, active := s.activeOccurrence(sch, now)
	if !active {
		// Repeated Stop on the same occurrence replays the existing STOP
		// control event instead of creating a new one or failing. An
		// occurrence closed by ring-window expiry (the scheduler emitted the
		// STOP itself) is also already stopped, so the call stays idempotent
		// inside the same ring window.
		var prior int64
		err := tx.QueryRowContext(ctx, `SELECT occurrence_unix FROM events
			WHERE source_id=? AND action=? AND detail IN (?, ?) AND due_unix>=?
			ORDER BY created_unix DESC LIMIT 1`,
			sch.Id, int32(ricardov1.EventAction_EVENT_ACTION_STOP),
			"stopped", DetailRingWindowExpired,
			now.Add(-s.cfg.RingWindow).Unix()).Scan(&prior)
		if err == nil {
			if err := tx.Commit(); err != nil {
				return nil, err
			}
			return sch, nil
		}
		return nil, fmt.Errorf("%w: schedule is not ringing", ErrPrecondition)
	}
	if _, _, err := s.insertEvent(ctx, tx, eventInsert{
		dedupeKey:   fmt.Sprintf("control:stop:%s:%d:0", sch.Id, occ),
		sourceID:    sch.Id,
		sourceKind:  sourceKindOf(sch.Kind),
		action:      ricardov1.EventAction_EVENT_ACTION_STOP,
		dueUnix:     now.Unix(),
		occurrence:  occ,
		label:       sch.Label,
		payloadJSON: sch.PayloadJson,
		state:       ricardov1.EventState_EVENT_STATE_PENDING,
		detail:      "stopped",
		createdUnix: now.Unix(),
	}); err != nil {
		return nil, err
	}
	if err := s.skipFireEvents(ctx, tx, sch.Id, now, "stopped"); err != nil {
		return nil, err
	}
	sch.ActiveOccurrenceUnix = 0
	sch.ActiveUntilUnix = 0
	sch.SnoozeUntilUnix = 0
	sch.Revision++
	sch.UpdatedUnix = now.Unix()
	if _, err := tx.ExecContext(ctx, `UPDATE schedules SET active_occurrence_unix=0, active_until_unix=0,
		snooze_until_unix=0, revision=?, updated_unix=? WHERE id=?`,
		sch.Revision, sch.UpdatedUnix, sch.Id); err != nil {
		return nil, err
	}
	if err := tx.Commit(); err != nil {
		return nil, err
	}
	return sch, nil
}

func sourceKindOf(kind ricardov1.ScheduleKind) ricardov1.SourceKind {
	switch kind {
	case ricardov1.ScheduleKind_SCHEDULE_KIND_ALARM:
		return ricardov1.SourceKind_SOURCE_KIND_ALARM
	case ricardov1.ScheduleKind_SCHEDULE_KIND_REMINDER:
		return ricardov1.SourceKind_SOURCE_KIND_REMINDER
	case ricardov1.ScheduleKind_SCHEDULE_KIND_SCENARIO:
		return ricardov1.SourceKind_SOURCE_KIND_SCENARIO
	default:
		return ricardov1.SourceKind_SOURCE_KIND_UNSPECIFIED
	}
}

// activeOccurrence resolves the occurrence that is eligible for stop/snooze
// from the persisted ringing window. It intentionally has no event fallback:
// StopSchedule clears the window, so a stale DELIVERED event must not
// resurrect snooze eligibility.
func (s *Store) activeOccurrence(sch *ricardov1.Schedule, now time.Time) (int64, bool) {
	if sch.ActiveOccurrenceUnix <= 0 {
		return 0, false
	}
	if sch.ActiveUntilUnix > now.Unix() {
		return sch.ActiveOccurrenceUnix, true
	}
	if sch.SnoozeUntilUnix > now.Unix() {
		return sch.ActiveOccurrenceUnix, true
	}
	return 0, false
}
