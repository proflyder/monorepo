package api

import (
	"context"
	"errors"

	"connectrpc.com/connect"

	ricardov1 "ricardo.local/monorepo/packages/proto/ricardo/v1"
)

// AlarmService preserves the existing ricardo.v1.AlarmService wire API on top
// of the same durable engine. It stores alarms as SchedulerService schedules
// with kind ALARM, so there is one owner of the data.
//
// Semantic caveat: Alarm.last_ring_unix means "last occurrence emitted", not
// "the device audibly rang". Audibility is proven only by an executor ack with
// outcome DELIVERED, and even that means the action was accepted.
type AlarmService struct {
	svc *Service
}

// NewAlarmService builds the legacy facade over svc.
func NewAlarmService(svc *Service) *AlarmService {
	return &AlarmService{svc: svc}
}

func legacyRepeat(r ricardov1.Repeat) (ricardov1.RepeatKind, error) {
	switch r {
	case ricardov1.Repeat_ONCE:
		return ricardov1.RepeatKind_REPEAT_KIND_ONCE, nil
	case ricardov1.Repeat_DAILY:
		return ricardov1.RepeatKind_REPEAT_KIND_DAILY, nil
	case ricardov1.Repeat_WEEKDAYS:
		return ricardov1.RepeatKind_REPEAT_KIND_WEEKDAYS, nil
	case ricardov1.Repeat_WEEKENDS:
		return ricardov1.RepeatKind_REPEAT_KIND_WEEKENDS, nil
	case ricardov1.Repeat_CUSTOM:
		return ricardov1.RepeatKind_REPEAT_KIND_CUSTOM, nil
	default:
		return ricardov1.RepeatKind_REPEAT_KIND_UNSPECIFIED, errors.New("unsupported repeat")
	}
}

func scheduleFromAlarm(a *ricardov1.Alarm) (*ricardov1.Schedule, error) {
	if a == nil {
		return nil, errors.New("alarm is required")
	}
	repeat, err := legacyRepeat(a.GetRepeat())
	if err != nil {
		return nil, err
	}
	return &ricardov1.Schedule{
		Id:       a.GetId(),
		Label:    a.GetLabel(),
		Kind:     ricardov1.ScheduleKind_SCHEDULE_KIND_ALARM,
		Timezone: a.GetTimezone(),
		At: &ricardov1.WallTime{
			Hour:   a.GetHour(),
			Minute: a.GetMinute(),
		},
		Repeat:   repeat,
		Weekdays: a.GetWeekdays(),
		Enabled:  a.GetEnabled(),
	}, nil
}

func alarmFromSchedule(s *ricardov1.Schedule) *ricardov1.Alarm {
	return &ricardov1.Alarm{
		Id:              s.GetId(),
		Label:           s.GetLabel(),
		Hour:            s.GetAt().GetHour(),
		Minute:          s.GetAt().GetMinute(),
		Timezone:        s.GetTimezone(),
		Repeat:          ricardov1.Repeat(s.GetRepeat()),
		Weekdays:        s.GetWeekdays(),
		Enabled:         s.GetEnabled(),
		NextRunUnix:     s.GetNextDueUnix(),
		SnoozeUntilUnix: s.GetSnoozeUntilUnix(),
		LastRingUnix:    s.GetLastFiredUnix(),
		Revision:        s.GetRevision(),
	}
}

func (a *AlarmService) getAlarmSchedule(ctx context.Context, id string) (*ricardov1.Schedule, error) {
	sch, err := a.svc.st.GetSchedule(ctx, id)
	if err != nil {
		return nil, a.svc.mapErr(err)
	}
	if sch.GetKind() != ricardov1.ScheduleKind_SCHEDULE_KIND_ALARM {
		return nil, connect.NewError(connect.CodeNotFound, errors.New("alarm not found"))
	}
	return sch, nil
}

// CreateAlarm creates an alarm schedule.
func (a *AlarmService) CreateAlarm(ctx context.Context, req *connect.Request[ricardov1.CreateAlarmRequest]) (*connect.Response[ricardov1.CreateAlarmResponse], error) {
	repeat, err := legacyRepeat(req.Msg.GetRepeat())
	if err != nil {
		return nil, connect.NewError(connect.CodeInvalidArgument, err)
	}
	sch, err := a.svc.st.CreateSchedule(ctx, &ricardov1.Schedule{
		Label:    req.Msg.GetLabel(),
		Kind:     ricardov1.ScheduleKind_SCHEDULE_KIND_ALARM,
		Timezone: req.Msg.GetTimezone(),
		At:       &ricardov1.WallTime{Hour: req.Msg.GetHour(), Minute: req.Msg.GetMinute()},
		Repeat:   repeat,
		Weekdays: req.Msg.GetWeekdays(),
		Enabled:  req.Msg.GetEnabled(),
	}, "", a.svc.nowTime())
	if err != nil {
		return nil, a.svc.mapErr(err)
	}
	return connect.NewResponse(&ricardov1.CreateAlarmResponse{Alarm: alarmFromSchedule(sch)}), nil
}

// GetAlarm returns an alarm.
func (a *AlarmService) GetAlarm(ctx context.Context, req *connect.Request[ricardov1.GetAlarmRequest]) (*connect.Response[ricardov1.GetAlarmResponse], error) {
	sch, err := a.getAlarmSchedule(ctx, req.Msg.GetId())
	if err != nil {
		return nil, err
	}
	return connect.NewResponse(&ricardov1.GetAlarmResponse{Alarm: alarmFromSchedule(sch)}), nil
}

// ListAlarms lists alarm schedules.
func (a *AlarmService) ListAlarms(ctx context.Context, req *connect.Request[ricardov1.ListAlarmsRequest]) (*connect.Response[ricardov1.ListAlarmsResponse], error) {
	items, err := a.svc.st.ListSchedules(ctx, req.Msg.GetIncludeDisabled(), ricardov1.ScheduleKind_SCHEDULE_KIND_ALARM)
	if err != nil {
		return nil, a.svc.mapErr(err)
	}
	alarms := make([]*ricardov1.Alarm, 0, len(items))
	for _, sch := range items {
		alarms = append(alarms, alarmFromSchedule(sch))
	}
	return connect.NewResponse(&ricardov1.ListAlarmsResponse{Alarms: alarms}), nil
}

// UpdateAlarm updates an alarm, using Alarm.revision for conflict detection.
func (a *AlarmService) UpdateAlarm(ctx context.Context, req *connect.Request[ricardov1.UpdateAlarmRequest]) (*connect.Response[ricardov1.UpdateAlarmResponse], error) {
	alarm := req.Msg.GetAlarm()
	if alarm.GetId() == "" {
		return nil, connect.NewError(connect.CodeInvalidArgument, errors.New("alarm.id is required"))
	}
	if _, err := a.getAlarmSchedule(ctx, alarm.GetId()); err != nil {
		return nil, err
	}
	sch, err := scheduleFromAlarm(alarm)
	if err != nil {
		return nil, connect.NewError(connect.CodeInvalidArgument, err)
	}
	updated, err := a.svc.st.UpdateSchedule(ctx, sch, alarm.GetRevision(), a.svc.nowTime())
	if err != nil {
		return nil, a.svc.mapErr(err)
	}
	return connect.NewResponse(&ricardov1.UpdateAlarmResponse{Alarm: alarmFromSchedule(updated)}), nil
}

// DeleteAlarm deletes an alarm.
func (a *AlarmService) DeleteAlarm(ctx context.Context, req *connect.Request[ricardov1.DeleteAlarmRequest]) (*connect.Response[ricardov1.DeleteAlarmResponse], error) {
	sch, err := a.getAlarmSchedule(ctx, req.Msg.GetId())
	if err != nil {
		return nil, err
	}
	deleted, err := a.svc.st.DeleteSchedule(ctx, sch.GetId(), a.svc.nowTime())
	if err != nil {
		return nil, a.svc.mapErr(err)
	}
	return connect.NewResponse(&ricardov1.DeleteAlarmResponse{Deleted: deleted}), nil
}

// SetAlarmEnabled toggles an alarm.
func (a *AlarmService) SetAlarmEnabled(ctx context.Context, req *connect.Request[ricardov1.SetAlarmEnabledRequest]) (*connect.Response[ricardov1.SetAlarmEnabledResponse], error) {
	sch, err := a.getAlarmSchedule(ctx, req.Msg.GetId())
	if err != nil {
		return nil, err
	}
	updated, err := a.svc.st.SetScheduleEnabled(ctx, sch.GetId(), req.Msg.GetEnabled(), a.svc.nowTime())
	if err != nil {
		return nil, a.svc.mapErr(err)
	}
	return connect.NewResponse(&ricardov1.SetAlarmEnabledResponse{Alarm: alarmFromSchedule(updated)}), nil
}

// SnoozeAlarm snoozes a ringing alarm.
func (a *AlarmService) SnoozeAlarm(ctx context.Context, req *connect.Request[ricardov1.SnoozeAlarmRequest]) (*connect.Response[ricardov1.SnoozeAlarmResponse], error) {
	sch, err := a.getAlarmSchedule(ctx, req.Msg.GetId())
	if err != nil {
		return nil, err
	}
	updated, _, err := a.svc.st.SnoozeSchedule(ctx, sch.GetId(), req.Msg.GetMinutes(), a.svc.nowTime())
	if err != nil {
		return nil, a.svc.mapErr(err)
	}
	return connect.NewResponse(&ricardov1.SnoozeAlarmResponse{Alarm: alarmFromSchedule(updated)}), nil
}

// StopAlarm stops a ringing alarm.
func (a *AlarmService) StopAlarm(ctx context.Context, req *connect.Request[ricardov1.StopAlarmRequest]) (*connect.Response[ricardov1.StopAlarmResponse], error) {
	sch, err := a.getAlarmSchedule(ctx, req.Msg.GetId())
	if err != nil {
		return nil, err
	}
	updated, err := a.svc.st.StopSchedule(ctx, sch.GetId(), a.svc.nowTime())
	if err != nil {
		return nil, a.svc.mapErr(err)
	}
	return connect.NewResponse(&ricardov1.StopAlarmResponse{Alarm: alarmFromSchedule(updated)}), nil
}
