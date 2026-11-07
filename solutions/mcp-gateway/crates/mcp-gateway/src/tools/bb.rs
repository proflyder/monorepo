//! `bb.read` / `bb.act`: interfaces only.
//!
//! The real SSH bridge and the `/v1/voice/*` routes are implemented in the
//! following change (PR-5). This module already publishes the exact tool set,
//! descriptions, schemas and annotations so `tools/list` matches the Go
//! gateway for every `MCP_GROUPS` combination; calls answer with a spoken
//! "bridge is not configured" until the dispatcher is wired.

use std::sync::Arc;

use serde_json::{Value, json};

use super::{
    Annotations, BoxFuture, Deps, ToolGroup, ToolOutcome, ToolSpec, empty_schema, failure,
};

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
                "bb_status" => Ok(not_ready(&self.deps)),
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
        _arguments: Value,
    ) -> BoxFuture<'a, Result<ToolOutcome, String>> {
        Box::pin(async move {
            match name {
                "bb_prepare_spawn" | "bb_confirm" | "bb_cancel" => Ok(not_ready(&self.deps)),
                other => Err(format!("unknown tool {other:?}")),
            }
        })
    }
}

fn not_ready(_deps: &Deps) -> ToolOutcome {
    failure("Мост bb не настроен.", None::<Value>)
}

/// The bb home-dispatcher client (PR-5).
pub trait BbApi: Send + Sync {}

/// The personal-only spoken allow-list (PR-5).
#[derive(Debug, Default)]
pub struct BbResolver;

/// One pending agent spawn (PR-5).
#[derive(Debug, Clone, Default)]
pub struct SpawnAction {
    /// Project id.
    pub project: String,
    /// Task text.
    pub task: String,
}
