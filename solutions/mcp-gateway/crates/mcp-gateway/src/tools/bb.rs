//! `bb.read` / `bb.act`: the bb home-dispatcher tool groups.
//!
//! Ports `solutions/mcp-gateway/internal/tools/bb.go`. Reading threads is
//! separate from starting one, mirroring the `infra.read`/`infra.fix` split.
//! `bb_status` lists only `profile == personal` threads; `bb_prepare_spawn`
//! resolves a spoken name exclusively among personal targets and stores one
//! pending spawn for the `MCP_PENDING_TTL` window; `bb_confirm` starts it and
//! turns on agent conversation mode.

use std::fmt::Write as _;
use std::sync::Arc;

use rand::Rng as _;
use serde::Serialize;
use serde_json::{Value, json};

use crate::bb::{self, PROFILE_PERSONAL, ResolveError, SpawnAction, speech_for_error};

use super::{
    Annotations, BoxFuture, Deps, ToolGroup, ToolOutcome, ToolSpec, empty_schema, failure, respond,
};

/// The pending key of `bb.act`; infra's restart store is separate.
pub const BB_PENDING_KEY: &str = "bb.spawn";

/// Upper bound for the task text accepted from speech.
pub const MAX_BB_TASK_RUNES: usize = 2000;

/// The `bb.read` group.
pub struct BbReadGroup {
    deps: Arc<Deps>,
}

impl BbReadGroup {
    /// Build the group.
    #[must_use]
    pub fn new(deps: Arc<Deps>) -> Self {
        Self { deps }
    }
}

impl ToolGroup for BbReadGroup {
    fn name(&self) -> &'static str {
        "bb.read"
    }

    fn tools(&self) -> Vec<ToolSpec> {
        vec![ToolSpec {
            name: "bb_status",
            description: "Short summary of running agent threads in the personal profile. Work-profile threads and projects are never listed. Returns a short Russian speech line plus structured thread details.",
            input_schema: empty_schema(),
            annotations: Annotations::read_only(),
        }]
    }

    fn call<'a>(
        &'a self,
        name: &'a str,
        _arguments: Value,
    ) -> BoxFuture<'a, Result<ToolOutcome, String>> {
        Box::pin(async move {
            match name {
                "bb_status" => Ok(bb_status(&self.deps).await),
                other => Err(format!("unknown tool {other:?}")),
            }
        })
    }
}

/// The `bb.act` group.
pub struct BbActGroup {
    deps: Arc<Deps>,
}

impl BbActGroup {
    /// Build the group.
    #[must_use]
    pub fn new(deps: Arc<Deps>) -> Self {
        Self { deps }
    }
}

impl ToolGroup for BbActGroup {
    fn name(&self) -> &'static str {
        "bb.act"
    }

    fn tools(&self) -> Vec<ToolSpec> {
        vec![
            ToolSpec {
                name: "bb_prepare_spawn",
                description: "Validate that an agent may be started in one personal project and store exactly one pending spawn for 90 seconds. Project is resolved only among personal targets, by spoken name or alias; provider, model and reasoning level must be allowed by the dispatcher. Nothing is started yet: returns a short Russian confirmation question.",
                input_schema: json!({
                    "type": "object",
                    "additionalProperties": false,
                    "properties": {
                        "project": {"type": "string", "description": "spoken project name or alias from the personal allowlist"},
                        "model": {"type": "string", "description": "optional model id allowed by the dispatcher"},
                        "effort": {"type": "string", "description": "optional reasoning level allowed by the dispatcher"},
                        "task": {"type": "string", "description": "task text for the agent"},
                    },
                    "required": ["project", "task"],
                }),
                annotations: Annotations::destructive(),
            },
            ToolSpec {
                name: "bb_confirm",
                description: "Start the agent prepared by bb_prepare_spawn. Takes no arguments, uses the stored idempotency key, and turns on agent conversation mode for the created thread.",
                input_schema: empty_schema(),
                annotations: Annotations::destructive(),
            },
            ToolSpec {
                name: "bb_cancel",
                description: "Drop the pending agent spawn without starting it.",
                input_schema: empty_schema(),
                annotations: Annotations::destructive(),
            },
        ]
    }

    fn call<'a>(
        &'a self,
        name: &'a str,
        arguments: Value,
    ) -> BoxFuture<'a, Result<ToolOutcome, String>> {
        Box::pin(async move {
            match name {
                "bb_prepare_spawn" => Ok(prepare_spawn(&self.deps, arguments).await),
                "bb_confirm" => Ok(confirm(&self.deps).await),
                "bb_cancel" => Ok(cancel(&self.deps)),
                other => Err(format!("unknown tool {other:?}")),
            }
        })
    }
}

// --- bb_status ------------------------------------------------------------

#[derive(Debug, Serialize)]
struct StatusDetail {
    threads: Vec<bb::Thread>,
    total: usize,
}

#[derive(Debug, Serialize)]
struct ErrorDetail {
    code: String,
}

async fn bb_status(deps: &Deps) -> ToolOutcome {
    let Some(api) = deps.bb() else {
        return failure(bb::SPEECH_NOT_READY, None::<Value>);
    };
    let status = match api.status(20).await {
        Ok(status) => status,
        Err(error) => {
            return failure(
                speech_for_error(&error),
                Some(ErrorDetail {
                    code: error.code.clone(),
                }),
            );
        }
    };
    let personal: Vec<bb::Thread> = status
        .threads
        .into_iter()
        .filter(|thread| thread.profile == PROFILE_PERSONAL)
        .collect();
    let total = personal.len();
    respond(
        status_speech(&personal),
        Some(StatusDetail {
            threads: personal,
            total,
        }),
    )
}

fn status_speech(threads: &[bb::Thread]) -> String {
    if threads.is_empty() {
        return "Работающих тредов нет.".to_owned();
    }
    let names: Vec<&str> = threads
        .iter()
        .take(2)
        .map(|thread| {
            if thread.title.is_empty() {
                thread.id.as_str()
            } else {
                thread.title.as_str()
            }
        })
        .collect();
    let mut speech = format!("Работают {} тредов: {}", threads.len(), names.join(", "));
    if threads.len() > names.len() {
        speech.push_str(" и другие");
    }
    speech.push('.');
    speech
}

// --- bb_prepare_spawn -----------------------------------------------------

#[derive(Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
struct PrepareDetail {
    allowed: bool,
    project: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    provider: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    model: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    effort: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    task: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    expires_at: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    reason: String,
}

impl PrepareDetail {
    fn refused(reason: impl Into<String>) -> Self {
        Self {
            reason: reason.into(),
            ..Self::default()
        }
    }
}

async fn prepare_spawn(deps: &Deps, arguments: Value) -> ToolOutcome {
    let input: SpawnInput = serde_json::from_value(arguments).unwrap_or_default();
    let Some(api) = deps.bb() else {
        return failure(bb::SPEECH_NOT_READY, None::<Value>);
    };
    let Some(resolver) = deps.bb_targets.as_deref() else {
        return failure(bb::SPEECH_NOT_READY, None::<Value>);
    };

    let task = input.task.trim().to_owned();
    if task.is_empty() {
        return failure(
            "Пустая задача для агента.",
            Some(PrepareDetail::refused("empty task")),
        );
    }
    if task.chars().count() > MAX_BB_TASK_RUNES {
        return failure(
            format!("Задача длиннее {MAX_BB_TASK_RUNES} символов."),
            Some(PrepareDetail::refused("task too long")),
        );
    }
    let spoken = input.project.trim();
    if spoken.is_empty() {
        return failure(
            "Не указан проект.",
            Some(PrepareDetail::refused("empty project")),
        );
    }

    let targets = match api.targets().await {
        Ok(targets) => targets,
        Err(error) => {
            return failure(
                speech_for_error(&error),
                Some(PrepareDetail::refused(error.code.clone())),
            );
        }
    };
    let resolved = match resolver.resolve(&targets, spoken, &input.model, &input.effort) {
        Ok(resolved) => resolved,
        Err(error) => {
            return failure(
                resolve_speech(error),
                Some(PrepareDetail::refused(error.to_string())),
            );
        }
    };

    let action = SpawnAction {
        project: resolved.project.clone(),
        project_id: resolved.project_id.clone(),
        provider_id: resolved.provider_id.clone(),
        model: resolved.model.clone(),
        reasoning_level: resolved.effort.clone(),
        title: title(&resolved.title_prefix, &task),
        prompt: task.clone(),
        idempotency_key: new_idempotency_key(),
    };
    if let Err(error) = api.validate_spawn(action.request()).await {
        return failure(
            speech_for_error(&error),
            Some(PrepareDetail::refused(error.code.clone())),
        );
    }
    let created_at = deps.bb_pending.put(BB_PENDING_KEY, action);
    let ttl = i128::try_from(deps.bb_pending.ttl().as_nanos()).unwrap_or(i128::MAX);
    let expires_at =
        jiff::Timestamp::from_nanosecond(created_at.as_nanosecond().saturating_add(ttl))
            .unwrap_or(jiff::Timestamp::MAX);
    tracing::info!(
        event = "action_prepared",
        action = "bb_spawn",
        target = %resolved.project_id,
        provider = %resolved.provider_id,
        model = %resolved.model,
        "bb spawn prepared"
    );
    let speech = format!(
        "Запустить агента в {}: {}. Подтверждаете?",
        resolved.project,
        shorten(&task, 90)
    );
    respond(
        speech,
        Some(PrepareDetail {
            allowed: true,
            project: resolved.project,
            provider: resolved.provider_id,
            model: resolved.model,
            effort: resolved.effort,
            task,
            expires_at: expires_at.to_string(),
            ..PrepareDetail::default()
        }),
    )
}

/// The tool input of `bb_prepare_spawn`.
#[derive(Debug, Default, serde::Deserialize)]
struct SpawnInput {
    #[serde(default)]
    project: String,
    #[serde(default)]
    model: String,
    #[serde(default)]
    effort: String,
    #[serde(default)]
    task: String,
}

fn resolve_speech(error: ResolveError) -> &'static str {
    match error {
        ResolveError::TargetNotFound => "Не нашёл такой личный проект.",
        ResolveError::ProviderUnavailable => "Провайдер для этого проекта недоступен.",
        ResolveError::ModelNotFound => "Эта модель недоступна.",
        ResolveError::EffortNotSupported => "Этот уровень усилия недоступен.",
    }
}

fn title(prefix: &str, task: &str) -> String {
    let summary = shorten(task, 60);
    if prefix.is_empty() {
        summary
    } else {
        format!("{prefix}: {summary}")
    }
}

fn shorten(text: &str, limit: usize) -> String {
    let trimmed = text.trim();
    let runes: Vec<char> = trimmed.chars().collect();
    if runes.len() <= limit {
        return trimmed.to_owned();
    }
    let head: String = runes[..limit.saturating_sub(1)].iter().collect();
    format!("{}…", head.trim())
}

fn new_idempotency_key() -> String {
    let mut buffer = [0u8; 16];
    rand::rng().fill_bytes(&mut buffer);
    let mut key = String::with_capacity(8 + 32);
    key.push_str("bb-spawn-");
    for byte in buffer {
        let _ = write!(key, "{byte:02x}");
    }
    key
}

// --- bb_confirm -----------------------------------------------------------

#[derive(Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
struct ConfirmDetail {
    started: bool,
    #[serde(skip_serializing_if = "String::is_empty")]
    thread_id: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    project: String,
    #[serde(skip_serializing_if = "is_false")]
    idempotent: bool,
    #[serde(skip_serializing_if = "String::is_empty")]
    reason: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    error: String,
}

async fn confirm(deps: &Deps) -> ToolOutcome {
    let Some(api) = deps.bb() else {
        return failure(bb::SPEECH_NOT_READY, None::<Value>);
    };
    let action: Option<SpawnAction> = deps.bb_pending.take(BB_PENDING_KEY);
    let Some(action) = action else {
        return respond(
            "Нет отложенного запуска.",
            Some(ConfirmDetail {
                reason: "no pending spawn".to_owned(),
                ..ConfirmDetail::default()
            }),
        );
    };
    let result = match api.spawn(action.request()).await {
        Ok(result) => result,
        Err(error) => {
            let code = error.code.clone();
            deps.audit("bb_spawn", &action.project_id, &format!("failure:{code}"));
            return failure(
                speech_for_error(&error),
                Some(ConfirmDetail {
                    project: action.project,
                    error: code,
                    ..ConfirmDetail::default()
                }),
            );
        }
    };
    if result.thread_id.is_empty() {
        deps.audit("bb_spawn", &action.project_id, "failure:empty thread id");
        return failure(
            bb::SPEECH_UNAVAILABLE,
            Some(ConfirmDetail {
                project: action.project,
                error: "empty thread id".to_owned(),
                ..ConfirmDetail::default()
            }),
        );
    }
    deps.bb_mode.enter(&result.thread_id);
    deps.audit("bb_spawn", &action.project_id, "success");
    respond(
        "Запустил агента. Теперь это режим агента.",
        Some(ConfirmDetail {
            started: true,
            thread_id: result.thread_id,
            project: action.project,
            idempotent: result.idempotent,
            ..ConfirmDetail::default()
        }),
    )
}

// --- bb_cancel ------------------------------------------------------------

#[derive(Debug, Default, Serialize)]
struct CancelDetail {
    cancelled: bool,
    #[serde(skip_serializing_if = "String::is_empty")]
    project: String,
}

fn cancel(deps: &Deps) -> ToolOutcome {
    let action: Option<SpawnAction> = deps.bb_pending.peek(BB_PENDING_KEY);
    deps.bb_pending.clear(BB_PENDING_KEY);
    match action {
        Some(action) => {
            deps.audit("bb_spawn", &action.project_id, "cancelled");
            respond(
                "Отменил запуск агента.",
                Some(CancelDetail {
                    cancelled: true,
                    project: action.project,
                }),
            )
        }
        None => respond("Нечего отменять.", Some(CancelDetail::default())),
    }
}

fn is_false(value: &bool) -> bool {
    !*value
}
