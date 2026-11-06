//! End-to-end HTTP tests through the real router and an in-memory store.

use std::sync::{Arc, Mutex};

use axum::Router;
use axum::body::Body;
use axum::http::{Method, Request, StatusCode};
use home_secret::Token;
use http_body_util::BodyExt as _;
use serde_json::{Value, json};
use time::OffsetDateTime;
use time::macros::datetime;
use tower::ServiceExt as _;
use wakegrant_protocol::{claim_mac_message, hmac_sha256_b64};

use crate::api::{Options, Service};
use crate::config::Keys;
use crate::domain::{Clock, MAX_TTL};
use crate::store::Store;

const NOW: OffsetDateTime = datetime!(2026-11-03 12:00:00 UTC);
const TOKEN: &str = "issuer-token";

#[derive(Clone)]
struct TestClock(Arc<Mutex<OffsetDateTime>>);

impl Clock for TestClock {
    fn now(&self) -> OffsetDateTime {
        *self.0.lock().expect("clock")
    }
}

struct Harness {
    router: Router,
    keys: Keys,
}

impl Harness {
    fn new() -> Self {
        let clock = TestClock(Arc::new(Mutex::new(NOW)));
        let mut keys = Keys::new();
        keys.insert("komp".to_owned(), (0u8..=0x1f).collect());
        let service = Arc::new(Service::new(Options {
            keys: keys.clone(),
            issuer_token: Token::new(TOKEN),
            store: Arc::new(Store::open_in_memory().expect("store")),
            max_ttl: MAX_TTL,
            clock: Arc::new(clock),
        }));
        Self {
            router: service.router(),
            keys,
        }
    }

    fn key(&self, machine: &str) -> &[u8] {
        &self.keys[machine]
    }

    async fn call(
        &self,
        method: Method,
        path: &str,
        body: Option<Value>,
        bearer: Option<&str>,
    ) -> (StatusCode, Value) {
        let mut builder = Request::builder()
            .method(method)
            .uri(path)
            .header("content-type", "application/json");
        if let Some(bearer) = bearer {
            builder = builder.header("authorization", format!("Bearer {bearer}"));
        }
        let body = body.map_or_else(Vec::new, |value| value.to_string().into_bytes());
        let response = self
            .router
            .clone()
            .oneshot(builder.body(Body::from(body)).expect("request"))
            .await
            .expect("service");
        let status = response.status();
        let bytes = response
            .into_body()
            .collect()
            .await
            .expect("body")
            .to_bytes();
        let value = serde_json::from_slice(&bytes)
            .unwrap_or_else(|_| Value::String(String::from_utf8_lossy(&bytes).into_owned()));
        (status, value)
    }

    async fn issue(&self, purpose: Option<&str>, ttl: Option<i64>) -> Value {
        let mut body = json!({"machine": "komp"});
        if let Some(purpose) = purpose {
            body["purpose"] = json!(purpose);
        }
        if let Some(ttl) = ttl {
            body["ttlSeconds"] = json!(ttl);
        }
        let (status, value) = self
            .call(Method::POST, "/v1/grants", Some(body), Some(TOKEN))
            .await;
        assert_eq!(status, StatusCode::CREATED, "{value}");
        value
    }

    async fn challenge(&self) -> String {
        let (status, value) = self
            .call(
                Method::POST,
                "/v1/challenge",
                Some(json!({"machine": "komp"})),
                None,
            )
            .await;
        assert_eq!(status, StatusCode::OK, "{value}");
        value["nonce"].as_str().expect("nonce").to_owned()
    }

    async fn claim(
        &self,
        nonce: &str,
        purpose: Option<&str>,
        wait: Option<i64>,
    ) -> (StatusCode, Value) {
        let message = purpose.map_or_else(
            || claim_mac_message("komp", nonce),
            |purpose| {
                wakegrant_protocol::claim_mac_message_for_purpose(
                    "komp",
                    nonce,
                    wakegrant_protocol::Purpose::parse(purpose).expect("purpose"),
                )
            },
        );
        let mac = hmac_sha256_b64(self.key("komp"), &message);
        let mut body = json!({"machine": "komp", "nonce": nonce, "mac": mac});
        if let Some(purpose) = purpose {
            body["purpose"] = json!(purpose);
        }
        if let Some(wait) = wait {
            body["waitSeconds"] = json!(wait);
        }
        self.call(Method::POST, "/v1/claim", Some(body), None).await
    }
}

#[tokio::test]
async fn v1_flow_issues_and_consumes_a_logon_grant() {
    let harness = Harness::new();
    let grant = harness.issue(None, None).await;
    let grant_id = grant["grantId"].as_str().expect("grant id");
    assert_eq!(grant["expiresAt"], "2026-11-03T12:03:00Z");

    let nonce = harness.challenge().await;
    let (status, claim) = harness.claim(&nonce, None, None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(claim["granted"], true);
    assert_eq!(claim["grantId"], grant_id);
    assert_eq!(
        claim["mac"],
        hmac_sha256_b64(
            harness.key("komp"),
            &wakegrant_protocol::grant_mac_message("komp", &nonce, true, grant_id)
        )
    );

    let (status, _) = harness.claim(&nonce, None, None).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn purposes_are_isolated_and_replace_only_their_own_grant() {
    let harness = Harness::new();
    let logon = harness.issue(Some("logon"), None).await;
    harness.issue(Some("unlock"), None).await;

    let (status, claim) = harness
        .claim(&harness.challenge().await, Some("unlock"), None)
        .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(claim["granted"], true);

    let (status, claim) = harness
        .claim(&harness.challenge().await, Some("logon"), Some(1))
        .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(claim["granted"], true);
    assert_eq!(claim["grantId"], logon["grantId"]);
}

#[tokio::test(flavor = "multi_thread")]
async fn long_poll_wakes_when_a_matching_grant_is_issued() {
    let harness = Arc::new(Harness::new());
    let nonce = harness.challenge().await;

    let waiter = {
        let harness = Arc::clone(&harness);
        tokio::spawn(async move { harness.claim(&nonce, Some("unlock"), Some(2)).await })
    };
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    let grant = harness.issue(Some("unlock"), None).await;

    let (status, claim) = tokio::time::timeout(std::time::Duration::from_secs(2), waiter)
        .await
        .expect("long-poll returns")
        .expect("task");
    assert_eq!(status, StatusCode::OK);
    assert_eq!(claim["granted"], true);
    assert_eq!(claim["grantId"], grant["grantId"]);
}

#[tokio::test]
async fn validation_and_rate_limits_are_enforced() {
    let harness = Harness::new();
    assert_eq!(
        harness
            .call(
                Method::POST,
                "/v1/grants",
                Some(json!({"machine": "komp"})),
                None
            )
            .await
            .0,
        StatusCode::UNAUTHORIZED
    );
    for ttl in [29, 601, 0, -1] {
        let (status, _) = harness
            .call(
                Method::POST,
                "/v1/grants",
                Some(json!({"machine": "komp", "ttlSeconds": ttl})),
                Some(TOKEN),
            )
            .await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "ttl {ttl}");
    }
    for _ in 0..crate::domain::CHALLENGE_LIMIT {
        assert!(harness.challenge().await.len() > 20);
    }
    let (status, _) = harness
        .call(
            Method::POST,
            "/v1/challenge",
            Some(json!({"machine": "komp"})),
            None,
        )
        .await;
    assert_eq!(status, StatusCode::TOO_MANY_REQUESTS);
}

#[tokio::test]
async fn healthz_reports_the_store() {
    let harness = Harness::new();
    let (status, body) = harness.call(Method::GET, "/healthz", None, None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body, "ok\n");
}
