//! Typed client of the bb home-dispatcher and the personal-only allow-list.
//!
//! Ports `solutions/mcp-gateway/internal/bb/{client,targets}.go`. Every call is
//! one SSH session running the forced command `bb-dispatch`: one JSON object on
//! stdin, one JSON answer on stdout. The wire methods are closed:
//! `targets`, `status`, `validate_spawn`, `spawn`, `send`, `turn_result` and
//! `updates`.
//!
//! The gateway never speaks HTTP to bb and never has a shell. The bridge owns
//! the plugin token and the HMAC secret; the gateway owns only an ed25519 key
//! restricted to that bridge and a pinned `known_hosts` file ([`crate::ssh`]).
//!
//! [`BbResolver`] maps a spoken project name onto a *personal* target from the
//! plugin's live list. A work-only project and an unknown alias are
//! deliberately indistinguishable, so Hermes can never learn that a work
//! project exists.

use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::time::Duration;

use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::ssh::{Answer, SshRunner};

/// Boxed future used by the object-safe bridge traits.
pub type BbFuture<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

/// Spoken when the bridge cannot be reached.
pub const SPEECH_UNAVAILABLE: &str = "Сервер агентов недоступен.";
/// Spoken when the bridge is not configured.
pub const SPEECH_NOT_READY: &str = "Сервер агентов не настроен.";

/// The only profile the MCP tools and Hermes may use.
pub const PROFILE_PERSONAL: &str = "personal";

/// Provider used when a target entry does not pin one.
pub const DEFAULT_PROVIDER: &str = "claude-code";

/// Default per-call bridge timeout.
pub const DEFAULT_TIMEOUT: Duration = Duration::from_secs(15);

/// Bridge method names. The list is closed.
pub mod method {
    /// List spawnable projects.
    pub const TARGETS: &str = "targets";
    /// Recent and running threads.
    pub const STATUS: &str = "status";
    /// Validate a spawn without creating a thread.
    pub const VALIDATE_SPAWN: &str = "validate_spawn";
    /// Create a thread.
    pub const SPAWN: &str = "spawn";
    /// Append a message to a thread.
    pub const SEND: &str = "send";
    /// Read one turn's outcome.
    pub const TURN_RESULT: &str = "turn_result";
    /// Poll completed dispatcher turns.
    pub const UPDATES: &str = "updates";
}

/// A refused or failed bridge call.
///
/// `code` is the plugin/bridge code and `speech` is the short spoken line. It
/// never contains key material or a payload.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Error {
    /// Stable machine code.
    pub code: String,
    /// Short Russian line to speak.
    pub speech: String,
}

impl Error {
    /// Builds an error with an explicit code and spoken line.
    #[must_use]
    pub fn new(code: impl Into<String>, speech: impl Into<String>) -> Self {
        Self {
            code: code.into(),
            speech: speech.into(),
        }
    }

    /// The generic "server unavailable" error.
    #[must_use]
    pub fn unavailable() -> Self {
        Self::new("bridge_error", SPEECH_UNAVAILABLE)
    }
}

impl std::fmt::Display for Error {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "bb: {}: {}", self.code, self.speech)
    }
}

impl std::error::Error for Error {}

/// The short spoken line of a bridge error.
#[must_use]
pub fn speech_for_error(error: &Error) -> &str {
    if error.speech.is_empty() {
        SPEECH_UNAVAILABLE
    } else {
        &error.speech
    }
}

/// Sends one request to the bridge and returns stdout plus the exit code.
///
/// A non-zero exit code still carries a JSON answer from the bridge. The trait
/// keeps the dispatcher testable without a real SSH server; [`SshRunner`] is
/// the production implementation.
pub trait Runner: Send + Sync {
    /// Run one forced-command call.
    fn run<'a>(&'a self, payload: &'a [u8]) -> BbFuture<'a, Result<Answer, crate::ssh::Error>>;
}

impl Runner for SshRunner {
    fn run<'a>(&'a self, payload: &'a [u8]) -> BbFuture<'a, Result<Answer, crate::ssh::Error>> {
        Box::pin(Self::run(self, payload))
    }
}

/// The typed dispatcher API on top of one [`Runner`].
pub struct Dispatcher {
    runner: Arc<dyn Runner>,
    timeout: Duration,
}

impl Dispatcher {
    /// Builds a dispatcher. A zero timeout defaults to 15 s.
    #[must_use]
    pub fn new(runner: Arc<dyn Runner>, timeout: Duration) -> Self {
        Self {
            runner,
            timeout: if timeout.is_zero() {
                DEFAULT_TIMEOUT
            } else {
                timeout
            },
        }
    }

    /// Performs one bridge call and returns the raw `result` value.
    pub async fn call(&self, method: &str, params: Value) -> Result<Value, Error> {
        let payload = serde_json::to_vec(&json!({"method": method, "params": params}))
            .map_err(|_| Error::new("invalid_request", "Некорректный запрос."))?;
        let answer = match tokio::time::timeout(self.timeout, self.runner.run(&payload)).await {
            Ok(Ok(answer)) => answer,
            Ok(Err(_)) => return Err(Error::unavailable()),
            Err(_) => return Err(Error::new("timeout", SPEECH_UNAVAILABLE)),
        };
        let parsed: Value =
            serde_json::from_slice(&answer.stdout).map_err(|_| Error::unavailable())?;
        let ok = parsed.get("ok").and_then(Value::as_bool).unwrap_or(false);
        if !ok || answer.exit_code.unwrap_or(0) != 0 {
            let code = parsed
                .get("code")
                .and_then(Value::as_str)
                .filter(|value| !value.is_empty())
                .unwrap_or("bridge_error");
            let speech = parsed
                .get("speech")
                .and_then(Value::as_str)
                .filter(|value| !value.is_empty())
                .unwrap_or(SPEECH_UNAVAILABLE);
            return Err(Error::new(code, speech));
        }
        Ok(parsed.get("result").cloned().unwrap_or(Value::Null))
    }

    async fn decode<T: DeserializeOwned>(&self, method: &str, params: Value) -> Result<T, Error> {
        let raw = self.call(method, params).await?;
        serde_json::from_value(raw).map_err(|_| Error::unavailable())
    }
}

/// The subset used by the MCP tools and the voice path.
pub trait BbApi: Send + Sync {
    /// Whether the bridge is configured.
    fn configured(&self) -> bool {
        true
    }
    /// Lists the projects the plugin can spawn into.
    fn targets(&self) -> BbFuture<'_, Result<Targets, Error>>;
    /// Returns recent and running threads.
    fn status(&self, limit: usize) -> BbFuture<'_, Result<StatusResult, Error>>;
    /// Validates a spawn without creating a thread.
    fn validate_spawn(&self, request: SpawnRequest) -> BbFuture<'_, Result<(), Error>>;
    /// Creates a visible thread; idempotent on `idempotency_key`.
    fn spawn(&self, request: SpawnRequest) -> BbFuture<'_, Result<SpawnResult, Error>>;
    /// Appends a message to a thread.
    fn send(&self, request: SendRequest) -> BbFuture<'_, Result<SendResult, Error>>;
    /// Reads the outcome of the turn that follows `after_seq`.
    fn turn_result(
        &self,
        request: TurnResultRequest,
    ) -> BbFuture<'_, Result<TurnResultResult, Error>>;
    /// Polls completed dispatcher turns the owner did not start.
    fn updates(&self, request: UpdatesRequest) -> BbFuture<'_, Result<UpdatesResult, Error>>;
}

impl BbApi for Dispatcher {
    fn targets(&self) -> BbFuture<'_, Result<Targets, Error>> {
        Box::pin(async move { self.decode(method::TARGETS, json!({})).await })
    }

    fn status(&self, limit: usize) -> BbFuture<'_, Result<StatusResult, Error>> {
        Box::pin(async move { self.decode(method::STATUS, json!({"limit": limit})).await })
    }

    fn validate_spawn(&self, request: SpawnRequest) -> BbFuture<'_, Result<(), Error>> {
        Box::pin(async move {
            let params = serde_json::to_value(request.target())
                .map_err(|_| Error::new("invalid_request", "Некорректный запрос."))?;
            let _: Value = self.decode(method::VALIDATE_SPAWN, params).await?;
            Ok(())
        })
    }

    fn spawn(&self, request: SpawnRequest) -> BbFuture<'_, Result<SpawnResult, Error>> {
        Box::pin(async move {
            let params = serde_json::to_value(request)
                .map_err(|_| Error::new("invalid_request", "Некорректный запрос."))?;
            self.decode(method::SPAWN, params).await
        })
    }

    fn send(&self, request: SendRequest) -> BbFuture<'_, Result<SendResult, Error>> {
        Box::pin(async move {
            let params = serde_json::to_value(request)
                .map_err(|_| Error::new("invalid_request", "Некорректный запрос."))?;
            self.decode(method::SEND, params).await
        })
    }

    fn turn_result(
        &self,
        request: TurnResultRequest,
    ) -> BbFuture<'_, Result<TurnResultResult, Error>> {
        Box::pin(async move {
            let params = serde_json::to_value(request)
                .map_err(|_| Error::new("invalid_request", "Некорректный запрос."))?;
            self.decode(method::TURN_RESULT, params).await
        })
    }

    fn updates(&self, request: UpdatesRequest) -> BbFuture<'_, Result<UpdatesResult, Error>> {
        Box::pin(async move {
            let params = serde_json::to_value(request)
                .map_err(|_| Error::new("invalid_request", "Некорректный запрос."))?;
            self.decode(method::UPDATES, params).await
        })
    }
}

// --- wire types -----------------------------------------------------------

/// The `targets` result.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Targets {
    /// Spawnable projects.
    #[serde(default)]
    pub projects: Vec<Target>,
}

/// One project the plugin can spawn into.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Target {
    /// Plugin project id.
    #[serde(default)]
    pub project_id: String,
    /// Human project name.
    #[serde(default)]
    pub name: String,
    /// Profile the project belongs to.
    #[serde(default)]
    pub profile: String,
    /// Effective profile policy.
    #[serde(default)]
    pub policy: Policy,
    /// Source machine of the project.
    #[serde(default)]
    pub machine: Machine,
    /// Providers available on the machine.
    #[serde(default)]
    pub providers: Vec<Provider>,
}

/// The effective profile policy from the profiles plugin.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Policy {
    /// Allowed providers.
    #[serde(default)]
    pub providers: Vec<String>,
    /// Allowed machines.
    #[serde(default)]
    pub machines: Vec<String>,
}

/// The project's source machine.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Machine {
    /// Host id.
    #[serde(default)]
    pub host_id: String,
    /// Host name.
    #[serde(default)]
    pub name: String,
    /// Host status.
    #[serde(default)]
    pub status: String,
    /// Whether the host is online.
    #[serde(default)]
    pub online: bool,
}

/// One agent provider available on the project's machine.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Provider {
    /// Provider id.
    #[serde(default)]
    pub provider_id: String,
    /// Display name.
    #[serde(default)]
    pub display_name: String,
    /// Models of the provider.
    #[serde(default)]
    pub models: Vec<Model>,
}

/// One model plus its reasoning levels.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Model {
    /// Model id.
    #[serde(default)]
    pub model: String,
    /// Display name.
    #[serde(default)]
    pub display_name: String,
    /// Reasoning level used when none is requested.
    #[serde(default)]
    pub default_reasoning_level: String,
    /// Supported reasoning levels.
    #[serde(default)]
    pub reasoning_levels: Vec<String>,
}

/// The `spawn` params.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SpawnRequest {
    /// Target profile.
    pub profile: String,
    /// Target project id.
    pub project_id: String,
    /// Provider id.
    pub provider_id: String,
    /// Model id.
    pub model: String,
    /// Reasoning level.
    pub reasoning_level: String,
    /// Thread title.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub title: String,
    /// Initial prompt.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub prompt: String,
    /// Idempotency key.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub idempotency_key: String,
}

/// The whole of `validate_spawn`'s params and the common part of `spawn`'s.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SpawnTarget {
    profile: String,
    project_id: String,
    provider_id: String,
    model: String,
    reasoning_level: String,
}

impl SpawnRequest {
    /// Narrows the request to the fields `validate_spawn` accepts.
    #[must_use]
    pub fn target(&self) -> SpawnTarget {
        SpawnTarget {
            profile: self.profile.clone(),
            project_id: self.project_id.clone(),
            provider_id: self.provider_id.clone(),
            model: self.model.clone(),
            reasoning_level: self.reasoning_level.clone(),
        }
    }
}

/// The spawned thread identity.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SpawnResult {
    /// Created thread id.
    #[serde(default)]
    pub thread_id: String,
    /// Whether the plugin reused an existing thread for the same key.
    #[serde(default)]
    pub idempotent: bool,
}

/// The `send` params.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SendRequest {
    /// Destination thread.
    pub thread_id: String,
    /// Text to append.
    pub text: String,
    /// Idempotency key.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub idempotency_key: String,
}

/// The `send` result and the cursor for `turn_result`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SendResult {
    /// Destination thread.
    #[serde(default)]
    pub thread_id: String,
    /// Sequence the answer follows.
    #[serde(default)]
    pub after_seq: i64,
    /// Request sequence, when the plugin reports one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub request_seq: Option<i64>,
    /// Request id, when the plugin reports one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub request_id: Option<String>,
    /// Delivery mode, when the plugin reports one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub delivery: Option<String>,
}

/// The `turn_result` params.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TurnResultRequest {
    /// Thread to poll.
    pub thread_id: String,
    /// Sequence the turn follows.
    pub after_seq: i64,
}

/// The `updates` params. `after_seq == None` is the bootstrap call.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdatesRequest {
    /// Cursor to continue from; `None` for the bootstrap call.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub after_seq: Option<i64>,
    /// Plugin long-poll budget.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub wait_seconds: Option<i32>,
}

/// One completed non-owner turn and its spoken line.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateItem {
    /// Event sequence.
    pub seq: i64,
    /// Spoken line.
    pub spoken: String,
    /// Turn-completed event time in epoch milliseconds.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub completed_at: Option<i64>,
}

/// The `updates` answer. `cursor` is the latest event sequence of the thread.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdatesResult {
    /// Latest event sequence.
    #[serde(default)]
    pub cursor: i64,
    /// Completed turns after the requested cursor.
    #[serde(default)]
    pub items: Vec<UpdateItem>,
}

/// The `turn_result` answer.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TurnResultResult {
    /// Turn status (`running`, `idle`, `waiting_input`, `failed`, ...).
    #[serde(default)]
    pub status: String,
    /// Spoken line emitted by the agent, if any.
    #[serde(default)]
    pub spoken: Option<String>,
    /// Full answer text.
    #[serde(default)]
    pub text: String,
    /// Thread id.
    #[serde(default)]
    pub thread_id: String,
    /// Request sequence.
    #[serde(default)]
    pub request_seq: Option<i64>,
    /// Sequence of the completed turn; the voice proxy's idempotency key.
    #[serde(default)]
    pub completed_seq: Option<i64>,
    /// Turn reference.
    #[serde(default)]
    pub reference: String,
}

/// One entry of the `status` result.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Thread {
    /// Thread id.
    #[serde(default)]
    pub id: String,
    /// Thread title.
    #[serde(default)]
    pub title: String,
    /// Project name.
    #[serde(default)]
    pub project: String,
    /// Project id.
    #[serde(default)]
    pub project_id: String,
    /// Profile.
    #[serde(default)]
    pub profile: String,
    /// Thread status.
    #[serde(default)]
    pub status: String,
    /// Runtime in seconds, when the plugin reports one.
    #[serde(default)]
    pub runtime_seconds: Option<i64>,
}

/// The `status` answer.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct StatusResult {
    /// Known threads.
    #[serde(default)]
    pub threads: Vec<Thread>,
}

/// The pending value of `bb.act`: everything confirmed later.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SpawnAction {
    /// Resolved project name.
    pub project: String,
    /// Plugin project id.
    pub project_id: String,
    /// Provider id.
    pub provider_id: String,
    /// Model id.
    pub model: String,
    /// Reasoning level.
    pub reasoning_level: String,
    /// Thread title.
    pub title: String,
    /// Initial prompt.
    pub prompt: String,
    /// Idempotency key.
    pub idempotency_key: String,
}

impl SpawnAction {
    /// Converts the pending action into the plugin spawn request.
    #[must_use]
    pub fn request(&self) -> SpawnRequest {
        SpawnRequest {
            profile: PROFILE_PERSONAL.to_owned(),
            project_id: self.project_id.clone(),
            provider_id: self.provider_id.clone(),
            model: self.model.clone(),
            reasoning_level: self.reasoning_level.clone(),
            title: self.title.clone(),
            prompt: self.prompt.clone(),
            idempotency_key: self.idempotency_key.clone(),
        }
    }
}

// --- personal allow-list --------------------------------------------------

/// The tracked allow-list of spoken project names.
#[derive(Debug, Deserialize)]
struct TargetsFile {
    #[serde(default)]
    targets: Vec<TargetEntry>,
}

/// Maps spoken aliases to one project and its optional pinning.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
pub struct TargetEntry {
    /// Plugin project id or its exact name.
    #[serde(default)]
    pub project: String,
    /// Spoken names; at least one is required.
    #[serde(default)]
    pub aliases: Vec<String>,
    /// Pinned provider.
    #[serde(default)]
    pub provider: Option<String>,
    /// Pinned model.
    #[serde(default)]
    pub model: Option<String>,
    /// Pinned reasoning level.
    #[serde(default)]
    pub effort: Option<String>,
    /// Prefix of created thread titles.
    #[serde(default)]
    pub title_prefix: Option<String>,
}

/// The personal-only spoken allow-list.
#[derive(Debug, Clone, Default)]
pub struct BbResolver {
    entries: Vec<TargetEntry>,
    default_provider: String,
}

/// Why a spoken project could not be resolved.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResolveError {
    /// Unknown alias, or a project that is not a personal target.
    TargetNotFound,
    /// The project does not expose the provider.
    ProviderUnavailable,
    /// The requested model is not allowed by the plugin.
    ModelNotFound,
    /// The model does not support the requested reasoning level.
    EffortNotSupported,
}

impl std::fmt::Display for ResolveError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(match self {
            Self::TargetNotFound => "bb: target not found",
            Self::ProviderUnavailable => "bb: provider unavailable",
            Self::ModelNotFound => "bb: model not found",
            Self::EffortNotSupported => "bb: reasoning level not supported",
        })
    }
}

impl std::error::Error for ResolveError {}

/// A fully validated spawn target.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Resolution {
    /// Resolved project name.
    pub project: String,
    /// Plugin project id.
    pub project_id: String,
    /// Provider id.
    pub provider_id: String,
    /// Model id.
    pub model: String,
    /// Reasoning level.
    pub effort: String,
    /// Title prefix.
    pub title_prefix: String,
}

impl BbResolver {
    /// Reads and validates the allow-list file.
    pub fn load(path: &str) -> Result<Self, String> {
        let data = std::fs::read(path).map_err(|error| format!("bb targets: {error}"))?;
        Self::parse(&data)
    }

    /// Validates an allow-list document.
    pub fn parse(data: &[u8]) -> Result<Self, String> {
        let file: TargetsFile =
            serde_saphyr::from_slice(data).map_err(|error| format!("bb targets: {error}"))?;
        if file.targets.is_empty() {
            return Err("bb targets: at least one target is required".to_owned());
        }
        let mut seen: std::collections::BTreeMap<String, String> =
            std::collections::BTreeMap::new();
        let mut entries = Vec::with_capacity(file.targets.len());
        for mut entry in file.targets {
            let project = entry.project.trim().to_owned();
            entry.project = project;
            entry.provider = trimmed_option(entry.provider);
            entry.model = trimmed_option(entry.model);
            entry.effort = trimmed_option(entry.effort);
            entry.title_prefix = trimmed_option(entry.title_prefix);
            if entry.project.is_empty() {
                return Err("bb targets: project is required".to_owned());
            }
            if entry.aliases.is_empty() {
                return Err(format!("bb targets: {:?} has no aliases", entry.project));
            }
            let mut aliases = Vec::with_capacity(entry.aliases.len());
            for alias in entry.aliases {
                let normalized = normalize(&alias);
                if normalized.is_empty() {
                    return Err(format!(
                        "bb targets: {:?} has an empty alias",
                        entry.project
                    ));
                }
                if let Some(other) = seen.insert(normalized, entry.project.clone()) {
                    return Err(format!(
                        "bb targets: alias {alias:?} is used by {other:?} and {:?}",
                        entry.project
                    ));
                }
                aliases.push(alias.trim().to_owned());
            }
            entry.aliases = aliases;
            entries.push(entry);
        }
        Ok(Self {
            entries,
            default_provider: DEFAULT_PROVIDER.to_owned(),
        })
    }

    /// The parsed allow-list.
    #[must_use]
    pub fn entries(&self) -> &[TargetEntry] {
        &self.entries
    }

    /// Maps a spoken project name onto a personal target from the plugin's
    /// live list. `model`/`effort` override the config pinning; both must still
    /// be allowed by the plugin.
    pub fn resolve(
        &self,
        targets: &Targets,
        spoken: &str,
        model: &str,
        effort: &str,
    ) -> Result<Resolution, ResolveError> {
        let normalized = normalize(spoken);
        if normalized.is_empty() {
            return Err(ResolveError::TargetNotFound);
        }
        let mut entry = self.match_entry(&normalized).unwrap_or_default();
        let target =
            find_personal(targets, &entry, &normalized).ok_or(ResolveError::TargetNotFound)?;
        if entry.project.is_empty() {
            entry.project = target.name.clone();
        }

        let mut provider_id = entry.provider.clone().unwrap_or_default();
        if provider_id.is_empty() {
            provider_id.clone_from(&self.default_provider);
        }
        let mut pinned_model = entry.model.clone().unwrap_or_default();
        let override_model = model.trim();
        if !override_model.is_empty() {
            override_model.clone_into(&mut pinned_model);
        }
        let mut pinned_effort = entry.effort.clone().unwrap_or_default();
        let override_effort = effort.trim();
        if !override_effort.is_empty() {
            override_effort.clone_into(&mut pinned_effort);
        }

        let provider = target
            .providers
            .iter()
            .find(|candidate| candidate.provider_id == provider_id)
            .ok_or(ResolveError::ProviderUnavailable)?;
        if provider.models.is_empty() {
            return Err(ResolveError::ProviderUnavailable);
        }
        let selected = if pinned_model.is_empty() {
            &provider.models[0]
        } else {
            provider
                .models
                .iter()
                .find(|candidate| candidate.model == pinned_model)
                .ok_or(ResolveError::ModelNotFound)?
        };
        let mut level = pinned_effort.trim().to_owned();
        if level.is_empty() {
            level.clone_from(&selected.default_reasoning_level);
        }
        if level.is_empty()
            && let Some(first) = selected.reasoning_levels.first()
        {
            level.clone_from(first);
        }
        if !level.is_empty()
            && !selected.reasoning_levels.is_empty()
            && !selected.reasoning_levels.contains(&level)
        {
            return Err(ResolveError::EffortNotSupported);
        }

        Ok(Resolution {
            project: target.name.clone(),
            project_id: target.project_id.clone(),
            provider_id: provider.provider_id.clone(),
            model: selected.model.clone(),
            effort: level,
            title_prefix: entry.title_prefix.clone().unwrap_or_default(),
        })
    }

    fn match_entry(&self, normalized: &str) -> Option<TargetEntry> {
        self.entries
            .iter()
            .find(|entry| {
                entry
                    .aliases
                    .iter()
                    .any(|alias| normalize(alias) == normalized)
            })
            .cloned()
    }
}

fn trimmed_option(value: Option<String>) -> Option<String> {
    value
        .map(|value| value.trim().to_owned())
        .filter(|value| !value.is_empty())
}

/// The personal target addressed by the configured entry or by the spoken
/// project name. A project that exists only in another profile is not found.
fn find_personal<'a>(
    targets: &'a Targets,
    entry: &TargetEntry,
    normalized: &str,
) -> Option<&'a Target> {
    let wanted = normalize(&entry.project);
    targets.projects.iter().find(|target| {
        if target.profile != PROFILE_PERSONAL {
            return false;
        }
        if !entry.project.is_empty() {
            return target.project_id.trim() == entry.project.trim()
                || normalize(&target.name) == wanted;
        }
        normalize(&target.name) == normalized
    })
}

/// Spoken-name normalization: lower case, ё→е, punctuation and repeated
/// spaces collapsed.
#[must_use]
pub fn normalize(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    let mut space = false;
    for character in value.trim().to_lowercase().chars() {
        let character = if character == 'ё' { 'е' } else { character };
        if character.is_alphanumeric() {
            if space && !out.is_empty() {
                out.push(' ');
            }
            space = false;
            out.push(character);
        } else {
            space = true;
        }
    }
    out.trim().to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    const TARGETS: &str = r#"
targets:
  - project: proj_personal
    aliases: ["стан лэб", "station lab"]
    provider: claude-code
    title_prefix: "Голос"
  - project: proj_work
    aliases: ["работа"]
"#;

    fn live_targets() -> Targets {
        Targets {
            projects: vec![
                Target {
                    project_id: "proj_personal".to_owned(),
                    name: "Station Lab".to_owned(),
                    profile: PROFILE_PERSONAL.to_owned(),
                    providers: vec![Provider {
                        provider_id: "claude-code".to_owned(),
                        models: vec![Model {
                            model: "sonnet".to_owned(),
                            default_reasoning_level: "medium".to_owned(),
                            reasoning_levels: vec![
                                "low".to_owned(),
                                "medium".to_owned(),
                                "high".to_owned(),
                            ],
                            ..Model::default()
                        }],
                        ..Provider::default()
                    }],
                    ..Target::default()
                },
                Target {
                    project_id: "proj_work".to_owned(),
                    name: "Work".to_owned(),
                    profile: "work".to_owned(),
                    providers: vec![Provider {
                        provider_id: "codex".to_owned(),
                        models: vec![Model {
                            model: "gpt".to_owned(),
                            default_reasoning_level: "low".to_owned(),
                            ..Model::default()
                        }],
                        ..Provider::default()
                    }],
                    ..Target::default()
                },
            ],
        }
    }

    #[test]
    fn resolves_aliases_and_pins_the_default_model() {
        let resolver = BbResolver::parse(TARGETS.as_bytes()).expect("targets");
        let resolution = resolver
            .resolve(&live_targets(), "Стан Лэб", "", "")
            .expect("resolve");
        assert_eq!(resolution.project, "Station Lab");
        assert_eq!(resolution.project_id, "proj_personal");
        assert_eq!(resolution.provider_id, "claude-code");
        assert_eq!(resolution.model, "sonnet");
        assert_eq!(resolution.effort, "medium");
        assert_eq!(resolution.title_prefix, "Голос");
    }

    #[test]
    fn a_work_only_project_is_not_found() {
        let resolver = BbResolver::parse(TARGETS.as_bytes()).expect("targets");
        assert_eq!(
            resolver.resolve(&live_targets(), "работа", "", ""),
            Err(ResolveError::TargetNotFound)
        );
        assert_eq!(
            resolver.resolve(&live_targets(), "неизвестный", "", ""),
            Err(ResolveError::TargetNotFound)
        );
    }

    #[test]
    fn models_and_levels_are_validated() {
        let resolver = BbResolver::parse(TARGETS.as_bytes()).expect("targets");
        assert_eq!(
            resolver.resolve(&live_targets(), "стан лэб", "gpt", ""),
            Err(ResolveError::ModelNotFound)
        );
        assert_eq!(
            resolver.resolve(&live_targets(), "стан лэб", "", "max"),
            Err(ResolveError::EffortNotSupported)
        );
    }

    #[test]
    fn a_duplicate_alias_is_rejected() {
        let duplicate = r#"
targets:
  - project: one
    aliases: ["дом"]
  - project: two
    aliases: ["Дом"]
"#;
        assert!(BbResolver::parse(duplicate.as_bytes()).is_err());
    }

    #[test]
    fn normalization_folds_yo_and_punctuation() {
        assert_eq!(normalize("  Стан-Лэб!  "), "стан лэб");
        assert_eq!(normalize("Ещё"), "еще");
    }

    struct FakeRunner {
        stdout: Mutex<Vec<u8>>,
        exit_code: Option<u32>,
        payloads: Mutex<Vec<Vec<u8>>>,
    }

    impl Runner for FakeRunner {
        fn run<'a>(&'a self, payload: &'a [u8]) -> BbFuture<'a, Result<Answer, crate::ssh::Error>> {
            Box::pin(async move {
                self.payloads.lock().expect("lock").push(payload.to_vec());
                Ok(Answer {
                    stdout: self.stdout.lock().expect("lock").clone(),
                    exit_code: self.exit_code,
                })
            })
        }
    }

    #[tokio::test]
    async fn the_dispatcher_decodes_a_bridge_answer() {
        let runner = Arc::new(FakeRunner {
            stdout: Mutex::new(
                br#"{"ok":true,"result":{"threads":[{"id":"thr_1","profile":"personal","status":"running"}]}}"#
                    .to_vec(),
            ),
            exit_code: Some(0),
            payloads: Mutex::new(Vec::new()),
        });
        let dispatcher = Dispatcher::new(runner.clone(), Duration::from_secs(1));
        let status = dispatcher.status(20).await.expect("status");
        assert_eq!(status.threads.len(), 1);
        assert_eq!(status.threads[0].id, "thr_1");
        let payload = runner.payloads.lock().expect("lock")[0].clone();
        let payload: Value = serde_json::from_slice(&payload).expect("payload");
        assert_eq!(payload["method"], "status");
        assert_eq!(payload["params"]["limit"], 20);
    }

    #[tokio::test]
    async fn a_refused_bridge_answer_becomes_a_spoken_error() {
        let runner = Arc::new(FakeRunner {
            stdout: Mutex::new(
                r#"{"ok":false,"code":"thread_limit","speech":"Лимит тредов."}"#
                    .as_bytes()
                    .to_vec(),
            ),
            exit_code: Some(1),
            payloads: Mutex::new(Vec::new()),
        });
        let dispatcher = Dispatcher::new(runner, Duration::from_secs(1));
        let error = dispatcher.status(20).await.expect_err("refused");
        assert_eq!(error.code, "thread_limit");
        assert_eq!(speech_for_error(&error), "Лимит тредов.");
    }

    #[tokio::test]
    async fn an_unparsable_answer_is_unavailable() {
        let runner = Arc::new(FakeRunner {
            stdout: Mutex::new(b"not json".to_vec()),
            exit_code: Some(0),
            payloads: Mutex::new(Vec::new()),
        });
        let dispatcher = Dispatcher::new(runner, Duration::from_secs(1));
        let error = dispatcher.targets().await.expect_err("invalid");
        assert_eq!(error.code, "bridge_error");
    }

    #[test]
    fn validate_spawn_sends_only_the_target_fields() {
        let request = SpawnRequest {
            profile: PROFILE_PERSONAL.to_owned(),
            project_id: "proj".to_owned(),
            provider_id: "claude-code".to_owned(),
            model: "sonnet".to_owned(),
            reasoning_level: "medium".to_owned(),
            title: "Голос".to_owned(),
            prompt: "task".to_owned(),
            idempotency_key: "key".to_owned(),
        };
        let spawn = serde_json::to_value(&request).expect("serialize");
        assert_eq!(spawn["title"], "Голос");
        assert_eq!(spawn["prompt"], "task");
        assert_eq!(spawn["idempotencyKey"], "key");

        let target = serde_json::to_value(request.target()).expect("serialize");
        let fields = target.as_object().expect("object");
        assert!(!fields.contains_key("title"));
        assert!(!fields.contains_key("prompt"));
        assert!(!fields.contains_key("idempotencyKey"));
        assert_eq!(fields["projectId"], "proj");
    }

    #[tokio::test]
    async fn the_ssh_path_runs_the_forced_command_end_to_end() {
        use std::net::SocketAddr;
        use std::path::{Path, PathBuf};

        use russh::keys::PrivateKey;
        use russh::keys::ssh_key::LineEnding;
        use russh::server::{
            self, Auth, ChannelOpenHandle, Server as _, Session as ServerSession, VerifiedPublicKey,
        };
        use russh::{Channel, ChannelId};
        use tokio::net::TcpListener;

        struct TestServer {
            response: Arc<Vec<u8>>,
        }

        impl server::Server for TestServer {
            type Handler = TestHandler;

            fn new_client(&mut self, _: Option<SocketAddr>) -> TestHandler {
                TestHandler {
                    response: Arc::clone(&self.response),
                }
            }
        }

        struct TestHandler {
            response: Arc<Vec<u8>>,
        }

        impl server::Handler for TestHandler {
            type Error = russh::Error;

            async fn auth_publickey(
                &mut self,
                _user: &str,
                _key: VerifiedPublicKey<'_>,
            ) -> Result<Auth, Self::Error> {
                Ok(Auth::Accept)
            }

            async fn channel_open_session(
                &mut self,
                _channel: Channel<server::Msg>,
                reply: ChannelOpenHandle,
                _session: &mut ServerSession,
            ) -> Result<(), Self::Error> {
                reply.accept().await;
                Ok(())
            }

            async fn exec_request(
                &mut self,
                channel: ChannelId,
                _data: &[u8],
                session: &mut ServerSession,
            ) -> Result<(), Self::Error> {
                session.channel_success(channel)?;
                Ok(())
            }

            async fn channel_eof(
                &mut self,
                channel: ChannelId,
                session: &mut ServerSession,
            ) -> Result<(), Self::Error> {
                for chunk in self.response.chunks(16 * 1024) {
                    session.data(channel, chunk.to_vec())?;
                }
                session.exit_status_request(channel, 0)?;
                session.eof(channel)?;
                session.close(channel)?;
                Ok(())
            }
        }

        fn key() -> PrivateKey {
            PrivateKey::random(&mut rand::rng(), russh::keys::Algorithm::Ed25519)
                .expect("generate key")
        }

        fn write_key(dir: &Path, name: &str, key: &PrivateKey) -> PathBuf {
            let path = dir.join(name);
            std::fs::write(
                &path,
                key.to_openssh(LineEnding::LF).expect("openssh").as_bytes(),
            )
            .expect("write key");
            path
        }

        async fn start(response: Vec<u8>) -> (String, PrivateKey) {
            let host_key = key();
            let server_key = host_key.clone();
            let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
            let address = listener.local_addr().expect("addr").to_string();
            let config = Arc::new(server::Config {
                keys: vec![server_key],
                auth_rejection_time: Duration::ZERO,
                auth_rejection_time_initial: Some(Duration::ZERO),
                ..server::Config::default()
            });
            let mut server = TestServer {
                response: Arc::new(response),
            };
            tokio::spawn(async move {
                let _ = server.run_on_socket(config, &listener).await;
            });
            (address, host_key)
        }

        let answer = r#"{"ok":true,"result":{"projects":[{"projectId":"proj","name":"Station Lab","profile":"personal"}]}}"#;
        let (address, host_key) = start(answer.as_bytes().to_vec()).await;
        let dir = tempfile::tempdir().expect("tempdir");
        let client_key = key();
        let known_hosts = dir.path().join("known_hosts");
        std::fs::write(
            &known_hosts,
            format!(
                "{address} {}\n",
                host_key.public_key().to_openssh().expect("public key")
            ),
        )
        .expect("write known_hosts");
        let runner = SshRunner::new(crate::ssh::Options {
            address: address.clone(),
            user: "bb".to_owned(),
            key_file: write_key(dir.path(), "id_ed25519", &client_key),
            known_hosts_file: known_hosts,
            timeout: Some(Duration::from_secs(5)),
        });
        let dispatcher = Dispatcher::new(Arc::new(runner), Duration::from_secs(5));
        let targets = dispatcher.targets().await.expect("targets");
        assert_eq!(targets.projects[0].project_id, "proj");
        assert_eq!(targets.projects[0].profile, PROFILE_PERSONAL);
    }
}
