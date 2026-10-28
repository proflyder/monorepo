// Package api exposes the scheduler contract over Connect (JSON/HTTP), gRPC
// and gRPC-Web. It contains no persistence logic and never plays audio.
package api

import (
	"context"
	"errors"
	"log/slog"
	"strings"
	"time"

	"connectrpc.com/connect"

	ricardov1 "ricardo.local/monorepo/packages/proto/ricardo/v1"
	"ricardo.local/monorepo/solutions/scheduler/internal/engine"
	"ricardo.local/monorepo/solutions/scheduler/internal/store"
)

// Service implements ricardov1connect.SchedulerServiceHandler.
type Service struct {
	st      *store.Store
	now     func() time.Time
	version string
	status  func() engine.Status
	allowed map[string]struct{}
	log     *slog.Logger
}

// Options wires a Service.
type Options struct {
	Store            *store.Store
	Now              func() time.Time
	Version          string
	Status           func() engine.Status
	AllowedConsumers []string
	Logger           *slog.Logger
}

// NewService builds the scheduler API.
func NewService(opts Options) *Service {
	now := opts.Now
	if now == nil {
		now = time.Now
	}
	logger := opts.Logger
	if logger == nil {
		logger = slog.Default()
	}
	allowed := make(map[string]struct{}, len(opts.AllowedConsumers))
	for _, c := range opts.AllowedConsumers {
		c = strings.TrimSpace(c)
		if c != "" {
			allowed[c] = struct{}{}
		}
	}
	status := opts.Status
	if status == nil {
		status = func() engine.Status { return engine.Status{} }
	}
	return &Service{
		st:      opts.Store,
		now:     now,
		version: opts.Version,
		status:  status,
		allowed: allowed,
		log:     logger,
	}
}

func (s *Service) nowTime() time.Time { return s.now().UTC() }

func (s *Service) checkConsumer(consumerID string) error {
	if len(s.allowed) == 0 {
		return nil
	}
	if _, ok := s.allowed[consumerID]; !ok {
		return store.ErrConsumerDenied
	}
	return nil
}

// mapErr converts domain errors to transport status without leaking internals.
func (s *Service) mapErr(err error) error {
	if err == nil {
		return nil
	}
	var connectErr *connect.Error
	if errors.As(err, &connectErr) {
		return err
	}
	var code connect.Code
	switch {
	case errors.Is(err, store.ErrNotFound):
		code = connect.CodeNotFound
	case errors.Is(err, store.ErrInvalid):
		code = connect.CodeInvalidArgument
	case errors.Is(err, store.ErrConflict):
		code = connect.CodeAborted
	case errors.Is(err, store.ErrIdempotency):
		code = connect.CodeAlreadyExists
	case errors.Is(err, store.ErrPrecondition):
		code = connect.CodeFailedPrecondition
	case errors.Is(err, store.ErrLimitExceeded):
		code = connect.CodeResourceExhausted
	case errors.Is(err, store.ErrConsumerDenied):
		code = connect.CodePermissionDenied
	default:
		s.log.Error("scheduler request failed", "error", err)
		return connect.NewError(connect.CodeInternal, errors.New("internal error"))
	}
	return connect.NewError(code, err)
}

func scheduleFromCreate(req *ricardov1.CreateScheduleRequest) *ricardov1.Schedule {
	return &ricardov1.Schedule{
		Label:       req.GetLabel(),
		Kind:        req.GetKind(),
		Timezone:    req.GetTimezone(),
		At:          req.GetAt(),
		OnceDate:    req.GetOnceDate(),
		Repeat:      req.GetRepeat(),
		Weekdays:    req.GetWeekdays(),
		Enabled:     req.GetEnabled(),
		PayloadJson: req.GetPayloadJson(),
	}
}

// CreateSchedule creates an alarm, reminder, or reserved scenario schedule.
func (s *Service) CreateSchedule(ctx context.Context, req *connect.Request[ricardov1.CreateScheduleRequest]) (*connect.Response[ricardov1.CreateScheduleResponse], error) {
	sch, err := s.st.CreateSchedule(ctx, scheduleFromCreate(req.Msg), req.Msg.GetIdempotencyKey(), s.nowTime())
	if err != nil {
		return nil, s.mapErr(err)
	}
	return connect.NewResponse(&ricardov1.CreateScheduleResponse{Schedule: sch}), nil
}

// GetSchedule returns one live schedule.
func (s *Service) GetSchedule(ctx context.Context, req *connect.Request[ricardov1.GetScheduleRequest]) (*connect.Response[ricardov1.GetScheduleResponse], error) {
	sch, err := s.st.GetSchedule(ctx, req.Msg.GetId())
	if err != nil {
		return nil, s.mapErr(err)
	}
	return connect.NewResponse(&ricardov1.GetScheduleResponse{Schedule: sch}), nil
}

// ListSchedules lists live schedules, optionally including disabled ones.
func (s *Service) ListSchedules(ctx context.Context, req *connect.Request[ricardov1.ListSchedulesRequest]) (*connect.Response[ricardov1.ListSchedulesResponse], error) {
	items, err := s.st.ListSchedules(ctx, req.Msg.GetIncludeDisabled(), req.Msg.GetKind())
	if err != nil {
		return nil, s.mapErr(err)
	}
	return connect.NewResponse(&ricardov1.ListSchedulesResponse{Schedules: items}), nil
}

// UpdateSchedule replaces a schedule rule with optimistic concurrency.
func (s *Service) UpdateSchedule(ctx context.Context, req *connect.Request[ricardov1.UpdateScheduleRequest]) (*connect.Response[ricardov1.UpdateScheduleResponse], error) {
	if req.Msg.GetSchedule() == nil {
		return nil, connect.NewError(connect.CodeInvalidArgument, errors.New("schedule is required"))
	}
	if req.Msg.GetExpectedRevision() < 0 {
		return nil, connect.NewError(connect.CodeInvalidArgument, errors.New("expected_revision must not be negative"))
	}
	sch, err := s.st.UpdateSchedule(ctx, req.Msg.GetSchedule(), req.Msg.GetExpectedRevision(), s.nowTime())
	if err != nil {
		return nil, s.mapErr(err)
	}
	return connect.NewResponse(&ricardov1.UpdateScheduleResponse{Schedule: sch}), nil
}

// DeleteSchedule soft-deletes a schedule and queues STOP when it is ringing.
func (s *Service) DeleteSchedule(ctx context.Context, req *connect.Request[ricardov1.DeleteScheduleRequest]) (*connect.Response[ricardov1.DeleteScheduleResponse], error) {
	deleted, err := s.st.DeleteSchedule(ctx, req.Msg.GetId(), s.nowTime())
	if err != nil {
		return nil, s.mapErr(err)
	}
	return connect.NewResponse(&ricardov1.DeleteScheduleResponse{Deleted: deleted}), nil
}

// SetScheduleEnabled toggles a schedule.
func (s *Service) SetScheduleEnabled(ctx context.Context, req *connect.Request[ricardov1.SetScheduleEnabledRequest]) (*connect.Response[ricardov1.SetScheduleEnabledResponse], error) {
	sch, err := s.st.SetScheduleEnabled(ctx, req.Msg.GetId(), req.Msg.GetEnabled(), s.nowTime())
	if err != nil {
		return nil, s.mapErr(err)
	}
	return connect.NewResponse(&ricardov1.SetScheduleEnabledResponse{Schedule: sch}), nil
}

// SnoozeSchedule queues STOP plus a delayed occurrence.
func (s *Service) SnoozeSchedule(ctx context.Context, req *connect.Request[ricardov1.SnoozeScheduleRequest]) (*connect.Response[ricardov1.SnoozeScheduleResponse], error) {
	sch, ev, err := s.st.SnoozeSchedule(ctx, req.Msg.GetId(), req.Msg.GetMinutes(), s.nowTime())
	if err != nil {
		return nil, s.mapErr(err)
	}
	return connect.NewResponse(&ricardov1.SnoozeScheduleResponse{Schedule: sch, Event: ev}), nil
}

// StopSchedule queues an immediate STOP control event.
func (s *Service) StopSchedule(ctx context.Context, req *connect.Request[ricardov1.StopScheduleRequest]) (*connect.Response[ricardov1.StopScheduleResponse], error) {
	sch, err := s.st.StopSchedule(ctx, req.Msg.GetId(), s.nowTime())
	if err != nil {
		return nil, s.mapErr(err)
	}
	return connect.NewResponse(&ricardov1.StopScheduleResponse{Schedule: sch}), nil
}

// StartTimer starts a durable countdown.
func (s *Service) StartTimer(ctx context.Context, req *connect.Request[ricardov1.StartTimerRequest]) (*connect.Response[ricardov1.StartTimerResponse], error) {
	tm, err := s.st.StartTimer(ctx, req.Msg.GetLabel(), req.Msg.GetDurationSeconds(), req.Msg.GetIdempotencyKey(), s.nowTime())
	if err != nil {
		return nil, s.mapErr(err)
	}
	return connect.NewResponse(&ricardov1.StartTimerResponse{Timer: tm}), nil
}

// GetTimer returns a timer.
func (s *Service) GetTimer(ctx context.Context, req *connect.Request[ricardov1.GetTimerRequest]) (*connect.Response[ricardov1.GetTimerResponse], error) {
	tm, err := s.st.GetTimer(ctx, req.Msg.GetId())
	if err != nil {
		return nil, s.mapErr(err)
	}
	return connect.NewResponse(&ricardov1.GetTimerResponse{Timer: tm}), nil
}

// ListTimers lists timers.
func (s *Service) ListTimers(ctx context.Context, req *connect.Request[ricardov1.ListTimersRequest]) (*connect.Response[ricardov1.ListTimersResponse], error) {
	items, err := s.st.ListTimers(ctx, req.Msg.GetIncludeFinished())
	if err != nil {
		return nil, s.mapErr(err)
	}
	return connect.NewResponse(&ricardov1.ListTimersResponse{Timers: items}), nil
}

// PauseTimer pauses a running timer.
func (s *Service) PauseTimer(ctx context.Context, req *connect.Request[ricardov1.PauseTimerRequest]) (*connect.Response[ricardov1.PauseTimerResponse], error) {
	tm, err := s.st.PauseTimer(ctx, req.Msg.GetId(), s.nowTime())
	if err != nil {
		return nil, s.mapErr(err)
	}
	return connect.NewResponse(&ricardov1.PauseTimerResponse{Timer: tm}), nil
}

// ResumeTimer resumes a paused timer.
func (s *Service) ResumeTimer(ctx context.Context, req *connect.Request[ricardov1.ResumeTimerRequest]) (*connect.Response[ricardov1.ResumeTimerResponse], error) {
	tm, err := s.st.ResumeTimer(ctx, req.Msg.GetId(), s.nowTime())
	if err != nil {
		return nil, s.mapErr(err)
	}
	return connect.NewResponse(&ricardov1.ResumeTimerResponse{Timer: tm}), nil
}

// CancelTimer cancels a timer or sends STOP for a fired timer.
func (s *Service) CancelTimer(ctx context.Context, req *connect.Request[ricardov1.CancelTimerRequest]) (*connect.Response[ricardov1.CancelTimerResponse], error) {
	tm, err := s.st.CancelTimer(ctx, req.Msg.GetId(), s.nowTime())
	if err != nil {
		return nil, s.mapErr(err)
	}
	return connect.NewResponse(&ricardov1.CancelTimerResponse{Timer: tm}), nil
}

// ListDueEvents leases due events for one consumer and optional queue.
func (s *Service) ListDueEvents(ctx context.Context, req *connect.Request[ricardov1.ListDueEventsRequest]) (*connect.Response[ricardov1.ListDueEventsResponse], error) {
	if err := s.checkConsumer(req.Msg.GetConsumerId()); err != nil {
		return nil, s.mapErr(err)
	}
	events, err := s.st.ListDueEvents(ctx, req.Msg.GetConsumerId(), req.Msg.GetLeaseSeconds(), req.Msg.GetLimit(), req.Msg.GetQueue(), s.nowTime())
	if err != nil {
		return nil, s.mapErr(err)
	}
	return connect.NewResponse(&ricardov1.ListDueEventsResponse{Events: events}), nil
}

// AckEvent acknowledges a leased event.
func (s *Service) AckEvent(ctx context.Context, req *connect.Request[ricardov1.AckEventRequest]) (*connect.Response[ricardov1.AckEventResponse], error) {
	if err := s.checkConsumer(req.Msg.GetConsumerId()); err != nil {
		return nil, s.mapErr(err)
	}
	ev, err := s.st.AckEvent(ctx, req.Msg.GetEventId(), req.Msg.GetConsumerId(), req.Msg.GetOutcome(), req.Msg.GetDetail(), s.nowTime())
	if err != nil {
		return nil, s.mapErr(err)
	}
	return connect.NewResponse(&ricardov1.AckEventResponse{Event: ev}), nil
}

// EnqueueTask submits a generic durable task event.
func (s *Service) EnqueueTask(ctx context.Context, req *connect.Request[ricardov1.EnqueueTaskRequest]) (*connect.Response[ricardov1.EnqueueTaskResponse], error) {
	ev, err := s.st.EnqueueTask(ctx, req.Msg.GetTaskType(), req.Msg.GetPayloadJson(), req.Msg.GetQueue(), req.Msg.GetRunAtUnix(), req.Msg.GetIdempotencyKey(), s.nowTime())
	if err != nil {
		return nil, s.mapErr(err)
	}
	return connect.NewResponse(&ricardov1.EnqueueTaskResponse{Event: ev}), nil
}

// CancelTask cancels a pending task event.
func (s *Service) CancelTask(ctx context.Context, req *connect.Request[ricardov1.CancelTaskRequest]) (*connect.Response[ricardov1.CancelTaskResponse], error) {
	ev, err := s.st.CancelTask(ctx, req.Msg.GetEventId(), s.nowTime())
	if err != nil {
		return nil, s.mapErr(err)
	}
	return connect.NewResponse(&ricardov1.CancelTaskResponse{Event: ev}), nil
}

// GetStatus reports the server clock, backlog, consumer liveness and tick
// readiness. It never claims audible sound.
func (s *Service) GetStatus(ctx context.Context, _ *connect.Request[ricardov1.GetStatusRequest]) (*connect.Response[ricardov1.GetStatusResponse], error) {
	now := s.nowTime()
	st, err := s.st.Status(ctx, now)
	if err != nil {
		return nil, s.mapErr(err)
	}
	es := s.status()
	resp := &ricardov1.GetStatusResponse{
		Version:                    s.version,
		SchemaVersion:              int32(st.SchemaVersion),
		NowUnix:                    now.Unix(),
		PendingEvents:              st.PendingEvents,
		LeasedEvents:               st.LeasedEvents,
		EnabledSchedules:           st.EnabledSchedules,
		RunningTimers:              st.RunningTimers,
		Consumers:                  st.Consumers,
		Queues:                     st.Queues,
		ScenarioExecutionSupported: false,
		Ready:                      es.Ready,
	}
	if !es.LastTick.IsZero() {
		resp.LastTickUnix = es.LastTick.Unix()
	}
	if !es.LastAttempt.IsZero() {
		resp.LastTickAttemptUnix = es.LastAttempt.Unix()
	}
	if !es.LastErrorAt.IsZero() {
		resp.LastTickErrorUnix = es.LastErrorAt.Unix()
		resp.LastTickError = es.LastError
	}
	return connect.NewResponse(resp), nil
}
