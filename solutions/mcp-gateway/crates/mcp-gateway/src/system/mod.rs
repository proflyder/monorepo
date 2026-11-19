pub mod build;
pub mod model;

use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use axum::Router;
use axum::body::Body;
use axum::extract::State;
use axum::http::{Response, StatusCode, header};
use axum::routing::get;
use serde::Serialize;
use tokio::sync::Mutex;

use crate::push::PushApi;
use crate::tools::{AlertLister, PromQuerier};

use build::Results;
use model::{Devices, Hosts, Kubernetes, Level, Sections, Summary};

pub const CACHE_TTL: Duration = Duration::from_secs(15);
pub const QUERY_TIMEOUT: Duration = Duration::from_secs(5);
pub const SECTION_TIMEOUT: Duration = Duration::from_secs(8);

#[derive(Clone, Default)]
pub struct Sources {
    pub prom: Option<Arc<dyn PromQuerier>>,
    pub alerts: Option<Arc<dyn AlertLister>>,
    pub push: Option<Arc<dyn PushApi>>,
}

struct Cached<T> {
    slot: Mutex<Option<(Instant, Arc<T>)>>,
}

impl<T> Default for Cached<T> {
    fn default() -> Self {
        Self {
            slot: Mutex::new(None),
        }
    }
}

impl<T> Cached<T> {
    async fn get_or<F, Fut>(&self, ttl: Duration, refresh: F) -> Arc<T>
    where
        F: FnOnce() -> Fut,
        Fut: Future<Output = T>,
    {
        let mut slot = self.slot.lock().await;
        if let Some((at, value)) = slot.as_ref()
            && at.elapsed() < ttl
        {
            return Arc::clone(value);
        }
        let value = Arc::new(refresh().await);
        *slot = Some((Instant::now(), Arc::clone(&value)));
        value
    }
}

pub struct Dashboard {
    sources: Sources,
    ttl: Duration,
    hosts: Cached<Hosts>,
    k8s: Cached<Kubernetes>,
    devices: Cached<Devices>,
    alerts: Cached<(bool, Vec<model::AlertItem>)>,
}

impl Dashboard {
    #[must_use]
    pub fn new(sources: Sources) -> Self {
        Self::with_ttl(sources, CACHE_TTL)
    }

    #[must_use]
    pub fn with_ttl(sources: Sources, ttl: Duration) -> Self {
        Self {
            sources,
            ttl,
            hosts: Cached::default(),
            k8s: Cached::default(),
            devices: Cached::default(),
            alerts: Cached::default(),
        }
    }

    async fn query(&self, queries: &'static [(&'static str, &'static str)]) -> Results {
        let mut results = Results::default();
        let Some(prom) = self.sources.prom.as_ref().filter(|prom| prom.configured()) else {
            return results;
        };
        let answers = futures::future::join_all(queries.iter().map(|(key, promql)| async move {
            let answer =
                tokio::time::timeout(QUERY_TIMEOUT, prom.query(promql, build::SAMPLE_LIMIT)).await;
            (*key, answer)
        }))
        .await;
        for (key, answer) in answers {
            match answer {
                Ok(Ok(samples)) => results.insert(key, samples),
                Ok(Err(error)) => tracing::warn!(query = key, %error, "system query failed"),
                Err(_) => tracing::warn!(query = key, "system query timed out"),
            }
        }
        results
    }

    async fn bounded<T>(&self, work: impl Future<Output = T>, fallback: impl FnOnce() -> T) -> T {
        tokio::time::timeout(SECTION_TIMEOUT, work)
            .await
            .unwrap_or_else(|_| fallback())
    }

    pub async fn hosts(&self) -> Arc<Hosts> {
        self.hosts
            .get_or(self.ttl, || {
                self.bounded(
                    async {
                        let (host, guests) = tokio::join!(
                            self.query(build::HOST_QUERIES),
                            self.query(build::GUEST_QUERIES)
                        );
                        build::hosts(&host, &guests)
                    },
                    || build::hosts(&Results::default(), &Results::default()),
                )
            })
            .await
    }

    pub async fn kubernetes(&self) -> Arc<Kubernetes> {
        self.k8s
            .get_or(self.ttl, || {
                self.bounded(
                    async { build::kubernetes(&self.query(build::K8S_QUERIES).await) },
                    || build::kubernetes(&Results::default()),
                )
            })
            .await
    }

    pub async fn devices(&self) -> Arc<Devices> {
        self.devices
            .get_or(self.ttl, || {
                self.bounded(
                    async {
                        let (station, phones) =
                            tokio::join!(self.query(build::STATION_QUERIES), self.phones());
                        build::devices(&station, phones)
                    },
                    || build::devices(&Results::default(), None),
                )
            })
            .await
    }

    async fn phones(&self) -> Option<Vec<crate::push::Device>> {
        let push = self
            .sources
            .push
            .as_ref()
            .filter(|push| push.configured())?;
        match tokio::time::timeout(QUERY_TIMEOUT, push.devices()).await {
            Ok(Ok(devices)) => Some(devices),
            Ok(Err(error)) => {
                tracing::warn!(%error, "system push registry failed");
                None
            }
            Err(_) => {
                tracing::warn!("system push registry timed out");
                None
            }
        }
    }

    async fn active_alerts(&self) -> Arc<(bool, Vec<model::AlertItem>)> {
        self.alerts
            .get_or(self.ttl, || async {
                let Some(lister) = self
                    .sources
                    .alerts
                    .as_ref()
                    .filter(|lister| lister.configured())
                else {
                    return (false, Vec::new());
                };
                match tokio::time::timeout(QUERY_TIMEOUT, lister.active(100)).await {
                    Ok(Ok(alerts)) => (true, build::alerts(&alerts)),
                    Ok(Err(error)) => {
                        tracing::warn!(%error, "system alerts failed");
                        (false, Vec::new())
                    }
                    Err(_) => (false, Vec::new()),
                }
            })
            .await
    }

    pub async fn summary(&self) -> Summary {
        let (hosts, k8s, devices, alerts) = tokio::join!(
            self.hosts(),
            self.kubernetes(),
            self.devices(),
            self.active_alerts()
        );
        let sections = Sections {
            host: hosts.status,
            guests: hosts.guests_status,
            k8s: k8s.status,
            devices: devices.status,
        };
        let (alerts_available, alerts) = (alerts.0, alerts.1.clone());
        let status = summary_level(sections, &alerts);
        Summary {
            status,
            generated_at: now_unix(),
            sections,
            alerts,
            alerts_available,
        }
    }
}

#[must_use]
pub fn summary_level(sections: Sections, alerts: &[model::AlertItem]) -> Level {
    let levels = [
        sections.host,
        sections.guests,
        sections.k8s,
        sections.devices,
    ];
    let measured = levels
        .iter()
        .copied()
        .filter(|level| *level != Level::Unknown)
        .fold(None, |worst: Option<Level>, level| {
            Some(worst.map_or(level, |worst| worst.worst(level)))
        });
    let alert = alerts
        .iter()
        .map(|alert| build::alert_level(&alert.severity))
        .fold(Level::Ok, Level::worst);
    match measured {
        Some(level) => level.worst(alert),
        None if alert == Level::Ok => Level::Unknown,
        None => alert,
    }
}

fn now_unix() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |elapsed| {
            i64::try_from(elapsed.as_secs()).unwrap_or(i64::MAX)
        })
}

pub fn router<S: Clone + Send + Sync + 'static>(dashboard: Arc<Dashboard>) -> Router<S> {
    Router::new()
        .route("/v1/system/summary", get(summary))
        .route("/v1/system/hosts", get(hosts))
        .route("/v1/system/k8s", get(kubernetes))
        .route("/v1/system/devices", get(devices))
        .with_state(dashboard)
}

async fn summary(State(dashboard): State<Arc<Dashboard>>) -> Response<Body> {
    json(&dashboard.summary().await)
}

async fn hosts(State(dashboard): State<Arc<Dashboard>>) -> Response<Body> {
    json(dashboard.hosts().await.as_ref())
}

async fn kubernetes(State(dashboard): State<Arc<Dashboard>>) -> Response<Body> {
    json(dashboard.kubernetes().await.as_ref())
}

async fn devices(State(dashboard): State<Arc<Dashboard>>) -> Response<Body> {
    json(dashboard.devices().await.as_ref())
}

fn json<T: Serialize>(value: &T) -> Response<Body> {
    let body = serde_json::to_vec(value).expect("serializing a dashboard section");
    Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, "application/json")
        .header(header::CACHE_CONTROL, "no-store")
        .body(Body::from(body))
        .expect("valid response")
}

#[cfg(test)]
mod tests;
