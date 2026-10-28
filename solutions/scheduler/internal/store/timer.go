package store

import (
	"context"
	"database/sql"
	"errors"
	"fmt"
	"strings"
	"time"

	ricardov1 "ricardo.local/monorepo/packages/proto/ricardo/v1"
	"ricardo.local/monorepo/solutions/scheduler/internal/idgen"
)

const timerCols = `id, label, duration_seconds, remaining_seconds, due_unix,
	state, revision, created_unix, updated_unix`

func scanTimer(scan func(dest ...any) error) (*ricardov1.Timer, error) {
	var (
		t     ricardov1.Timer
		state int32
	)
	if err := scan(&t.Id, &t.Label, &t.DurationSeconds, &t.RemainingSeconds, &t.DueUnix,
		&state, &t.Revision, &t.CreatedUnix, &t.UpdatedUnix); err != nil {
		return nil, err
	}
	t.State = ricardov1.TimerState(state)
	return &t, nil
}

func canonicalTimer(label string, duration int64) string {
	return fmt.Sprintf("timer|%s|%d", label, duration)
}

// StartTimer starts a durable countdown. A non-empty idempotency key replays
// the original timer when the request matches.
func (s *Store) StartTimer(ctx context.Context, label string, durationSeconds int64, idempotencyKey string, now time.Time) (*ricardov1.Timer, error) {
	if len(label) > 200 {
		return nil, fmt.Errorf("%w: label exceeds 200 bytes", ErrInvalid)
	}
	if durationSeconds <= 0 {
		return nil, fmt.Errorf("%w: duration_seconds must be positive", ErrInvalid)
	}
	if time.Duration(durationSeconds)*time.Second > s.cfg.MaxTimerDuration {
		return nil, fmt.Errorf("%w: duration exceeds %s", ErrInvalid, s.cfg.MaxTimerDuration)
	}
	if err := validIdempotencyKey(idempotencyKey); err != nil {
		return nil, err
	}
	now = now.UTC()
	id, err := idgen.NewResourceID(now)
	if err != nil {
		return nil, err
	}
	hash := idgen.HashRequest(canonicalTimer(label, durationSeconds))

	tx, err := s.begin(ctx)
	if err != nil {
		return nil, err
	}
	defer func() { _ = tx.Rollback() }()

	if idempotencyKey != "" {
		var kind, resourceID, existingHash string
		err := tx.QueryRowContext(ctx,
			`SELECT resource_kind, resource_id, request_hash FROM idempotency WHERE key = ?`,
			idempotencyKey).Scan(&kind, &resourceID, &existingHash)
		switch {
		case err == nil:
			if kind != "timer" || existingHash != hash {
				return nil, fmt.Errorf("%w: idempotency key reused with a different request", ErrIdempotency)
			}
			row := tx.QueryRowContext(ctx, `SELECT `+timerCols+` FROM timers WHERE id=?`, resourceID)
			tm, err := scanTimer(row.Scan)
			if err != nil {
				return nil, fmt.Errorf("%w: idempotent timer no longer exists", ErrIdempotency)
			}
			if err := tx.Commit(); err != nil {
				return nil, err
			}
			return tm, nil
		case !errors.Is(err, sql.ErrNoRows):
			return nil, err
		}
	}

	var active int64
	if err := tx.QueryRowContext(ctx, `SELECT COUNT(*) FROM timers WHERE state IN (?, ?)`,
		int32(ricardov1.TimerState_TIMER_STATE_RUNNING),
		int32(ricardov1.TimerState_TIMER_STATE_PAUSED)).Scan(&active); err != nil {
		return nil, err
	}
	if active >= int64(s.cfg.MaxTimers) {
		return nil, fmt.Errorf("%w: at most %d active timers", ErrLimitExceeded, s.cfg.MaxTimers)
	}

	tm := &ricardov1.Timer{
		Id:              id,
		Label:           label,
		DurationSeconds: durationSeconds,
		DueUnix:         now.Add(time.Duration(durationSeconds) * time.Second).Unix(),
		State:           ricardov1.TimerState_TIMER_STATE_RUNNING,
		Revision:        1,
		CreatedUnix:     now.Unix(),
		UpdatedUnix:     now.Unix(),
	}
	if _, err := tx.ExecContext(ctx, `INSERT INTO timers (`+timerCols+`) VALUES (?,?,?,?,?,?,?,?,?)`,
		tm.Id, tm.Label, tm.DurationSeconds, tm.RemainingSeconds, tm.DueUnix,
		int32(tm.State), tm.Revision, tm.CreatedUnix, tm.UpdatedUnix); err != nil {
		return nil, fmt.Errorf("insert timer: %w", err)
	}
	if idempotencyKey != "" {
		if _, err := tx.ExecContext(ctx,
			`INSERT INTO idempotency (key, resource_kind, resource_id, request_hash, created_unix) VALUES (?,?,?,?,?)`,
			idempotencyKey, "timer", tm.Id, hash, now.Unix()); err != nil {
			return nil, fmt.Errorf("%w: idempotency key already exists", ErrIdempotency)
		}
	}
	if err := tx.Commit(); err != nil {
		return nil, err
	}
	return tm, nil
}

// GetTimer returns a timer by id.
func (s *Store) GetTimer(ctx context.Context, id string) (*ricardov1.Timer, error) {
	if strings.TrimSpace(id) == "" {
		return nil, fmt.Errorf("%w: id is required", ErrInvalid)
	}
	row := s.db.QueryRowContext(ctx, `SELECT `+timerCols+` FROM timers WHERE id=?`, id)
	tm, err := scanTimer(row.Scan)
	if errors.Is(err, sql.ErrNoRows) {
		return nil, ErrNotFound
	}
	return tm, err
}

// ListTimers returns timers ordered by creation time.
func (s *Store) ListTimers(ctx context.Context, includeFinished bool) ([]*ricardov1.Timer, error) {
	query := `SELECT ` + timerCols + ` FROM timers`
	if !includeFinished {
		query += ` WHERE state IN (1, 2)`
	}
	query += ` ORDER BY created_unix, id`
	rows, err := s.db.QueryContext(ctx, query)
	if err != nil {
		return nil, err
	}
	defer func() { _ = rows.Close() }()
	out := make([]*ricardov1.Timer, 0)
	for rows.Next() {
		tm, err := scanTimer(rows.Scan)
		if err != nil {
			return nil, err
		}
		out = append(out, tm)
	}
	return out, rows.Err()
}

func getTimerTx(ctx context.Context, tx *sql.Tx, id string) (*ricardov1.Timer, error) {
	row := tx.QueryRowContext(ctx, `SELECT `+timerCols+` FROM timers WHERE id=?`, id)
	tm, err := scanTimer(row.Scan)
	if errors.Is(err, sql.ErrNoRows) {
		return nil, ErrNotFound
	}
	return tm, err
}

// PauseTimer stores the remaining duration. Pausing a paused timer replays the
// current state; pausing a finished timer is a precondition error.
func (s *Store) PauseTimer(ctx context.Context, id string, now time.Time) (*ricardov1.Timer, error) {
	now = now.UTC()
	tx, err := s.begin(ctx)
	if err != nil {
		return nil, err
	}
	defer func() { _ = tx.Rollback() }()
	tm, err := getTimerTx(ctx, tx, id)
	if err != nil {
		return nil, err
	}
	switch tm.State {
	case ricardov1.TimerState_TIMER_STATE_PAUSED:
		if err := tx.Commit(); err != nil {
			return nil, err
		}
		return tm, nil
	case ricardov1.TimerState_TIMER_STATE_RUNNING:
	default:
		return nil, fmt.Errorf("%w: timer is %s", ErrPrecondition, strings.ToLower(tm.State.String()))
	}
	remaining := tm.DueUnix - now.Unix()
	if remaining < 0 {
		remaining = 0
	}
	tm.RemainingSeconds = remaining
	tm.DueUnix = 0
	tm.State = ricardov1.TimerState_TIMER_STATE_PAUSED
	tm.Revision++
	tm.UpdatedUnix = now.Unix()
	if _, err := tx.ExecContext(ctx, `UPDATE timers SET remaining_seconds=?, due_unix=0, state=?,
		revision=?, updated_unix=? WHERE id=?`,
		tm.RemainingSeconds, int32(tm.State), tm.Revision, tm.UpdatedUnix, tm.Id); err != nil {
		return nil, err
	}
	if err := tx.Commit(); err != nil {
		return nil, err
	}
	return tm, nil
}

// ResumeTimer continues a paused timer from its stored remaining duration.
func (s *Store) ResumeTimer(ctx context.Context, id string, now time.Time) (*ricardov1.Timer, error) {
	now = now.UTC()
	tx, err := s.begin(ctx)
	if err != nil {
		return nil, err
	}
	defer func() { _ = tx.Rollback() }()
	tm, err := getTimerTx(ctx, tx, id)
	if err != nil {
		return nil, err
	}
	switch tm.State {
	case ricardov1.TimerState_TIMER_STATE_RUNNING:
		if err := tx.Commit(); err != nil {
			return nil, err
		}
		return tm, nil
	case ricardov1.TimerState_TIMER_STATE_PAUSED:
	default:
		return nil, fmt.Errorf("%w: timer is %s", ErrPrecondition, strings.ToLower(tm.State.String()))
	}
	tm.DueUnix = now.Add(time.Duration(tm.RemainingSeconds) * time.Second).Unix()
	tm.State = ricardov1.TimerState_TIMER_STATE_RUNNING
	tm.RemainingSeconds = 0
	tm.Revision++
	tm.UpdatedUnix = now.Unix()
	if _, err := tx.ExecContext(ctx, `UPDATE timers SET remaining_seconds=0, due_unix=?, state=?,
		revision=?, updated_unix=? WHERE id=?`,
		tm.DueUnix, int32(tm.State), tm.Revision, tm.UpdatedUnix, tm.Id); err != nil {
		return nil, err
	}
	if err := tx.Commit(); err != nil {
		return nil, err
	}
	return tm, nil
}

// CancelTimer cancels a running or paused timer, or sends STOP for a fired
// timer that may still be ringing.
func (s *Store) CancelTimer(ctx context.Context, id string, now time.Time) (*ricardov1.Timer, error) {
	now = now.UTC()
	tx, err := s.begin(ctx)
	if err != nil {
		return nil, err
	}
	defer func() { _ = tx.Rollback() }()
	tm, err := getTimerTx(ctx, tx, id)
	if err != nil {
		return nil, err
	}
	switch tm.State {
	case ricardov1.TimerState_TIMER_STATE_CANCELLED:
		if err := tx.Commit(); err != nil {
			return nil, err
		}
		return tm, nil
	case ricardov1.TimerState_TIMER_STATE_FIRED:
		if _, _, err := s.insertEvent(ctx, tx, eventInsert{
			dedupeKey:   fmt.Sprintf("control:stop:timer:%s:%d", tm.Id, tm.DueUnix),
			sourceID:    tm.Id,
			sourceKind:  ricardov1.SourceKind_SOURCE_KIND_TIMER,
			action:      ricardov1.EventAction_EVENT_ACTION_STOP,
			dueUnix:     now.Unix(),
			occurrence:  0,
			label:       tm.Label,
			payloadJSON: "{}",
			state:       ricardov1.EventState_EVENT_STATE_PENDING,
			detail:      "cancelled",
			createdUnix: now.Unix(),
		}); err != nil {
			return nil, err
		}
	case ricardov1.TimerState_TIMER_STATE_RUNNING, ricardov1.TimerState_TIMER_STATE_PAUSED:
	default:
		return nil, fmt.Errorf("%w: timer is %s", ErrPrecondition, strings.ToLower(tm.State.String()))
	}
	tm.State = ricardov1.TimerState_TIMER_STATE_CANCELLED
	tm.DueUnix = 0
	tm.RemainingSeconds = 0
	tm.Revision++
	tm.UpdatedUnix = now.Unix()
	if _, err := tx.ExecContext(ctx, `UPDATE timers SET state=?, due_unix=0, remaining_seconds=0,
		revision=?, updated_unix=? WHERE id=?`,
		int32(tm.State), tm.Revision, tm.UpdatedUnix, tm.Id); err != nil {
		return nil, err
	}
	if err := tx.Commit(); err != nil {
		return nil, err
	}
	return tm, nil
}
