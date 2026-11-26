pub mod catalog;
pub mod history;
pub mod model;

use std::sync::Arc;

use axum::Router;
use axum::body::Body;
use axum::extract::State;
use axum::http::{Response, StatusCode, header};
use axum::routing::get;
use serde::Serialize;

use crate::bb::{BbApi, PROFILE_PERSONAL, StatusQuery, Thread};
use crate::domain::state::AgentMode;

use catalog::{CatalogApi, Skills, Toolsets};
use history::{Health, HistoryApi};
use model::{Agent, Agents, Channel, MessageRow, Reports, SessionPage, Voice};

pub struct Sources {
    pub bb: Option<Arc<dyn BbApi>>,
    pub history: Option<Arc<dyn HistoryApi>>,
    pub catalog: Option<Arc<dyn CatalogApi>>,
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
        let query = StatusQuery::personal(model::AGENT_LIMIT).recent(model::RECENT_AGENT_HOURS);
        match bb.status(query).await {
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
        let (telegram, api) = tokio::join!(
            self.channel_pages(model::TELEGRAM_SOURCE),
            self.channel_pages(model::VOICE_SOURCE)
        );
        let available = telegram.is_some() || api.is_some();
        let pages = telegram.into_iter().chain(api).flatten().collect();
        Reports {
            available,
            reports: model::reports(pages),
        }
    }

    async fn channel_pages(&self, source: &str) -> Option<Vec<SessionPage>> {
        let ids = self.session_ids(source, model::REPORT_SESSIONS, "").await?;
        let history = self.sources.history.as_ref()?;
        let pages = futures::future::join_all(ids.iter().map(|id| history.messages(id))).await;
        Some(
            ids.iter()
                .zip(pages)
                .filter_map(|(id, page)| {
                    page.ok().map(|rows| SessionPage {
                        channel: Channel::of_session(source, id),
                        rows,
                    })
                })
                .collect(),
        )
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

    pub async fn skills(&self) -> Skills {
        let catalog = self.sources.catalog.as_ref();
        catalog::skills(futures::future::OptionFuture::from(catalog.map(|api| api.skills())).await)
    }

    pub async fn toolsets(&self) -> Toolsets {
        let catalog = self.sources.catalog.as_ref();
        catalog::toolsets(
            futures::future::OptionFuture::from(catalog.map(|api| api.toolsets())).await,
        )
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
        let ids = self.session_ids(source, limit, prefix).await?;
        let history = self.sources.history.as_ref()?;
        let pages = futures::future::join_all(ids.iter().map(|id| history.messages(id))).await;
        Some(pages.into_iter().filter_map(Result::ok).collect())
    }
}

impl Panel {
    async fn session_ids(&self, source: &str, limit: usize, prefix: &str) -> Option<Vec<String>> {
        let history = self.sources.history.as_ref()?;
        let list = history
            .sessions(source)
            .await
            .inspect_err(|error| tracing::warn!(?error, source, "hermes panel: sessions failed"))
            .ok()?;
        Some(model::recent_sessions(list, limit, prefix))
    }
}

fn agent(thread: Thread) -> Agent {
    Agent {
        id: thread.id,
        title: thread.title,
        project: thread.project,
        status: thread.status,
        runtime_seconds: thread.runtime_seconds,
        updated_at: thread.updated_at,
    }
}

pub fn router<S: Clone + Send + Sync + 'static>(panel: Arc<Panel>) -> Router<S> {
    Router::new()
        .route("/v1/hermes/agents", get(agents))
        .route("/v1/hermes/reports", get(reports))
        .route("/v1/hermes/voice", get(voice))
        .route("/v1/hermes/skills", get(skills))
        .route("/v1/hermes/toolsets", get(toolsets))
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

async fn skills(State(panel): State<Arc<Panel>>) -> Response<Body> {
    json(&panel.skills().await)
}

async fn toolsets(State(panel): State<Arc<Panel>>) -> Response<Body> {
    json(&panel.toolsets().await)
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
