package store

import (
	"context"
	"database/sql"
	"encoding/json"
	"errors"
	"fmt"
	"regexp"
	"strings"
	"time"

	ricardov1 "ricardo.local/monorepo/packages/proto/ricardo/v1"
	"ricardo.local/monorepo/solutions/scheduler/internal/idgen"
)

var (
	taskTypePattern = regexp.MustCompile(`^[a-z][a-z0-9._-]{0,63}$`)
	queuePattern    = regexp.MustCompile(`^[a-z][a-z0-9._-]{0,31}$`)
)

// MaxTaskDelay bounds far-future task scheduling.
const MaxTaskDelay = 10 * 365 * 24 * time.Hour

func validateTaskType(taskType string) error {
	if !taskTypePattern.MatchString(taskType) {
		return fmt.Errorf("%w: task_type must be 1..64 chars of [a-z0-9._-] starting with a letter", ErrInvalid)
	}
	return nil
}

func validateQueue(queue string) error {
	if !queuePattern.MatchString(queue) {
		return fmt.Errorf("%w: queue must be 1..32 chars of [a-z0-9._-] starting with a letter", ErrInvalid)
	}
	return nil
}

func canonicalTask(taskType, queue, payload string, runAt int64) string {
	return fmt.Sprintf("task|%s|%s|%s|%d", taskType, queue, payload, runAt)
}

// EnqueueTask durably queues a generic task event. It is one-shot: recurring
// generic task schedules are not implemented. The event flows through the same
// lease/ack outbox as alarm/reminder/timer events.
func (s *Store) EnqueueTask(ctx context.Context, taskType, payloadJSON, queue string, runAtUnix int64, idempotencyKey string, now time.Time) (*ricardov1.DueEvent, error) {
	if err := validateTaskType(taskType); err != nil {
		return nil, err
	}
	if strings.TrimSpace(queue) == "" {
		queue = QueueTasks
	}
	if err := validateQueue(queue); err != nil {
		return nil, err
	}
	if payloadJSON == "" {
		payloadJSON = "{}"
	}
	if len(payloadJSON) > s.cfg.MaxPayloadBytes {
		return nil, fmt.Errorf("%w: payload exceeds %d bytes", ErrInvalid, s.cfg.MaxPayloadBytes)
	}
	if !json.Valid([]byte(payloadJSON)) {
		return nil, fmt.Errorf("%w: payload_json must be valid JSON", ErrInvalid)
	}
	if runAtUnix < 0 {
		return nil, fmt.Errorf("%w: run_at_unix must not be negative", ErrInvalid)
	}
	if err := validIdempotencyKey(idempotencyKey); err != nil {
		return nil, err
	}
	now = now.UTC()
	due := runAtUnix
	if due == 0 || due < now.Unix() {
		due = now.Unix()
	}
	if due > now.Add(MaxTaskDelay).Unix() {
		return nil, fmt.Errorf("%w: run_at_unix is more than %s ahead", ErrInvalid, MaxTaskDelay)
	}

	hash := idgen.HashRequest(canonicalTask(taskType, queue, payloadJSON, runAtUnix))

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
			if kind != "task" || existingHash != hash {
				return nil, fmt.Errorf("%w: idempotency key reused with a different request", ErrIdempotency)
			}
			row := tx.QueryRowContext(ctx, `SELECT `+eventCols+` FROM events WHERE id=?`, resourceID)
			rec, err := scanEvent(row.Scan)
			if errors.Is(err, sql.ErrNoRows) {
				return nil, fmt.Errorf("%w: idempotent task no longer exists", ErrIdempotency)
			}
			if err != nil {
				return nil, err
			}
			if err := tx.Commit(); err != nil {
				return nil, err
			}
			return rec.event, nil
		case !errors.Is(err, sql.ErrNoRows):
			return nil, err
		}
	}

	taskID, err := idgen.NewResourceID(now)
	if err != nil {
		return nil, err
	}
	ev, _, err := s.insertEvent(ctx, tx, eventInsert{
		dedupeKey:   fmt.Sprintf("task:%s:0", taskID),
		sourceID:    taskID,
		sourceKind:  ricardov1.SourceKind_SOURCE_KIND_TASK,
		action:      ricardov1.EventAction_EVENT_ACTION_FIRE,
		dueUnix:     due,
		occurrence:  0,
		label:       "",
		payloadJSON: payloadJSON,
		state:       ricardov1.EventState_EVENT_STATE_PENDING,
		createdUnix: now.Unix(),
		taskType:    taskType,
		queue:       queue,
	})
	if err != nil {
		return nil, err
	}
	if idempotencyKey != "" {
		if _, err := tx.ExecContext(ctx,
			`INSERT INTO idempotency (key, resource_kind, resource_id, request_hash, created_unix) VALUES (?,?,?,?,?)`,
			idempotencyKey, "task", ev.Id, hash, now.Unix()); err != nil {
			return nil, fmt.Errorf("%w: idempotency key already exists", ErrIdempotency)
		}
	}
	if err := tx.Commit(); err != nil {
		return nil, err
	}
	return ev, nil
}

// CancelTask cancels a task event that is still pending/unstarted. A leased
// task is a precondition error because an executor may already be running it.
// Terminal events replay unchanged after verification.
func (s *Store) CancelTask(ctx context.Context, eventID string, now time.Time) (*ricardov1.DueEvent, error) {
	if strings.TrimSpace(eventID) == "" {
		return nil, fmt.Errorf("%w: event_id is required", ErrInvalid)
	}
	now = now.UTC()
	tx, err := s.begin(ctx)
	if err != nil {
		return nil, err
	}
	defer func() { _ = tx.Rollback() }()
	row := tx.QueryRowContext(ctx, `SELECT `+eventCols+` FROM events WHERE id=?`, eventID)
	rec, err := scanEvent(row.Scan)
	if errors.Is(err, sql.ErrNoRows) {
		return nil, ErrNotFound
	}
	if err != nil {
		return nil, err
	}
	ev := rec.event
	if ev.SourceKind != ricardov1.SourceKind_SOURCE_KIND_TASK {
		return nil, fmt.Errorf("%w: only task events can be cancelled; use StopSchedule", ErrPrecondition)
	}
	switch ev.State {
	case ricardov1.EventState_EVENT_STATE_DELIVERED,
		ricardov1.EventState_EVENT_STATE_FAILED,
		ricardov1.EventState_EVENT_STATE_SKIPPED:
		if err := tx.Commit(); err != nil {
			return nil, err
		}
		return ev, nil
	case ricardov1.EventState_EVENT_STATE_LEASED:
		return nil, fmt.Errorf("%w: task is already leased by an executor", ErrPrecondition)
	}
	if _, err := tx.ExecContext(ctx, `UPDATE events SET state=?, acked_unix=?, ack_outcome=?,
		lease_owner='', lease_until_unix=0, detail='cancelled' WHERE id=?`,
		int32(ricardov1.EventState_EVENT_STATE_SKIPPED), now.Unix(),
		int32(ricardov1.AckOutcome_ACK_OUTCOME_SKIPPED), eventID); err != nil {
		return nil, err
	}
	row = tx.QueryRowContext(ctx, `SELECT `+eventCols+` FROM events WHERE id=?`, eventID)
	updated, err := scanEvent(row.Scan)
	if err != nil {
		return nil, err
	}
	if err := tx.Commit(); err != nil {
		return nil, err
	}
	return updated.event, nil
}
