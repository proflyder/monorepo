pub mod history;
pub mod model;

use std::sync::Arc;

use axum::Router;
use axum::body::Body;
use axum::extract::State;
use axum::http::{Response, StatusCode, header};
use axum::routing::get;
use serde::Serialize;

use crate::bb::{BbApi, PROFILE_PERSONAL, Thread};
use crate::domain::state::AgentMode;

use history::{Health, HistoryApi};
use model::{Agent, Agents, MessageRow, Reports, Voice};

pub struct Sources {
    pub bb: Option<Arc<dyn BbApi>>,
    pub history: Option<Arc<dyn HistoryApi>>,
    pub mode: Arc<AgentMode>,
}

pub struct Panel {
    sources: Sources,
}

impl Panel {
    #[must_use]
    pub fn new(sources: Sources) -> Self {
        Self { sources }
    }

    pub async fn agents(&self) -> Agents {
        let Some(bb) = self.sources.bb.as_ref().filter(|bb| bb.configured()) else {
            return Agents {
                available: false,
                agents: Vec::new(),
            };
        };
        match bb.status(model::AGENT_LIMIT).await {
            Ok(status) => Agents {
                available: true,
                agents: status
                    .threads
                    .into_iter()
                    .filter(|thread| thread.profile == PROFILE_PERSONAL)
                    .map(agent)
                    .collect(),
            },
            Err(error) => {
                tracing::warn!(%error, "hermes panel: agent status failed");
                Agents {
                    available: false,
                    agents: Vec::new(),
                }
            }
        }
    }

    pub async fn reports(&self) -> Reports {
        let pages = self
            .pages(model::TELEGRAM_SOURCE, model::REPORT_SESSIONS, "")
            .await;
        Reports {
            available: pages.is_some(),
            reports: pages.map(model::reports).unwrap_or_default(),
        }
    }

    pub async fn voice(&self) -> Voice {
        let (health, pages) = tokio::join!(
            self.health(),
            self.pages(
                model::VOICE_SOURCE,
                model::VOICE_SESSIONS,
                model::VOICE_SESSION_PREFIX,
            )
        );
        let (relay_up, hermes_up, hermes_version) = match health {
            Health::Up { version } => (true, true, version),
            Health::HermesDown => (true, false, String::new()),
            Health::RelayDown => (false, false, String::new()),
        };
        Voice {
            relay_up,
            hermes_up,
            hermes_version,
            agent_mode: self.sources.mode.active().is_some(),
            requests_available: pages.is_some(),
            requests: pages.map(model::voice_requests).unwrap_or_default(),
        }
    }

    async fn health(&self) -> Health {
        match self.sources.history.as_ref() {
            Some(history) => history.health().await,
            None => Health::RelayDown,
        }
    }

    async fn pages(
        &self,
        source: &str,
        limit: usize,
        prefix: &str,
    ) -> Option<Vec<Vec<MessageRow>>> {
        let history = self.sources.history.as_ref()?;
        let list = history
            .sessions(source)
            .await
            .inspect_err(|error| tracing::warn!(?error, source, "hermes panel: sessions failed"))
            .ok()?;
        let ids = model::recent_sessions(list, limit, prefix);
        let pages = futures::future::join_all(ids.iter().map(|id| history.messages(id))).await;
        Some(pages.into_iter().filter_map(Result::ok).collect())
    }
}

fn agent(thread: Thread) -> Agent {
    Agent {
        id: thread.id,
        title: thread.title,
        project: thread.project,
        status: thread.status,
        runtime_seconds: thread.runtime_seconds,
    }
}

pub fn router<S: Clone + Send + Sync + 'static>(panel: Arc<Panel>) -> Router<S> {
    Router::new()
        .route("/v1/hermes/agents", get(agents))
        .route("/v1/hermes/reports", get(reports))
        .route("/v1/hermes/voice", get(voice))
        .with_state(panel)
}

async fn agents(State(panel): State<Arc<Panel>>) -> Response<Body> {
    json(&panel.agents().await)
}

async fn reports(State(panel): State<Arc<Panel>>) -> Response<Body> {
    json(&panel.reports().await)
}

async fn voice(State(panel): State<Arc<Panel>>) -> Response<Body> {
    json(&panel.voice().await)
}

fn json<T: Serialize>(value: &T) -> Response<Body> {
    let body = serde_json::to_vec(value).unwrap_or_default();
    Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, "application/json")
        .header(header::CACHE_CONTROL, "no-store")
        .body(Body::from(body))
        .unwrap_or_default()
}

#[cfg(test)]
mod tests;
