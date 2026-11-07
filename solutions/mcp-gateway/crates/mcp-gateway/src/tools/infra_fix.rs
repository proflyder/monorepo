//! `infra.fix`: prepare, confirm and cancel exactly one rollout restart.

use std::sync::Arc;

use serde::Serialize;
use serde_json::Value;

use super::{
    Annotations, BoxFuture, Deps, ToolGroup, ToolOutcome, ToolSpec, empty_schema, failure,
    required_string, respond,
};
use crate::domain::catalog;
use crate::domain::state::{self, ACTION_ROLLOUT_RESTART, Action};
use crate::kubernetes;

/// The `infra.fix` group.
pub struct FixGroup {
    deps: Arc<Deps>,
}

impl FixGroup {
    /// Build the group.
    #[must_use]
    pub fn new(deps: Arc<Deps>) -> Self {
        Self { deps }
    }
}

impl ToolGroup for FixGroup {
    fn name(&self) -> &'static str {
        "infra.fix"
    }

    fn tools(&self) -> Vec<ToolSpec> {
        vec![
            ToolSpec {
                name: "infra_prepare_restart",
                description: "Validate that one catalog service may be restarted now and store exactly one pending action for 90 seconds. Returns a short Russian confirmation question; nothing is restarted yet.",
                input_schema: required_string("spoken service name from the catalog"),
                annotations: Annotations::destructive(),
            },
            ToolSpec {
                name: "infra_confirm",
                description: "Execute the pending restart prepared by infra_prepare_restart. Takes no arguments, so it works even if the client lost conversation history. Self-disrupting services restart after a short delay.",
                input_schema: empty_schema(),
                annotations: Annotations::destructive(),
            },
            ToolSpec {
                name: "infra_cancel",
                description: "Drop the pending restart without executing it.",
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
            let deps = &self.deps;
            match name {
                "infra_prepare_restart" => Ok(prepare(deps, &arguments).await),
                "infra_confirm" => Ok(confirm(deps).await),
                "infra_cancel" => Ok(cancel(deps)),
                other => Err(format!("unknown tool {other:?}")),
            }
        })
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct PrepareDetail {
    allowed: bool,
    service: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    namespace: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    kind: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    name: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    warning: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    expires_at: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    reason: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct ConfirmDetail {
    executed: bool,
    scheduled: bool,
    #[serde(skip_serializing_if = "String::is_empty")]
    service: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    kind: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    name: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    namespace: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    delay: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    reason: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    error: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct CancelDetail {
    cancelled: bool,
    #[serde(skip_serializing_if = "String::is_empty")]
    service: String,
}

async fn prepare(deps: &Deps, arguments: &Value) -> ToolOutcome {
    let raw = arguments
        .get("name")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let service = match deps.resolve(raw) {
        Ok(service) => service.clone(),
        Err(error) => {
            return failure(
                format!("Не нашёл сервис {}.", raw.trim()),
                Some(PrepareDetail {
                    allowed: false,
                    service: String::new(),
                    namespace: String::new(),
                    kind: String::new(),
                    name: String::new(),
                    warning: String::new(),
                    expires_at: String::new(),
                    reason: error.to_string(),
                }),
            );
        }
    };
    let display = service.display_name();
    let mut detail = PrepareDetail {
        allowed: false,
        service: service.id.clone(),
        namespace: String::new(),
        kind: String::new(),
        name: String::new(),
        warning: String::new(),
        expires_at: String::new(),
        reason: String::new(),
    };

    if service.kind != catalog::SERVICE_K8S {
        "external service".clone_into(&mut detail.reason);
        return respond(
            format!("Перезапуск {display} через шлюз недоступен."),
            Some(&detail),
        );
    }
    service.namespace.clone_into(&mut detail.namespace);
    service.workload.kind.clone_into(&mut detail.kind);
    service.workload.name.clone_into(&mut detail.name);
    if !service.restart.allowed {
        "restart is not allowed by the catalog".clone_into(&mut detail.reason);
        return respond(format!("Перезапуск {display} запрещён."), Some(&detail));
    }
    service.restart.warning.clone_into(&mut detail.warning);
    let Some(kube) = deps.kube() else {
        "kubernetes is not configured".clone_into(&mut detail.reason);
        return failure("Кластер недоступен, перезапуск невозможен.", Some(&detail));
    };

    let workload = match kube
        .get_workload(
            &service.namespace,
            &service.workload.kind,
            &service.workload.name,
        )
        .await
    {
        Ok(workload) => workload,
        Err(error) => {
            detail.reason = error;
            return failure(format!("Не нашёл {display} в кластере."), Some(&detail));
        }
    };
    if workload.rollout_in_progress() {
        "rollout in progress".clone_into(&mut detail.reason);
        return respond(
            format!("{display} уже обновляется, подождите."),
            Some(&detail),
        );
    }
    if let Some(cooldown) = state::restart_cooldown(
        kubernetes::last_restart(&workload)
            .and_then(|at| jiff::Timestamp::from_second(at.unix_timestamp()).ok()),
        deps.now(),
        deps.cooldown,
    ) {
        "cooldown".clone_into(&mut detail.reason);
        return respond(
            format!(
                "{display} перезапускали {} назад, подождите ещё {}.",
                state::human_duration(cooldown.elapsed_seconds),
                state::human_duration(cooldown.remaining_seconds)
            ),
            Some(&detail),
        );
    }

    let action = Action {
        service_id: service.id.clone(),
        namespace: service.namespace.clone(),
        workload_kind: service.workload.kind.clone(),
        workload_name: service.workload.name.clone(),
        kind: ACTION_ROLLOUT_RESTART.to_owned(),
        self_disrupting: service.restart.self_disrupting,
        delay: deps.self_restart_delay,
        warning: service.restart.warning.clone(),
    };
    let created_at = deps.pending.put(action);
    detail.allowed = true;
    detail.expires_at = deps.pending.expires_at(created_at).to_string();

    let mut question = format!("Перезапустить {}?", display.to_lowercase());
    let warning = service.restart.warning.trim();
    if !warning.is_empty() {
        question.push(' ');
        question.push_str(warning);
        question.push_str(" Подтверждаете?");
    }
    deps.audit("rollout-restart", &service.id, "prepared");
    respond(question, Some(&detail))
}

async fn confirm(deps: &Deps) -> ToolOutcome {
    let Some(pending) = deps.pending.take() else {
        return respond(
            "Нет отложенного действия.",
            Some(ConfirmDetail {
                executed: false,
                scheduled: false,
                service: String::new(),
                kind: String::new(),
                name: String::new(),
                namespace: String::new(),
                delay: String::new(),
                reason: "no pending action".to_owned(),
                error: String::new(),
            }),
        );
    };
    let action = pending.action;
    let service = deps
        .catalog
        .as_ref()
        .and_then(|catalog| catalog.find(&action.service_id).cloned())
        .unwrap_or_else(|| catalog::Service {
            id: action.service_id.clone(),
            ..catalog::Service::default()
        });
    let display = service.display_name();
    let mut detail = ConfirmDetail {
        executed: false,
        scheduled: false,
        service: action.service_id.clone(),
        kind: action.workload_kind.clone(),
        name: action.workload_name.clone(),
        namespace: action.namespace.clone(),
        delay: String::new(),
        reason: String::new(),
        error: String::new(),
    };

    if deps.kube().is_none() {
        deps.audit("restart", &action.service_id, "failure");
        return failure("Кластер недоступен, перезапуск не выполнен.", Some(&detail));
    }
    let kube = deps.kube.as_ref().expect("checked above").clone();

    if action.self_disrupting {
        let delay = if action.delay.is_zero() {
            deps.self_restart_delay
        } else {
            action.delay
        };
        let service_id = action.service_id.clone();
        let namespace = action.namespace.clone();
        let kind = action.workload_kind.clone();
        let name = action.workload_name.clone();
        let at = jiff::Timestamp::now();
        let kube = Arc::clone(&kube);
        let audit_metrics = Arc::clone(&deps.metrics);
        deps.timers.after_func(
            delay,
            Box::new(move || {
                let kube = Arc::clone(&kube);
                let service_id = service_id.clone();
                let audit_metrics = Arc::clone(&audit_metrics);
                tokio::spawn(async move {
                    if kube
                        .patch_restart(&namespace, &kind, &name, at)
                        .await
                        .is_err()
                    {
                        audit_metrics.observe_action("restart", &service_id, "failure");
                        return;
                    }
                    audit_metrics.observe_action("restart", &service_id, "success");
                });
            }),
        );
        deps.audit("restart", &action.service_id, "scheduled");
        detail.scheduled = true;
        detail.delay = format!("{}s", delay.as_secs());
        return respond(
            format!(
                "Перезапущу {} через {} секунд.",
                display.to_lowercase(),
                delay.as_secs()
            ),
            Some(&detail),
        );
    }

    if let Err(error) = kube
        .patch_restart(
            &action.namespace,
            &action.workload_kind,
            &action.workload_name,
            deps.now(),
        )
        .await
    {
        detail.error = error;
        deps.audit("restart", &action.service_id, "failure");
        return failure(
            format!("Не удалось перезапустить {display}."),
            Some(&detail),
        );
    }
    deps.audit("restart", &action.service_id, "success");
    detail.executed = true;
    respond(
        format!("Перезапускаю {}.", display.to_lowercase()),
        Some(&detail),
    )
}

fn cancel(deps: &Deps) -> ToolOutcome {
    let pending = deps.pending.peek();
    deps.pending.clear();
    match pending {
        Some(pending) => {
            deps.audit("restart", &pending.action.service_id, "cancelled");
            respond(
                "Отменил перезапуск.",
                Some(CancelDetail {
                    cancelled: true,
                    service: pending.action.service_id,
                }),
            )
        }
        None => respond(
            "Нечего отменять.",
            Some(CancelDetail {
                cancelled: false,
                service: String::new(),
            }),
        ),
    }
}

#[allow(dead_code)]
fn _json(_: &Value) {}
