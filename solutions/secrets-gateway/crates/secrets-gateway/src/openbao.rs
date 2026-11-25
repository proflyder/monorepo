use std::collections::BTreeMap;
use std::time::{Duration, Instant};

use home_secret::Secret;
use serde::Deserialize;
use serde_json::Value;
use thiserror::Error;
use tokio::sync::Mutex;

const MIN_TOKEN_LIFETIME: u64 = 30;
const TOKEN_SKEW: u64 = 60;
const MAX_LIST_DIRECTORIES: usize = 64;
const MAX_LIST_DEPTH: usize = 5;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
pub enum Error {
    #[error("secret is not found")]
    NotFound,
    #[error("check-and-set conflict")]
    Conflict,
    #[error("OpenBao denied the request")]
    Forbidden,
    #[error("OpenBao is unavailable")]
    Unavailable,
    #[error("OpenBao rejected the request")]
    Invalid,
    #[error("OpenBao returned an unexpected response")]
    Parse,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VersionInfo {
    pub version: u64,
    pub created_time: String,
    pub deletion_time: String,
    pub destroyed: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Metadata {
    pub current_version: u64,
    pub oldest_version: u64,
    pub versions: Vec<VersionInfo>,
}

impl VersionInfo {
    #[must_use]
    pub fn is_available(&self) -> bool {
        !self.destroyed && self.deletion_time.is_empty()
    }
}

impl Metadata {
    #[must_use]
    pub fn version(&self, version: u64) -> Option<&VersionInfo> {
        self.versions.iter().find(|item| item.version == version)
    }

    #[must_use]
    pub fn is_available(&self, version: u64) -> bool {
        self.version(version).is_some_and(VersionInfo::is_available)
    }
}

#[derive(Clone, PartialEq, Eq)]
pub struct SecretVersion {
    pub version: u64,
    pub data: BTreeMap<String, String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ListResult {
    pub paths: Vec<String>,
    pub truncated: bool,
}

impl std::fmt::Debug for SecretVersion {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("SecretVersion")
            .field("version", &self.version)
            .field("keys", &self.data.len())
            .finish()
    }
}

pub struct OpenBao {
    http: reqwest::Client,
    base_url: String,
    role: String,
    jwt_file: String,
    token: Mutex<Option<CachedToken>>,
}

struct CachedToken {
    token: Secret,
    expires_at: Instant,
}

#[derive(Debug, Deserialize)]
struct LoginResponse {
    auth: LoginAuth,
}

#[derive(Debug, Deserialize)]
struct LoginAuth {
    client_token: String,
    #[serde(default)]
    lease_duration: u64,
}

impl OpenBao {
    pub fn new(
        base_url: &str,
        role: &str,
        jwt_file: &str,
        timeout: Duration,
    ) -> Result<Self, String> {
        let http = home_tls::public_https_client()
            .map_err(|error| format!("OpenBao TLS: {error}"))?
            .timeout(timeout)
            .build()
            .map_err(|error| format!("OpenBao HTTP client: {error}"))?;
        Ok(Self {
            http,
            base_url: base_url.trim_end_matches('/').to_owned(),
            role: role.to_owned(),
            jwt_file: jwt_file.to_owned(),
            token: Mutex::new(None),
        })
    }

    pub async fn list_all(
        &self,
        deny: &crate::deny::Deny,
        max_entries: usize,
    ) -> Result<ListResult, Error> {
        let mut paths = Vec::new();
        let mut queue = vec![String::new()];
        let max_directories = MAX_LIST_DIRECTORIES.min(max_entries.saturating_mul(4).max(16));
        let mut visited = 0;
        let mut truncated = false;
        while let Some(prefix) = queue.pop() {
            visited += 1;
            if visited > max_directories {
                truncated = true;
                break;
            }
            let keys = self.list_metadata(&prefix).await?;
            for key in keys {
                let full = format!("{prefix}{key}");
                if deny.is_denied(&full) {
                    continue;
                }
                if key.ends_with('/') {
                    if prefix_depth(&full) >= MAX_LIST_DEPTH {
                        truncated = true;
                        continue;
                    }
                    queue.push(full);
                    continue;
                }
                paths.push(full);
                if paths.len() >= max_entries {
                    truncated = true;
                    return Ok(ListResult {
                        paths: sorted(paths),
                        truncated,
                    });
                }
            }
        }
        Ok(ListResult {
            paths: sorted(paths),
            truncated,
        })
    }

    pub async fn health(&self) -> Result<(), Error> {
        let response = self
            .http
            .get(format!("{}/v1/sys/health", self.base_url))
            .send()
            .await
            .map_err(|_| Error::Unavailable)?;
        if response.status().is_success() || response.status().as_u16() == 429 {
            return Ok(());
        }
        Err(Error::Unavailable)
    }

    pub async fn list_metadata(&self, prefix: &str) -> Result<Vec<String>, Error> {
        let path = match prefix.is_empty() {
            true => "secret/metadata".to_owned(),
            false => format!("secret/metadata/{prefix}"),
        };
        let value = match self.send(list_method(), &path, None).await {
            Ok(value) => value,
            Err(Error::NotFound) => return Ok(Vec::new()),
            Err(error) => return Err(error),
        };
        let keys = value
            .get("data")
            .and_then(|data| data.get("keys"))
            .and_then(Value::as_array)
            .map_or_else(Vec::new, |items| {
                items
                    .iter()
                    .filter_map(Value::as_str)
                    .map(str::to_owned)
                    .collect()
            });
        Ok(keys)
    }

    pub async fn metadata(&self, path: &str) -> Result<Metadata, Error> {
        let value = self
            .send(
                reqwest::Method::GET,
                &format!("secret/metadata/{path}"),
                None,
            )
            .await?;
        let data = value.get("data").ok_or(Error::Parse)?;
        let current_version = unsigned(data, "current_version");
        let oldest_version = unsigned(data, "oldest_version");
        let mut versions =
            data.get("versions")
                .and_then(Value::as_object)
                .map_or_else(Vec::new, |items| {
                    items
                        .iter()
                        .filter_map(|(key, value)| version_info(key, value))
                        .collect()
                });
        versions.sort_by_key(|item| item.version);
        Ok(Metadata {
            current_version,
            oldest_version,
            versions,
        })
    }

    pub async fn read(&self, path: &str, version: Option<u64>) -> Result<SecretVersion, Error> {
        let path = version.map_or_else(
            || format!("secret/data/{path}"),
            |version| format!("secret/data/{path}?version={version}"),
        );
        let value = self.send(reqwest::Method::GET, &path, None).await?;
        let data = value.get("data").ok_or(Error::Parse)?;
        let metadata = data.get("metadata").ok_or(Error::Parse)?;
        let version = metadata
            .get("version")
            .and_then(Value::as_u64)
            .ok_or(Error::Parse)?;
        let values = data
            .get("data")
            .and_then(Value::as_object)
            .ok_or(Error::Parse)?;
        let mut data = BTreeMap::new();
        for (key, value) in values {
            let text = value.as_str().ok_or(Error::Parse)?;
            data.insert(key.clone(), text.to_owned());
        }
        Ok(SecretVersion { version, data })
    }

    pub async fn write(
        &self,
        path: &str,
        data: &BTreeMap<String, String>,
        cas: u64,
    ) -> Result<u64, Error> {
        let payload = serde_json::json!({
            "data": data,
            "options": { "cas": cas },
        });
        let value = self
            .send(
                reqwest::Method::POST,
                &format!("secret/data/{path}"),
                Some(&payload),
            )
            .await?;
        Ok(value
            .get("data")
            .and_then(|data| data.get("version"))
            .and_then(Value::as_u64)
            .unwrap_or(cas.saturating_add(1)))
    }

    pub async fn delete_latest(&self, path: &str) -> Result<(), Error> {
        self.send(
            reqwest::Method::DELETE,
            &format!("secret/data/{path}"),
            None,
        )
        .await?;
        Ok(())
    }

    async fn send(
        &self,
        method: reqwest::Method,
        path: &str,
        body: Option<&Value>,
    ) -> Result<Value, Error> {
        match self.send_once(&method, path, body).await {
            Ok(value) => Ok(value),
            Err(Error::Forbidden) => {
                self.forget_token().await;
                self.send_once(&method, path, body).await
            }
            Err(error) => Err(error),
        }
    }

    async fn send_once(
        &self,
        method: &reqwest::Method,
        path: &str,
        body: Option<&Value>,
    ) -> Result<Value, Error> {
        let token = self.token().await?;
        let mut request = self
            .http
            .request(method.clone(), format!("{}/v1/{path}", self.base_url))
            .header("X-Vault-Token", token.expose());
        if let Some(body) = body {
            let payload = serde_json::to_vec(body).map_err(|_| Error::Parse)?;
            request = request
                .header(reqwest::header::CONTENT_TYPE, "application/json")
                .body(payload);
        }
        let response = request.send().await.map_err(|_| Error::Unavailable)?;
        let status = response.status();
        if status.as_u16() == 404 {
            return Err(Error::NotFound);
        }
        if status.as_u16() == 403 {
            return Err(Error::Forbidden);
        }
        if status.as_u16() == 400 {
            let text = response.text().await.unwrap_or_default();
            if text.contains("check-and-set") {
                return Err(Error::Conflict);
            }
            return Err(Error::Invalid);
        }
        if !status.is_success() {
            return Err(Error::Unavailable);
        }
        if status.as_u16() == 204 {
            return Ok(Value::Null);
        }
        let text = response.text().await.map_err(|_| Error::Parse)?;
        if text.trim().is_empty() {
            return Ok(Value::Null);
        }
        serde_json::from_str(&text).map_err(|_| Error::Parse)
    }

    async fn token(&self) -> Result<Secret, Error> {
        let mut guard = self.token.lock().await;
        if let Some(cached) = guard.as_ref()
            && cached.expires_at > Instant::now()
        {
            return Ok(cached.token.clone());
        }
        let (token, expires_at) = self.login().await?;
        *guard = Some(CachedToken {
            token: token.clone(),
            expires_at,
        });
        drop(guard);
        Ok(token)
    }

    async fn login(&self) -> Result<(Secret, Instant), Error> {
        let jwt = std::fs::read_to_string(&self.jwt_file).map_err(|_| Error::Unavailable)?;
        let jwt = jwt.trim();
        if jwt.is_empty() {
            return Err(Error::Unavailable);
        }
        let payload = serde_json::to_vec(&serde_json::json!({
            "role": self.role,
            "jwt": jwt,
        }))
        .map_err(|_| Error::Parse)?;
        let response = self
            .http
            .post(format!("{}/v1/auth/kubernetes/login", self.base_url))
            .header(reqwest::header::CONTENT_TYPE, "application/json")
            .body(payload)
            .send()
            .await
            .map_err(|_| Error::Unavailable)?;
        let status = response.status();
        if status.as_u16() == 401 || status.as_u16() == 403 {
            return Err(Error::Forbidden);
        }
        if !status.is_success() {
            return Err(Error::Unavailable);
        }
        let text = response.text().await.map_err(|_| Error::Parse)?;
        let body: LoginResponse = serde_json::from_str(&text).map_err(|_| Error::Parse)?;
        let lease = body.auth.lease_duration.max(MIN_TOKEN_LIFETIME);
        let lifetime = lease.saturating_sub(TOKEN_SKEW).max(MIN_TOKEN_LIFETIME);
        let expires_at = Instant::now() + Duration::from_secs(lifetime);
        Ok((Secret::new(body.auth.client_token), expires_at))
    }

    async fn forget_token(&self) {
        let mut guard = self.token.lock().await;
        *guard = None;
    }
}

fn list_method() -> reqwest::Method {
    reqwest::Method::from_bytes(b"LIST").expect("LIST is a valid method")
}

fn unsigned(data: &Value, field: &str) -> u64 {
    data.get(field).and_then(Value::as_u64).unwrap_or(0)
}

fn version_info(key: &str, value: &Value) -> Option<VersionInfo> {
    let version = key.parse::<u64>().ok()?;
    Some(VersionInfo {
        version,
        created_time: text(value, "created_time"),
        deletion_time: text(value, "deletion_time"),
        destroyed: value
            .get("destroyed")
            .and_then(Value::as_bool)
            .unwrap_or(false),
    })
}

fn text(value: &Value, field: &str) -> String {
    value
        .get(field)
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_owned()
}

fn prefix_depth(prefix: &str) -> usize {
    prefix
        .trim_end_matches('/')
        .split('/')
        .filter(|segment| !segment.is_empty())
        .count()
}

fn sorted(mut paths: Vec<String>) -> Vec<String> {
    paths.sort();
    paths.dedup();
    paths
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn metadata_versions_are_parsed_and_sorted() {
        let metadata = serde_json::json!({
            "current_version": 3,
            "oldest_version": 1,
            "versions": {
                "3": {"created_time": "c3", "deletion_time": "", "destroyed": false},
                "1": {"created_time": "c1", "deletion_time": "d1", "destroyed": false},
                "2": {"created_time": "c2", "deletion_time": "", "destroyed": true}
            }
        });
        let versions = metadata
            .get("versions")
            .and_then(Value::as_object)
            .map(|items| {
                items
                    .iter()
                    .filter_map(|(key, value)| version_info(key, value))
                    .collect::<Vec<_>>()
            })
            .expect("versions");
        assert_eq!(versions.len(), 3);
        assert_eq!(text(&metadata, "current_version"), "");
        assert_eq!(unsigned(&metadata, "current_version"), 3);
    }

    #[test]
    fn a_list_body_without_keys_is_empty() {
        let value = serde_json::json!({"data": {}});
        let keys: Vec<String> = value
            .get("data")
            .and_then(|data| data.get("keys"))
            .and_then(Value::as_array)
            .map_or_else(Vec::new, |items| {
                items
                    .iter()
                    .filter_map(Value::as_str)
                    .map(str::to_owned)
                    .collect()
            });
        assert!(keys.is_empty());
    }

    #[test]
    fn the_list_method_is_upper_case() {
        assert_eq!(list_method().as_str(), "LIST");
    }

    #[tokio::test]
    async fn a_rotated_service_account_token_is_read_on_every_login() {
        use axum::extract::State;
        use axum::routing::{get, post};
        use std::sync::{Arc, Mutex};

        #[derive(Clone)]
        struct Seen(Arc<Mutex<Vec<String>>>);

        async fn login(
            State(seen): State<Seen>,
            axum::Json(body): axum::Json<Value>,
        ) -> axum::Json<Value> {
            let jwt = body["jwt"].as_str().unwrap_or_default().to_owned();
            seen.0.lock().expect("seen").push(jwt);
            axum::Json(serde_json::json!({
                "auth": {"client_token": "bao-token", "lease_duration": 3600}
            }))
        }

        async fn metadata() -> axum::Json<Value> {
            axum::Json(serde_json::json!({
                "data": {"current_version": 1, "oldest_version": 1, "versions": {}}
            }))
        }

        home_tls::install_crypto_provider();
        let seen = Seen(Arc::new(Mutex::new(Vec::new())));
        let app = axum::Router::new()
            .route("/v1/auth/kubernetes/login", post(login))
            .route("/v1/secret/metadata/x", get(metadata))
            .with_state(seen.clone());
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind");
        let address = listener.local_addr().expect("addr");
        tokio::spawn(async move {
            let _ = axum::serve(listener, app).await;
        });
        let directory = tempfile::tempdir().expect("tempdir");
        let token_file = directory.path().join("token");
        std::fs::write(&token_file, "jwt-one\n").expect("first token");
        let openbao = OpenBao::new(
            &format!("http://{address}"),
            "role",
            token_file.to_str().expect("path"),
            Duration::from_secs(5),
        )
        .expect("openbao");
        openbao.metadata("x").await.expect("first login");
        openbao.forget_token().await;
        std::fs::write(&token_file, "jwt-two\n").expect("second token");
        openbao.metadata("x").await.expect("second login");
        assert_eq!(
            seen.0.lock().expect("seen").as_slice(),
            ["jwt-one", "jwt-two"]
        );
    }

    #[tokio::test]
    async fn a_forbidden_response_relogs_once_and_reuses_the_new_token() {
        use axum::extract::State;
        use axum::http::StatusCode;
        use axum::response::IntoResponse;
        use axum::routing::{get, post};
        use std::sync::{Arc, Mutex};

        #[derive(Clone)]
        struct Counters {
            logins: Arc<Mutex<usize>>,
            reads: Arc<Mutex<usize>>,
        }

        async fn login(
            State(counters): State<Counters>,
            axum::Json(body): axum::Json<Value>,
        ) -> axum::Json<Value> {
            let _ = body;
            *counters.logins.lock().expect("logins") += 1;
            axum::Json(serde_json::json!({
                "auth": {"client_token": "bao-token", "lease_duration": 3600}
            }))
        }

        async fn metadata(State(counters): State<Counters>) -> axum::response::Response {
            let mut reads = counters.reads.lock().expect("reads");
            *reads += 1;
            let first = *reads == 1;
            drop(reads);
            if first {
                return StatusCode::FORBIDDEN.into_response();
            }
            axum::Json(serde_json::json!({
                "data": {"current_version": 1, "oldest_version": 1, "versions": {}}
            }))
            .into_response()
        }

        home_tls::install_crypto_provider();
        let counters = Counters {
            logins: Arc::new(Mutex::new(0)),
            reads: Arc::new(Mutex::new(0)),
        };
        let app = axum::Router::new()
            .route("/v1/auth/kubernetes/login", post(login))
            .route("/v1/secret/metadata/x", get(metadata))
            .with_state(counters.clone());
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind");
        let address = listener.local_addr().expect("addr");
        tokio::spawn(async move {
            let _ = axum::serve(listener, app).await;
        });
        let directory = tempfile::tempdir().expect("tempdir");
        let token_file = directory.path().join("token");
        std::fs::write(&token_file, "jwt-one\n").expect("token");
        let openbao = OpenBao::new(
            &format!("http://{address}"),
            "role",
            token_file.to_str().expect("path"),
            Duration::from_secs(5),
        )
        .expect("openbao");
        openbao.metadata("x").await.expect("metadata");
        assert_eq!(*counters.logins.lock().expect("logins"), 2);
        assert_eq!(*counters.reads.lock().expect("reads"), 2);
    }

    #[test]
    fn a_secret_version_never_prints_its_values() {
        let version = SecretVersion {
            version: 3,
            data: BTreeMap::from([("password".to_owned(), "hidden".to_owned())]),
        };
        let debug = format!("{version:?}");
        assert!(debug.contains("3"));
        assert!(debug.contains("keys"));
        assert!(!debug.contains("hidden"));
        assert!(!debug.contains("password"));
    }
}
