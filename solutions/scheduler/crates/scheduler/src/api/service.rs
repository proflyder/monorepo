//! `home.v1.SchedulerService` implementation.

use std::sync::Arc;

use connectrpc::{ConnectError, RequestContext, Response, ServiceRequest, ServiceResult};
use home_proto::v1 as proto;
use home_sqlite::SqliteError;
use proto::ScenarioService;
use proto::SchedulerService;

use crate::domain::Clock;
use crate::store::{Store, StoreError};

use super::convert::{
    ack_outcome, create_view_to_scenario, create_view_to_schedule, event_to_proto,
    scenario_to_proto, schedule_to_proto, timer_to_proto, view_to_scenario, view_to_schedule,
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
    scenario_execution: bool,
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
            scenario_execution: false,
        }
    }

    /// Advertise the scenario executor in `GetStatus`. Deployment-controlled:
    /// the scheduler stores definitions but `ha-scenario` runs them.
    #[must_use]
    pub fn with_scenario_execution(mut self, supported: bool) -> Self {
        self.scenario_execution = supported;
        self
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

    /// Runs one store operation on the blocking pool.
    ///
    /// The store talks to a single SQLite writer thread and can block on the
    /// connection mutex; calling it directly from a handler would stall the
    /// async runtime. `spawn_blocking` keeps every RPC (and the engine's
    /// tick/purge) off the reactor threads.
    pub async fn store_call<T, F>(&self, call: F) -> Result<T, ConnectError>
    where
        T: Send + 'static,
        F: FnOnce(&Store) -> Result<T, StoreError> + Send + 'static,
    {
        let store = Arc::clone(&self.store);
        tokio::task::spawn_blocking(move || call(&store))
            .await
            .map_err(|_| ConnectError::from(StoreError::Sqlite(SqliteError::Panicked)))?
            .map_err(ConnectError::from)
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
        let now = self.now_unix();
        let key = message.idempotency_key.clone();
        let created = self
            .store_call(move |store| store.create_schedule(&schedule, &key, now))
            .await?;
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
        let id = request.id.to_owned();
        let schedule = self
            .store_call(move |store| store.get_schedule(&id))
            .await?;
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
        let include_disabled = request.include_disabled;
        let schedules = self
            .store_call(move |store| store.list_schedules(include_disabled, kind))
            .await?;
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
            return Err(super::text::connect(
                ConnectError::invalid_argument("schedule is required"),
                "scheduler.error.schedule_required",
            ));
        };
        if message.expected_revision < 0 {
            return Err(super::text::connect(
                ConnectError::invalid_argument("expected_revision must not be negative"),
                "scheduler.error.expected_revision_negative",
            ));
        }
        let schedule = view_to_schedule(view);
        let expected_revision = message.expected_revision;
        let now = self.now_unix();
        let updated = self
            .store_call(move |store| store.update_schedule(&schedule, expected_revision, now))
            .await?;
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
        let id = request.id.to_owned();
        let now = self.now_unix();
        let deleted = self
            .store_call(move |store| store.delete_schedule(&id, now))
            .await?;
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
        let id = request.id.to_owned();
        let enabled = request.enabled;
        let now = self.now_unix();
        let schedule = self
            .store_call(move |store| store.set_schedule_enabled(&id, enabled, now))
            .await?;
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
        let id = request.id.to_owned();
        let minutes = request.minutes;
        let now = self.now_unix();
        let (schedule, event) = self
            .store_call(move |store| store.snooze_schedule(&id, minutes, now))
            .await?;
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
        let id = request.id.to_owned();
        let now = self.now_unix();
        let schedule = self
            .store_call(move |store| store.stop_schedule(&id, now))
            .await?;
        response(proto::StopScheduleResponse {
            schedule: schedule_to_proto(&schedule).into(),
            ..proto::StopScheduleResponse::default()
        })
    }

    async fn skip_schedule_occurrence<'a>(
        &'a self,
        _ctx: RequestContext,
        request: ServiceRequest<'_, proto::SkipScheduleOccurrenceRequest>,
    ) -> ServiceResult<
        impl connectrpc::Encodable<proto::SkipScheduleOccurrenceResponse> + Send + use<'a>,
    > {
        let id = request.id.to_owned();
        let now = self.now_unix();
        let schedule = self
            .store_call(move |store| store.skip_next_occurrence(&id, now))
            .await?;
        response(proto::SkipScheduleOccurrenceResponse {
            schedule: schedule_to_proto(&schedule).into(),
            ..proto::SkipScheduleOccurrenceResponse::default()
        })
    }

    async fn start_timer<'a>(
        &'a self,
        _ctx: RequestContext,
        request: ServiceRequest<'_, proto::StartTimerRequest>,
    ) -> ServiceResult<impl connectrpc::Encodable<proto::StartTimerResponse> + Send + use<'a>> {
        let label = request.label.to_owned();
        let duration_seconds = request.duration_seconds;
        let key = request.idempotency_key.to_owned();
        let now = self.now_unix();
        let timer = self
            .store_call(move |store| store.start_timer(&label, duration_seconds, &key, now))
            .await?;
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
        let id = request.id.to_owned();
        let timer = self.store_call(move |store| store.get_timer(&id)).await?;
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
        let include_finished = request.include_finished;
        let timers = self
            .store_call(move |store| store.list_timers(include_finished))
            .await?;
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
        let id = request.id.to_owned();
        let now = self.now_unix();
        let timer = self
            .store_call(move |store| store.pause_timer(&id, now))
            .await?;
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
        let id = request.id.to_owned();
        let now = self.now_unix();
        let timer = self
            .store_call(move |store| store.resume_timer(&id, now))
            .await?;
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
        let id = request.id.to_owned();
        let now = self.now_unix();
        let timer = self
            .store_call(move |store| store.cancel_timer(&id, now))
            .await?;
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
        let consumer = request.consumer_id.to_owned();
        let lease_seconds = request.lease_seconds;
        let limit = request.limit;
        let queue = request.queue.to_owned();
        let now = self.now_unix();
        let events = self
            .store_call(move |store| {
                store.list_due_events(&consumer, lease_seconds, limit, &queue, now)
            })
            .await?;
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
        let event_id = request.event_id.to_owned();
        let consumer = request.consumer_id.to_owned();
        let outcome = ack_outcome(request.outcome.to_i32());
        let detail = request.detail.to_owned();
        let now = self.now_unix();
        let event = self
            .store_call(move |store| store.ack_event(&event_id, &consumer, outcome, &detail, now))
            .await?;
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
        let task_type = request.task_type.to_owned();
        let payload = request.payload_json.to_owned();
        let queue = request.queue.to_owned();
        let run_at_unix = request.run_at_unix;
        let key = request.idempotency_key.to_owned();
        let now = self.now_unix();
        let event = self
            .store_call(move |store| {
                store.enqueue_task(&task_type, &payload, &queue, run_at_unix, &key, now)
            })
            .await?;
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
        let event_id = request.event_id.to_owned();
        let now = self.now_unix();
        let event = self
            .store_call(move |store| store.cancel_task(&event_id, now))
            .await?;
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
        let status = self.store_call(move |store| store.status(now)).await?;
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
        let tick_error_text = super::text::tick_error_text(&engine.last_tick_error);
        let tick_error_text = if tick_error_text.key.is_empty() {
            proto::GetStatusResponse::default().last_tick_error_text
        } else {
            tick_error_text.into()
        };
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
            scenario_execution_supported: self.scenario_execution,
            scenario_store_supported: true,
            queues,
            ready: engine.ready,
            last_tick_attempt_unix: engine.last_tick_attempt_unix,
            last_tick_error_unix: engine.last_tick_error_unix,
            last_tick_error_text: tick_error_text,
            last_tick_error: engine.last_tick_error,
            ..proto::GetStatusResponse::default()
        })
    }
}

/// Rejects a request that contains an unknown oneof variant. The domain
/// conversion drops unknown kinds, so a length mismatch means the client sent
/// a trigger/condition/step this server does not understand.
fn check_scenario_kinds(
    scenario: &crate::domain::Scenario,
    triggers: usize,
    conditions: usize,
    steps: usize,
) -> Result<(), ConnectError> {
    if scenario.triggers.len() != triggers
        || scenario.conditions.len() != conditions
        || scenario.steps.len() != steps
    {
        return Err(super::text::connect(
            ConnectError::invalid_argument(
                "scenario contains an unsupported trigger, condition or step kind",
            ),
            "scheduler.error.unsupported_scenario_part",
        ));
    }
    Ok(())
}

impl ScenarioService for Service {
    async fn create_scenario<'a>(
        &'a self,
        _ctx: RequestContext,
        request: ServiceRequest<'_, proto::CreateScenarioRequest>,
    ) -> ServiceResult<impl connectrpc::Encodable<proto::CreateScenarioResponse> + Send + use<'a>>
    {
        let message = request.to_owned_message();
        let scenario = create_view_to_scenario(&message);
        check_scenario_kinds(
            &scenario,
            message.triggers.len(),
            message.conditions.len(),
            message.steps.len(),
        )?;
        let now = self.now_unix();
        let key = message.idempotency_key.clone();
        let created = self
            .store_call(move |store| store.create_scenario(&scenario, &key, now))
            .await?;
        response(proto::CreateScenarioResponse {
            scenario: scenario_to_proto(&created).into(),
            ..proto::CreateScenarioResponse::default()
        })
    }

    async fn get_scenario<'a>(
        &'a self,
        _ctx: RequestContext,
        request: ServiceRequest<'_, proto::GetScenarioRequest>,
    ) -> ServiceResult<impl connectrpc::Encodable<proto::GetScenarioResponse> + Send + use<'a>>
    {
        let id = request.id.to_owned();
        let scenario = self
            .store_call(move |store| store.get_scenario(&id))
            .await?;
        response(proto::GetScenarioResponse {
            scenario: scenario_to_proto(&scenario).into(),
            ..proto::GetScenarioResponse::default()
        })
    }

    async fn list_scenarios<'a>(
        &'a self,
        _ctx: RequestContext,
        request: ServiceRequest<'_, proto::ListScenariosRequest>,
    ) -> ServiceResult<impl connectrpc::Encodable<proto::ListScenariosResponse> + Send + use<'a>>
    {
        let include_disabled = request.include_disabled;
        let scenarios = self
            .store_call(move |store| store.list_scenarios(include_disabled))
            .await?;
        response(proto::ListScenariosResponse {
            scenarios: scenarios.iter().map(scenario_to_proto).collect(),
            ..proto::ListScenariosResponse::default()
        })
    }

    async fn update_scenario<'a>(
        &'a self,
        _ctx: RequestContext,
        request: ServiceRequest<'_, proto::UpdateScenarioRequest>,
    ) -> ServiceResult<impl connectrpc::Encodable<proto::UpdateScenarioResponse> + Send + use<'a>>
    {
        let message = request.to_owned_message();
        let Some(view) = message.scenario.as_option() else {
            return Err(super::text::connect(
                ConnectError::invalid_argument("scenario is required"),
                "scheduler.error.scenario_required",
            ));
        };
        if message.expected_revision < 0 {
            return Err(super::text::connect(
                ConnectError::invalid_argument("expected_revision must not be negative"),
                "scheduler.error.expected_revision_negative",
            ));
        }
        let scenario = view_to_scenario(view);
        check_scenario_kinds(
            &scenario,
            view.triggers.len(),
            view.conditions.len(),
            view.steps.len(),
        )?;
        let expected_revision = message.expected_revision;
        let now = self.now_unix();
        let updated = self
            .store_call(move |store| store.update_scenario(&scenario, expected_revision, now))
            .await?;
        response(proto::UpdateScenarioResponse {
            scenario: scenario_to_proto(&updated).into(),
            ..proto::UpdateScenarioResponse::default()
        })
    }

    async fn delete_scenario<'a>(
        &'a self,
        _ctx: RequestContext,
        request: ServiceRequest<'_, proto::DeleteScenarioRequest>,
    ) -> ServiceResult<impl connectrpc::Encodable<proto::DeleteScenarioResponse> + Send + use<'a>>
    {
        let id = request.id.to_owned();
        let now = self.now_unix();
        let deleted = self
            .store_call(move |store| store.delete_scenario(&id, now))
            .await?;
        response(proto::DeleteScenarioResponse {
            deleted,
            ..proto::DeleteScenarioResponse::default()
        })
    }

    async fn set_scenario_enabled<'a>(
        &'a self,
        _ctx: RequestContext,
        request: ServiceRequest<'_, proto::SetScenarioEnabledRequest>,
    ) -> ServiceResult<impl connectrpc::Encodable<proto::SetScenarioEnabledResponse> + Send + use<'a>>
    {
        let id = request.id.to_owned();
        let enabled = request.enabled;
        let now = self.now_unix();
        let scenario = self
            .store_call(move |store| store.set_scenario_enabled(&id, enabled, now))
            .await?;
        response(proto::SetScenarioEnabledResponse {
            scenario: scenario_to_proto(&scenario).into(),
            ..proto::SetScenarioEnabledResponse::default()
        })
    }

    async fn run_scenario<'a>(
        &'a self,
        _ctx: RequestContext,
        request: ServiceRequest<'_, proto::RunScenarioRequest>,
    ) -> ServiceResult<impl connectrpc::Encodable<proto::RunScenarioResponse> + Send + use<'a>>
    {
        let id = request.id.to_owned();
        let key = request.idempotency_key.to_owned();
        let now = self.now_unix();
        let event = self
            .store_call(move |store| store.run_scenario(&id, &key, now))
            .await?;
        response(proto::RunScenarioResponse {
            event: event_to_proto(&event).into(),
            ..proto::RunScenarioResponse::default()
        })
    }
}
