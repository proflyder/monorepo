use std::sync::Arc;

use axum::body::Body;
use axum::http::{Request, StatusCode, header};
use serde_json::{Value, json};
use tower::ServiceExt;

use super::{AlarmFacade, EngineStatus, Service, VpnAccess, router};
use crate::domain::{FixedClock, SyncChange, SyncEntity};
use crate::engine::{DEFAULT_PURGE_INTERVAL, Engine};
use crate::store::{Config, Store};
use crate::sync::{Announcement, MemorySync, ORIGIN_HEADER};

const TOKEN: &str = "sync-test-token";
const NOW: i64 = 1_767_268_800;

struct Fixture {
    app: axum::Router,
    store: Arc<Store>,
    sync: Arc<MemorySync>,
}

fn fixture() -> Fixture {
    let store = Arc::new(Store::open_in_memory(Config::default()).expect("store"));
    let sync = Arc::new(MemorySync::default());
    let service = Service::new(
        Arc::clone(&store),
        Arc::new(FixedClock::new(NOW)),
        "test",
        Arc::new(EngineStatus::default),
        Vec::new(),
    )
    .with_sync(Arc::clone(&sync) as Arc<dyn crate::sync::SyncSink>);
    let app = router(
        service.clone(),
        AlarmFacade::new(service),
        TOKEN,
        VpnAccess::default(),
    );
    Fixture { app, store, sync }
}

async fn call(app: &axum::Router, method: &str, body: Value, origin: Option<&str>) -> Value {
    let mut request = Request::builder()
        .method("POST")
        .uri(format!("/home.v1.SchedulerService/{method}"))
        .header(header::AUTHORIZATION, format!("Bearer {TOKEN}"))
        .header(header::CONTENT_TYPE, "application/json")
        .header("connect-protocol-version", "1");
    if let Some(origin) = origin {
        request = request.header(ORIGIN_HEADER, origin);
    }
    let response = app
        .clone()
        .oneshot(request.body(Body::from(body.to_string())).expect("request"))
        .await
        .expect("response");
    assert_eq!(response.status(), StatusCode::OK, "{method}");
    let bytes = axum::body::to_bytes(response.into_body(), 64 * 1024)
        .await
        .expect("body");
    serde_json::from_slice(&bytes).expect("json")
}

#[tokio::test]
async fn timer_mutations_announce_sync_changes_with_the_origin_device() {
    let fixture = fixture();
    let started = call(
        &fixture.app,
        "StartTimer",
        json!({"label": "tea", "durationSeconds": 60, "idempotencyKey": "k-1"}),
        Some("phone-a"),
    )
    .await;
    let id = started["timer"]["id"].as_str().expect("id").to_owned();
    call(&fixture.app, "CancelTimer", json!({"id": id}), None).await;

    assert_eq!(
        fixture.sync.taken(),
        vec![
            Announcement {
                change: SyncChange::upsert(SyncEntity::Timers, &id),
                origin: Some("phone-a".to_owned()),
            },
            Announcement {
                change: SyncChange::delete(SyncEntity::Timers, &id),
                origin: None,
            },
        ]
    );
}

#[tokio::test]
async fn reads_announce_nothing() {
    let fixture = fixture();
    call(&fixture.app, "ListTimers", json!({}), Some("phone-a")).await;
    assert!(fixture.sync.taken().is_empty());
}

#[tokio::test]
async fn a_fired_timer_is_announced_by_the_engine_tick() {
    let fixture = fixture();
    let started = call(
        &fixture.app,
        "StartTimer",
        json!({"label": "tea", "durationSeconds": 5, "idempotencyKey": "k-2"}),
        None,
    )
    .await;
    let id = started["timer"]["id"].as_str().expect("id").to_owned();
    let _started = fixture.sync.taken();
    let sync = Arc::new(MemorySync::default());
    let engine = Engine::new(
        Arc::new(FixedClock::new(NOW + 10)),
        std::time::Duration::from_secs(1),
        DEFAULT_PURGE_INTERVAL,
    )
    .with_sync(Arc::clone(&sync) as Arc<dyn crate::sync::SyncSink>);
    engine.tick(&fixture.store).await.expect("tick");

    assert_eq!(
        sync.taken(),
        vec![Announcement {
            change: SyncChange::upsert(SyncEntity::Timers, &id),
            origin: None,
        }]
    );
}
