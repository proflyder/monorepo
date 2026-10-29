// Package control executes phone commands that arrive through the scheduler.
//
// The phone enqueues a generic task (task_type "home.control") on the control
// queue; this executor leases that queue, resolves the task's entry id and
// action through the catalog discovered from Home Assistant right now, calls
// Home Assistant and acknowledges. The payload is data only: it names a
// catalog entry, and an id that is not in the current catalog is rejected, so
// it can never reach an entity the discovery rules do not expose.
package control

import (
	"context"
	"encoding/json"
	"fmt"
	"log/slog"
	"strings"
	"time"

	"ricardo.local/monorepo/solutions/ha-bridge/internal/catalog"
	"ricardo.local/monorepo/solutions/ha-bridge/internal/discovery"
	"ricardo.local/monorepo/solutions/ha-bridge/internal/httpapi"
	"ricardo.local/monorepo/solutions/ha-bridge/internal/scheduler"
)

// DefaultQueue is the scheduler queue carrying control commands.
const DefaultQueue = "control"

// SchedulerAPI is the subset of SchedulerService the executor uses.
type SchedulerAPI interface {
	ListDueEvents(ctx context.Context, req scheduler.ListRequest) ([]scheduler.Event, error)
	AckEvent(ctx context.Context, eventID, consumerID, outcome, detail string) error
}

// Catalog yields the current discovered catalog. A command is honoured only if
// its entry is in this catalog at execution time.
type Catalog interface {
	Get(ctx context.Context) (discovery.Snapshot, error)
}

// HomeAssistant is the side-effect surface.
type HomeAssistant interface {
	CallService(ctx context.Context, domain, service, entity string) error
}

// Config is the executor configuration.
type Config struct {
	ConsumerID   string
	Queue        string
	BatchSize    int
	LeaseSeconds int
	PollInterval time.Duration
	MaxBackoff   time.Duration
	MaxAttempts  int
}

// Executor is the control loop.
type Executor struct {
	cfg     Config
	catalog Catalog
	sched   SchedulerAPI
	ha      HomeAssistant
	log     *slog.Logger
	sleep   func(ctx context.Context, d time.Duration) error
}

// New builds an executor.
func New(cfg Config, cat Catalog, sched SchedulerAPI, ha HomeAssistant, logger *slog.Logger) *Executor {
	if logger == nil {
		logger = slog.New(slog.DiscardHandler)
	}
	return &Executor{cfg: cfg, catalog: cat, sched: sched, ha: ha, log: logger, sleep: sleepContext}
}

// Run leases and executes until ctx is cancelled.
func (e *Executor) Run(ctx context.Context) error {
	e.log.Info("control executor starting", "queue", e.cfg.Queue, "consumer_id", e.cfg.ConsumerID)
	failures := 0
	for ctx.Err() == nil {
		wait := e.cfg.PollInterval
		if err := e.cycle(ctx); err != nil && ctx.Err() == nil {
			failures++
			wait = e.backoff(failures)
			e.log.Warn("scheduler cycle failed; retrying after backoff", "error", err, "retry_in", wait.String())
		} else {
			failures = 0
		}
		if err := e.sleep(ctx, wait); err != nil {
			break
		}
	}
	e.log.Info("control executor stopped")
	return nil
}

func (e *Executor) cycle(ctx context.Context) error {
	events, err := e.sched.ListDueEvents(ctx, scheduler.ListRequest{
		ConsumerID:   e.cfg.ConsumerID,
		Queue:        e.cfg.Queue,
		LeaseSeconds: e.cfg.LeaseSeconds,
		Limit:        e.cfg.BatchSize,
	})
	if err != nil {
		return fmt.Errorf("list due events: %w", err)
	}
	for _, ev := range events {
		if ctx.Err() != nil {
			return ctx.Err()
		}
		e.handle(ctx, ev)
	}
	return nil
}

type command struct {
	Entry  string `json:"entry"`
	Action string `json:"action"`
}

func (e *Executor) handle(ctx context.Context, ev scheduler.Event) {
	log := e.log.With("event_id", ev.ID)
	if ev.ID == "" {
		e.log.Error("scheduler returned an event without id; ignoring it")
		return
	}

	snap, err := e.catalog.Get(ctx)
	if err != nil {
		if ctx.Err() != nil {
			return
		}
		// Home Assistant is unreachable, so the command cannot run either:
		// leave it unacknowledged and let the lease expiry retry it.
		log.Warn("catalog unavailable; leaving event for retry", "attempts", ev.Attempts, "error", err)
		if ev.Attempts >= e.cfg.MaxAttempts {
			e.ack(ctx, log, ev, scheduler.AckFailed, "home assistant unreachable repeatedly")
		}
		return
	}
	call, entryID, err := e.plan(ev, snap.Catalog)
	if err != nil {
		log.Warn("control command rejected", "reason", err.Error())
		e.ack(ctx, log, ev, scheduler.AckFailed, err.Error())
		return
	}

	if err := e.ha.CallService(ctx, call.Domain, call.Service, call.Entity); err != nil {
		if ctx.Err() != nil {
			return
		}
		status := httpapi.StatusCode(err)
		switch {
		case httpapi.IsPermanent(err) && status != 401 && status != 403:
			log.Error("home assistant rejected the call", "entry", entryID, "error", err)
			e.ack(ctx, log, ev, scheduler.AckFailed, "home assistant rejected the call: "+detail(err.Error()))
		case ev.Attempts >= e.cfg.MaxAttempts:
			log.Error("home assistant call kept failing", "entry", entryID, "attempts", ev.Attempts, "error", err)
			e.ack(ctx, log, ev, scheduler.AckFailed, "home assistant call failed repeatedly: "+detail(err.Error()))
		default:
			// Transient or auth problem: leave unacknowledged so lease expiry retries.
			log.Warn("home assistant call failed; leaving event for retry", "entry", entryID, "attempts", ev.Attempts, "error", err)
		}
		return
	}
	log.Info("home assistant accepted the command", "entry", entryID, "service", call.Domain+"."+call.Service)
	e.ack(ctx, log, ev, scheduler.AckDelivered, "home assistant accepted "+entryID)
}

// plan validates the event and resolves it to a service call.
func (e *Executor) plan(ev scheduler.Event, cat *catalog.Catalog) (catalog.ServiceCall, string, error) {
	if ev.Queue != "" && ev.Queue != e.cfg.Queue {
		return catalog.ServiceCall{}, "", fmt.Errorf("event queue %q does not match %q", ev.Queue, e.cfg.Queue)
	}
	if ev.SourceKind != scheduler.SourceKindTask || ev.TaskType != catalog.TaskType {
		return catalog.ServiceCall{}, "", fmt.Errorf("unsupported event kind=%q task_type=%q", string(ev.SourceKind), ev.TaskType)
	}
	var cmd command
	if err := json.Unmarshal([]byte(ev.Payload), &cmd); err != nil {
		return catalog.ServiceCall{}, "", fmt.Errorf("invalid command payload")
	}
	entry, ok := cat.Lookup(cmd.Entry)
	if !ok {
		return catalog.ServiceCall{}, "", fmt.Errorf("entry %q is not in the catalog", detail(cmd.Entry))
	}
	call, err := entry.Resolve(cmd.Action)
	if err != nil {
		return catalog.ServiceCall{}, "", err
	}
	return call, entry.ID, nil
}

func (e *Executor) ack(ctx context.Context, log *slog.Logger, ev scheduler.Event, outcome, text string) {
	if err := e.sched.AckEvent(ctx, ev.ID, e.cfg.ConsumerID, outcome, text); err != nil {
		log.Warn("acknowledgement failed; the scheduler will re-deliver", "outcome", outcome, "error", err)
	}
}

func (e *Executor) backoff(failures int) time.Duration {
	wait := e.cfg.PollInterval
	for i := 1; i < failures && wait < e.cfg.MaxBackoff; i++ {
		wait *= 2
	}
	if wait > e.cfg.MaxBackoff {
		wait = e.cfg.MaxBackoff
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

func detail(text string) string {
	text = strings.Join(strings.Fields(text), " ")
	if len(text) > 200 {
		text = text[:200]
	}
	return text
}
