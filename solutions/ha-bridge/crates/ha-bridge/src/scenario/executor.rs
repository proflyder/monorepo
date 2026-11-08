//! The `scenario` queue executor.
//!
//! One `scenario.fire` event runs the stored definition: conditions first, then
//! each step in order. The catalog and the Home Assistant service call path are
//! the same as the `control` queue, so a scenario can only reach entities the
//! live catalog exposes. A `scenario.stop` event cancels a running instance;
//! the registry of running scenarios also prevents one scenario from running
//! twice at the same time.
//!
//! Retries (no ack, lease expiry) happen only before the first step: an
//! unavailable scheduler, Home Assistant or catalog. Once a step ran, a failure
//! is terminal. The ledger records `started` before the first step, so a
//! re-delivery of an interrupted run is acknowledged `FAILED "interrupted"`
//! instead of replaying side effects.

use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

use home_ha::ErrorKind;
use home_worker::{Action, Event, Outcome, Source};
use jiff::Zoned;
use jiff::tz::TimeZone;
use serde::Deserialize;
use serde_json::{Map, Value};
use tokio_util::sync::CancellationToken;

use crate::catalog::{self, Catalog, Params, ServiceCall};
use crate::discovery::CatalogSource;
use crate::scenario::ledger::{ScenarioLedger, Start};
use crate::scenario::{Condition, Definition, Step};
use crate::scenario_config::ScenarioConfig;

/// The Home Assistant surface the scenario executor needs.
pub trait ScenarioHa: Send + Sync {
    fn state(&self, entity: &str) -> impl Future<Output = Result<String, home_ha::Error>> + Send;

    fn call_service(
        &self,
        domain: &str,
        service: &str,
        entity: &str,
        data: Map<String, Value>,
    ) -> impl Future<Output = Result<(), home_ha::Error>> + Send;
}

impl ScenarioHa for home_ha::Client {
    async fn state(&self, entity: &str) -> Result<String, home_ha::Error> {
        self.state(entity).await
    }

    fn call_service(
        &self,
        domain: &str,
        service: &str,
        entity: &str,
        data: Map<String, Value>,
    ) -> impl Future<Output = Result<(), home_ha::Error>> + Send {
        self.call_service_on(domain, service, entity, Value::Object(data))
    }
}

impl<T: ScenarioHa + ?Sized> ScenarioHa for Arc<T> {
    fn state(&self, entity: &str) -> impl Future<Output = Result<String, home_ha::Error>> + Send {
        (**self).state(entity)
    }

    fn call_service(
        &self,
        domain: &str,
        service: &str,
        entity: &str,
        data: Map<String, Value>,
    ) -> impl Future<Output = Result<(), home_ha::Error>> + Send {
        (**self).call_service(domain, service, entity, data)
    }
}

/// Why a definition could not be read.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ScenarioLoadError {
    /// A retry after a lease expiry can succeed.
    pub transient: bool,
}

/// The definition source (the scheduler `ScenarioService` in production).
pub trait ScenarioSource: Send + Sync {
    fn get(
        &self,
        id: &str,
    ) -> Pin<Box<dyn Future<Output = Result<Option<Definition>, ScenarioLoadError>> + Send + '_>>;
}

impl ScenarioSource for home_proto::ScenarioClient {
    fn get(
        &self,
        id: &str,
    ) -> Pin<Box<dyn Future<Output = Result<Option<Definition>, ScenarioLoadError>> + Send + '_>>
    {
        let id = id.to_owned();
        Box::pin(async move {
            self.get_scenario(&id)
                .await
                .map(|scenario| scenario.as_ref().map(Definition::from_proto))
                .map_err(|error| ScenarioLoadError {
                    transient: error.is_transient(),
                })
        })
    }
}

type Clock = Arc<dyn Fn() -> i64 + Send + Sync>;

/// The executor. One instance owns the configured `scenario` queue.
pub struct Executor<H: ScenarioHa> {
    config: Arc<ScenarioConfig>,
    scenarios: Arc<dyn ScenarioSource>,
    catalog: Arc<dyn CatalogSource>,
    ha: Arc<H>,
    ledger: Arc<ScenarioLedger>,
    clock: Clock,
    running: Mutex<HashMap<String, CancellationToken>>,
}

impl<H: ScenarioHa> std::fmt::Debug for Executor<H> {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("ScenarioExecutor")
            .field("config", &self.config)
            .field(
                "running",
                &self.running.lock().map_or(0, |running| running.len()),
            )
            .finish_non_exhaustive()
    }
}

impl<H: ScenarioHa> Executor<H> {
    /// Build the executor.
    #[must_use]
    pub fn new(
        config: Arc<ScenarioConfig>,
        scenarios: Arc<dyn ScenarioSource>,
        catalog: Arc<dyn CatalogSource>,
        ha: Arc<H>,
        ledger: Arc<ScenarioLedger>,
    ) -> Self {
        Self {
            config,
            scenarios,
            catalog,
            ha,
            ledger,
            clock: Arc::new(|| {
                let now = jiff::Timestamp::now();
                now.as_second()
            }),
            running: Mutex::new(HashMap::new()),
        }
    }

    /// Replace the clock (tests).
    #[must_use]
    pub fn with_clock(mut self, clock: impl Fn() -> i64 + Send + Sync + 'static) -> Self {
        self.clock = Arc::new(clock);
        self
    }

    /// Claim a running slot for one scenario. `None` means an instance is
    /// already running: the second FIRE must not run in parallel.
    pub fn begin(&self, scenario_id: &str) -> Option<CancellationToken> {
        let mut running = self.lock_running();
        if running.contains_key(scenario_id) {
            return None;
        }
        let token = CancellationToken::new();
        running.insert(scenario_id.to_owned(), token.clone());
        drop(running);
        Some(token)
    }

    /// Release the running slot (always called by the fire task).
    pub fn finish(&self, scenario_id: &str) {
        self.lock_running().remove(scenario_id);
    }

    /// Cancel a running instance. Returns whether one was running.
    pub fn cancel(&self, scenario_id: &str) -> bool {
        let running = self.lock_running();
        running.get(scenario_id).is_some_and(|token| {
            token.cancel();
            true
        })
    }

    /// Cancel every running instance (graceful shutdown).
    pub fn cancel_all(&self) {
        for token in self.lock_running().values() {
            token.cancel();
        }
    }

    /// Handle one event. A FIRE claims the running slot itself, so a
    /// re-delivery while the first run is still active is `SKIPPED`.
    pub async fn handle(&self, event: &Event) -> Outcome {
        if event.id.is_empty() {
            tracing::error!("scheduler returned an event without id; ignoring it");
            return Outcome::Retry;
        }
        match event.action {
            Action::Stop => self.handle_stop(event),
            Action::Fire => self.handle_fire(event).await,
            Action::Other(_) => Outcome::Failed("unsupported scenario event action".to_owned()),
        }
    }

    fn handle_stop(&self, event: &Event) -> Outcome {
        let Some(payload) = payload(event) else {
            return Outcome::Failed("invalid scenario event payload".to_owned());
        };
        if payload.scenario_id.is_empty() {
            return Outcome::Failed("invalid scenario event payload".to_owned());
        }
        if self.cancel(&payload.scenario_id) {
            tracing::info!(scenario = %payload.scenario_id, "scenario stopped");
            Outcome::Delivered("scenario stopped".to_owned())
        } else {
            Outcome::Skipped("scenario is not running".to_owned())
        }
    }

    async fn handle_fire(&self, event: &Event) -> Outcome {
        if !event.queue.is_empty() && event.queue != self.config.queue {
            return Outcome::Failed(format!(
                "event queue {:?} does not match {:?}",
                event.queue, self.config.queue
            ));
        }
        if event.source != Source::Scenario || event.task_type != scenario_task_type() {
            return Outcome::Failed(format!(
                "unsupported event kind={:?} task_type={:?}",
                source_label(event.source),
                event.task_type
            ));
        }
        let Some(payload) = payload(event) else {
            return Outcome::Failed("invalid scenario event payload".to_owned());
        };
        if payload.scenario_id.is_empty() {
            return Outcome::Failed("invalid scenario event payload".to_owned());
        }
        let manual = payload.is_manual();
        if !manual && payload.trigger_index.is_none() {
            return Outcome::Failed("invalid scenario event payload".to_owned());
        }

        // A second FIRE while this scenario runs never queues parallel work.
        // The guard releases the slot on every return path.
        let Some(token) = self.begin(&payload.scenario_id) else {
            return Outcome::Skipped("already running".to_owned());
        };
        let _running = RunningGuard {
            executor: self,
            scenario_id: payload.scenario_id.clone(),
        };

        // Everything up to the first step is retryable without side effects.
        let scenario = match self.scenarios.get(&payload.scenario_id).await {
            Ok(Some(scenario)) => scenario,
            Ok(None) => {
                tracing::warn!(scenario = %payload.scenario_id, "scenario no longer exists");
                return Outcome::Skipped("scenario not found".to_owned());
            }
            Err(error) => {
                tracing::warn!(
                    scenario = %payload.scenario_id,
                    transient = error.transient,
                    "scenario definition is unavailable"
                );
                if error.transient && event.attempts < self.config.max_attempts {
                    return Outcome::Retry;
                }
                return Outcome::Failed("scenario service unavailable".to_owned());
            }
        };
        if !manual && !scenario.enabled {
            return Outcome::Skipped("scenario is disabled".to_owned());
        }

        let timezone = match self.timezone(&scenario) {
            Ok(timezone) => timezone,
            Err(detail) => {
                tracing::warn!(scenario = %payload.scenario_id, %detail, "scenario timezone is invalid");
                return Outcome::Failed(detail);
            }
        };
        let now = self.zoned_now(&timezone);

        match self.conditions_met(&scenario, &now).await {
            Ok(true) => {}
            Ok(false) => {
                tracing::info!(scenario = %payload.scenario_id, "scenario condition not met");
                return Outcome::Skipped("condition not met".to_owned());
            }
            Err(ConditionError::Unavailable) => {
                tracing::warn!(
                    scenario = %payload.scenario_id,
                    attempts = event.attempts,
                    "home assistant is unavailable; leaving scenario for retry"
                );
                if event.attempts < self.config.max_attempts {
                    return Outcome::Retry;
                }
                return Outcome::Failed("home assistant unreachable repeatedly".to_owned());
            }
        }

        let catalog = match self.catalog_for(&scenario, event).await {
            Ok(catalog) => catalog,
            Err(outcome) => return outcome,
        };

        match self.ledger.start(&event.id, &payload.scenario_id) {
            Ok(Start::New) => {}
            Ok(Start::Interrupted) => {
                tracing::warn!(
                    scenario = %payload.scenario_id,
                    "scenario run was interrupted by a restart"
                );
                return Outcome::Failed("interrupted".to_owned());
            }
            Ok(Start::Completed) => {
                tracing::info!(
                    scenario = %payload.scenario_id,
                    "scenario event was already executed"
                );
                return Outcome::Skipped("already completed".to_owned());
            }
            Err(error) => {
                tracing::error!(%error, scenario = %payload.scenario_id, "scenario ledger failed");
                if event.attempts < self.config.max_attempts {
                    return Outcome::Retry;
                }
                return Outcome::Failed("scenario ledger unavailable".to_owned());
            }
        }

        let outcome = self.execute_steps(&scenario, catalog, &token).await;
        if let Err(error) = self.ledger.complete(&event.id) {
            tracing::error!(%error, "scenario ledger completion failed");
        }
        outcome
    }

    fn timezone(&self, scenario: &Definition) -> Result<TimeZone, String> {
        let name = scenario.timezone().unwrap_or(self.config.timezone.as_str());
        TimeZone::get(name).map_err(|_| "scenario timezone is invalid".to_owned())
    }

    fn zoned_now(&self, timezone: &TimeZone) -> Zoned {
        let unix = (self.clock.as_ref())();
        jiff::Timestamp::from_second(unix)
            .unwrap_or_default()
            .to_zoned(timezone.clone())
    }

    async fn conditions_met(
        &self,
        scenario: &Definition,
        now: &Zoned,
    ) -> Result<bool, ConditionError> {
        if !crate::scenario::time_windows_met(&scenario.conditions, now) {
            return Ok(false);
        }
        for condition in &scenario.conditions {
            let Condition::EntityState { entity_id, state } = condition else {
                continue;
            };
            let actual = match self.ha.state(entity_id).await {
                Ok(state) => state,
                // A missing entity is a condition that does not hold, not an
                // outage; retrying the same read cannot succeed.
                Err(error) if error_status(&error) == Some(404) => return Ok(false),
                Err(error) => {
                    tracing::warn!(
                        entity = %entity_id,
                        %error,
                        "scenario condition state read failed"
                    );
                    return Err(ConditionError::Unavailable);
                }
            };
            if actual != *state {
                return Ok(false);
            }
        }
        Ok(true)
    }

    async fn catalog_for(
        &self,
        scenario: &Definition,
        event: &Event,
    ) -> Result<Option<Arc<Catalog>>, Outcome> {
        if !scenario
            .steps
            .iter()
            .any(|step| matches!(step, Step::Catalog { .. } | Step::Speak { .. }))
        {
            return Ok(None);
        }
        match self.catalog.get().await {
            Ok(snapshot) => Ok(Some(snapshot.catalog)),
            Err(error) => {
                tracing::warn!(
                    attempts = event.attempts,
                    %error,
                    "catalog unavailable; leaving scenario for retry"
                );
                if event.attempts >= self.config.max_attempts {
                    return Err(Outcome::Failed(
                        "home assistant unreachable repeatedly".to_owned(),
                    ));
                }
                Err(Outcome::Retry)
            }
        }
    }

    async fn execute_steps(
        &self,
        scenario: &Definition,
        catalog: Option<Arc<Catalog>>,
        cancel: &CancellationToken,
    ) -> Outcome {
        let mut first_error: Option<String> = None;
        for (index, step) in scenario.steps.iter().enumerate() {
            if cancel.is_cancelled() {
                return Outcome::Skipped("stopped".to_owned());
            }
            let result = match step {
                Step::Catalog {
                    entry_id,
                    action,
                    params_json,
                } => {
                    self.run_catalog_step(catalog.as_deref(), entry_id, action, params_json)
                        .await
                }
                Step::Speak { text } => self.run_speak_step(catalog.as_deref(), text).await,
                Step::Delay { seconds } => {
                    tokio::select! {
                        () = tokio::time::sleep(Duration::from_secs(
                            u64::try_from(*seconds).unwrap_or(0),
                        )) => Ok(()),
                        () = cancel.cancelled() => return Outcome::Skipped("stopped".to_owned()),
                    }
                }
            };
            if let Err(reason) = result {
                let detail = format!("step {}: {}", index + 1, reason);
                tracing::warn!(
                    scenario = %scenario.id,
                    step = index + 1,
                    %detail,
                    "scenario step failed"
                );
                if first_error.is_none() {
                    first_error = Some(detail);
                }
                if !scenario.continue_on_error {
                    return Outcome::Failed(first_error.unwrap_or_default());
                }
            }
        }
        first_error.map_or_else(
            || Outcome::Delivered("scenario completed".to_owned()),
            Outcome::Failed,
        )
    }

    async fn run_catalog_step(
        &self,
        catalog: Option<&Catalog>,
        entry_id: &str,
        action: &str,
        params_json: &str,
    ) -> Result<(), String> {
        let catalog = catalog.ok_or_else(|| "catalog is unavailable for this step".to_owned())?;
        let entry = catalog.lookup(entry_id).ok_or_else(|| {
            format!(
                "entry {:?} is not in the catalog",
                sanitize_detail(entry_id)
            )
        })?;
        let params = parse_params(params_json)?;
        let call = entry
            .resolve(action, &params)
            .map_err(|error| sanitize_detail(&format!("action {action:?}: {error}")))?;
        self.call_service(&call, entry_id).await
    }

    async fn run_speak_step(&self, catalog: Option<&Catalog>, text: &str) -> Result<(), String> {
        let catalog = catalog.ok_or_else(|| "catalog is unavailable for this step".to_owned())?;
        let entry = catalog
            .lookup(catalog::SPEAK_ENTRY_ID)
            .ok_or_else(|| "speak action is not available in the catalog".to_owned())?;
        let params = Params {
            text: Some(text.to_owned()),
            ..Params::default()
        };
        // ResolveError and SpeakError messages never contain the text.
        let call = entry
            .resolve(catalog::ACTION_SPEAK, &params)
            .map_err(|error| sanitize_detail(&error.to_string()))?;
        self.call_service(&call, catalog::SPEAK_ENTRY_ID).await
    }

    async fn call_service(&self, call: &ServiceCall, entry_id: &str) -> Result<(), String> {
        if let Err(error) = self
            .ha
            .call_service(&call.domain, &call.service, &call.entity, call.data.clone())
            .await
        {
            let status = error_status(&error);
            let detail = ha_error_detail(&error);
            if error.kind() == ErrorKind::Permanent && !matches!(status, Some(401 | 403)) {
                return Err(format!(
                    "home assistant rejected {}: {}",
                    sanitize_detail(entry_id),
                    sanitize_detail(&detail)
                ));
            }
            return Err(format!(
                "home assistant call for {} failed: {}",
                sanitize_detail(entry_id),
                sanitize_detail(&detail)
            ));
        }
        tracing::info!(
            entry = %entry_id,
            service = %format!("{}.{}", call.domain, call.service),
            "scenario step accepted by home assistant"
        );
        Ok(())
    }

    fn lock_running(&self) -> std::sync::MutexGuard<'_, HashMap<String, CancellationToken>> {
        self.running.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

/// Releases a running slot when the fire task ends, including an early return.
struct RunningGuard<'a, H: ScenarioHa> {
    executor: &'a Executor<H>,
    scenario_id: String,
}

impl<H: ScenarioHa> Drop for RunningGuard<'_, H> {
    fn drop(&mut self) {
        self.executor.finish(&self.scenario_id);
    }
}

/// Why conditions could not be evaluated.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ConditionError {
    Unavailable,
}

/// The `scenario.fire` payload. A time trigger carries `trigger_index`; a
/// manual run carries `"trigger": "manual"`.
#[derive(Debug, Default, Deserialize)]
struct FirePayload {
    #[serde(default)]
    scenario_id: String,
    #[serde(default)]
    trigger_index: Option<i64>,
    #[serde(default)]
    trigger: Option<String>,
}

impl FirePayload {
    fn is_manual(&self) -> bool {
        self.trigger.as_deref() == Some("manual")
    }
}

fn payload(event: &Event) -> Option<FirePayload> {
    serde_json::from_str(&event.payload).ok()
}

fn scenario_task_type() -> &'static str {
    "scenario.fire"
}

fn parse_params(params_json: &str) -> Result<Params, String> {
    if params_json.trim().is_empty() {
        return Ok(Params::default());
    }
    serde_json::from_str(params_json).map_err(|_| "invalid step parameters".to_owned())
}

fn source_label(source: Source) -> &'static str {
    match source {
        Source::Alarm => "alarm",
        Source::Reminder => "reminder",
        Source::Timer => "timer",
        Source::Scenario => "scenario",
        Source::Task => "task",
        Source::Unspecified => "",
        Source::Other(_) => "unknown",
    }
}

fn error_status(error: &home_ha::Error) -> Option<u16> {
    match error {
        home_ha::Error::Status { status, .. } => Some(*status),
        _ => None,
    }
}

/// Render an HA failure like the control executor: operation and status only,
/// never a response body.
fn ha_error_detail(error: &home_ha::Error) -> String {
    match error {
        home_ha::Error::Status { op, status, .. } => format!("{op}: status={status}"),
        home_ha::Error::Transport { op, .. } => format!("{op}: request failed"),
        home_ha::Error::Decode { op } => format!("{op}: invalid response"),
        home_ha::Error::Invalid { op, detail } => format!("{op}: {detail}"),
        home_ha::Error::Certificate { op } => format!("{op}: unusable CA"),
        home_ha::Error::WebSocket { op } => format!("{op}: websocket failed"),
    }
}

/// Collapse whitespace and bound the text by characters, never bytes.
fn sanitize_detail(text: &str) -> String {
    let joined = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if joined.chars().count() <= 200 {
        return joined;
    }
    let end = joined
        .char_indices()
        .nth(200)
        .map_or(joined.len(), |(index, _)| index);
    joined[..end].to_owned()
}

#[cfg(test)]
mod tests {
    use std::future::Future;
    use std::sync::{Arc, Mutex};

    use home_ha::Error;
    use home_worker::{Event, Outcome, Source as EventSource};
    use serde_json::{Map, Value};
    use tokio_util::sync::CancellationToken;

    use super::{Executor, ScenarioHa, ScenarioSource};
    use crate::catalog::{Catalog, Raw, Rules, build};
    use crate::discovery::{CatalogSource, DiscoveryError, Snapshot};
    use crate::scenario::ledger::{RunState, ScenarioLedger};
    use crate::scenario::{Condition, Definition, Step, WallTime};
    use crate::scenario_config::ScenarioConfig;

    type Calls = Vec<(String, String, String, Map<String, Value>)>;

    #[derive(Default)]
    struct FakeHa {
        calls: Mutex<Calls>,
        states: Mutex<Map<String, Value>>,
        status: Option<u16>,
        state_error: bool,
        state_status: Option<u16>,
    }

    impl ScenarioHa for FakeHa {
        async fn state(&self, entity: &str) -> Result<String, Error> {
            if let Some(status) = self.state_status {
                return Err(Error::Status {
                    op: "state",
                    status,
                    message: String::new(),
                });
            }
            if self.state_error {
                return Err(Error::Invalid {
                    op: "state",
                    detail: "down".to_owned(),
                });
            }
            Ok(self
                .states
                .lock()
                .expect("lock")
                .get(entity)
                .and_then(Value::as_str)
                .unwrap_or("unknown")
                .to_owned())
        }

        async fn call_service(
            &self,
            domain: &str,
            service: &str,
            entity: &str,
            data: Map<String, Value>,
        ) -> Result<(), Error> {
            self.calls.lock().expect("lock").push((
                domain.to_owned(),
                service.to_owned(),
                entity.to_owned(),
                data,
            ));
            self.status.map_or(Ok(()), |status| {
                Err(Error::Status {
                    op: "call service",
                    status,
                    message: String::new(),
                })
            })
        }
    }

    struct FakeScenarios {
        definition: Result<Option<Definition>, super::ScenarioLoadError>,
    }

    impl ScenarioSource for FakeScenarios {
        fn get(
            &self,
            _id: &str,
        ) -> std::pin::Pin<
            Box<
                dyn Future<Output = Result<Option<Definition>, super::ScenarioLoadError>>
                    + Send
                    + '_,
            >,
        > {
            let definition = self.definition.clone();
            Box::pin(async move { definition })
        }
    }

    struct FakeCatalog {
        snapshot: Result<Snapshot, DiscoveryError>,
    }

    impl CatalogSource for FakeCatalog {
        fn get(
            &self,
        ) -> std::pin::Pin<Box<dyn Future<Output = Result<Snapshot, DiscoveryError>> + Send + '_>>
        {
            let snapshot = self.snapshot.clone();
            Box::pin(async move { snapshot })
        }
    }

    fn catalog() -> Arc<Catalog> {
        Arc::new(Catalog::new(build(
            &[
                Raw {
                    entity: "scene.focus".to_owned(),
                    state: "x".to_owned(),
                    name: "Focus".to_owned(),
                    ..Raw::default()
                },
                Raw {
                    entity: "light.lamp".to_owned(),
                    state: "on".to_owned(),
                    name: "Lamp".to_owned(),
                    ..Raw::default()
                },
                Raw {
                    entity: crate::catalog::SPEAK_SCRIPT.to_owned(),
                    state: "off".to_owned(),
                    name: "Speak text".to_owned(),
                    ..Raw::default()
                },
                Raw {
                    entity: "assist_satellite.station_satellite".to_owned(),
                    state: "idle".to_owned(),
                    name: "Station Assist satellite".to_owned(),
                    device_id: "station-1".to_owned(),
                    device: "Station".to_owned(),
                    ..Raw::default()
                },
            ],
            &Rules {
                scenario_label: "scenario".to_owned(),
                profile_label: "voice_profile".to_owned(),
                exclude: Vec::new(),
            },
        )))
    }

    fn config() -> ScenarioConfig {
        ScenarioConfig::load_from(
            &|key| match key {
                "HA_URL" => Some("http://ha.example:8123".to_owned()),
                "HA_TOKEN" => Some("ha-secret".to_owned()),
                "SCHEDULER_ENDPOINT" => Some("http://scheduler.example:8098".to_owned()),
                "SCHEDULER_TOKEN" => Some("scheduler-secret".to_owned()),
                "SCENARIO_CONSUMER_ID" => Some("ha-scenario-home".to_owned()),
                _ => None,
            },
            &|_| Ok(Vec::new()),
        )
        .expect("config")
    }

    fn make_executor(
        definition: Result<Option<Definition>, super::ScenarioLoadError>,
        snapshot: Result<Snapshot, DiscoveryError>,
        ha: Arc<FakeHa>,
    ) -> Executor<FakeHa> {
        Executor::new(
            Arc::new(config()),
            Arc::new(FakeScenarios { definition }),
            Arc::new(FakeCatalog { snapshot }),
            ha,
            Arc::new(ScenarioLedger::memory()),
        )
        .with_clock(|| 1_800_000_000)
    }

    fn with_snapshot(definition: Definition) -> Executor<FakeHa> {
        make_executor(
            Ok(Some(definition)),
            Ok(Snapshot {
                catalog: catalog(),
                states_ok: true,
            }),
            Arc::new(FakeHa::default()),
        )
    }

    fn fire(id: &str, payload: &str) -> Event {
        Event {
            id: id.to_owned(),
            source_id: "sc_1".to_owned(),
            task_type: "scenario.fire".to_owned(),
            queue: "scenario".to_owned(),
            label: "evening".to_owned(),
            payload: payload.to_owned(),
            action: home_worker::Action::Fire,
            source: EventSource::Scenario,
            attempts: 1,
            due_unix: 0,
            occurrence_unix: 0,
            lease_until_unix: 0,
        }
    }

    fn stop(id: &str, payload: &str) -> Event {
        Event {
            action: home_worker::Action::Stop,
            ..fire(id, payload)
        }
    }

    fn manual(scenario: &str) -> String {
        format!(r#"{{"scenario_id":"{scenario}","trigger":"manual"}}"#)
    }

    fn timed(scenario: &str) -> String {
        format!(r#"{{"scenario_id":"{scenario}","trigger_index":0}}"#)
    }

    fn time_trigger() -> crate::scenario::Trigger {
        crate::scenario::Trigger::Time {
            timezone: "Asia/Almaty".to_owned(),
        }
    }

    fn base(steps: Vec<Step>) -> Definition {
        Definition {
            id: "sc_1".to_owned(),
            label: "evening".to_owned(),
            enabled: true,
            triggers: vec![time_trigger()],
            conditions: Vec::new(),
            steps,
            continue_on_error: false,
        }
    }

    fn catalog_step(entry: &str, action: &str, params: &str) -> Step {
        Step::Catalog {
            entry_id: entry.to_owned(),
            action: action.to_owned(),
            params_json: params.to_owned(),
        }
    }

    #[tokio::test]
    async fn catalog_and_speak_steps_resolve_through_the_live_catalog() {
        let definition = base(vec![
            catalog_step("scene.focus", "run", ""),
            Step::Speak {
                text: "  Добрый вечер.  ".to_owned(),
            },
        ]);
        let ha = Arc::new(FakeHa::default());
        let executor = make_executor(
            Ok(Some(definition)),
            Ok(Snapshot {
                catalog: catalog(),
                states_ok: true,
            }),
            Arc::clone(&ha),
        );
        let outcome = executor.handle(&fire("ev_1", &timed("sc_1"))).await;
        assert_eq!(outcome, Outcome::Delivered("scenario completed".to_owned()));
        let calls = ha.calls.lock().expect("lock");
        assert_eq!(calls.len(), 2);
        assert_eq!(calls[0].0, "scene");
        assert_eq!(calls[0].1, "turn_on");
        assert_eq!(calls[1].0, "script");
        assert_eq!(calls[1].2, crate::catalog::SPEAK_SCRIPT);
        assert_eq!(
            calls[1].3["variables"]["message"],
            Value::from("Добрый вечер.")
        );
    }

    #[tokio::test]
    async fn an_unknown_entry_fails_with_the_step_number_and_no_speak_text() {
        let definition = base(vec![
            catalog_step("script.alarm_stop", "run", ""),
            Step::Speak {
                text: "совершенно секретный текст".to_owned(),
            },
        ]);
        let executor = with_snapshot(definition);
        let outcome = executor.handle(&fire("ev_1", &timed("sc_1"))).await;
        match outcome {
            Outcome::Failed(detail) => {
                assert!(detail.starts_with("step 1: "), "{detail}");
                assert!(!detail.contains("секретный"), "leaked text: {detail}");
            }
            other => panic!("{other:?}"),
        }
    }

    #[tokio::test]
    async fn invalid_speak_text_fails_without_leaking_the_text() {
        let secret = "очень секретный текст";
        for text in [
            format!("{secret}\0"),
            format!("{secret}{}", "ж".repeat(500)),
        ] {
            let executor = with_snapshot(base(vec![Step::Speak { text }]));
            match executor.handle(&fire("ev_1", &timed("sc_1"))).await {
                Outcome::Failed(detail) => {
                    assert!(detail.starts_with("step 1: "), "{detail}");
                    assert!(!detail.contains(secret), "leaked text: {detail}");
                }
                other => panic!("{other:?}"),
            }
        }
    }

    #[tokio::test]
    async fn invalid_params_fail_the_step_without_leaking_values() {
        let definition = base(vec![catalog_step(
            "light.lamp",
            "set",
            r#"{"brightness":"очень секретно"}"#,
        )]);
        let executor = with_snapshot(definition);
        let outcome = executor.handle(&fire("ev_1", &timed("sc_1"))).await;
        match outcome {
            Outcome::Failed(detail) => {
                assert_eq!(detail, "step 1: invalid step parameters");
            }
            other => panic!("{other:?}"),
        }
    }

    #[tokio::test]
    async fn continue_on_error_runs_every_step_and_reports_the_first_failure() {
        let definition = Definition {
            continue_on_error: true,
            steps: vec![
                catalog_step("script.alarm_stop", "run", ""),
                catalog_step("scene.focus", "run", ""),
            ],
            ..base(Vec::new())
        };
        let ha = Arc::new(FakeHa::default());
        let executor = make_executor(
            Ok(Some(definition)),
            Ok(Snapshot {
                catalog: catalog(),
                states_ok: true,
            }),
            Arc::clone(&ha),
        );
        let outcome = executor.handle(&fire("ev_1", &timed("sc_1"))).await;
        match outcome {
            Outcome::Failed(detail) => assert!(detail.starts_with("step 1: "), "{detail}"),
            other => panic!("{other:?}"),
        }
        // The second step still ran.
        assert_eq!(ha.calls.lock().expect("lock").len(), 1);
    }

    #[tokio::test]
    async fn a_stop_cancels_a_delay_and_acknowledges_delivered() {
        let definition = base(vec![Step::Delay { seconds: 60 }]);
        let executor = Arc::new(with_snapshot(definition));
        let event = fire("ev_1", &timed("sc_1"));
        let running = Arc::clone(&executor);
        let handle = tokio::spawn(async move { running.handle(&event).await });
        // Wait until the run holds the slot, then stop it.
        for _ in 0..100 {
            if executor.running.lock().expect("lock").contains_key("sc_1") {
                break;
            }
            tokio::task::yield_now().await;
        }
        let stop_event = stop("ev_2", &timed("sc_1"));
        assert_eq!(
            executor.handle(&stop_event).await,
            Outcome::Delivered("scenario stopped".to_owned())
        );
        assert_eq!(
            handle.await.expect("join"),
            Outcome::Skipped("stopped".to_owned())
        );
    }

    #[tokio::test]
    async fn a_second_fire_while_running_is_skipped() {
        let executor = with_snapshot(base(vec![Step::Delay { seconds: 1 }]));
        let _token = executor.begin("sc_1").expect("first claim");
        assert!(executor.begin("sc_1").is_none());
        assert_eq!(
            executor.handle(&fire("ev_1", &timed("sc_1"))).await,
            Outcome::Skipped("already running".to_owned())
        );
    }

    #[tokio::test]
    async fn a_stop_for_an_idle_scenario_is_skipped() {
        let executor = with_snapshot(base(Vec::new()));
        assert_eq!(
            executor.handle(&stop("ev_2", &timed("sc_1"))).await,
            Outcome::Skipped("scenario is not running".to_owned())
        );
    }

    #[tokio::test]
    async fn an_interrupted_run_is_failed_instead_of_replayed() {
        let ledger = Arc::new(ScenarioLedger::memory());
        ledger.start("ev_1", "sc_1").expect("start");
        let ha = Arc::new(FakeHa::default());
        let executor = Executor::new(
            Arc::new(config()),
            Arc::new(FakeScenarios {
                definition: Ok(Some(base(vec![catalog_step("scene.focus", "run", "")]))),
            }),
            Arc::new(FakeCatalog {
                snapshot: Ok(Snapshot {
                    catalog: catalog(),
                    states_ok: true,
                }),
            }),
            Arc::clone(&ha),
            ledger,
        );
        assert_eq!(
            executor.handle(&fire("ev_1", &timed("sc_1"))).await,
            Outcome::Failed("interrupted".to_owned())
        );
        assert!(ha.calls.lock().expect("lock").is_empty());
    }

    #[tokio::test]
    async fn a_completed_event_is_not_replayed() {
        let ledger = Arc::new(ScenarioLedger::memory());
        ledger.start("ev_1", "sc_1").expect("start");
        ledger.complete("ev_1").expect("complete");
        let executor = Executor::new(
            Arc::new(config()),
            Arc::new(FakeScenarios {
                definition: Ok(Some(base(vec![catalog_step("scene.focus", "run", "")]))),
            }),
            Arc::new(FakeCatalog {
                snapshot: Ok(Snapshot {
                    catalog: catalog(),
                    states_ok: true,
                }),
            }),
            Arc::new(FakeHa::default()),
            ledger,
        );
        assert_eq!(
            executor.handle(&fire("ev_1", &timed("sc_1"))).await,
            Outcome::Skipped("already completed".to_owned())
        );
    }

    #[tokio::test]
    async fn a_disabled_scenario_skips_time_triggers_but_runs_manually() {
        let mut definition = base(vec![catalog_step("scene.focus", "run", "")]);
        definition.enabled = false;
        let executor = with_snapshot(definition);
        assert_eq!(
            executor.handle(&fire("ev_1", &timed("sc_1"))).await,
            Outcome::Skipped("scenario is disabled".to_owned())
        );
        assert_eq!(
            executor.handle(&fire("ev_2", &manual("sc_1"))).await,
            Outcome::Delivered("scenario completed".to_owned())
        );
    }

    #[tokio::test]
    async fn entity_state_conditions_gate_execution() {
        let mut definition = base(vec![catalog_step("scene.focus", "run", "")]);
        definition.conditions = vec![Condition::EntityState {
            entity_id: "light.lamp".to_owned(),
            state: "on".to_owned(),
        }];
        let ha = Arc::new(FakeHa::default());
        ha.states
            .lock()
            .expect("lock")
            .insert("light.lamp".to_owned(), Value::from("off"));
        let executor = make_executor(
            Ok(Some(definition)),
            Ok(Snapshot {
                catalog: catalog(),
                states_ok: true,
            }),
            Arc::clone(&ha),
        );
        assert_eq!(
            executor.handle(&fire("ev_1", &timed("sc_1"))).await,
            Outcome::Skipped("condition not met".to_owned())
        );
        assert!(ha.calls.lock().expect("lock").is_empty());
    }

    #[tokio::test]
    async fn a_missing_entity_means_the_condition_does_not_hold() {
        let mut definition = base(vec![catalog_step("scene.focus", "run", "")]);
        definition.conditions = vec![Condition::EntityState {
            entity_id: "light.lamp".to_owned(),
            state: "on".to_owned(),
        }];
        let ha = Arc::new(FakeHa {
            state_status: Some(404),
            ..FakeHa::default()
        });
        let executor = make_executor(
            Ok(Some(definition)),
            Ok(Snapshot {
                catalog: catalog(),
                states_ok: true,
            }),
            Arc::clone(&ha),
        );
        assert_eq!(
            executor.handle(&fire("ev_1", &timed("sc_1"))).await,
            Outcome::Skipped("condition not met".to_owned())
        );
    }

    #[tokio::test]
    async fn time_window_conditions_use_the_scenario_timezone() {
        // 2026-11-09 12:00 Asia/Almaty = 1_794_...; the clock is injected below.
        let mut definition = base(vec![catalog_step("scene.focus", "run", "")]);
        definition.conditions = vec![Condition::TimeWindow {
            start: WallTime {
                hour: 9,
                minute: 0,
                second: 0,
            },
            end: WallTime {
                hour: 18,
                minute: 0,
                second: 0,
            },
            weekdays: Vec::new(),
        }];
        let ha = Arc::new(FakeHa::default());
        let at_noon = jiff::civil::date(2026, 11, 9)
            .at(12, 0, 0, 0)
            .to_zoned(jiff::tz::TimeZone::get("Asia/Almaty").expect("zone"))
            .expect("zoned")
            .timestamp()
            .as_second();
        let executor = Executor::new(
            Arc::new(config()),
            Arc::new(FakeScenarios {
                definition: Ok(Some(definition)),
            }),
            Arc::new(FakeCatalog {
                snapshot: Ok(Snapshot {
                    catalog: catalog(),
                    states_ok: true,
                }),
            }),
            Arc::clone(&ha),
            Arc::new(ScenarioLedger::memory()),
        )
        .with_clock(move || at_noon);
        assert_eq!(
            executor.handle(&fire("ev_1", &timed("sc_1"))).await,
            Outcome::Delivered("scenario completed".to_owned())
        );
    }

    #[tokio::test]
    async fn an_unavailable_scenario_service_or_ha_retries_before_the_first_step() {
        let executor = make_executor(
            Err(super::ScenarioLoadError { transient: true }),
            Ok(Snapshot {
                catalog: catalog(),
                states_ok: true,
            }),
            Arc::new(FakeHa::default()),
        );
        assert_eq!(
            executor.handle(&fire("ev_1", &timed("sc_1"))).await,
            Outcome::Retry
        );
        let mut event = fire("ev_1", &timed("sc_1"));
        event.attempts = 10;
        assert_eq!(
            executor.handle(&event).await,
            Outcome::Failed("scenario service unavailable".to_owned())
        );

        let ha = Arc::new(FakeHa {
            state_error: true,
            ..FakeHa::default()
        });
        let mut definition = base(vec![catalog_step("scene.focus", "run", "")]);
        definition.conditions = vec![Condition::EntityState {
            entity_id: "light.lamp".to_owned(),
            state: "on".to_owned(),
        }];
        let executor = make_executor(
            Ok(Some(definition)),
            Ok(Snapshot {
                catalog: catalog(),
                states_ok: true,
            }),
            Arc::clone(&ha),
        );
        assert_eq!(
            executor.handle(&fire("ev_1", &timed("sc_1"))).await,
            Outcome::Retry
        );
    }

    #[tokio::test]
    async fn a_catalog_outage_retries_before_the_first_step() {
        let executor = make_executor(
            Ok(Some(base(vec![catalog_step("scene.focus", "run", "")]))),
            Err(DiscoveryError::Unavailable),
            Arc::new(FakeHa::default()),
        );
        assert_eq!(
            executor.handle(&fire("ev_1", &timed("sc_1"))).await,
            Outcome::Retry
        );
    }

    #[tokio::test]
    async fn a_missing_scenario_is_skipped() {
        let executor = make_executor(
            Ok(None),
            Ok(Snapshot {
                catalog: catalog(),
                states_ok: true,
            }),
            Arc::new(FakeHa::default()),
        );
        assert_eq!(
            executor.handle(&fire("ev_1", &timed("sc_1"))).await,
            Outcome::Skipped("scenario not found".to_owned())
        );
    }

    #[tokio::test]
    async fn retry_keeps_the_ledger_clean_until_the_first_step() {
        let ledger = Arc::new(ScenarioLedger::memory());
        let executor = Executor::new(
            Arc::new(config()),
            Arc::new(FakeScenarios {
                definition: Ok(Some(base(vec![catalog_step("scene.focus", "run", "")]))),
            }),
            Arc::new(FakeCatalog {
                snapshot: Err(DiscoveryError::Unavailable),
            }),
            Arc::new(FakeHa::default()),
            Arc::clone(&ledger),
        );
        assert_eq!(
            executor.handle(&fire("ev_1", &timed("sc_1"))).await,
            Outcome::Retry
        );
        assert!(ledger.is_empty());
        assert_eq!(ledger.state("ev_1"), None);
    }

    #[tokio::test]
    async fn completed_runs_are_marked_in_the_ledger() {
        let ledger = Arc::new(ScenarioLedger::memory());
        let executor = Executor::new(
            Arc::new(config()),
            Arc::new(FakeScenarios {
                definition: Ok(Some(base(vec![catalog_step("scene.focus", "run", "")]))),
            }),
            Arc::new(FakeCatalog {
                snapshot: Ok(Snapshot {
                    catalog: catalog(),
                    states_ok: true,
                }),
            }),
            Arc::new(FakeHa::default()),
            Arc::clone(&ledger),
        );
        executor.handle(&fire("ev_1", &timed("sc_1"))).await;
        assert_eq!(ledger.state("ev_1"), Some(RunState::Completed));
    }

    #[tokio::test]
    async fn a_wrong_queue_or_task_type_is_rejected_without_side_effects() {
        let executor = with_snapshot(base(vec![catalog_step("scene.focus", "run", "")]));
        let mut event = fire("ev_1", &timed("sc_1"));
        event.queue = "home".to_owned();
        assert!(matches!(executor.handle(&event).await, Outcome::Failed(_)));
        let mut event = fire("ev_2", &timed("sc_1"));
        event.task_type = "home.control".to_owned();
        assert!(matches!(executor.handle(&event).await, Outcome::Failed(_)));
    }

    #[test]
    fn cancel_reports_whether_a_run_was_active() {
        let executor = with_snapshot(base(Vec::new()));
        assert!(!executor.cancel("sc_1"));
        let token = executor.begin("sc_1").expect("begin");
        assert!(executor.cancel("sc_1"));
        assert!(token.is_cancelled());
        executor.finish("sc_1");
        assert!(!executor.cancel("sc_1"));
    }

    #[test]
    fn a_cancellation_token_is_only_removed_by_finish() {
        let executor = with_snapshot(base(Vec::new()));
        let _token: CancellationToken = executor.begin("sc_1").expect("begin");
        assert!(executor.begin("sc_1").is_none());
        executor.finish("sc_1");
        assert!(executor.begin("sc_1").is_some());
    }
}
