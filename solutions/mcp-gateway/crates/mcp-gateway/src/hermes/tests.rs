use std::collections::HashMap;
use std::sync::Arc;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use serde_json::{Value, json};
use tower::ServiceExt;

use super::history::{Health, HistoryApi, HistoryError};
use super::model::{self, MessagePage, MessageRow, SessionList};
use super::{Panel, Sources};
use crate::bb::{BbApi, BbFuture, Dispatcher, Runner};
use crate::domain::state::AgentMode;
use crate::ssh::Answer;
use crate::tools::BoxFuture;

struct FixedRunner(Value);

impl Runner for FixedRunner {
    fn run<'a>(&'a self, _payload: &'a [u8]) -> BbFuture<'a, Result<Answer, crate::ssh::Error>> {
        let stdout = serde_json::to_vec(&self.0).expect("json");
        Box::pin(async move {
            Ok(Answer {
                stdout,
                exit_code: Some(0),
            })
        })
    }
}

fn bb(answer: Value) -> Arc<dyn BbApi> {
    Arc::new(Dispatcher::new(
        Arc::new(FixedRunner(answer)),
        std::time::Duration::from_secs(1),
    ))
}

#[derive(Default)]
struct FakeHistory {
    health: Option<Health>,
    sessions: HashMap<&'static str, Value>,
    messages: HashMap<&'static str, Value>,
}

impl HistoryApi for FakeHistory {
    fn health(&self) -> BoxFuture<'_, Health> {
        let health = self.health.clone().unwrap_or(Health::RelayDown);
        Box::pin(async move { health })
    }

    fn sessions<'a>(&'a self, source: &'a str) -> BoxFuture<'a, Result<SessionList, HistoryError>> {
        Box::pin(async move {
            self.sessions
                .get(source)
                .map(|value| serde_json::from_value(value.clone()).expect("list"))
                .ok_or(HistoryError::HermesFailed)
        })
    }

    fn messages<'a>(
        &'a self,
        session: &'a str,
    ) -> BoxFuture<'a, Result<Vec<MessageRow>, HistoryError>> {
        Box::pin(async move {
            self.messages
                .get(session)
                .map(|value| {
                    serde_json::from_value::<MessagePage>(value.clone())
                        .expect("messages")
                        .data
                })
                .ok_or(HistoryError::HermesFailed)
        })
    }
}

fn panel(bb: Option<Arc<dyn BbApi>>, history: Option<FakeHistory>) -> Panel {
    Panel::new(Sources {
        bb,
        history: history.map(|history| Arc::new(history) as Arc<dyn HistoryApi>),
        mode: Arc::new(AgentMode::new()),
    })
}

fn message(id: i64, role: &str, content: &Value, timestamp: f64) -> Value {
    json!({"id": id, "role": role, "content": content, "timestamp": timestamp})
}

#[tokio::test]
async fn agents_list_only_personal_threads_with_status() {
    let answer = json!({"ok": true, "result": {"threads": [
        {"id": "thr_1", "title": "Fix lights", "project": "Lab", "profile": "personal", "status": "active", "runtimeSeconds": 42},
        {"id": "thr_2", "title": "Work", "project": "Job", "profile": "work", "status": "idle"},
        {"id": "thr_3", "title": "Notes", "project": "Notes", "profile": "personal", "status": "idle"}
    ]}});
    let agents = panel(Some(bb(answer)), None).agents().await;
    assert!(agents.available);
    let ids: Vec<&str> = agents
        .agents
        .iter()
        .map(|agent| agent.id.as_str())
        .collect();
    assert_eq!(ids, vec!["thr_1", "thr_3"]);
    assert_eq!(agents.agents[0].runtime_seconds, Some(42));
}

#[tokio::test]
async fn agents_are_unavailable_without_a_bridge_or_on_a_refusal() {
    assert!(!panel(None, None).agents().await.available);
    let refused = json!({"ok": false, "code": "bridge_error", "speech": "нет"});
    assert!(!panel(Some(bb(refused)), None).agents().await.available);
}

#[tokio::test]
async fn reports_are_the_answers_to_report_invocations() {
    let history = FakeHistory {
        sessions: HashMap::from([(
            model::TELEGRAM_SOURCE,
            json!({"data": [{"id": "tg_1", "last_active": 10.0}]}),
        )]),
        messages: HashMap::from([(
            "tg_1",
            json!({"data": [
                message(4, "assistant", &json!("Всё в порядке, token=abc123"), 130.5),
                message(3, "user", &json!("[IMPORTANT: The user has invoked the \"report\" skill, indicating ...]"), 100.0),
                message(2, "assistant", &json!("Привет"), 20.0),
                message(1, "user", &json!("привет"), 10.0)
            ]}),
        )]),
        ..FakeHistory::default()
    };
    let reports = panel(None, Some(history)).reports().await;
    assert!(reports.available);
    assert_eq!(reports.reports.len(), 1);
    let report = &reports.reports[0];
    assert_eq!(report.id, "3");
    assert_eq!(report.requested_at, 100);
    assert_eq!(report.completed_at, Some(130));
    assert!(report.text.starts_with("Всё в порядке"));
    assert!(!report.text.contains("abc123"));
}

#[tokio::test]
async fn voice_reports_health_and_recent_exchanges_with_latency() {
    let history = FakeHistory {
        health: Some(Health::Up {
            version: "1.2".to_owned(),
        }),
        sessions: HashMap::from([(
            model::VOICE_SOURCE,
            json!({"data": [
                {"id": "api-aa", "last_active": 50.0},
                {"id": "app_chat", "last_active": 60.0},
                {"id": "api-bb", "last_active": 40.0}
            ]}),
        )]),
        messages: HashMap::from([
            (
                "api-aa",
                json!({"data": [
                    message(1, "user", &json!("включи свет"), 50.0),
                    {"id": 2, "role": "assistant", "content": "", "timestamp": 50.4, "tool_calls": []},
                    message(3, "assistant", &json!([{"type": "text", "text": "Готово"}]), 51.25)
                ]}),
            ),
            (
                "api-bb",
                json!({"data": [
                    message(7, "user", &json!("какая погода"), 40.0),
                    {"id": 8, "role": "assistant", "content": "", "timestamp": 45.0, "finish_reason": "error"}
                ]}),
            ),
        ]),
    };
    let voice = panel(None, Some(history)).voice().await;
    assert!(voice.relay_up && voice.hermes_up);
    assert_eq!(voice.hermes_version, "1.2");
    assert!(!voice.agent_mode);
    assert!(voice.requests_available);
    assert_eq!(voice.requests.len(), 2);
    let latest = &voice.requests[0];
    assert_eq!(latest.text, "включи свет");
    assert_eq!(latest.reply.as_deref(), Some("Готово"));
    assert_eq!(latest.latency_ms, Some(1250));
    assert!(!latest.failed);
    let failed = &voice.requests[1];
    assert!(failed.failed);
    assert_eq!(failed.reply, None);
}

#[tokio::test]
async fn voice_without_a_relay_is_down_and_empty() {
    let voice = panel(None, None).voice().await;
    assert!(!voice.relay_up && !voice.hermes_up && !voice.requests_available);
    let hermes_down = FakeHistory {
        health: Some(Health::HermesDown),
        ..FakeHistory::default()
    };
    let voice = panel(None, Some(hermes_down)).voice().await;
    assert!(voice.relay_up && !voice.hermes_up && !voice.requests_available);
}

#[test]
fn content_text_reads_strings_and_text_parts() {
    assert_eq!(model::content_text(&json!("  hi ")), "hi");
    assert_eq!(
        model::content_text(
            &json!([{"type": "text", "text": "a"}, {"type": "image_url"}, {"type": "text", "text": "b"}])
        ),
        "a\nb"
    );
    assert_eq!(model::content_text(&json!(null)), "");
}

#[tokio::test]
async fn the_routes_need_the_chat_module() {
    let owner = home_app_auth::sha256_hex("owner");
    let member = home_app_auth::sha256_hex("member");
    let record = |id: &str, sha256: String, owner: bool, modules: home_app_auth::Modules| {
        home_app_auth::TokenRecord {
            id: id.to_owned(),
            name: id.to_owned(),
            sha256,
            owner,
            modules: modules.ids().into_iter().map(str::to_owned).collect(),
        }
    };
    let snapshot = home_app_auth::AccessSnapshot {
        revision: 1,
        tokens: vec![
            record("owner", owner, true, home_app_auth::Modules::owner()),
            record("member", member, false, home_app_auth::Modules::member()),
        ],
    };
    let app = crate::server::router(crate::server::Options {
        auth_token: "mcp".to_owned(),
        voice_token: None,
        voice: None,
        access: Some(crate::server::AccessBinding {
            client: home_app_auth::AccessClient::fixed(snapshot),
        }),
        system: None,
        chat: None,
        hermes: Some(Arc::new(panel(None, None))),
        chat_alerts_token: None,
        groups: Vec::new(),
        deps: Arc::new(crate::system::tests::empty_deps()),
        metrics: Arc::new(crate::metrics::Metrics::new()),
        version: "test".to_owned(),
        ready: true,
    });
    let status = |token: &'static str, path: &'static str| {
        let app = app.clone();
        async move {
            let request = Request::get(path)
                .header("authorization", format!("Bearer {token}"))
                .body(Body::empty())
                .expect("request");
            app.oneshot(request).await.expect("response").status()
        }
    };
    for path in [
        "/v1/hermes/agents",
        "/v1/hermes/reports",
        "/v1/hermes/voice",
    ] {
        assert_eq!(status("owner", path).await, StatusCode::OK);
        assert_eq!(status("member", path).await, StatusCode::FORBIDDEN);
        assert_eq!(status("nope", path).await, StatusCode::UNAUTHORIZED);
    }
}
