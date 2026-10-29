// Package scheduler is a minimal Connect-JSON client for the canonical
// ricardo.v1.SchedulerService contract in
// packages/proto/ricardo/v1/scheduler.proto.
//
// Only the executor surface is implemented: ListDueEvents, AckEvent and
// GetStatus. The bridge never creates, snoozes or stops schedules itself; the
// scheduler owns all schedule state.
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
	// SourceKindTask is a generic task event. It is not an HA action: the
	// bridge must never lease the generic task queue and must fail such an
	// event safely if one is ever delivered.
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

// Event is one delivered DueEvent. Only the fields the executor needs are
// decoded; payload_json is intentionally ignored: no executor reads it today,
// and an event payload must never select an HA service or entity.
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
	LeaseUntilUnix int64
	Action         Action
	Detail         string
}

// ListRequest mirrors ListDueEventsRequest. Queue must always be set: an empty
// queue means "all queues" on the server, and the HA executor may only lease
// its own queue.
type ListRequest struct {
	ConsumerID   string
	Queue        string
	LeaseSeconds int
	Limit        int
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
