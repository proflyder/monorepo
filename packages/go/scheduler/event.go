// Package scheduler is a minimal Connect-JSON client for the canonical
// ricardo.v1.SchedulerService contract in
// packages/proto/ricardo/v1/scheduler.proto.
//
// It covers the durable event surface: ListDueEvents, AckEvent, EnqueueTask and
// GetStatus. It never owns schedule state, never mutates a due event itself and
// never interprets a payload; the scheduler is the only authority for
// schedules, timers and queues.
//
// The package is project-agnostic: it must not embed endpoints, queue names,
// task types, entity ids or deployment detail. Callers pass those in.
package scheduler

// ServicePath is the Connect route prefix for SchedulerService.
const ServicePath = "/ricardo.v1.SchedulerService/"

// SourceKind is a normalized DueEvent.source_kind value.
type SourceKind string

const (
	SourceKindUnspecified SourceKind = ""
	SourceKindAlarm       SourceKind = "alarm"
	SourceKindReminder    SourceKind = "reminder"
	SourceKindTimer       SourceKind = "timer"
	SourceKindScenario    SourceKind = "scenario"
	// SourceKindTask is a generic task event produced by EnqueueTask. It is
	// not a domain occurrence: an executor that leases a domain queue must
	// never lease the generic task queue, and must fail such an event safely
	// if one is ever delivered to it.
	SourceKindTask SourceKind = "task"
)

// Action is a normalized DueEvent.action value.
type Action string

const (
	// ActionFire starts an occurrence. A legacy unspecified value also maps
	// to FIRE, matching the proto contract.
	ActionFire Action = "FIRE"
	// ActionStop cancels sound for an occurrence. It must not snooze.
	ActionStop Action = "STOP"
)

// AckOutcome values accepted by AckEvent.
const (
	AckDelivered = "ACK_OUTCOME_DELIVERED"
	AckFailed    = "ACK_OUTCOME_FAILED"
	AckSkipped   = "ACK_OUTCOME_SKIPPED"
)

// Event is one delivered DueEvent. Only the fields an executor or producer
// needs are decoded. Payload is opaque data: callers must treat it as data
// only, never as a service call or entity identifier.
type Event struct {
	ID             string
	SourceID       string
	SourceKind     SourceKind
	TaskType       string
	Queue          string
	DueUnix        int64
	OccurrenceUnix int64
	Label          string
	State          string
	Attempts       int
	SnoozeSeq      int
	// CreatedUnix is when the scheduler durably queued the event.
	CreatedUnix    int64
	LeaseUntilUnix int64
	Action         Action
	Detail         string
	// Payload is the opaque payload_json (max 16 KiB). Treat it as data only.
	Payload string
}

// ListRequest mirrors ListDueEventsRequest. Queue must always be set: an empty
// queue means "all queues" on the server, and an executor may only lease its
// own queue.
type ListRequest struct {
	ConsumerID   string
	Queue        string
	LeaseSeconds int
	Limit        int
}

// EnqueueRequest mirrors EnqueueTaskRequest. TaskType and PayloadJSON are sent
// as-is; an empty Queue keeps the server default ("tasks") and RunAtUnix 0
// means "now". IdempotencyKey is optional: replaying the same key with an
// identical request returns the original event.
type EnqueueRequest struct {
	TaskType       string
	PayloadJSON    string
	RunAtUnix      int64
	Queue          string
	IdempotencyKey string
}

// QueueInfo mirrors QueueInfo in GetStatusResponse.
type QueueInfo struct {
	Name    string
	Pending int32
	Leased  int32
}

// ConsumerStatus mirrors ConsumerStatus.
type ConsumerStatus struct {
	ConsumerID   string
	LastSeenUnix int64
	LastAckUnix  int64
	Alive        bool
}

// Status mirrors GetStatusResponse. It is the only scheduler call made in
// dry-run mode, and it never leases an event.
type Status struct {
	Version                    string
	SchemaVersion              int32
	NowUnix                    int64
	LastTickUnix               int64
	PendingEvents              int32
	LeasedEvents               int32
	EnabledSchedules           int32
	RunningTimers              int32
	ScenarioExecutionSupported bool
	// Ready mirrors GetStatusResponse.ready: the last tick succeeded recently.
	Ready     bool
	Queues    []QueueInfo
	Consumers []ConsumerStatus
}
