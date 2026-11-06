//! Announces the final answer of a routed bb turn.
//!
//! The immediate acknowledgement was already spoken by the router, so this
//! background worker long-polls `/v1/voice/result` for up to 900 s and then
//! either enqueues the answer (durable queue) or announces it directly.
//!
//! Two behaviours are deliberate: a timeout, a gateway error or a still-running
//! turn speaks nothing at all; an empty successful answer becomes "Готово.", an
//! explicitly failed turn becomes "Агент завершил работу с ошибкой.".

use std::sync::Arc;
use std::time::{Duration, Instant};

use home_runtime::Supervisor;
use tokio_util::sync::CancellationToken;

use super::queue::Producer;
use super::{Gateway, HaAnnouncer};
use crate::domain::announce::answer_text;
use crate::domain::profile::Profile;
use crate::domain::routing::Announcement;

/// Tuning of the late-answer poller.
#[derive(Debug, Clone)]
pub struct ResultsOptions {
    pub max_chars: usize,
    pub timeout: Duration,
    pub wait: u64,
    pub retry: Duration,
}

impl Default for ResultsOptions {
    fn default() -> Self {
        Self {
            max_chars: 200,
            timeout: Duration::from_secs(900),
            wait: 25,
            retry: Duration::from_secs(2),
        }
    }
}

/// Starts one background announcer per routed turn.
pub struct Results {
    gateway: Arc<Gateway>,
    announcer: Arc<HaAnnouncer>,
    producer: Option<Arc<Producer>>,
    options: ResultsOptions,
    supervisor: Supervisor,
}

impl std::fmt::Debug for Results {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("Results")
            .field("options", &self.options)
            .field("queue", &self.producer.is_some())
            .finish_non_exhaustive()
    }
}

impl Results {
    #[must_use]
    pub fn new(
        gateway: Arc<Gateway>,
        announcer: Arc<HaAnnouncer>,
        producer: Option<Arc<Producer>>,
        options: ResultsOptions,
        supervisor: Supervisor,
    ) -> Self {
        Self {
            gateway,
            announcer,
            producer,
            options,
            supervisor,
        }
    }

    /// Spawns one background announcer; it outlives the HTTP request.
    pub fn start(self: &Arc<Self>, announcement: &Announcement, profile: Profile) {
        let this = Arc::clone(self);
        let cancel = self.supervisor.token();
        let announcement = announcement.clone();
        self.supervisor.spawn("result", async move {
            this.poll(&announcement, profile, &cancel).await;
        });
    }

    /// Long-polls one turn and speaks or enqueues its answer.
    pub async fn poll(
        &self,
        announcement: &Announcement,
        profile: Profile,
        cancel: &CancellationToken,
    ) {
        let deadline = Instant::now() + self.options.timeout;
        let mut status = String::from("timeout");
        let mut speech = String::new();
        let mut completed_seq = None;
        let mut finished = false;

        while Instant::now() < deadline {
            let remaining = deadline.saturating_duration_since(Instant::now());
            let wait = self.options.wait.min(remaining.as_secs().max(1));
            let result = tokio::select! {
                () = cancel.cancelled() => return,
                result = self.gateway.result(&announcement.thread_id, announcement.after_seq, wait) => result,
            };
            match result {
                Err(_) => {
                    status.clear();
                    status.push_str("gateway_error");
                    if Instant::now() + self.options.retry >= deadline {
                        finished = true;
                        break;
                    }
                    if !sleep(cancel, self.options.retry).await {
                        return;
                    }
                }
                Ok(response) => {
                    status = response.status;
                    if status == "running" || status == "starting" {
                        status.clear();
                        status.push_str("running");
                        continue;
                    }
                    speech = response.speech.unwrap_or_default();
                    completed_seq = response.completed_seq;
                    finished = true;
                    break;
                }
            }
        }
        // Reaching the deadline without an answer is a timeout, even when the
        // last poll said "running".
        if !finished {
            status.clear();
            status.push_str("timeout");
        }

        let Some(text) = answer_text(&status, &speech, self.options.max_chars) else {
            tracing::info!(
                event = "announce",
                thread = %announcement.thread_id,
                outcome = %status_or_unknown(&status),
                announced = false,
                listen = false,
                "late answer not spoken"
            );
            return;
        };

        if let Some(producer) = &self.producer
            && producer.enabled()
        {
            let seq = completed_seq.map_or_else(
                || format!("after:{}", announcement.after_seq),
                |seq| format!("done:{seq}"),
            );
            if producer
                .enqueue(&text, profile, &announcement.thread_id, &seq, None)
                .await
            {
                tracing::info!(
                    event = "queue_enqueued",
                    thread = %announcement.thread_id,
                    profile = profile.as_str(),
                    outcome = %status_or_unknown(&status),
                    "late answer queued"
                );
                return;
            }
            tracing::info!(
                event = "queue_enqueue_failed",
                thread = %announcement.thread_id,
                fallback = "direct",
                "late answer falls back to a direct announcement"
            );
        }

        let (announced, listening) = self.announcer.announce(&text).await;
        tracing::info!(
            event = "announce",
            thread = %announcement.thread_id,
            outcome = %status_or_unknown(&status),
            announced,
            listen = listening,
            "late answer announced"
        );
    }
}

fn status_or_unknown(status: &str) -> &str {
    if status.is_empty() { "unknown" } else { status }
}

/// Sleeps unless cancelled; returns `false` when cancelled.
pub async fn sleep(cancel: &CancellationToken, delay: Duration) -> bool {
    if delay.is_zero() {
        return !cancel.is_cancelled();
    }
    tokio::select! {
        () = cancel.cancelled() => false,
        () = tokio::time::sleep(delay) => true,
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::time::Duration;

    use axum::extract::State;
    use axum::routing::post;
    use axum::{Json, Router};
    use serde_json::{Value, json};
    use tokio_util::sync::CancellationToken;

    use super::{Results, ResultsOptions};
    use crate::domain::profile::Profile;
    use crate::domain::routing::Announcement;
    use crate::services::{Gateway, HaAccess, HaAnnouncer};

    #[derive(Clone, Default)]
    struct Spoken(Arc<std::sync::Mutex<Vec<String>>>);

    async fn serve(app: Router) -> String {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind");
        let address = listener.local_addr().expect("address");
        tokio::spawn(async move { axum::serve(listener, app).await.expect("serve") });
        format!("http://{address}")
    }

    fn gateway(base: &str) -> Arc<Gateway> {
        Arc::new(
            Gateway::new(
                base,
                Arc::new(|| "token".to_owned()),
                Duration::from_secs(2),
            )
            .expect("gateway"),
        )
    }

    fn announcer(base: &str) -> Arc<HaAnnouncer> {
        Arc::new(HaAnnouncer::new(
            Arc::new(HaAccess::new(
                base,
                Duration::from_secs(2),
                Arc::new(|| "token".to_owned()),
            )),
            "script.announce",
            "assist.satellite",
        ))
    }

    async fn script(
        State(spoken): State<Spoken>,
        Json(body): Json<Value>,
    ) -> axum::http::StatusCode {
        if let Some(message) = body["variables"]["message"].as_str() {
            spoken.0.lock().expect("lock").push(message.to_owned());
        }
        axum::http::StatusCode::OK
    }

    fn results(gateway: Arc<Gateway>, announcer: Arc<HaAnnouncer>, timeout: Duration) -> Results {
        Results::new(
            gateway,
            announcer,
            None,
            ResultsOptions {
                timeout,
                wait: 1,
                retry: Duration::from_millis(10),
                ..ResultsOptions::default()
            },
            home_runtime::Supervisor::new(),
        )
    }

    #[tokio::test]
    async fn a_finished_turn_is_sanitised_and_spoken() {
        let spoken = Spoken::default();
        let ha = serve(
            Router::new()
                .route("/api/services/script/turn_on", post(script))
                .with_state(spoken.clone()),
        )
        .await;
        let gw = serve(Router::new().route(
            "/v1/voice/result",
            post(|| async {
                Json(json!({"status": "done", "speech": "**Готово**.", "completedSeq": 9}))
            }),
        ))
        .await;
        let results = results(gateway(&gw), announcer(&ha), Duration::from_secs(2));
        results
            .poll(
                &Announcement {
                    thread_id: "thr_1".to_owned(),
                    after_seq: 7,
                },
                Profile::Work,
                &CancellationToken::new(),
            )
            .await;
        assert_eq!(spoken.0.lock().expect("lock").as_slice(), ["Готово."]);
    }

    #[tokio::test]
    async fn timeout_and_running_speak_nothing() {
        let spoken = Spoken::default();
        let ha = serve(
            Router::new()
                .route("/api/services/script/turn_on", post(script))
                .with_state(spoken.clone()),
        )
        .await;
        let gw = serve(Router::new().route(
            "/v1/voice/result",
            post(|| async { Json(json!({"status": "running"})) }),
        ))
        .await;
        let results = results(gateway(&gw), announcer(&ha), Duration::from_millis(150));
        results
            .poll(
                &Announcement {
                    thread_id: "thr_1".to_owned(),
                    after_seq: 7,
                },
                Profile::Personal,
                &CancellationToken::new(),
            )
            .await;
        assert!(spoken.0.lock().expect("lock").is_empty());
    }

    #[tokio::test]
    async fn an_empty_success_speaks_done() {
        let spoken = Spoken::default();
        let ha = serve(
            Router::new()
                .route("/api/services/script/turn_on", post(script))
                .with_state(spoken.clone()),
        )
        .await;
        let gw = serve(Router::new().route(
            "/v1/voice/result",
            post(|| async { Json(json!({"status": "done", "speech": ""})) }),
        ))
        .await;
        let results = results(gateway(&gw), announcer(&ha), Duration::from_secs(1));
        results
            .poll(
                &Announcement {
                    thread_id: "thr_1".to_owned(),
                    after_seq: 1,
                },
                Profile::Work,
                &CancellationToken::new(),
            )
            .await;
        assert_eq!(spoken.0.lock().expect("lock").as_slice(), ["Готово."]);
    }
}
