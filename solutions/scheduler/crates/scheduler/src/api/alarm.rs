use connectrpc::{ConnectError, RequestContext, Response, ServiceRequest, ServiceResult};
use home_proto::v1 as proto;
use proto::AlarmService;

use crate::domain::{Repeat, Schedule, ScheduleKind, SyncChange, SyncOp};

use super::convert::{alarm_to_schedule, schedule_to_alarm};
use super::service::Service;
use super::zone::ClientZone;

#[derive(Debug)]
pub struct AlarmFacade {
    inner: Service,
}

impl AlarmFacade {
    #[must_use]
    pub const fn new(inner: Service) -> Self {
        Self { inner }
    }

    async fn get_alarm_schedule(&self, id: &str) -> Result<Schedule, ConnectError> {
        let id = id.to_owned();
        let schedule = self
            .inner
            .store_call(move |store| store.get_schedule(&id))
            .await?;
        if schedule.kind != ScheduleKind::Alarm {
            return Err(ConnectError::not_found("alarm not found").with_detail(
                home_proto::error_detail(&home_proto::localized("scheduler.error.alarm_not_found")),
            ));
        }
        Ok(schedule)
    }
}

fn legacy_repeat(code: i32) -> Result<Repeat, ConnectError> {
    match code {
        1 => Ok(Repeat::Once),
        2 => Ok(Repeat::Daily),
        3 => Ok(Repeat::Weekdays),
        4 => Ok(Repeat::Weekends),
        5 => Ok(Repeat::Custom),
        _ => Err(super::text::connect(
            ConnectError::invalid_argument("unsupported repeat"),
            "scheduler.error.unsupported_repeat",
        )),
    }
}

fn rule_error(error: &crate::domain::DomainError) -> ConnectError {
    super::text::domain_error(error)
}

fn repeat_to_proto(repeat: Repeat) -> proto::Repeat {
    match repeat {
        Repeat::Once => proto::Repeat::ONCE,
        Repeat::Daily => proto::Repeat::DAILY,
        Repeat::Weekdays => proto::Repeat::WEEKDAYS,
        Repeat::Weekends => proto::Repeat::WEEKENDS,
        Repeat::Custom => proto::Repeat::CUSTOM,
        Repeat::Unspecified => proto::Repeat::REPEAT_UNSPECIFIED,
    }
}

fn response<T>(message: T) -> ServiceResult<T> {
    Response::ok(message)
}

impl AlarmService for AlarmFacade {
    async fn create_alarm<'a>(
        &'a self,
        ctx: RequestContext,
        request: ServiceRequest<'_, proto::CreateAlarmRequest>,
    ) -> ServiceResult<impl connectrpc::Encodable<proto::CreateAlarmResponse> + Send + use<'a>>
    {
        let zone = ClientZone::from_request(&ctx)?;
        let repeat = legacy_repeat(request.repeat.to_i32())?;
        let view = proto::Alarm {
            label: request.label.to_owned(),
            hour: request.hour,
            minute: request.minute,
            repeat: repeat_to_proto(repeat).into(),
            weekdays: request.weekdays.iter().copied().collect(),
            enabled: request.enabled,
            once_date: request.once_date.to_owned(),
            ..proto::Alarm::default()
        };
        let schedule = alarm_to_schedule(&view, &zone).map_err(|error| rule_error(&error))?;
        let now = self.inner.now_unix();
        let created = self
            .inner
            .store_call(move |store| store.create_schedule(&schedule, "", now))
            .await?;
        self.inner.announce(
            &ctx,
            SyncChange::schedule(created.kind, SyncOp::Upsert, &created.id),
        );
        response(proto::CreateAlarmResponse {
            alarm: schedule_to_alarm(&created, &zone).into(),
            ..proto::CreateAlarmResponse::default()
        })
    }

    async fn get_alarm<'a>(
        &'a self,
        ctx: RequestContext,
        request: ServiceRequest<'_, proto::GetAlarmRequest>,
    ) -> ServiceResult<impl connectrpc::Encodable<proto::GetAlarmResponse> + Send + use<'a>> {
        let zone = ClientZone::from_request(&ctx)?;
        let schedule = self.get_alarm_schedule(request.id).await?;
        response(proto::GetAlarmResponse {
            alarm: schedule_to_alarm(&schedule, &zone).into(),
            ..proto::GetAlarmResponse::default()
        })
    }

    async fn list_alarms<'a>(
        &'a self,
        ctx: RequestContext,
        request: ServiceRequest<'_, proto::ListAlarmsRequest>,
    ) -> ServiceResult<impl connectrpc::Encodable<proto::ListAlarmsResponse> + Send + use<'a>> {
        let zone = ClientZone::from_request(&ctx)?;
        let include_disabled = request.include_disabled;
        let schedules = self
            .inner
            .store_call(move |store| store.list_schedules(include_disabled, ScheduleKind::Alarm))
            .await?;
        response(proto::ListAlarmsResponse {
            alarms: schedules
                .iter()
                .map(|schedule| schedule_to_alarm(schedule, &zone))
                .collect(),
            ..proto::ListAlarmsResponse::default()
        })
    }

    async fn update_alarm<'a>(
        &'a self,
        ctx: RequestContext,
        request: ServiceRequest<'_, proto::UpdateAlarmRequest>,
    ) -> ServiceResult<impl connectrpc::Encodable<proto::UpdateAlarmResponse> + Send + use<'a>>
    {
        let zone = ClientZone::from_request(&ctx)?;
        let message = request.to_owned_message();
        let Some(alarm) = message.alarm.as_option() else {
            return Err(super::text::connect(
                ConnectError::invalid_argument("alarm.id is required"),
                "scheduler.error.alarm_id_required",
            ));
        };
        if alarm.id.is_empty() {
            return Err(super::text::connect(
                ConnectError::invalid_argument("alarm.id is required"),
                "scheduler.error.alarm_id_required",
            ));
        }
        self.get_alarm_schedule(&alarm.id).await?;
        let revision = alarm.revision;
        let schedule = alarm_to_schedule(alarm, &zone).map_err(|error| rule_error(&error))?;
        let now = self.inner.now_unix();
        let updated = self
            .inner
            .store_call(move |store| store.update_schedule(&schedule, revision, now))
            .await?;
        self.inner.announce(
            &ctx,
            SyncChange::schedule(updated.kind, SyncOp::Upsert, &updated.id),
        );
        response(proto::UpdateAlarmResponse {
            alarm: schedule_to_alarm(&updated, &zone).into(),
            ..proto::UpdateAlarmResponse::default()
        })
    }

    async fn delete_alarm<'a>(
        &'a self,
        ctx: RequestContext,
        request: ServiceRequest<'_, proto::DeleteAlarmRequest>,
    ) -> ServiceResult<impl connectrpc::Encodable<proto::DeleteAlarmResponse> + Send + use<'a>>
    {
        let schedule = self.get_alarm_schedule(request.id).await?;
        let deleted = self.inner.delete_schedule_synced(&ctx, schedule.id).await?;
        response(proto::DeleteAlarmResponse {
            deleted,
            ..proto::DeleteAlarmResponse::default()
        })
    }

    async fn set_alarm_enabled<'a>(
        &'a self,
        ctx: RequestContext,
        request: ServiceRequest<'_, proto::SetAlarmEnabledRequest>,
    ) -> ServiceResult<impl connectrpc::Encodable<proto::SetAlarmEnabledResponse> + Send + use<'a>>
    {
        let zone = ClientZone::from_request(&ctx)?;
        let schedule = self.get_alarm_schedule(request.id).await?;
        let id = schedule.id.clone();
        let enabled = request.enabled;
        let now = self.inner.now_unix();
        let updated = self
            .inner
            .store_call(move |store| store.set_schedule_enabled(&id, enabled, now))
            .await?;
        self.inner.announce(
            &ctx,
            SyncChange::schedule(updated.kind, SyncOp::Upsert, &updated.id),
        );
        response(proto::SetAlarmEnabledResponse {
            alarm: schedule_to_alarm(&updated, &zone).into(),
            ..proto::SetAlarmEnabledResponse::default()
        })
    }

    async fn snooze_alarm<'a>(
        &'a self,
        ctx: RequestContext,
        request: ServiceRequest<'_, proto::SnoozeAlarmRequest>,
    ) -> ServiceResult<impl connectrpc::Encodable<proto::SnoozeAlarmResponse> + Send + use<'a>>
    {
        let zone = ClientZone::from_request(&ctx)?;
        let schedule = self.get_alarm_schedule(request.id).await?;
        let id = schedule.id.clone();
        let minutes = request.minutes;
        let now = self.inner.now_unix();
        let (updated, _) = self
            .inner
            .store_call(move |store| store.snooze_schedule(&id, minutes, now))
            .await?;
        self.inner.announce(
            &ctx,
            SyncChange::schedule(updated.kind, SyncOp::Upsert, &updated.id),
        );
        response(proto::SnoozeAlarmResponse {
            alarm: schedule_to_alarm(&updated, &zone).into(),
            ..proto::SnoozeAlarmResponse::default()
        })
    }

    async fn stop_alarm<'a>(
        &'a self,
        ctx: RequestContext,
        request: ServiceRequest<'_, proto::StopAlarmRequest>,
    ) -> ServiceResult<impl connectrpc::Encodable<proto::StopAlarmResponse> + Send + use<'a>> {
        let zone = ClientZone::from_request(&ctx)?;
        let schedule = self.get_alarm_schedule(request.id).await?;
        let id = schedule.id.clone();
        let now = self.inner.now_unix();
        let updated = self
            .inner
            .store_call(move |store| store.stop_schedule(&id, now))
            .await?;
        self.inner.announce(
            &ctx,
            SyncChange::schedule(updated.kind, SyncOp::Upsert, &updated.id),
        );
        response(proto::StopAlarmResponse {
            alarm: schedule_to_alarm(&updated, &zone).into(),
            ..proto::StopAlarmResponse::default()
        })
    }
}
