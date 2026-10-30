// Package bridge runs the scheduler-to-Home-Assistant executor loop.
//
// It leases only the configured queue, executes an allowlisted script per
// (kind, action) pair, records a local receipt after Home Assistant accepts the
// call, and only then acknowledges the event. There is no HTTP listener, no
// schedule creation, no snooze and no inbound endpoint.
package bridge

import (
	"context"
	"fmt"
	"log/slog"
	"sort"
	"strings"
	"time"

	"ricardo.local/monorepo/solutions/ha-bridge/internal/config"
	"ricardo.local/monorepo/solutions/ha-bridge/internal/httpapi"
	"ricardo.local/monorepo/solutions/ha-bridge/internal/ledger"
	"ricardo.local/monorepo/solutions/ha-bridge/internal/scheduler"
)

// SchedulerAPI is the subset of SchedulerService used by the bridge.
type SchedulerAPI interface {
	ListDueEvents(ctx context.Context, req scheduler.ListRequest) ([]scheduler.Event, error)
	AckEvent(ctx context.Context, eventID, consumerID, outcome, detail string) error
	GetStatus(ctx context.Context) (scheduler.Status, error)
}

// HAController is the Home Assistant side effect surface.
type HAController interface {
	CallScript(ctx context.Context, script string, variables map[string]any) error
	Ping(ctx context.Context) error
}

// Receipts is the local deduplication and active-alarm ledger.
type Receipts interface {
	Seen(eventID string) bool
	Record(receipt ledger.Receipt) error
	ActiveAlarm() (ledger.ActiveAlarm, bool)
	RecordStart(receipt ledger.Receipt, active ledger.ActiveAlarm) error
	RecordStop(receipt ledger.Receipt, expectSourceID string) (bool, error)
}

// Runner owns the executor loop.
type Runner struct {
	cfg      config.Config
	sched    SchedulerAPI
	ha       HAController
	receipts Receipts
	log      *slog.Logger
	sleep    func(ctx context.Context, d time.Duration) error
}

// New builds a runner. receipts may be nil (deduplication disabled); ha may be
// nil only in dry-run tests.
func New(cfg config.Config, sched SchedulerAPI, ha HAController, receipts Receipts, logger *slog.Logger) *Runner {
	if logger == nil {
		logger = slog.New(slog.DiscardHandler)
	}
	return &Runner{
		cfg:      cfg,
		sched:    sched,
		ha:       ha,
		receipts: receipts,
		log:      logger,
		sleep:    sleepContext,
	}
}

// Mode reports "execute" or "dry-run".
func (r *Runner) Mode() string {
	if r.cfg.Execute {
		return "execute"
	}
	return "dry-run"
}

// LogConfiguration writes the effective configuration. Tokens are reported as
// set/unset and never printed; URLs cannot contain credentials (config rejects
// them) so they are safe to log.
func (r *Runner) LogConfiguration() {
	ledgerTarget := fmt.Sprintf("memory (%s)", r.cfg.LedgerPath)
	if r.cfg.LedgerPath == "" {
		ledgerTarget = "memory only"
	}
	r.log.Info("ha-bridge configuration",
		"mode", r.Mode(),
		"consumer_id", r.cfg.ConsumerID,
		"queue", r.cfg.Queue,
		"scheduler_endpoint", r.cfg.SchedulerURL,
		"ha_url", r.cfg.HAURL,
		"batch_size", r.cfg.BatchSize,
		"lease_seconds", r.cfg.LeaseSeconds,
		"poll_interval", r.cfg.PollInterval.String(),
		"max_backoff", r.cfg.MaxBackoff.String(),
		"alarm_script", r.cfg.AlarmScript,
		"announce_script", r.cfg.AnnounceScript,
		"stop_script", r.cfg.StopScript,
		"ledger", ledgerTarget,
		"scheduler_token", presence(r.cfg.SchedulerToken),
		"ha_token", presence(r.cfg.HAToken),
	)
}

// Run either describes the configuration once (dry-run) or runs the executor
// loop until the context is cancelled.
func (r *Runner) Run(ctx context.Context) error {
	if !r.cfg.Execute {
		return r.Describe(ctx)
	}
	if r.ha == nil {
		return fmt.Errorf("execute mode requires a Home Assistant client")
	}
	if r.receipts == nil {
		return fmt.Errorf("execute mode requires a receipt ledger for active alarm tracking")
	}

	r.LogConfiguration()
	r.log.Info("executor starting", "queue", r.cfg.Queue)

	failures := 0
	for {
		if ctx.Err() != nil {
			r.log.Info("shutdown requested", "queue", r.cfg.Queue)
			return nil
		}

		err := r.cycle(ctx)
		if ctx.Err() != nil {
			r.log.Info("shutdown requested", "queue", r.cfg.Queue)
			return nil
		}

		wait := r.cfg.PollInterval
		if err != nil {
			failures++
			wait = r.backoff(failures)
			r.log.Warn("scheduler cycle failed; retrying after backoff",
				"error", err, "failures", failures, "retry_in", wait.String())
		} else {
			failures = 0
		}

		if err := r.sleep(ctx, wait); err != nil {
			r.log.Info("shutdown requested", "queue", r.cfg.Queue)
			return nil
		}
	}
}

// Describe is the safe dry-run: it reads scheduler status and, when an HA token
// is configured, probes Home Assistant. It never leases or acknowledges events
// and never executes a script.
func (r *Runner) Describe(ctx context.Context) error {
	r.LogConfiguration()
	r.log.Info("dry-run: no events will be leased, executed or acknowledged")

	status, err := r.sched.GetStatus(ctx)
	if err != nil {
		return fmt.Errorf("read scheduler status: %w", err)
	}
	r.log.Info("scheduler status",
		"version", status.Version,
		"schema_version", status.SchemaVersion,
		"pending_events", status.PendingEvents,
		"leased_events", status.LeasedEvents,
		"enabled_schedules", status.EnabledSchedules,
		"running_timers", status.RunningTimers,
		"scenario_execution_supported", status.ScenarioExecutionSupported,
	)
	for _, queue := range status.Queues {
		r.log.Info("scheduler queue",
			"name", queue.Name,
			"pending", queue.Pending,
			"leased", queue.Leased,
		)
	}
	for _, consumer := range status.Consumers {
		r.log.Info("scheduler consumer",
			"consumer_id", consumer.ConsumerID,
			"alive", consumer.Alive,
			"last_seen_unix", consumer.LastSeenUnix,
			"last_ack_unix", consumer.LastAckUnix,
		)
	}

	if r.ha == nil {
		r.log.Warn("no Home Assistant client configured; skipping reachability probe")
	} else if r.cfg.HAToken == "" {
		r.log.Warn("HA token not configured; skipping Home Assistant reachability probe")
	} else if err := r.ha.Ping(ctx); err != nil {
		return fmt.Errorf("home assistant reachability: %w", err)
	} else {
		r.log.Info("home assistant reachable",
			"alarm_script", r.cfg.AlarmScript,
			"announce_script", r.cfg.AnnounceScript,
			"stop_script", r.cfg.StopScript,
		)
	}

	r.log.Info("dry-run finished: no scheduler events were leased or acknowledged")
	return nil
}

// cycle leases one bounded batch and processes it sequentially.
func (r *Runner) cycle(ctx context.Context) error {
	events, err := r.sched.ListDueEvents(ctx, scheduler.ListRequest{
		ConsumerID:   r.cfg.ConsumerID,
		Queue:        r.cfg.Queue,
		LeaseSeconds: r.cfg.LeaseSeconds,
		Limit:        r.cfg.BatchSize,
	})
	if err != nil {
		return fmt.Errorf("list due events: %w", err)
	}
	if len(events) == 0 {
		return nil
	}

	ordered := orderEvents(events)
	r.log.Info("leased events", "count", len(ordered), "queue", r.cfg.Queue)
	for i := range ordered {
		if ctx.Err() != nil {
			// Do not acknowledge the remainder: the lease expires and the
			// next run (or another single consumer) sees them again.
			return ctx.Err()
		}
		r.handle(ctx, ordered[i])
	}
	return nil
}

func (r *Runner) handle(ctx context.Context, ev scheduler.Event) {
	log := r.log.With(
		"event_id", ev.ID,
		"source_id", ev.SourceID,
		"kind", string(ev.SourceKind),
		"action", string(ev.Action),
	)

	if ev.ID == "" {
		r.log.Error("scheduler returned an event without id; ignoring it")
		return
	}

	plan, err := r.plan(ev)
	if err != nil {
		log.Warn("event cannot be executed by the HA executor", "reason", err.Error())
		r.ack(ctx, log, ev, scheduler.AckFailed, err.Error())
		return
	}

	if r.receipts != nil && r.receipts.Seen(ev.ID) {
		log.Info("duplicate delivery suppressed by receipt ledger")
		r.ack(ctx, log, ev, scheduler.AckDelivered, "duplicate delivery; Home Assistant side effect already applied")
		return
	}

	if plan.skipReason != "" {
		log.Info("event acknowledged without an HA call", "reason", plan.skipReason)
		r.ack(ctx, log, ev, scheduler.AckSkipped, plan.skipReason)
		return
	}

	// A global alarm_stop may only silence the alarm this bridge started.
	// A stale STOP for an older occurrence, or a STOP that races a newer
	// alarm, must never touch the currently ringing alarm.
	var active ledger.ActiveAlarm
	var activeKnown bool
	if ev.Action == scheduler.ActionStop {
		if r.receipts != nil {
			active, activeKnown = r.receipts.ActiveAlarm()
		}
		if !activeKnown || !stopMatchesActive(ev, active) {
			reason := "stop ignored: source or occurrence does not match the active alarm started by this bridge"
			log.Info("event acknowledged without an HA call", "reason", reason)
			r.ack(ctx, log, ev, scheduler.AckSkipped, reason)
			return
		}
		// Pass the identity of the FIRE this bridge itself started. Home
		// Assistant writes that identity synchronously inside alarm_start
		// before the service call returns 2xx, and this consumer handles one
		// batch at a time, so a newer FIRE processed later always owns a
		// different stored id. The script fails closed when the stored id is
		// unknown while the alarm is still ringing.
		plan.variables["expected_source_id"] = active.SourceID
		plan.variables["expected_event_id"] = active.EventID
	}

	if err := r.ha.CallScript(ctx, plan.script, plan.variables); err != nil {
		if ctx.Err() != nil {
			return
		}
		status := httpapi.StatusCode(err)
		switch {
		case httpapi.IsPermanent(err) && status != 401 && status != 403:
			log.Error("home assistant rejected the call; failing the event", "error", err)
			r.ack(ctx, log, ev, scheduler.AckFailed, "home assistant rejected the call: "+sanitizeDetail(err.Error()))
		case ev.Attempts >= r.cfg.MaxAttempts:
			log.Error("home assistant call kept failing; failing the event after the attempt limit",
				"error", err, "attempts", ev.Attempts, "max_attempts", r.cfg.MaxAttempts)
			r.ack(ctx, log, ev, scheduler.AckFailed, "home assistant call failed repeatedly: "+sanitizeDetail(err.Error()))
		default:
			// Transient or auth/configuration error: leave the event
			// unacknowledged so lease expiry re-delivers it. Never ack
			// DELIVERED without a 2xx from Home Assistant.
			log.Warn("home assistant call failed; leaving event unacknowledged for retry",
				"error", err, "attempts", ev.Attempts)
		}
		return
	}

	r.recordReceipt(log, ev, plan.script)

	log.Info("home assistant accepted the call", "script", plan.script)
	r.ack(ctx, log, ev, scheduler.AckDelivered, "home assistant accepted "+plan.script)
}

// stopMatchesActive reports whether a STOP may silence the active alarm. Both
// the occurrence identity and the due ordering come from scheduler events, so
// a late STOP cannot outlive a snooze re-fire or a newer recurring occurrence.
// Bridge wall-clock time (SinceUnix) is never part of this comparison.
func stopMatchesActive(ev scheduler.Event, active ledger.ActiveAlarm) bool {
	if active.SourceID == "" || ev.SourceID != active.SourceID {
		return false
	}
	if active.OccurrenceUnix != 0 && ev.OccurrenceUnix != 0 && active.OccurrenceUnix != ev.OccurrenceUnix {
		return false
	}
	if active.FireDueUnix != 0 && ev.DueUnix != 0 && ev.DueUnix < active.FireDueUnix {
		return false
	}
	return true
}

// recordReceipt persists the dedupe entry together with the active-alarm marker
// in one atomic ledger write. A marker write failure is logged but does not
// retract the acknowledged HA side effect.
func (r *Runner) recordReceipt(log *slog.Logger, ev scheduler.Event, script string) {
	if r.receipts == nil {
		log.Warn("no receipt ledger; duplicate suppression and active alarm tracking are disabled")
		return
	}
	receipt := ledger.Receipt{
		EventID:       ev.ID,
		SourceID:      ev.SourceID,
		Action:        string(ev.Action),
		Script:        script,
		CompletedUnix: time.Now().Unix(),
	}
	switch {
	case ev.Action == scheduler.ActionFire && ev.SourceKind == scheduler.SourceKindAlarm:
		active := ledger.ActiveAlarm{
			SourceID:       ev.SourceID,
			EventID:        ev.ID,
			OccurrenceUnix: ev.OccurrenceUnix,
			FireDueUnix:    ev.DueUnix,
			SinceUnix:      receipt.CompletedUnix,
		}
		if err := r.receipts.RecordStart(receipt, active); err != nil {
			log.Error("active alarm marker write failed; a later STOP may be skipped after restart",
				"error", err)
		}
	case ev.Action == scheduler.ActionStop && ev.SourceKind == scheduler.SourceKindAlarm:
		cleared, err := r.receipts.RecordStop(receipt, ev.SourceID)
		if err != nil {
			log.Error("receipt ledger write failed; a re-delivery may duplicate the Home Assistant side effect",
				"error", err)
		}
		if !cleared {
			log.Warn("active alarm marker changed before STOP was recorded; a newer alarm was left untouched")
		}
	default:
		if err := r.receipts.Record(receipt); err != nil {
			log.Error("receipt ledger write failed; a re-delivery may duplicate the Home Assistant side effect",
				"error", err)
		}
	}
}

func (r *Runner) ack(ctx context.Context, log *slog.Logger, ev scheduler.Event, outcome, detail string) {
	if err := r.sched.AckEvent(ctx, ev.ID, r.cfg.ConsumerID, outcome, detail); err != nil {
		log.Warn("acknowledgement failed; the scheduler will re-deliver",
			"outcome", outcome, "error", err)
	}
}

// plannedAction is the result of mapping one event. skipReason means the event
// is acknowledged as SKIPPED without touching Home Assistant.
type plannedAction struct {
	script     string
	variables  map[string]any
	skipReason string
}

// plan maps an allowlisted (action, kind) pair to a configured script. It never
// derives a service or entity from the event payload.
func (r *Runner) plan(ev scheduler.Event) (plannedAction, error) {
	if ev.Queue != "" && ev.Queue != r.cfg.Queue {
		return plannedAction{}, fmt.Errorf("event queue %q does not match the configured queue %q", ev.Queue, r.cfg.Queue)
	}

	base := map[string]any{
		"event_id":  ev.ID,
		"source_id": ev.SourceID,
		"label":     ev.Label,
		"kind":      string(ev.SourceKind),
	}

	switch ev.Action {
	case scheduler.ActionStop:
		// The only sound this bridge can safely stop globally is the alarm it
		// started. One-shot timer/reminder announcements cannot be stopped
		// with alarm_stop without risking an unrelated alarm.
		switch ev.SourceKind {
		case scheduler.SourceKindAlarm:
			variables := cloneVariables(base)
			variables["reason"] = "scheduler_stop"
			return plannedAction{script: r.cfg.StopScript, variables: variables}, nil
		case scheduler.SourceKindTimer, scheduler.SourceKindReminder:
			return plannedAction{
				skipReason: "timer/reminder announcements are one-shot; no safe global HA stop exists",
			}, nil
		}

	case scheduler.ActionFire:
		switch ev.SourceKind {
		case scheduler.SourceKindAlarm:
			return plannedAction{script: r.cfg.AlarmScript, variables: base}, nil
		case scheduler.SourceKindTimer, scheduler.SourceKindReminder:
			variables := cloneVariables(base)
			variables["message"] = announceMessage(ev)
			variables["chime"] = true
			return plannedAction{script: r.cfg.AnnounceScript, variables: variables}, nil
		case scheduler.SourceKindTask:
			return plannedAction{}, fmt.Errorf("generic task events are not Home Assistant actions (task_type=%q); the HA executor must not lease the %q queue", ev.TaskType, config.QueueTasks)
		}
	}

	return plannedAction{}, fmt.Errorf("unsupported event kind=%q action=%q", string(ev.SourceKind), string(ev.Action))
}

func cloneVariables(src map[string]any) map[string]any {
	dst := make(map[string]any, len(src)+2)
	for key, value := range src {
		dst[key] = value
	}
	return dst
}

func announceMessage(ev scheduler.Event) string {
	if label := strings.TrimSpace(ev.Label); label != "" {
		return label
	}
	switch ev.SourceKind {
	case scheduler.SourceKindTimer:
		return "Timer"
	case scheduler.SourceKindReminder:
		return "Reminder"
	default:
		return "Alarm"
	}
}

// orderEvents groups a batch by source, then by due time, and processes STOP
// before FIRE when both are due at the same instant. Combined with sequential
// processing and a single consumer this removes the common same-source races;
// it cannot reorder events that arrive in different batches.
func orderEvents(events []scheduler.Event) []scheduler.Event {
	ordered := make([]scheduler.Event, len(events))
	copy(ordered, events)
	sort.SliceStable(ordered, func(i, j int) bool {
		a, b := ordered[i], ordered[j]
		if a.SourceID != b.SourceID {
			return a.SourceID < b.SourceID
		}
		if a.DueUnix != b.DueUnix {
			return a.DueUnix < b.DueUnix
		}
		if actionRank(a.Action) != actionRank(b.Action) {
			return actionRank(a.Action) < actionRank(b.Action)
		}
		return a.ID < b.ID
	})
	return ordered
}

func actionRank(action scheduler.Action) int {
	if action == scheduler.ActionStop {
		return 0
	}
	return 1
}

func (r *Runner) backoff(failures int) time.Duration {
	wait := r.cfg.PollInterval
	for i := 1; i < failures && wait < r.cfg.MaxBackoff; i++ {
		wait *= 2
	}
	if wait > r.cfg.MaxBackoff {
		wait = r.cfg.MaxBackoff
	}
	return wait
}

func sleepContext(ctx context.Context, d time.Duration) error {
	timer := time.NewTimer(d)
	defer timer.Stop()
	select {
	case <-ctx.Done():
		return ctx.Err()
	case <-timer.C:
		return nil
	}
}

func presence(value string) string {
	if value == "" {
		return "unset"
	}
	return "set"
}

func sanitizeDetail(text string) string {
	text = strings.Join(strings.Fields(text), " ")
	if len(text) > 300 {
		text = text[:300]
	}
	return text
}
