use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use axum::body::Body;
use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, Method, Request, StatusCode, header};
use axum::response::{IntoResponse, Response};
use axum::routing::{any, get, post};
use axum::{Json, Router};
use base64::Engine as _;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use home_app_auth::{AccessClient, AccessSnapshot, Modules, TokenRecord, sha256_hex};
use home_secret::Token;
use p256::ecdsa::SigningKey;
use p256::ecdsa::signature::Signer as _;
use p256::pkcs8::EncodePublicKey as _;
use secrets_gateway::api::{
    self, GatewayState, STEP_UP_CHALLENGE_HEADER, STEP_UP_SESSION_HEADER, STEP_UP_SIGNATURE_HEADER,
};
use secrets_gateway::audit::{Auditor, PushClient};
use secrets_gateway::catalog::Catalog;
use secrets_gateway::deny::Deny;
use secrets_gateway::device::{self, DeviceStore};
use secrets_gateway::health;
use secrets_gateway::openbao::OpenBao;
use secrets_gateway::stepup::StepUp;
use serde_json::{Value, json};
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender, unbounded_channel};
use tower::ServiceExt;

const OWNER_TOKEN: &str = "owner-token";
const SECOND_OWNER_TOKEN: &str = "second-owner-token";
const MEMBER_TOKEN: &str = "member-token";
const SERVICE_TOKEN: &str = "service-token";
const SECRET_VALUE: &str = "s3cr3t";
const DEVICE_ID: &str = "test-device";
const POLICY: &str =
    include_str!("../../../../../projects/home/kubernetes/secrets-gateway/policy.hcl");
const DENY: &str =
    include_str!("../../../../../projects/home/kubernetes/secrets-gateway/manifests/deny.json");

#[derive(Debug, Clone)]
struct Rule {
    path: String,
    capabilities: Vec<String>,
}

#[derive(Debug, Clone)]
struct Policy {
    rules: Vec<Rule>,
}

impl Policy {
    fn parse(text: &str) -> Self {
        let mut rules = Vec::new();
        let mut rest = text;
        while let Some(start) = rest.find("path \"") {
            let after = &rest[start + 6..];
            let Some(end_quote) = after.find('"') else {
                break;
            };
            let path = &after[..end_quote];
            let Some(open) = after[end_quote..].find('{') else {
                break;
            };
            let Some(close) = after[end_quote + open..].find('}') else {
                break;
            };
            let block = &after[end_quote + open..end_quote + open + close];
            let mut capabilities = Vec::new();
            let mut search = block;
            while let Some(quote) = search.find('"') {
                let tail = &search[quote + 1..];
                let Some(end) = tail.find('"') else {
                    break;
                };
                let capability = &tail[..end];
                if capability
                    .chars()
                    .all(|character| character.is_ascii_lowercase())
                {
                    capabilities.push(capability.to_owned());
                }
                search = &tail[end + 1..];
            }
            rules.push(Rule {
                path: path.to_owned(),
                capabilities,
            });
            rest = &after[end_quote + open + close + 1..];
        }
        Self { rules }
    }

    fn require(&self, method: &Method, path: &str) -> bool {
        let capability = capability_for(method, path);
        let matching: Vec<&Rule> = self
            .rules
            .iter()
            .filter(|rule| rule_matches(&rule.path, path))
            .collect();
        if matching
            .iter()
            .any(|rule| rule.capabilities.iter().any(|item| item == "deny"))
        {
            return false;
        }
        matching
            .iter()
            .any(|rule| rule.capabilities.iter().any(|item| item == capability))
    }
}

fn capability_for(method: &Method, path: &str) -> &'static str {
    if path.starts_with("secret/data/") {
        return match method.as_str() {
            "GET" => "read",
            "DELETE" => "delete",
            _ => "update",
        };
    }
    if path.starts_with("secret/metadata") {
        return match method.as_str() {
            "LIST" => "list",
            "DELETE" => "delete",
            _ => "read",
        };
    }
    "read"
}

fn rule_matches(rule: &str, path: &str) -> bool {
    if rule == path {
        return true;
    }
    rule.strip_suffix('*')
        .is_some_and(|prefix| path.starts_with(prefix))
}

#[derive(Clone, Debug)]
struct OpenBaoState {
    inner: Arc<Mutex<FakeOpenBao>>,
}

#[derive(Debug)]
struct FakeOpenBao {
    current: u64,
    versions: BTreeMap<u64, BTreeMap<String, String>>,
    deleted: BTreeSet<u64>,
    requests: Vec<(String, String)>,
    reject_write: bool,
    policy: Policy,
    device: BTreeMap<String, String>,
    device_version: u64,
}

impl OpenBaoState {
    fn new() -> Self {
        let mut versions = BTreeMap::new();
        versions.insert(
            1,
            BTreeMap::from([
                ("password".to_owned(), "old".to_owned()),
                ("user".to_owned(), "admin".to_owned()),
            ]),
        );
        versions.insert(
            2,
            BTreeMap::from([
                ("password".to_owned(), SECRET_VALUE.to_owned()),
                ("user".to_owned(), "admin".to_owned()),
            ]),
        );
        Self {
            inner: Arc::new(Mutex::new(FakeOpenBao {
                current: 2,
                versions,
                deleted: BTreeSet::new(),
                requests: Vec::new(),
                reject_write: false,
                policy: Policy::parse(POLICY),
                device: BTreeMap::new(),
                device_version: 0,
            })),
        }
    }

    fn set_device(&self, public_key: &str) {
        let mut inner = self.inner.lock().expect("lock");
        inner.device = BTreeMap::from([
            ("device_id".to_owned(), DEVICE_ID.to_owned()),
            ("device_public_key".to_owned(), public_key.to_owned()),
            ("device_enrolled_at".to_owned(), "1".to_owned()),
        ]);
        inner.device_version = 1;
    }

    fn clear_device(&self) {
        let mut inner = self.inner.lock().expect("lock");
        inner.device.clear();
        inner.device_version = inner.device_version.saturating_add(1);
    }

    fn revoke_device(&self) {
        let mut inner = self.inner.lock().expect("lock");
        let device_id = inner.device.get("device_id").cloned();
        inner.device.clear();
        if let Some(device_id) = device_id {
            inner
                .device
                .insert("device_revoke_device_id".to_owned(), device_id);
            inner
                .device
                .insert("device_revoked_at".to_owned(), "2".to_owned());
        }
        inner.device_version = inner.device_version.saturating_add(1);
    }

    fn set_enrollment(&self, code_hash: &str, expires_at: u64) {
        let mut inner = self.inner.lock().expect("lock");
        inner
            .device
            .insert("enrollment_code_hash".to_owned(), code_hash.to_owned());
        inner
            .device
            .insert("enrollment_expires_at".to_owned(), expires_at.to_string());
        inner.device_version = inner.device_version.saturating_add(1);
    }

    fn device(&self) -> BTreeMap<String, String> {
        self.inner.lock().expect("lock").device.clone()
    }

    fn record(&self, method: &Method, path: &str) {
        self.inner
            .lock()
            .expect("lock")
            .requests
            .push((method.as_str().to_owned(), path.to_owned()));
    }

    fn allow(&self, method: &Method, path: &str) -> bool {
        self.inner
            .lock()
            .expect("lock")
            .policy
            .require(method, path)
    }

    fn requests(&self) -> Vec<(String, String)> {
        self.inner.lock().expect("lock").requests.clone()
    }

    fn current(&self) -> u64 {
        self.inner.lock().expect("lock").current
    }

    fn version(&self, version: u64) -> BTreeMap<String, String> {
        self.inner
            .lock()
            .expect("lock")
            .versions
            .get(&version)
            .cloned()
            .expect("version")
    }

    fn mark_deleted(&self, version: u64) {
        self.inner.lock().expect("lock").deleted.insert(version);
    }

    fn reject_next_write(&self) {
        self.inner.lock().expect("lock").reject_write = true;
    }

    fn deleted(&self, version: u64) -> bool {
        self.inner.lock().expect("lock").deleted.contains(&version)
    }
}

async fn login() -> Json<Value> {
    Json(json!({"auth": {"client_token": "bao-token", "lease_duration": 3600}}))
}

async fn bao_health() -> StatusCode {
    StatusCode::OK
}

async fn metadata_root(State(state): State<OpenBaoState>, method: Method) -> Response {
    metadata_response(&state, "", &method)
}

async fn metadata_entry(
    State(state): State<OpenBaoState>,
    Path(path): Path<String>,
    method: Method,
) -> Response {
    metadata_response(&state, &path, &method)
}

fn metadata_response(state: &OpenBaoState, path: &str, method: &Method) -> Response {
    let full = match path.is_empty() {
        true => "secret/metadata".to_owned(),
        false => format!("secret/metadata/{path}"),
    };
    state.record(method, &full);
    if !state.allow(method, &full) {
        return StatusCode::FORBIDDEN.into_response();
    }
    if method.as_str() == "LIST" {
        return Json(json!({"data": {"keys": list_keys(path)}})).into_response();
    }
    if *method != Method::GET || path != "k8s/test" {
        return StatusCode::NOT_FOUND.into_response();
    }
    let inner = state.inner.lock().expect("lock");
    let current = inner.current;
    let deleted = inner.deleted.clone();
    drop(inner);
    let versions = (1..=current)
        .map(|version| {
            (
                version.to_string(),
                json!({
                    "created_time": format!("2026-01-01T00:00:{version:02}Z"),
                    "deletion_time": if deleted.contains(&version) {
                        "2026-01-02T00:00:00Z"
                    } else {
                        ""
                    },
                    "destroyed": false,
                }),
            )
        })
        .collect::<serde_json::Map<_, _>>();
    Json(json!({
        "data": {
            "current_version": current,
            "oldest_version": 1,
            "versions": versions,
        }
    }))
    .into_response()
}

fn list_keys(prefix: &str) -> Vec<String> {
    let paths = [
        "ci/test",
        "infra/backup/offsite",
        "infra/vpn/client-ca",
        "k8s/flux-system/git-deploy-key",
        "k8s/home-assistant/phone-push",
        "k8s/home-assistant/scheduler-auth",
        "k8s/home-assistant/secrets-gateway",
        "k8s/home-assistant/secrets-gateway-device",
        "k8s/test",
        "openbao/seal",
    ];
    let mut keys = BTreeSet::new();
    for path in paths {
        let Some(rest) = path.strip_prefix(prefix) else {
            continue;
        };
        if rest.is_empty() {
            continue;
        }
        let segment = rest.split('/').next().unwrap_or(rest);
        if rest.contains('/') {
            keys.insert(format!("{segment}/"));
        } else {
            keys.insert(segment.to_owned());
        }
    }
    keys.into_iter().collect()
}

async fn data_get(
    State(state): State<OpenBaoState>,
    Path(path): Path<String>,
    Query(query): Query<HashMap<String, String>>,
) -> Response {
    let full = format!("secret/data/{path}");
    state.record(&Method::GET, &full);
    if !state.allow(&Method::GET, &full) {
        return StatusCode::FORBIDDEN.into_response();
    }
    if path == device::DEVICE_PATH {
        let inner = state.inner.lock().expect("lock");
        if inner.device_version == 0 {
            return StatusCode::NOT_FOUND.into_response();
        }
        let version = inner.device_version;
        let data = inner.device.clone();
        drop(inner);
        return Json(json!({"data": {"data": data, "metadata": {"version": version}}}))
            .into_response();
    }
    if path != "k8s/test" {
        return StatusCode::NOT_FOUND.into_response();
    }
    let inner = state.inner.lock().expect("lock");
    let version = query
        .get("version")
        .and_then(|value| value.parse::<u64>().ok())
        .unwrap_or(inner.current);
    if inner.deleted.contains(&version) {
        return StatusCode::NOT_FOUND.into_response();
    }
    let Some(data) = inner.versions.get(&version).cloned() else {
        return StatusCode::NOT_FOUND.into_response();
    };
    drop(inner);
    Json(json!({
        "data": {
            "data": data,
            "metadata": {"version": version},
        }
    }))
    .into_response()
}

async fn data_write(
    State(state): State<OpenBaoState>,
    Path(path): Path<String>,
    Json(body): Json<Value>,
) -> Response {
    let full = format!("secret/data/{path}");
    state.record(&Method::POST, &full);
    if !state.allow(&Method::POST, &full) {
        return StatusCode::FORBIDDEN.into_response();
    }
    if path == device::DEVICE_PATH {
        let mut inner = state.inner.lock().expect("lock");
        let cas = body
            .get("options")
            .and_then(|options| options.get("cas"))
            .and_then(Value::as_u64);
        if let Some(cas) = cas
            && cas != inner.device_version
        {
            return (
                StatusCode::BAD_REQUEST,
                r#"{"errors":["check-and-set parameter did not match the current version"]}"#,
            )
                .into_response();
        }
        let data = body
            .get("data")
            .and_then(Value::as_object)
            .map(|values| {
                values
                    .iter()
                    .filter_map(|(key, value)| {
                        value.as_str().map(|text| (key.clone(), text.to_owned()))
                    })
                    .collect::<BTreeMap<_, _>>()
            })
            .unwrap_or_default();
        inner.device = data;
        inner.device_version += 1;
        let version = inner.device_version;
        drop(inner);
        return Json(json!({"data": {"version": version}})).into_response();
    }
    if path != "k8s/test" {
        return StatusCode::NOT_FOUND.into_response();
    }
    let mut inner = state.inner.lock().expect("lock");
    if inner.reject_write {
        inner.reject_write = false;
        return (
            StatusCode::BAD_REQUEST,
            r#"{"errors":["check-and-set parameter did not match the current version"]}"#,
        )
            .into_response();
    }
    let cas = body
        .get("options")
        .and_then(|options| options.get("cas"))
        .and_then(Value::as_u64);
    if let Some(cas) = cas
        && cas != inner.current
    {
        return (
            StatusCode::BAD_REQUEST,
            r#"{"errors":["check-and-set parameter did not match the current version"]}"#,
        )
            .into_response();
    }
    let data = body
        .get("data")
        .and_then(Value::as_object)
        .map(|values| {
            values
                .iter()
                .filter_map(|(key, value)| {
                    value.as_str().map(|text| (key.clone(), text.to_owned()))
                })
                .collect::<BTreeMap<_, _>>()
        })
        .unwrap_or_default();
    inner.current += 1;
    let version = inner.current;
    inner.versions.insert(version, data);
    drop(inner);
    Json(json!({"data": {"version": version}})).into_response()
}

async fn data_delete(State(state): State<OpenBaoState>, Path(path): Path<String>) -> Response {
    let full = format!("secret/data/{path}");
    state.record(&Method::DELETE, &full);
    if !state.allow(&Method::DELETE, &full) {
        return StatusCode::FORBIDDEN.into_response();
    }
    if path != "k8s/test" {
        return StatusCode::NOT_FOUND.into_response();
    }
    let mut inner = state.inner.lock().expect("lock");
    let current = inner.current;
    inner.deleted.insert(current);
    drop(inner);
    StatusCode::NO_CONTENT.into_response()
}

async fn destroy(State(state): State<OpenBaoState>, Path(path): Path<String>) -> Response {
    let full = format!("secret/destroy/{path}");
    state.record(&Method::POST, &full);
    if !state.allow(&Method::POST, &full) {
        return StatusCode::FORBIDDEN.into_response();
    }
    StatusCode::NO_CONTENT.into_response()
}

fn openbao_router(state: OpenBaoState) -> Router {
    Router::new()
        .route("/v1/auth/kubernetes/login", post(login))
        .route("/v1/sys/health", get(bao_health))
        .route("/v1/secret/metadata", any(metadata_root))
        .route("/v1/secret/metadata/{*path}", any(metadata_entry))
        .route(
            "/v1/secret/data/{*path}",
            get(data_get).post(data_write).delete(data_delete),
        )
        .route("/v1/secret/destroy/{*path}", any(destroy))
        .with_state(state)
}

#[derive(Clone, Debug)]
struct PushState {
    received: Arc<Mutex<Vec<Value>>>,
    sender: UnboundedSender<Value>,
}

async fn push_notify(
    State(state): State<PushState>,
    headers: HeaderMap,
    Json(body): Json<Value>,
) -> Response {
    let authorization = headers
        .get(header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .unwrap_or_default();
    if authorization != format!("Bearer {SERVICE_TOKEN}") {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    if body.to_string().contains(SECRET_VALUE) {
        return StatusCode::BAD_REQUEST.into_response();
    }
    state.received.lock().expect("lock").push(body.clone());
    let _ = state.sender.send(body);
    StatusCode::OK.into_response()
}

fn push_router(state: PushState) -> Router {
    Router::new()
        .route("/v1/push/notify", post(push_notify))
        .with_state(state)
}

struct Harness {
    app: Router,
    openbao: OpenBaoState,
    pushes: UnboundedReceiver<Value>,
}

async fn serve(app: Router) -> String {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind");
    let address = listener.local_addr().expect("addr");
    tokio::spawn(async move {
        let _ = axum::serve(listener, app).await;
    });
    format!("http://{address}")
}

async fn harness() -> Harness {
    let openbao_state = OpenBaoState::new();
    let openbao_url = serve(openbao_router(openbao_state.clone())).await;
    let (sender, pushes) = unbounded_channel();
    let push_url = serve(push_router(PushState {
        received: Arc::new(Mutex::new(Vec::new())),
        sender,
    }))
    .await;

    let snapshot = AccessSnapshot {
        revision: 1,
        tokens: vec![
            TokenRecord {
                id: "owner".to_owned(),
                name: "owner".to_owned(),
                sha256: sha256_hex(OWNER_TOKEN),
                owner: true,
                modules: Modules::owner()
                    .ids()
                    .into_iter()
                    .map(str::to_owned)
                    .collect(),
            },
            TokenRecord {
                id: "second-owner".to_owned(),
                name: "second-owner".to_owned(),
                sha256: sha256_hex(SECOND_OWNER_TOKEN),
                owner: true,
                modules: Modules::owner()
                    .ids()
                    .into_iter()
                    .map(str::to_owned)
                    .collect(),
            },
            TokenRecord {
                id: "member".to_owned(),
                name: "member".to_owned(),
                sha256: sha256_hex(MEMBER_TOKEN),
                owner: false,
                modules: Modules::member()
                    .ids()
                    .into_iter()
                    .map(str::to_owned)
                    .collect(),
            },
        ],
    };
    let catalog = Catalog::parse(
        r#"{"paths":{
            "k8s/test":{"purpose":"test entry","consumer":"tests"},
            "infra/vpn/client-ca":{"purpose":"ca","consumer":"owner"}
        }}"#,
    )
    .expect("catalog");
    let deny = Deny::parse(DENY).expect("deny");
    openbao_state.set_device(&public_key(&signing_key()));
    let openbao = Arc::new(
        OpenBao::new(
            &openbao_url,
            "secrets-gateway",
            Token::new("service-account-jwt"),
            Duration::from_secs(5),
        )
        .expect("openbao"),
    );
    let device = Arc::new(DeviceStore::new(Arc::clone(&openbao), device::DEVICE_PATH));
    let push = PushClient::new(&push_url, Token::new(SERVICE_TOKEN), Duration::from_secs(5))
        .expect("push");
    let state = GatewayState {
        access: AccessClient::fixed(snapshot),
        openbao,
        catalog: Arc::new(catalog),
        deny: Arc::new(deny),
        device: Arc::clone(&device),
        stepup: Arc::new(StepUp::new(device)),
        audit: Arc::new(Auditor::new(push)),
        max_list_entries: 50,
    };
    let app = api::router(state.clone()).merge(health::router(state));
    Harness {
        app,
        openbao: openbao_state,
        pushes,
    }
}

async fn call(
    app: &Router,
    method: Method,
    uri: &str,
    token: Option<&str>,
    extra: &[(&str, &str)],
    body: Option<Value>,
) -> (StatusCode, HeaderMap, Value) {
    let mut builder = Request::builder().method(method).uri(uri);
    if let Some(token) = token {
        builder = builder.header(header::AUTHORIZATION, format!("Bearer {token}"));
    }
    for (name, value) in extra {
        builder = builder.header(*name, *value);
    }
    let request = match body {
        Some(body) => builder
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(serde_json::to_vec(&body).expect("body")))
            .expect("request"),
        None => builder.body(Body::empty()).expect("request"),
    };
    let response = app.clone().oneshot(request).await.expect("response");
    let status = response.status();
    let headers = response.headers().clone();
    let bytes = axum::body::to_bytes(response.into_body(), 1 << 20)
        .await
        .expect("bytes");
    let value = serde_json::from_slice(&bytes).unwrap_or(Value::Null);
    (status, headers, value)
}

fn signing_key() -> SigningKey {
    SigningKey::from_slice(&[7u8; 32]).expect("signing key")
}

fn other_signing_key() -> SigningKey {
    SigningKey::from_slice(&[9u8; 32]).expect("signing key")
}

fn encoded(bytes: &[u8]) -> String {
    URL_SAFE_NO_PAD.encode(bytes)
}

fn public_key(key: &SigningKey) -> String {
    encoded(
        key.verifying_key()
            .to_public_key_der()
            .expect("public key DER")
            .as_bytes(),
    )
}

fn signature(payload: &str) -> String {
    sign_with(&signing_key(), payload)
}

fn sign_with(key: &SigningKey, payload: &str) -> String {
    let signature: p256::ecdsa::Signature = key.sign(payload.as_bytes());
    encoded(signature.to_der().as_bytes())
}

fn signed_headers<'a>(challenge: &'a str, signature: &'a str) -> [(&'a str, &'a str); 2] {
    [
        (STEP_UP_CHALLENGE_HEADER, challenge),
        (STEP_UP_SIGNATURE_HEADER, signature),
    ]
}

async fn mutation(harness: &Harness, body: Value) -> (String, String) {
    let (challenge, payload) = issue(&harness.app, OWNER_TOKEN, body).await;
    (challenge, signature(&payload))
}

async fn issue(app: &Router, token: &str, body: Value) -> (String, String) {
    let (status, _, value) = call(
        app,
        Method::POST,
        "/v1/step-up/challenge",
        Some(token),
        &[],
        Some(body),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{value}");
    (
        value["challenge"].as_str().expect("challenge").to_owned(),
        value["payload"].as_str().expect("payload").to_owned(),
    )
}

async fn attestation(app: &Router, token: &str) -> String {
    let (status, _, value) = call(
        app,
        Method::POST,
        "/v1/device/challenge",
        Some(token),
        &[],
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{value}");
    value["challenge"].as_str().expect("challenge").to_owned()
}

fn enrollment_hash(code: &str) -> String {
    device::hash_code(&device::normalize_code(code))
}

async fn pushes(pushes: &mut UnboundedReceiver<Value>) -> Vec<Value> {
    tokio::time::sleep(Duration::from_millis(100)).await;
    let mut received = Vec::new();
    while let Ok(value) = pushes.try_recv() {
        received.push(value);
    }
    received
}

async fn session_for(harness: &Harness, token: &str) -> String {
    let (challenge, payload) = issue(&harness.app, token, json!({"action": "session"})).await;
    let (status, _, body) = call(
        &harness.app,
        Method::POST,
        "/v1/step-up",
        Some(token),
        &[],
        Some(json!({
            "challenge": challenge,
            "signature": signature(&payload),
        })),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    body["session"].as_str().expect("session").to_owned()
}

async fn session(harness: &Harness) -> String {
    session_for(harness, OWNER_TOKEN).await
}

#[test]
fn the_fake_acl_requires_the_policy_capability() {
    let policy = Policy::parse(
        r#"path "secret/data/*" {
  capabilities = ["read", "update"]
}"#,
    );
    assert!(policy.require(&Method::GET, "secret/data/k8s/test"));
    assert!(!policy.require(&Method::DELETE, "secret/data/k8s/test"));
}

#[tokio::test]
async fn the_owner_lists_the_whole_catalog_except_the_core() {
    let mut harness = harness().await;
    let (status, _, body) = call(
        &harness.app,
        Method::GET,
        "/v1/secrets",
        Some(OWNER_TOKEN),
        &[],
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["truncated"], false);
    let entries = body["entries"].as_array().expect("entries");
    let test = entries
        .iter()
        .find(|entry| entry["path"] == "k8s/test")
        .expect("test entry");
    assert_eq!(test["description"], "test entry");
    assert_eq!(test["live"], true);
    assert_eq!(test["inCatalog"], true);
    for path in [
        "ci/test",
        "infra/backup/offsite",
        "k8s/home-assistant/phone-push",
    ] {
        assert!(
            entries.iter().any(|entry| entry["path"] == path),
            "missing visible {path}"
        );
    }
    for path in [
        "infra/vpn/client-ca",
        "k8s/flux-system/git-deploy-key",
        "k8s/home-assistant/scheduler-auth",
        "k8s/home-assistant/secrets-gateway",
        "openbao/seal",
    ] {
        assert!(
            !entries.iter().any(|entry| entry["path"] == path),
            "hidden {path}"
        );
    }
    let received = pushes(&mut harness.pushes).await;
    assert!(received.is_empty(), "{received:?}");
    let requests = harness.openbao.requests();
    assert!(!requests.iter().any(|(_, path)| {
        path.contains("infra/vpn/client-ca")
            || path.contains("k8s/flux-system")
            || path.contains("scheduler-auth")
            || (path.contains("secrets-gateway") && !path.contains("secrets-gateway-device"))
            || path.contains("openbao")
    }));
}

#[tokio::test]
async fn only_the_owner_module_can_use_the_gateway() {
    let harness = harness().await;
    let cases = [
        (Method::GET, "/v1/secrets", None),
        (Method::GET, "/v1/secrets/meta/k8s/test", None),
        (
            Method::POST,
            "/v1/secrets/reveal/k8s/test",
            Some(json!({"key": "password", "version": 2})),
        ),
        (
            Method::POST,
            "/v1/secrets/update/k8s/test",
            Some(json!({"key": "password", "value": "new", "cas": 2})),
        ),
        (
            Method::POST,
            "/v1/secrets/rollback/k8s/test",
            Some(json!({"version": 1, "cas": 2})),
        ),
        (Method::DELETE, "/v1/secrets/k8s/test?cas=2", None),
        (
            Method::POST,
            "/v1/step-up",
            Some(json!({"challenge": "x", "signature": "y"})),
        ),
        (
            Method::POST,
            "/v1/step-up/challenge",
            Some(json!({"action": "session"})),
        ),
        (Method::GET, "/v1/device", None),
        (Method::POST, "/v1/device/challenge", None),
        (
            Method::POST,
            "/v1/device/enroll",
            Some(json!({
                "code": "ABCD",
                "publicKey": "key",
                "attestationChallenge": "challenge",
                "attestationChain": ["cert"],
            })),
        ),
    ];
    for (method, uri, body) in cases {
        let (status, _, value) =
            call(&harness.app, method, uri, Some(MEMBER_TOKEN), &[], body).await;
        assert_eq!(status, StatusCode::FORBIDDEN, "{uri} {value}");
        assert_eq!(value["error"], "module_forbidden", "{uri}");
        assert_eq!(value["module"], "secrets", "{uri}");
    }
    let (status, _, _) = call(&harness.app, Method::GET, "/v1/secrets", None, &[], None).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn metadata_reports_versions_and_key_names_without_values() {
    let mut harness = harness().await;
    let (status, _, body) = call(
        &harness.app,
        Method::GET,
        "/v1/secrets/meta/k8s/test",
        Some(OWNER_TOKEN),
        &[],
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["currentVersion"], 2);
    assert_eq!(body["oldestVersion"], 1);
    assert_eq!(body["versions"].as_array().expect("versions").len(), 2);
    let keys = body["keys"].as_array().expect("keys");
    assert!(keys.iter().any(|key| key == "password"));
    assert!(keys.iter().any(|key| key == "user"));
    assert!(!body.to_string().contains(SECRET_VALUE));
    assert!(pushes(&mut harness.pushes).await.is_empty());
}

#[tokio::test]
async fn reveal_needs_a_device_signed_session_and_disables_caching() {
    let mut harness = harness().await;
    let (status, _, body) = call(
        &harness.app,
        Method::POST,
        "/v1/secrets/reveal/k8s/test",
        Some(OWNER_TOKEN),
        &[],
        Some(json!({"key": "password", "version": 2})),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    assert_eq!(body["error"], "step_up_required");

    let (challenge, payload) = issue(&harness.app, OWNER_TOKEN, json!({"action": "session"})).await;
    let (status, headers, body) = call(
        &harness.app,
        Method::POST,
        "/v1/step-up",
        Some(OWNER_TOKEN),
        &[],
        Some(json!({
            "challenge": challenge,
            "signature": signature(&payload),
        })),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(
        headers
            .get(header::CACHE_CONTROL)
            .and_then(|v| v.to_str().ok()),
        Some("no-store")
    );
    let session = body["session"].as_str().expect("session").to_owned();

    let (status, headers, body) = call(
        &harness.app,
        Method::POST,
        "/v1/secrets/reveal/k8s/test",
        Some(OWNER_TOKEN),
        &[(STEP_UP_SESSION_HEADER, &session)],
        Some(json!({"key": "password", "version": 2})),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["value"], SECRET_VALUE);
    assert_eq!(
        headers
            .get(header::CACHE_CONTROL)
            .and_then(|v| v.to_str().ok()),
        Some("no-store")
    );
    let received = pushes(&mut harness.pushes).await;
    assert_eq!(received.len(), 2, "{received:?}");
    let push = received.last().expect("reveal push");
    assert_eq!(push["module"], "secrets");
    assert_eq!(push["data"]["entity"], "secrets");
    assert_eq!(push["data"]["path"], "k8s/test");
    assert_eq!(push["deepLink"], "home://secrets/path/k8s%2Ftest");
    assert!(!push.to_string().contains(SECRET_VALUE));
}

#[tokio::test(start_paused = true)]
async fn a_step_up_session_expires_after_five_minutes() {
    let harness = harness().await;
    let session = session(&harness).await;
    tokio::time::advance(Duration::from_secs(301)).await;
    let (status, _, body) = call(
        &harness.app,
        Method::POST,
        "/v1/secrets/reveal/k8s/test",
        Some(OWNER_TOKEN),
        &[(STEP_UP_SESSION_HEADER, &session)],
        Some(json!({"key": "password", "version": 2})),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    assert_eq!(body["error"], "step_up_required");
}

#[tokio::test]
async fn a_step_up_session_is_bound_to_the_token_that_started_it() {
    let harness = harness().await;
    let owner_session = session(&harness).await;
    let (status, _, _) = call(
        &harness.app,
        Method::POST,
        "/v1/secrets/reveal/k8s/test",
        Some(OWNER_TOKEN),
        &[(STEP_UP_SESSION_HEADER, &owner_session)],
        Some(json!({"key": "password", "version": 2})),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let other_session = session(&harness).await;
    let (status, _, body) = call(
        &harness.app,
        Method::POST,
        "/v1/secrets/reveal/k8s/test",
        Some(SECOND_OWNER_TOKEN),
        &[(STEP_UP_SESSION_HEADER, &other_session)],
        Some(json!({"key": "password", "version": 2})),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    assert_eq!(body["error"], "step_up_required");
}

#[tokio::test]
async fn update_requires_a_device_signature_and_a_matching_cas() {
    let harness = harness().await;
    let (status, _, body) = call(
        &harness.app,
        Method::POST,
        "/v1/secrets/update/k8s/test",
        Some(OWNER_TOKEN),
        &[],
        Some(json!({"key": "password", "value": "new", "cas": 2})),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    assert_eq!(body["error"], "step_up_required");

    let (challenge, signature) = mutation(
        &harness,
        json!({"action": "update", "path": "k8s/test", "key": "password", "cas": 1}),
    )
    .await;
    let (status, _, body) = call(
        &harness.app,
        Method::POST,
        "/v1/secrets/update/k8s/test",
        Some(OWNER_TOKEN),
        &signed_headers(&challenge, &signature),
        Some(json!({"key": "password", "value": "new", "cas": 1})),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(body["error"], "conflict");
    assert_eq!(body["currentVersion"], 2);
    assert_eq!(harness.openbao.current(), 2);
}

#[tokio::test]
async fn update_refuses_a_soft_deleted_current_version() {
    let harness = harness().await;
    harness.openbao.mark_deleted(2);
    let (challenge, signature) = mutation(
        &harness,
        json!({"action": "update", "path": "k8s/test", "key": "password", "cas": 2}),
    )
    .await;
    let (status, _, body) = call(
        &harness.app,
        Method::POST,
        "/v1/secrets/update/k8s/test",
        Some(OWNER_TOKEN),
        &signed_headers(&challenge, &signature),
        Some(json!({"key": "password", "value": "new", "cas": 2})),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(body["error"], "version_unavailable");
    assert_eq!(harness.openbao.current(), 2);
}

#[tokio::test]
async fn update_writes_a_new_version_without_leaking_the_value() {
    let mut harness = harness().await;
    let (challenge, signature) = mutation(
        &harness,
        json!({"action": "update", "path": "k8s/test", "key": "password", "cas": 2}),
    )
    .await;
    let (status, _, body) = call(
        &harness.app,
        Method::POST,
        "/v1/secrets/update/k8s/test",
        Some(OWNER_TOKEN),
        &signed_headers(&challenge, &signature),
        Some(json!({"key": "password", "value": "rotated", "cas": 2})),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["version"], 3);
    assert_eq!(harness.openbao.current(), 3);
    let data = harness.openbao.version(3);
    assert_eq!(data.get("password").map(String::as_str), Some("rotated"));
    assert_eq!(data.get("user").map(String::as_str), Some("admin"));
    let received = pushes(&mut harness.pushes).await;
    assert!(!received.is_empty());
    assert!(received.iter().all(|push| push["module"] == "secrets"));
    assert!(
        received
            .iter()
            .all(|push| !push.to_string().contains("rotated"))
    );
}

#[tokio::test]
async fn rollback_copies_an_old_version_as_a_new_one() {
    let harness = harness().await;
    let (challenge, signature) = mutation(
        &harness,
        json!({"action": "rollback", "path": "k8s/test", "version": 1, "cas": 2}),
    )
    .await;
    let (status, _, body) = call(
        &harness.app,
        Method::POST,
        "/v1/secrets/rollback/k8s/test",
        Some(OWNER_TOKEN),
        &signed_headers(&challenge, &signature),
        Some(json!({"version": 1, "cas": 2})),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["fromVersion"], 1);
    assert_eq!(body["version"], 3);
    let data = harness.openbao.version(3);
    assert_eq!(data.get("password").map(String::as_str), Some("old"));
    assert_eq!(data.get("user").map(String::as_str), Some("admin"));
}

#[tokio::test]
async fn delete_is_soft_and_never_calls_destroy() {
    let harness = harness().await;
    let (challenge, signature) = mutation(
        &harness,
        json!({"action": "delete", "path": "k8s/test", "version": 2, "cas": 2}),
    )
    .await;
    let (status, _, body) = call(
        &harness.app,
        Method::DELETE,
        "/v1/secrets/k8s/test?cas=2&version=2",
        Some(OWNER_TOKEN),
        &signed_headers(&challenge, &signature),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["deleted"], true);
    assert_eq!(body["version"], 2);
    assert!(harness.openbao.deleted(2));
    let requests = harness.openbao.requests();
    assert!(
        requests
            .iter()
            .any(|(method, path)| method == "DELETE" && path == "secret/data/k8s/test")
    );
    assert!(
        !requests
            .iter()
            .any(|(_, path)| path.starts_with("secret/destroy/"))
    );
}

#[tokio::test]
async fn delete_rejects_a_stale_cas() {
    let harness = harness().await;
    let (challenge, signature) = mutation(
        &harness,
        json!({"action": "delete", "path": "k8s/test", "cas": 1}),
    )
    .await;
    let (status, _, body) = call(
        &harness.app,
        Method::DELETE,
        "/v1/secrets/k8s/test?cas=1",
        Some(OWNER_TOKEN),
        &signed_headers(&challenge, &signature),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(body["error"], "conflict");
    assert!(!harness.openbao.deleted(2));
}

#[tokio::test]
async fn an_openbao_cas_race_is_reported_as_conflict() {
    let harness = harness().await;
    harness.openbao.reject_next_write();
    let (challenge, signature) = mutation(
        &harness,
        json!({"action": "update", "path": "k8s/test", "key": "password", "cas": 2}),
    )
    .await;
    let (status, _, body) = call(
        &harness.app,
        Method::POST,
        "/v1/secrets/update/k8s/test",
        Some(OWNER_TOKEN),
        &signed_headers(&challenge, &signature),
        Some(json!({"key": "password", "value": "new", "cas": 2})),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(body["error"], "conflict");
    assert_eq!(harness.openbao.current(), 2);
}

#[tokio::test]
async fn denied_paths_are_rejected_before_openbao_is_called() {
    let mut harness = harness().await;
    call(
        &harness.app,
        Method::GET,
        "/v1/secrets",
        Some(OWNER_TOKEN),
        &[],
        None,
    )
    .await;
    let before = harness.openbao.requests().len();
    let (status, _, body) = call(
        &harness.app,
        Method::GET,
        "/v1/secrets/meta/infra/vpn/client-ca",
        Some(OWNER_TOKEN),
        &[],
        None,
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    assert_eq!(body["error"], "path_forbidden");
    let (status, _, _) = call(
        &harness.app,
        Method::POST,
        "/v1/secrets/reveal/openbao/seal",
        Some(OWNER_TOKEN),
        &[],
        Some(json!({"key": "value", "version": 1})),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    assert_eq!(harness.openbao.requests().len(), before);
    let received = pushes(&mut harness.pushes).await;
    assert!(
        received
            .iter()
            .any(|push| push.to_string().contains("путь запрещён"))
    );
}

#[tokio::test]
async fn traversal_and_percent_encoding_are_rejected() {
    let harness = harness().await;
    for uri in [
        "/v1/secrets/meta/%2e%2e%2fsys%2fseal",
        "/v1/secrets/meta/k8s%2f..%2f..%2fsys%2fseal",
        "/v1/secrets/meta/k8s//test",
    ] {
        let (status, _, _) =
            call(&harness.app, Method::GET, uri, Some(OWNER_TOKEN), &[], None).await;
        assert!(
            status == StatusCode::BAD_REQUEST || status == StatusCode::FORBIDDEN,
            "{uri}: {status}"
        );
    }
}

#[tokio::test]
async fn readiness_checks_openbao_and_the_access_snapshot() {
    let harness = harness().await;
    let (status, _, body) = call(&harness.app, Method::GET, "/readyz", None, &[], None).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["status"], "ready");
}

#[tokio::test]
async fn the_device_state_path_is_hidden_and_forbidden() {
    let harness = harness().await;
    let (status, _, body) = call(
        &harness.app,
        Method::GET,
        "/v1/secrets",
        Some(OWNER_TOKEN),
        &[],
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let entries = body["entries"].as_array().expect("entries");
    assert!(
        !entries
            .iter()
            .any(|entry| entry["path"] == device::DEVICE_PATH)
    );
    let (status, _, body) = call(
        &harness.app,
        Method::GET,
        "/v1/secrets/meta/k8s/home-assistant/secrets-gateway-device",
        Some(OWNER_TOKEN),
        &[],
        None,
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    assert_eq!(body["error"], "path_forbidden");
}

#[tokio::test]
async fn a_challenge_is_bound_to_the_exact_operation() {
    let harness = harness().await;
    let (challenge, payload) = issue(
        &harness.app,
        OWNER_TOKEN,
        json!({"action": "update", "path": "k8s/test", "key": "password", "cas": 2}),
    )
    .await;
    let signed = signature(&payload);
    let headers = signed_headers(&challenge, &signed);
    let (status, _, body) = call(
        &harness.app,
        Method::POST,
        "/v1/secrets/update/k8s/other",
        Some(OWNER_TOKEN),
        &headers,
        Some(json!({"key": "password", "value": "new", "cas": 2})),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    assert_eq!(body["error"], "step_up_invalid");

    let (challenge, payload) = issue(
        &harness.app,
        OWNER_TOKEN,
        json!({"action": "update", "path": "k8s/test", "key": "password", "cas": 2}),
    )
    .await;
    let signed = signature(&payload);
    let headers = signed_headers(&challenge, &signed);
    let (status, _, body) = call(
        &harness.app,
        Method::POST,
        "/v1/secrets/update/k8s/test",
        Some(OWNER_TOKEN),
        &headers,
        Some(json!({"key": "user", "value": "new", "cas": 2})),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    assert_eq!(body["error"], "step_up_invalid");

    let (challenge, payload) = issue(
        &harness.app,
        OWNER_TOKEN,
        json!({"action": "update", "path": "k8s/test", "key": "password", "cas": 2}),
    )
    .await;
    let signed = signature(&payload);
    let headers = signed_headers(&challenge, &signed);
    let (status, _, body) = call(
        &harness.app,
        Method::POST,
        "/v1/secrets/rollback/k8s/test",
        Some(OWNER_TOKEN),
        &headers,
        Some(json!({"version": 1, "cas": 2})),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    assert_eq!(body["error"], "step_up_invalid");
    assert_eq!(harness.openbao.current(), 2);
}

#[tokio::test]
async fn a_signed_challenge_cannot_be_replayed() {
    let harness = harness().await;
    let (challenge, payload) = issue(
        &harness.app,
        OWNER_TOKEN,
        json!({"action": "update", "path": "k8s/test", "key": "password", "cas": 2}),
    )
    .await;
    let signed = signature(&payload);
    let headers = signed_headers(&challenge, &signed);
    let (status, _, body) = call(
        &harness.app,
        Method::POST,
        "/v1/secrets/update/k8s/test",
        Some(OWNER_TOKEN),
        &headers,
        Some(json!({"key": "password", "value": "first", "cas": 2})),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let (status, _, body) = call(
        &harness.app,
        Method::POST,
        "/v1/secrets/update/k8s/test",
        Some(OWNER_TOKEN),
        &headers,
        Some(json!({"key": "password", "value": "second", "cas": 2})),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    assert_eq!(body["error"], "step_up_invalid");
    assert_eq!(harness.openbao.current(), 3);
}

#[tokio::test]
async fn a_signature_from_another_key_is_rejected() {
    let harness = harness().await;
    let (challenge, payload) = issue(
        &harness.app,
        OWNER_TOKEN,
        json!({"action": "update", "path": "k8s/test", "key": "password", "cas": 2}),
    )
    .await;
    let signature = sign_with(&other_signing_key(), &payload);
    let headers = signed_headers(&challenge, &signature);
    let (status, _, body) = call(
        &harness.app,
        Method::POST,
        "/v1/secrets/update/k8s/test",
        Some(OWNER_TOKEN),
        &headers,
        Some(json!({"key": "password", "value": "new", "cas": 2})),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    assert_eq!(body["error"], "step_up_invalid");
    assert_eq!(harness.openbao.current(), 2);
}

#[tokio::test]
async fn a_revoked_device_cannot_open_sessions_or_reveal() {
    let harness = harness().await;
    let session = session(&harness).await;
    harness.openbao.clear_device();
    let (status, _, body) = call(
        &harness.app,
        Method::POST,
        "/v1/secrets/reveal/k8s/test",
        Some(OWNER_TOKEN),
        &[(STEP_UP_SESSION_HEADER, &session)],
        Some(json!({"key": "password", "version": 2})),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    assert_eq!(body["error"], "step_up_required");
    let (status, _, body) = call(
        &harness.app,
        Method::POST,
        "/v1/step-up/challenge",
        Some(OWNER_TOKEN),
        &[],
        Some(json!({"action": "session"})),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    assert_eq!(body["error"], "device_not_enrolled");
}

#[tokio::test]
async fn enrollment_binds_a_new_key_once() {
    let harness = harness().await;
    harness.openbao.clear_device();
    harness
        .openbao
        .set_enrollment(&enrollment_hash("ABCD2345EFGH"), unix_now() + 900);
    let key = other_signing_key();
    let public_key = public_key(&key);
    let nonce = attestation(&harness.app, OWNER_TOKEN).await;
    let (status, _, body) = call(
        &harness.app,
        Method::POST,
        "/v1/device/enroll",
        Some(OWNER_TOKEN),
        &[],
        Some(json!({
            "code": "abcd-2345-efgh",
            "publicKey": public_key.clone(),
            "attestationChallenge": nonce,
            "attestationChain": [encoded(b"certificate")],
        })),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["enrolled"], true);
    assert!(body["deviceId"].as_str().is_some_and(|id| !id.is_empty()));
    let stored = harness.openbao.device();
    assert_eq!(stored.get("device_public_key"), Some(&public_key));
    assert!(!stored.contains_key("enrollment_code_hash"));

    let nonce = attestation(&harness.app, OWNER_TOKEN).await;
    let (status, _, body) = call(
        &harness.app,
        Method::POST,
        "/v1/device/enroll",
        Some(OWNER_TOKEN),
        &[],
        Some(json!({
            "code": "ABCD2345EFGH",
            "publicKey": public_key,
            "attestationChallenge": nonce,
            "attestationChain": [encoded(b"certificate")],
        })),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    assert_eq!(body["error"], "enrollment_closed");

    let (challenge, payload) = issue(&harness.app, OWNER_TOKEN, json!({"action": "session"})).await;
    let (status, _, body) = call(
        &harness.app,
        Method::POST,
        "/v1/step-up",
        Some(OWNER_TOKEN),
        &[],
        Some(json!({
            "challenge": challenge,
            "signature": sign_with(&key, &payload),
        })),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
}

#[tokio::test]
async fn enrollment_expiry_is_enforced() {
    let harness = harness().await;
    harness.openbao.clear_device();
    harness
        .openbao
        .set_enrollment(&enrollment_hash("ABCD2345EFGH"), unix_now() - 1);
    let nonce = attestation(&harness.app, OWNER_TOKEN).await;
    let (status, _, body) = call(
        &harness.app,
        Method::POST,
        "/v1/device/enroll",
        Some(OWNER_TOKEN),
        &[],
        Some(json!({
            "code": "ABCD2345EFGH",
            "publicKey": public_key(&signing_key()),
            "attestationChallenge": nonce,
            "attestationChain": [encoded(b"certificate")],
        })),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    assert_eq!(body["error"], "enrollment_expired");
}

#[tokio::test]
async fn enrollment_is_single_use_and_rate_limited() {
    let harness = harness().await;
    harness.openbao.clear_device();
    harness
        .openbao
        .set_enrollment(&enrollment_hash("ABCD2345EFGH"), unix_now() + 900);
    let mut last = StatusCode::OK;
    for _ in 0..5 {
        let nonce = attestation(&harness.app, OWNER_TOKEN).await;
        let (status, _, _) = call(
            &harness.app,
            Method::POST,
            "/v1/device/enroll",
            Some(OWNER_TOKEN),
            &[],
            Some(json!({
                "code": "WRONGCODE",
                "publicKey": public_key(&signing_key()),
                "attestationChallenge": nonce,
                "attestationChain": [encoded(b"certificate")],
            })),
        )
        .await;
        last = status;
    }
    assert_eq!(last, StatusCode::TOO_MANY_REQUESTS);
    let nonce = attestation(&harness.app, OWNER_TOKEN).await;
    let (status, _, body) = call(
        &harness.app,
        Method::POST,
        "/v1/device/enroll",
        Some(OWNER_TOKEN),
        &[],
        Some(json!({
            "code": "ABCD2345EFGH",
            "publicKey": public_key(&signing_key()),
            "attestationChallenge": nonce,
            "attestationChain": [encoded(b"certificate")],
        })),
    )
    .await;
    assert_eq!(status, StatusCode::TOO_MANY_REQUESTS);
    assert_eq!(body["error"], "rate_limited");
}

#[tokio::test]
async fn enrollment_rejects_a_missing_code_a_bad_key_and_an_empty_chain() {
    let harness = harness().await;
    harness.openbao.clear_device();
    let nonce = attestation(&harness.app, OWNER_TOKEN).await;
    let (status, _, body) = call(
        &harness.app,
        Method::POST,
        "/v1/device/enroll",
        Some(OWNER_TOKEN),
        &[],
        Some(json!({
            "code": "ABCD2345EFGH",
            "publicKey": public_key(&signing_key()),
            "attestationChallenge": nonce,
            "attestationChain": [encoded(b"certificate")],
        })),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    assert_eq!(body["error"], "enrollment_closed");

    harness
        .openbao
        .set_enrollment(&enrollment_hash("ABCD2345EFGH"), unix_now() + 900);
    let nonce = attestation(&harness.app, OWNER_TOKEN).await;
    let (status, _, body) = call(
        &harness.app,
        Method::POST,
        "/v1/device/enroll",
        Some(OWNER_TOKEN),
        &[],
        Some(json!({
            "code": "ABCD2345EFGH",
            "publicKey": "not-a-key",
            "attestationChallenge": nonce,
            "attestationChain": [encoded(b"certificate")],
        })),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["error"], "public_key_invalid");

    let nonce = attestation(&harness.app, OWNER_TOKEN).await;
    let (status, _, body) = call(
        &harness.app,
        Method::POST,
        "/v1/device/enroll",
        Some(OWNER_TOKEN),
        &[],
        Some(json!({
            "code": "ABCD2345EFGH",
            "publicKey": public_key(&signing_key()),
            "attestationChallenge": nonce,
            "attestationChain": [],
        })),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["error"], "attestation_invalid");
}

#[tokio::test]
async fn a_new_enrollment_replaces_the_previous_key() {
    let harness = harness().await;
    harness
        .openbao
        .set_enrollment(&enrollment_hash("ABCD2345EFGH"), unix_now() + 900);
    let new_key = other_signing_key();
    let nonce = attestation(&harness.app, OWNER_TOKEN).await;
    let (status, _, _) = call(
        &harness.app,
        Method::POST,
        "/v1/device/enroll",
        Some(OWNER_TOKEN),
        &[],
        Some(json!({
            "code": "ABCD2345EFGH",
            "publicKey": public_key(&new_key),
            "attestationChallenge": nonce,
            "attestationChain": [encoded(b"certificate")],
        })),
    )
    .await;
    assert_eq!(status, StatusCode::OK);

    let (challenge, payload) = issue(&harness.app, OWNER_TOKEN, json!({"action": "session"})).await;
    let (status, _, body) = call(
        &harness.app,
        Method::POST,
        "/v1/step-up",
        Some(OWNER_TOKEN),
        &[],
        Some(json!({
            "challenge": challenge,
            "signature": signature(&payload),
        })),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    assert_eq!(body["error"], "step_up_invalid");

    let (challenge, payload) = issue(&harness.app, OWNER_TOKEN, json!({"action": "session"})).await;
    let (status, _, body) = call(
        &harness.app,
        Method::POST,
        "/v1/step-up",
        Some(OWNER_TOKEN),
        &[],
        Some(json!({
            "challenge": challenge,
            "signature": sign_with(&new_key, &payload),
        })),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
}

#[tokio::test]
async fn enrollment_needs_a_fresh_attestation_challenge() {
    let harness = harness().await;
    harness.openbao.clear_device();
    harness
        .openbao
        .set_enrollment(&enrollment_hash("ABCD2345EFGH"), unix_now() + 900);
    let nonce = attestation(&harness.app, OWNER_TOKEN).await;
    let body = json!({
        "code": "WRONGCODE",
        "publicKey": public_key(&signing_key()),
        "attestationChallenge": nonce,
        "attestationChain": [encoded(b"certificate")],
    });
    let (status, _, _) = call(
        &harness.app,
        Method::POST,
        "/v1/device/enroll",
        Some(OWNER_TOKEN),
        &[],
        Some(body.clone()),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    let (status, _, body) = call(
        &harness.app,
        Method::POST,
        "/v1/device/enroll",
        Some(OWNER_TOKEN),
        &[],
        Some(body),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    assert_eq!(body["error"], "attestation_challenge_invalid");
}

fn unix_now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("clock")
        .as_secs()
}

#[tokio::test]
async fn a_revoked_device_is_announced_once() {
    let mut harness = harness().await;
    harness.openbao.revoke_device();
    let (status, _, body) = call(
        &harness.app,
        Method::GET,
        "/v1/secrets",
        Some(OWNER_TOKEN),
        &[],
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let received = pushes(&mut harness.pushes).await;
    assert_eq!(received.len(), 1, "{received:?}");
    assert_eq!(received[0]["data"]["entity"], "secrets");
    assert!(received[0]["data"].get("path").is_none());
    assert_eq!(received[0]["deepLink"], "home://secrets/device");
    assert!(
        received[0]["body"]
            .as_str()
            .unwrap_or_default()
            .contains("Отзыв телефона")
    );
    assert!(
        !harness
            .openbao
            .device()
            .contains_key("device_revoke_device_id")
    );

    let (status, _, _) = call(
        &harness.app,
        Method::GET,
        "/v1/secrets",
        Some(OWNER_TOKEN),
        &[],
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert!(pushes(&mut harness.pushes).await.is_empty());
}

#[tokio::test]
async fn repeated_step_up_failures_are_rate_limited() {
    let harness = harness().await;
    let mut last = StatusCode::OK;
    for _ in 0..5 {
        let (challenge, _) = issue(&harness.app, OWNER_TOKEN, json!({"action": "session"})).await;
        let (status, _, _) = call(
            &harness.app,
            Method::POST,
            "/v1/step-up",
            Some(OWNER_TOKEN),
            &[],
            Some(json!({
                "challenge": challenge,
                "signature": sign_with(&other_signing_key(), "wrong"),
            })),
        )
        .await;
        last = status;
    }
    assert_eq!(last, StatusCode::TOO_MANY_REQUESTS);
    let (status, _, body) = call(
        &harness.app,
        Method::POST,
        "/v1/step-up/challenge",
        Some(OWNER_TOKEN),
        &[],
        Some(json!({"action": "session"})),
    )
    .await;
    assert_eq!(status, StatusCode::TOO_MANY_REQUESTS);
    assert_eq!(body["error"], "rate_limited");
    assert!(body["retryAfter"].as_u64().unwrap_or_default() > 0);
}
