//! `ricardo.v1.SchedulerService` implementation.

use std::sync::Arc;

use connectrpc::{ConnectError, RequestContext, Response, ServiceRequest, ServiceResult};
use home_proto::v1 as proto;
use proto::SchedulerService;

use crate::domain::Clock;
use crate::store::{Store, StoreError};

use super::convert::{
    ack_outcome, create_view_to_schedule, event_to_proto, schedule_to_proto, timer_to_proto,
    view_to_schedule,
};

/// Engine readiness and last-tick facts reported by `GetStatus`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct EngineStatus {
    pub ready: bool,
    pub last_tick_unix: i64,
    pub last_tick_attempt_unix: i64,
    pub last_tick_error_unix: i64,
    pub last_tick_error: String,
}

/// Shared scheduler API state.
#[derive(Clone)]
pub struct Service {
    store: Arc<Store>,
    clock: Arc<dyn Clock>,
    version: String,
    engine: Arc<dyn Fn() -> EngineStatus + Send + Sync>,
    allowed: Vec<String>,
}

impl std::fmt::Debug for Service {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("Service")
            .field("version", &self.version)
            .field("allowed", &self.allowed)
            .finish_non_exhaustive()
    }
}

impl Service {
    /// Builds the scheduler API.
    #[must_use]
    pub fn new(
        store: Arc<Store>,
        clock: Arc<dyn Clock>,
        version: impl Into<String>,
        engine: Arc<dyn Fn() -> EngineStatus + Send + Sync>,
        allowed_consumers: Vec<String>,
    ) -> Self {
        let allowed = allowed_consumers
            .into_iter()
            .map(|consumer| consumer.trim().to_owned())
            .filter(|consumer| !consumer.is_empty())
            .collect();
        Self {
            store,
            clock,
            version: version.into(),
            engine,
            allowed,
        }
    }

    /// The durable store behind this service.
    #[must_use]
    pub fn store(&self) -> &Arc<Store> {
        &self.store
    }

    /// Current server clock.
    #[must_use]
    pub fn now_unix(&self) -> i64 {
        self.clock.now_unix()
    }

    pub fn check_consumer(&self, consumer_id: &str) -> Result<(), ConnectError> {
        if self.allowed.is_empty() || self.allowed.iter().any(|value| value == consumer_id) {
            return Ok(());
        }
        Err(ConnectError::from(StoreError::ConsumerDenied))
    }

    pub fn map<T>(result: Result<T, StoreError>) -> Result<T, ConnectError> {
        result.map_err(ConnectError::from)
    }
}

fn response<T>(message: T) -> ServiceResult<T> {
    Response::ok(message)
}

impl SchedulerService for Service {
    async fn create_schedule<'a>(
        &'a self,
        _ctx: RequestContext,
        request: ServiceRequest<'_, proto::CreateScheduleRequest>,
    ) -> ServiceResult<impl connectrpc::Encodable<proto::CreateScheduleResponse> + Send + use<'a>>
    {
        let message = request.to_owned_message();
        let schedule = create_view_to_schedule(&message);
        let created = Self::map(self.store.create_schedule(
            &schedule,
            &message.idempotency_key,
            self.now_unix(),
        ))?;
        response(proto::CreateScheduleResponse {
            schedule: schedule_to_proto(&created).into(),
            ..proto::CreateScheduleResponse::default()
        })
    }

    async fn get_schedule<'a>(
        &'a self,
        _ctx: RequestContext,
        request: ServiceRequest<'_, proto::GetScheduleRequest>,
    ) -> ServiceResult<impl connectrpc::Encodable<proto::GetScheduleResponse> + Send + use<'a>>
    {
        let schedule = Self::map(self.store.get_schedule(request.id))?;
        response(proto::GetScheduleResponse {
            schedule: schedule_to_proto(&schedule).into(),
            ..proto::GetScheduleResponse::default()
        })
    }

    async fn list_schedules<'a>(
        &'a self,
        _ctx: RequestContext,
        request: ServiceRequest<'_, proto::ListSchedulesRequest>,
    ) -> ServiceResult<impl connectrpc::Encodable<proto::ListSchedulesResponse> + Send + use<'a>>
    {
        let kind = super::convert::schedule_kind(request.kind.to_i32());
        let schedules = Self::map(self.store.list_schedules(request.include_disabled, kind))?;
        response(proto::ListSchedulesResponse {
            schedules: schedules.iter().map(schedule_to_proto).collect(),
            ..proto::ListSchedulesResponse::default()
        })
    }

    async fn update_schedule<'a>(
        &'a self,
        _ctx: RequestContext,
        request: ServiceRequest<'_, proto::UpdateScheduleRequest>,
    ) -> ServiceResult<impl connectrpc::Encodable<proto::UpdateScheduleResponse> + Send + use<'a>>
    {
        let message = request.to_owned_message();
        let Some(view) = message.schedule.as_option() else {
            return Err(ConnectError::invalid_argument("schedule is required"));
        };
        if message.expected_revision < 0 {
            return Err(ConnectError::invalid_argument(
                "expected_revision must not be negative",
            ));
        }
        let schedule = view_to_schedule(view);
        let updated = Self::map(self.store.update_schedule(
            &schedule,
            message.expected_revision,
            self.now_unix(),
        ))?;
        response(proto::UpdateScheduleResponse {
            schedule: schedule_to_proto(&updated).into(),
            ..proto::UpdateScheduleResponse::default()
        })
    }

    async fn delete_schedule<'a>(
        &'a self,
        _ctx: RequestContext,
        request: ServiceRequest<'_, proto::DeleteScheduleRequest>,
    ) -> ServiceResult<impl connectrpc::Encodable<proto::DeleteScheduleResponse> + Send + use<'a>>
    {
        let deleted = Self::map(self.store.delete_schedule(request.id, self.now_unix()))?;
        response(proto::DeleteScheduleResponse {
            deleted,
            ..proto::DeleteScheduleResponse::default()
        })
    }

    async fn set_schedule_enabled<'a>(
        &'a self,
        _ctx: RequestContext,
        request: ServiceRequest<'_, proto::SetScheduleEnabledRequest>,
    ) -> ServiceResult<impl connectrpc::Encodable<proto::SetScheduleEnabledResponse> + Send + use<'a>>
    {
        let schedule = Self::map(self.store.set_schedule_enabled(
            request.id,
            request.enabled,
            self.now_unix(),
        ))?;
        response(proto::SetScheduleEnabledResponse {
            schedule: schedule_to_proto(&schedule).into(),
            ..proto::SetScheduleEnabledResponse::default()
        })
    }

    async fn snooze_schedule<'a>(
        &'a self,
        _ctx: RequestContext,
        request: ServiceRequest<'_, proto::SnoozeScheduleRequest>,
    ) -> ServiceResult<impl connectrpc::Encodable<proto::SnoozeScheduleResponse> + Send + use<'a>>
    {
        let (schedule, event) = Self::map(self.store.snooze_schedule(
            request.id,
            request.minutes,
            self.now_unix(),
        ))?;
        response(proto::SnoozeScheduleResponse {
            schedule: schedule_to_proto(&schedule).into(),
            event: event_to_proto(&event).into(),
            ..proto::SnoozeScheduleResponse::default()
        })
    }

    async fn stop_schedule<'a>(
        &'a self,
        _ctx: RequestContext,
        request: ServiceRequest<'_, proto::StopScheduleRequest>,
    ) -> ServiceResult<impl connectrpc::Encodable<proto::StopScheduleResponse> + Send + use<'a>>
    {
        let schedule = Self::map(self.store.stop_schedule(request.id, self.now_unix()))?;
        response(proto::StopScheduleResponse {
            schedule: schedule_to_proto(&schedule).into(),
            ..proto::StopScheduleResponse::default()
        })
    }

    async fn start_timer<'a>(
        &'a self,
        _ctx: RequestContext,
        request: ServiceRequest<'_, proto::StartTimerRequest>,
    ) -> ServiceResult<impl connectrpc::Encodable<proto::StartTimerResponse> + Send + use<'a>> {
        let timer = Self::map(self.store.start_timer(
            request.label,
            request.duration_seconds,
            request.idempotency_key,
            self.now_unix(),
        ))?;
        response(proto::StartTimerResponse {
            timer: timer_to_proto(&timer).into(),
            ..proto::StartTimerResponse::default()
        })
    }

    async fn get_timer<'a>(
        &'a self,
        _ctx: RequestContext,
        request: ServiceRequest<'_, proto::GetTimerRequest>,
    ) -> ServiceResult<impl connectrpc::Encodable<proto::GetTimerResponse> + Send + use<'a>> {
        let timer = Self::map(self.store.get_timer(request.id))?;
        response(proto::GetTimerResponse {
            timer: timer_to_proto(&timer).into(),
            ..proto::GetTimerResponse::default()
        })
    }

    async fn list_timers<'a>(
        &'a self,
        _ctx: RequestContext,
        request: ServiceRequest<'_, proto::ListTimersRequest>,
    ) -> ServiceResult<impl connectrpc::Encodable<proto::ListTimersResponse> + Send + use<'a>> {
        let timers = Self::map(self.store.list_timers(request.include_finished))?;
        response(proto::ListTimersResponse {
            timers: timers.iter().map(timer_to_proto).collect(),
            ..proto::ListTimersResponse::default()
        })
    }

    async fn pause_timer<'a>(
        &'a self,
        _ctx: RequestContext,
        request: ServiceRequest<'_, proto::PauseTimerRequest>,
    ) -> ServiceResult<impl connectrpc::Encodable<proto::PauseTimerResponse> + Send + use<'a>> {
        let timer = Self::map(self.store.pause_timer(request.id, self.now_unix()))?;
        response(proto::PauseTimerResponse {
            timer: timer_to_proto(&timer).into(),
            ..proto::PauseTimerResponse::default()
        })
    }

    async fn resume_timer<'a>(
        &'a self,
        _ctx: RequestContext,
        request: ServiceRequest<'_, proto::ResumeTimerRequest>,
    ) -> ServiceResult<impl connectrpc::Encodable<proto::ResumeTimerResponse> + Send + use<'a>>
    {
        let timer = Self::map(self.store.resume_timer(request.id, self.now_unix()))?;
        response(proto::ResumeTimerResponse {
            timer: timer_to_proto(&timer).into(),
            ..proto::ResumeTimerResponse::default()
        })
    }

    async fn cancel_timer<'a>(
        &'a self,
        _ctx: RequestContext,
        request: ServiceRequest<'_, proto::CancelTimerRequest>,
    ) -> ServiceResult<impl connectrpc::Encodable<proto::CancelTimerResponse> + Send + use<'a>>
    {
        let timer = Self::map(self.store.cancel_timer(request.id, self.now_unix()))?;
        response(proto::CancelTimerResponse {
            timer: timer_to_proto(&timer).into(),
            ..proto::CancelTimerResponse::default()
        })
    }

    async fn list_due_events<'a>(
        &'a self,
        _ctx: RequestContext,
        request: ServiceRequest<'_, proto::ListDueEventsRequest>,
    ) -> ServiceResult<impl connectrpc::Encodable<proto::ListDueEventsResponse> + Send + use<'a>>
    {
        self.check_consumer(request.consumer_id)?;
        let events = Self::map(self.store.list_due_events(
            request.consumer_id,
            request.lease_seconds,
            request.limit,
            request.queue,
            self.now_unix(),
        ))?;
        response(proto::ListDueEventsResponse {
            events: events.iter().map(event_to_proto).collect(),
            ..proto::ListDueEventsResponse::default()
        })
    }

    async fn ack_event<'a>(
        &'a self,
        _ctx: RequestContext,
        request: ServiceRequest<'_, proto::AckEventRequest>,
    ) -> ServiceResult<impl connectrpc::Encodable<proto::AckEventResponse> + Send + use<'a>> {
        self.check_consumer(request.consumer_id)?;
        let event = Self::map(self.store.ack_event(
            request.event_id,
            request.consumer_id,
            ack_outcome(request.outcome.to_i32()),
            request.detail,
            self.now_unix(),
        ))?;
        response(proto::AckEventResponse {
            event: event_to_proto(&event).into(),
            ..proto::AckEventResponse::default()
        })
    }

    async fn enqueue_task<'a>(
        &'a self,
        _ctx: RequestContext,
        request: ServiceRequest<'_, proto::EnqueueTaskRequest>,
    ) -> ServiceResult<impl connectrpc::Encodable<proto::EnqueueTaskResponse> + Send + use<'a>>
    {
        let event = Self::map(self.store.enqueue_task(
            request.task_type,
            request.payload_json,
            request.queue,
            request.run_at_unix,
            request.idempotency_key,
            self.now_unix(),
        ))?;
        response(proto::EnqueueTaskResponse {
            event: event_to_proto(&event).into(),
            ..proto::EnqueueTaskResponse::default()
        })
    }

    async fn cancel_task<'a>(
        &'a self,
        _ctx: RequestContext,
        request: ServiceRequest<'_, proto::CancelTaskRequest>,
    ) -> ServiceResult<impl connectrpc::Encodable<proto::CancelTaskResponse> + Send + use<'a>> {
        let event = Self::map(self.store.cancel_task(request.event_id, self.now_unix()))?;
        response(proto::CancelTaskResponse {
            event: event_to_proto(&event).into(),
            ..proto::CancelTaskResponse::default()
        })
    }

    async fn get_status<'a>(
        &'a self,
        _ctx: RequestContext,
        _request: ServiceRequest<'_, proto::GetStatusRequest>,
    ) -> ServiceResult<impl connectrpc::Encodable<proto::GetStatusResponse> + Send + use<'a>> {
        let now = self.now_unix();
        let status = Self::map(self.store.status(now))?;
        let engine = (self.engine)();
        let consumers = status
            .consumers
            .iter()
            .map(|consumer| proto::ConsumerStatus {
                consumer_id: consumer.consumer_id.clone(),
                last_seen_unix: consumer.last_seen_unix,
                last_ack_unix: consumer.last_ack_unix,
                alive: consumer.alive,
                ..proto::ConsumerStatus::default()
            })
            .collect();
        let queues = status
            .queues
            .iter()
            .map(|queue| proto::QueueInfo {
                name: queue.name.clone(),
                pending: queue.pending,
                leased: queue.leased,
                ..proto::QueueInfo::default()
            })
            .collect();
        response(proto::GetStatusResponse {
            version: self.version.clone(),
            schema_version: i32::try_from(status.schema_version).unwrap_or(i32::MAX),
            now_unix: now,
            last_tick_unix: engine.last_tick_unix,
            pending_events: status.pending_events,
            leased_events: status.leased_events,
            enabled_schedules: status.enabled_schedules,
            running_timers: status.running_timers,
            consumers,
            scenario_execution_supported: false,
            queues,
            ready: engine.ready,
            last_tick_attempt_unix: engine.last_tick_attempt_unix,
            last_tick_error_unix: engine.last_tick_error_unix,
            last_tick_error: engine.last_tick_error,
            ..proto::GetStatusResponse::default()
        })
    }
}
