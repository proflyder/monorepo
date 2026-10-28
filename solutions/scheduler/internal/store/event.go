package store

import (
	"context"
	"database/sql"
	"errors"
	"fmt"
	"regexp"
	"sort"
	"strings"
	"time"

	ricardov1 "ricardo.local/monorepo/packages/proto/ricardo/v1"
	"ricardo.local/monorepo/solutions/scheduler/internal/idgen"
)

const eventCols = `id, dedupe_key, source_id, source_kind, action, due_unix,
	occurrence_unix, label, payload_json, state, lease_owner, lease_until_unix,
	attempts, snooze_seq, detail, created_unix, acked_unix, ack_outcome,
	task_type, queue`

// Queue names used by the built-in domains. Generic tasks default to
// QueueTasks; convenience alarm/reminder/timer events route to QueueHome.
const (
	QueueHome     = "home"
	QueueTasks    = "tasks"
	QueueScenario = "scenario"
)

var consumerIDPattern = regexp.MustCompile(`^[A-Za-z0-9._-]{1,64}$`)

type eventInsert struct {
	dedupeKey   string
	sourceID    string
	sourceKind  ricardov1.SourceKind
	action      ricardov1.EventAction
	dueUnix     int64
	occurrence  int64
	label       string
	payloadJSON string
	state       ricardov1.EventState
	detail      string
	snoozeSeq   int32
	createdUnix int64
	ackedUnix   int64
	ackOutcome  ricardov1.AckOutcome
	taskType    string
	queue       string
}

// eventRecord carries columns that are intentionally not exposed on the wire
// (lease owner, dedupe identity) next to the protobuf event.
type eventRecord struct {
	event      *ricardov1.DueEvent
	dedupeKey  string
	leaseOwner string
}

func scanEvent(scan func(dest ...any) error) (*eventRecord, error) {
	var (
		rec                eventRecord
		e                  ricardov1.DueEvent
		sourceKind, action int32
		state              int32
		ackOutcome         int32
	)
	if err := scan(
		&e.Id, &rec.dedupeKey, &e.SourceId, &sourceKind, &action, &e.DueUnix,
		&e.OccurrenceUnix, &e.Label, &e.PayloadJson, &state, &rec.leaseOwner,
		&e.LeaseUntilUnix, &e.Attempts, &e.SnoozeSeq, &e.Detail, &e.CreatedUnix,
		&e.AckedUnix, &ackOutcome, &e.TaskType, &e.Queue,
	); err != nil {
		return nil, err
	}
	e.SourceKind = ricardov1.SourceKind(sourceKind)
	e.Action = ricardov1.EventAction(action)
	e.State = ricardov1.EventState(state)
	e.AckOutcome = ricardov1.AckOutcome(ackOutcome)
	rec.event = &e
	return &rec, nil
}

func defaultTaskType(source ricardov1.SourceKind, action ricardov1.EventAction) string {
	prefix := ""
	switch source {
	case ricardov1.SourceKind_SOURCE_KIND_ALARM:
		prefix = "alarm"
	case ricardov1.SourceKind_SOURCE_KIND_REMINDER:
		prefix = "reminder"
	case ricardov1.SourceKind_SOURCE_KIND_TIMER:
		prefix = "timer"
	case ricardov1.SourceKind_SOURCE_KIND_SCENARIO:
		prefix = "scenario"
	case ricardov1.SourceKind_SOURCE_KIND_TASK:
		prefix = "task"
	default:
		prefix = "event"
	}
	if action == ricardov1.EventAction_EVENT_ACTION_STOP {
		return prefix + ".stop"
	}
	return prefix + ".fire"
}

// insertEvent inserts an outbox row, ignoring an existing identical dedupe
// identity. It reports whether a new row was written and returns the stored
// row.
func (s *Store) insertEvent(ctx context.Context, tx *sql.Tx, in eventInsert) (*ricardov1.DueEvent, bool, error) {
	if in.dedupeKey == "" {
		return nil, false, fmt.Errorf("%w: empty dedupe key", ErrInvalid)
	}
	if in.queue == "" {
		switch in.sourceKind {
		case ricardov1.SourceKind_SOURCE_KIND_TASK:
			in.queue = QueueTasks
		case ricardov1.SourceKind_SOURCE_KIND_SCENARIO:
			// Scenario execution is not implemented; keep events out of the
			// home executor queue until an executor exists.
			in.queue = QueueScenario
		default:
			in.queue = QueueHome
		}
	}
	if in.taskType == "" {
		in.taskType = defaultTaskType(in.sourceKind, in.action)
	}
	id := idgen.EventID(in.dedupeKey)
	res, err := tx.ExecContext(ctx, `INSERT OR IGNORE INTO events (`+eventCols+`)
		VALUES (?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?)`,
		id, in.dedupeKey, in.sourceID, int32(in.sourceKind), int32(in.action),
		in.dueUnix, in.occurrence, in.label, in.payloadJSON, int32(in.state),
		"", int64(0), int32(0), in.snoozeSeq, in.detail, in.createdUnix,
		in.ackedUnix, int32(in.ackOutcome), in.taskType, in.queue,
	)
	if err != nil {
		return nil, false, fmt.Errorf("insert event: %w", err)
	}
	inserted, _ := res.RowsAffected()
	row := tx.QueryRowContext(ctx, `SELECT `+eventCols+` FROM events WHERE id = ?`, id)
	rec, err := scanEvent(row.Scan)
	if err != nil {
		return nil, false, err
	}
	return rec.event, inserted > 0, nil
}

func (s *Store) skipFireEvents(ctx context.Context, tx *sql.Tx, sourceID string, now time.Time, detail string) error {
	_, err := tx.ExecContext(ctx, `UPDATE events SET state=?, lease_owner='', lease_until_unix=0,
		acked_unix=?, ack_outcome=?, detail=?
		WHERE source_id=? AND action=? AND state IN (?, ?)`,
		int32(ricardov1.EventState_EVENT_STATE_SKIPPED), now.Unix(),
		int32(ricardov1.AckOutcome_ACK_OUTCOME_SKIPPED), detail,
		sourceID, int32(ricardov1.EventAction_EVENT_ACTION_FIRE),
		int32(ricardov1.EventState_EVENT_STATE_PENDING),
		int32(ricardov1.EventState_EVENT_STATE_LEASED),
	)
	if err != nil {
		return fmt.Errorf("skip fire events: %w", err)
	}
	return nil
}

// releaseExpiredLeases returns timed-out leases to PENDING, or fails an event
// after MaxAttempts. The event id is unchanged, so redelivery is stable.
func (s *Store) releaseExpiredLeases(ctx context.Context, tx *sql.Tx, now time.Time) (int64, error) {
	res, err := tx.ExecContext(ctx, `UPDATE events SET state=?, lease_owner='', lease_until_unix=0,
		detail=CASE WHEN detail='' THEN 'lease expired' ELSE detail END
		WHERE state=? AND lease_until_unix>0 AND lease_until_unix<=? AND attempts < ?`,
		int32(ricardov1.EventState_EVENT_STATE_PENDING),
		int32(ricardov1.EventState_EVENT_STATE_LEASED), now.Unix(), s.cfg.MaxAttempts,
	)
	if err != nil {
		return 0, err
	}
	released, _ := res.RowsAffected()
	if _, err := tx.ExecContext(ctx, `UPDATE events SET state=?, lease_owner='', lease_until_unix=0,
		acked_unix=?, ack_outcome=?, detail='lease attempts exhausted'
		WHERE state=? AND lease_until_unix>0 AND lease_until_unix<=? AND attempts >= ?`,
		int32(ricardov1.EventState_EVENT_STATE_FAILED), now.Unix(),
		int32(ricardov1.AckOutcome_ACK_OUTCOME_FAILED),
		int32(ricardov1.EventState_EVENT_STATE_LEASED), now.Unix(), s.cfg.MaxAttempts,
	); err != nil {
		return 0, err
	}
	return released, nil
}

func (s *Store) touchConsumer(ctx context.Context, tx *sql.Tx, consumerID string, now time.Time, ack bool) error {
	lastAck := int64(0)
	if ack {
		lastAck = now.Unix()
	}
	_, err := tx.ExecContext(ctx, `INSERT INTO consumers (consumer_id, last_seen_unix, last_ack_unix)
		VALUES (?,?,?)
		ON CONFLICT(consumer_id) DO UPDATE SET
			last_seen_unix=excluded.last_seen_unix,
			last_ack_unix=CASE WHEN excluded.last_ack_unix > consumers.last_ack_unix
				THEN excluded.last_ack_unix ELSE consumers.last_ack_unix END`,
		consumerID, now.Unix(), lastAck)
	if err != nil {
		return fmt.Errorf("touch consumer: %w", err)
	}
	return nil
}

func validateConsumer(consumerID string) error {
	if !consumerIDPattern.MatchString(consumerID) {
		return fmt.Errorf("%w: consumer_id must be 1..64 chars of [A-Za-z0-9._-]", ErrInvalid)
	}
	return nil
}

// ListDueEvents leases up to limit due events for a consumer, optionally
// restricted to one queue. Expired leases from any consumer are released
// first.
func (s *Store) ListDueEvents(ctx context.Context, consumerID string, leaseSeconds, limit int32, queue string, now time.Time) ([]*ricardov1.DueEvent, error) {
	if err := validateConsumer(consumerID); err != nil {
		return nil, err
	}
	if queue != "" {
		if err := validateQueue(queue); err != nil {
			return nil, err
		}
	}
	lease := time.Duration(leaseSeconds) * time.Second
	if leaseSeconds == 0 {
		lease = 60 * time.Second
	} else if leaseSeconds < 0 || lease > s.cfg.MaxLease {
		return nil, fmt.Errorf("%w: lease_seconds must be 1..%d", ErrInvalid, int(s.cfg.MaxLease.Seconds()))
	}
	if limit == 0 {
		limit = 50
	} else if limit < 0 || limit > 200 {
		return nil, fmt.Errorf("%w: limit must be 1..200", ErrInvalid)
	}
	now = now.UTC()

	tx, err := s.begin(ctx)
	if err != nil {
		return nil, err
	}
	defer func() { _ = tx.Rollback() }()
	if _, err := s.releaseExpiredLeases(ctx, tx, now); err != nil {
		return nil, err
	}
	if err := s.touchConsumer(ctx, tx, consumerID, now, false); err != nil {
		return nil, err
	}
	query := `SELECT id FROM events WHERE state = ? AND due_unix <= ?`
	args := []any{int32(ricardov1.EventState_EVENT_STATE_PENDING), now.Unix()}
	if queue != "" {
		query += ` AND queue = ?`
		args = append(args, queue)
	}
	query += ` ORDER BY due_unix, id LIMIT ?`
	args = append(args, limit)
	rows, err := tx.QueryContext(ctx, query, args...)
	if err != nil {
		return nil, err
	}
	ids := make([]string, 0, limit)
	for rows.Next() {
		var id string
		if err := rows.Scan(&id); err != nil {
			_ = rows.Close()
			return nil, err
		}
		ids = append(ids, id)
	}
	if err := rows.Err(); err != nil {
		_ = rows.Close()
		return nil, err
	}
	_ = rows.Close()

	out := make([]*ricardov1.DueEvent, 0, len(ids))
	for _, id := range ids {
		res, err := tx.ExecContext(ctx, `UPDATE events SET state=?, lease_owner=?, lease_until_unix=?, attempts=attempts+1
			WHERE id=? AND state=?`,
			int32(ricardov1.EventState_EVENT_STATE_LEASED), consumerID,
			now.Add(lease).Unix(), id, int32(ricardov1.EventState_EVENT_STATE_PENDING))
		if err != nil {
			return nil, err
		}
		if n, _ := res.RowsAffected(); n == 0 {
			continue
		}
		row := tx.QueryRowContext(ctx, `SELECT `+eventCols+` FROM events WHERE id=?`, id)
		rec, err := scanEvent(row.Scan)
		if err != nil {
			return nil, err
		}
		out = append(out, rec.event)
	}
	if err := tx.Commit(); err != nil {
		return nil, err
	}
	return out, nil
}

// AckEvent records a terminal outcome. Repeated acks are idempotent.
func (s *Store) AckEvent(ctx context.Context, eventID, consumerID string, outcome ricardov1.AckOutcome, detail string, now time.Time) (*ricardov1.DueEvent, error) {
	if strings.TrimSpace(eventID) == "" {
		return nil, fmt.Errorf("%w: event_id is required", ErrInvalid)
	}
	if err := validateConsumer(consumerID); err != nil {
		return nil, err
	}
	switch outcome {
	case ricardov1.AckOutcome_ACK_OUTCOME_DELIVERED,
		ricardov1.AckOutcome_ACK_OUTCOME_FAILED,
		ricardov1.AckOutcome_ACK_OUTCOME_SKIPPED:
	default:
		return nil, fmt.Errorf("%w: ack outcome must be DELIVERED, FAILED or SKIPPED", ErrInvalid)
	}
	if len(detail) > 1024 {
		return nil, fmt.Errorf("%w: detail exceeds 1024 bytes", ErrInvalid)
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
	switch ev.State {
	case ricardov1.EventState_EVENT_STATE_DELIVERED,
		ricardov1.EventState_EVENT_STATE_FAILED,
		ricardov1.EventState_EVENT_STATE_SKIPPED:
		// Terminal ack replay is idempotent; it is not a new delivery and must
		// not manufacture consumer liveness.
		if err := tx.Commit(); err != nil {
			return nil, err
		}
		return ev, nil
	case ricardov1.EventState_EVENT_STATE_LEASED:
		// First ack requires the current lease.
	default:
		return nil, fmt.Errorf("%w: event is not leased", ErrPrecondition)
	}
	if rec.leaseOwner != consumerID {
		return nil, fmt.Errorf("%w: event is leased by another consumer", ErrPrecondition)
	}
	if ev.LeaseUntilUnix <= now.Unix() {
		return nil, fmt.Errorf("%w: lease has expired", ErrPrecondition)
	}
	if err := s.touchConsumer(ctx, tx, consumerID, now, true); err != nil {
		return nil, err
	}
	if detail == "" {
		detail = ev.Detail
	}
	state := ricardov1.EventState_EVENT_STATE_DELIVERED
	switch outcome {
	case ricardov1.AckOutcome_ACK_OUTCOME_FAILED:
		state = ricardov1.EventState_EVENT_STATE_FAILED
	case ricardov1.AckOutcome_ACK_OUTCOME_SKIPPED:
		state = ricardov1.EventState_EVENT_STATE_SKIPPED
	}
	if _, err := tx.ExecContext(ctx, `UPDATE events SET state=?, acked_unix=?, ack_outcome=?,
		lease_owner='', lease_until_unix=0, detail=? WHERE id=?`,
		int32(state), now.Unix(), int32(outcome), detail, eventID); err != nil {
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

// Status is an honest snapshot of scheduler state. It never asserts that sound
// was audible; it only reports events, leases and consumer liveness.
type Status struct {
	SchemaVersion    int
	PendingEvents    int32
	LeasedEvents     int32
	EnabledSchedules int32
	RunningTimers    int32
	Consumers        []*ricardov1.ConsumerStatus
	Queues           []*ricardov1.QueueInfo
}

// Status collects counters and consumer liveness at now.
func (s *Store) Status(ctx context.Context, now time.Time) (Status, error) {
	st := Status{SchemaVersion: SchemaVersion}
	counters := []struct {
		query string
		dest  *int32
	}{
		{`SELECT COUNT(*) FROM events WHERE state = 1`, &st.PendingEvents},
		{`SELECT COUNT(*) FROM events WHERE state = 2`, &st.LeasedEvents},
		{`SELECT COUNT(*) FROM schedules WHERE enabled = 1 AND deleted_unix = 0`, &st.EnabledSchedules},
		{`SELECT COUNT(*) FROM timers WHERE state IN (1, 2)`, &st.RunningTimers},
	}
	for _, c := range counters {
		var n int64
		if err := s.db.QueryRowContext(ctx, c.query).Scan(&n); err != nil {
			return st, err
		}
		*c.dest = int32(n)
	}
	rows, err := s.db.QueryContext(ctx, `SELECT consumer_id, last_seen_unix, last_ack_unix FROM consumers ORDER BY consumer_id`)
	if err != nil {
		return st, err
	}
	defer func() { _ = rows.Close() }()
	for rows.Next() {
		var c ricardov1.ConsumerStatus
		if err := rows.Scan(&c.ConsumerId, &c.LastSeenUnix, &c.LastAckUnix); err != nil {
			return st, err
		}
		c.Alive = c.LastSeenUnix > 0 && now.Sub(time.Unix(c.LastSeenUnix, 0)) <= s.cfg.ConsumerLiveness
		st.Consumers = append(st.Consumers, &c)
	}
	if err := rows.Err(); err != nil {
		return st, err
	}
	if err := rows.Close(); err != nil {
		return st, err
	}
	qrows, err := s.db.QueryContext(ctx, `SELECT queue,
		SUM(CASE WHEN state = 1 THEN 1 ELSE 0 END),
		SUM(CASE WHEN state = 2 THEN 1 ELSE 0 END)
		FROM events GROUP BY queue ORDER BY queue`)
	if err != nil {
		return st, err
	}
	defer func() { _ = qrows.Close() }()
	for qrows.Next() {
		var q ricardov1.QueueInfo
		if err := qrows.Scan(&q.Name, &q.Pending, &q.Leased); err != nil {
			return st, err
		}
		st.Queues = append(st.Queues, &q)
	}
	if err := qrows.Err(); err != nil {
		return st, err
	}
	// Built-in queues are discoverable even when idle, so executors do not
	// have to guess names before the first event exists.
	present := make(map[string]bool, len(st.Queues))
	for _, q := range st.Queues {
		present[q.GetName()] = true
	}
	for _, name := range []string{QueueHome, QueueTasks, QueueScenario} {
		if !present[name] {
			st.Queues = append(st.Queues, &ricardov1.QueueInfo{Name: name})
		}
	}
	sort.Slice(st.Queues, func(i, j int) bool { return st.Queues[i].GetName() < st.Queues[j].GetName() })
	return st, nil
}

// Purge removes terminal events and idempotency rows older than the configured
// retention window.
func (s *Store) Purge(ctx context.Context, now time.Time) (int64, error) {
	cutoff := now.Add(-s.cfg.EventRetention).Unix()
	res, err := s.db.ExecContext(ctx, `DELETE FROM events
		WHERE state IN (?, ?, ?) AND created_unix < ?`,
		int32(ricardov1.EventState_EVENT_STATE_DELIVERED),
		int32(ricardov1.EventState_EVENT_STATE_FAILED),
		int32(ricardov1.EventState_EVENT_STATE_SKIPPED),
		cutoff)
	if err != nil {
		return 0, err
	}
	n, _ := res.RowsAffected()
	if _, err := s.db.ExecContext(ctx, `DELETE FROM idempotency WHERE created_unix < ?`, cutoff); err != nil {
		return n, err
	}
	return n, nil
}
