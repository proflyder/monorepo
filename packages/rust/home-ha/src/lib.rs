//! Narrow Home Assistant client shared by the home services.
//!
//! It calls scripts and services, reads one entity state or the whole state
//! list, renders a template and probes `/api/`. The entity registry has no REST
//! endpoint, so [`Client::entity_registry`] opens one short WebSocket
//! connection (`/api/websocket`), authenticates and asks for
//! `config/entity_registry/list`. The scheduler owns schedule state; Home
//! Assistant only performs the side effect, so nothing here knows about
//! schedules, queues or MCP.
//!
//! A private Home Assistant certificate is supplied as PEM bytes or a file
//! path ([`Client::new_with_ca_pem`], [`Client::new_with_ca_file`]). The crate
//! never reads `HA_CA_FILE` itself: a consumer resolves its own environment.
//!
//! The crate is project-agnostic: callers pass entity ids, scripts and
//! variables in. The bearer token is a [`home_secret::Token`] and never appears
//! in `Debug`, an error message or a log line.
//!
//! Failures are classified exactly like `packages/go/httpapi`:
//! [`Error::kind`] is [`ErrorKind::Transient`] for network failures, timeouts,
//! `408`, `425`, `429` and `5xx` (a retried lease may succeed), and
//! [`ErrorKind::Permanent`] for every other `4xx` and for a malformed answer.

use std::fmt;
use std::sync::Arc;
use std::time::Duration;

use futures_util::{SinkExt as _, StreamExt as _};
use home_secret::Token;
use rustls_pki_types::CertificateDer;
use rustls_pki_types::pem::PemObject as _;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use tokio_tungstenite::Connector;
use tokio_tungstenite::tungstenite::Message;

const STATES_PATH: &str = "/api/states";
const SERVICE_PATH: &str = "/api/services";
const TEMPLATE_PATH: &str = "/api/template";
const PING_PATH: &str = "/api/";
const WEBSOCKET_PATH: &str = "/api/websocket";
const REGISTRY_MESSAGE_ID: i64 = 1;
const MAX_STATE_BYTES: usize = 1 << 20;
const MAX_STATES_BYTES: usize = 8 << 20;
const MAX_MESSAGE: usize = 200;

/// How a failed call may be retried.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorKind {
    /// A network failure, timeout, throttling or a `5xx`: retrying later may
    /// succeed.
    Transient,
    /// A `4xx`, a malformed answer or an invalid argument: retrying the same
    /// call cannot succeed.
    Permanent,
}

/// One Home Assistant call failure. It carries no URL, headers or response
/// body: only the operation, a safe status/reason and a bounded message.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("{op}: request failed")]
    Transport {
        op: &'static str,
        #[source]
        source: reqwest::Error,
    },
    #[error("{op}: HTTP {status}")]
    Status {
        op: &'static str,
        status: u16,
        message: String,
    },
    #[error("{op}: invalid response")]
    Decode { op: &'static str },
    #[error("{op}: {detail}")]
    Invalid { op: &'static str, detail: String },
    #[error("{op}: unusable CA")]
    Certificate { op: &'static str },
    #[error("{op}: websocket failed")]
    WebSocket { op: &'static str },
}

impl Error {
    /// Whether a retry may succeed.
    #[must_use]
    pub fn kind(&self) -> ErrorKind {
        match self {
            // A dropped WebSocket, a timeout or a closed listener may recover.
            Self::Transport { .. } | Self::WebSocket { .. } => ErrorKind::Transient,
            Self::Status { status, .. } => match *status {
                408 | 425 | 429 => ErrorKind::Transient,
                status if status >= 500 => ErrorKind::Transient,
                _ => ErrorKind::Permanent,
            },
            Self::Decode { .. } | Self::Invalid { .. } | Self::Certificate { .. } => {
                ErrorKind::Permanent
            }
        }
    }

    /// Convenience predicate for the worker loop.
    #[must_use]
    pub fn is_transient(&self) -> bool {
        self.kind() == ErrorKind::Transient
    }
}

/// A Home Assistant REST client with a fixed base URL and timeout.
#[derive(Clone)]
pub struct Client {
    http: reqwest::Client,
    base_url: String,
    token: Token,
    timeout: Duration,
    /// PEM bundle used for the private-CA WebSocket connector; never logged.
    ca_pem: Option<Vec<u8>>,
}

impl fmt::Debug for Client {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("home_ha::Client")
            .field("base_url", &self.base_url)
            .field("token", &self.token)
            .finish()
    }
}

impl Client {
    /// Build a client. `base_url` must not contain credentials or a path.
    ///
    /// The workspace builds `reqwest` with rustls and without a default crypto
    /// provider; building a client panics ("No provider set") unless one is
    /// installed. The process-wide ring provider is installed once here; a
    /// provider installed earlier is kept and the error is ignored, so this is
    /// safe to call for every client.
    pub fn new(
        base_url: impl Into<String>,
        token: Token,
        timeout: Duration,
    ) -> Result<Self, Error> {
        Self::build(base_url, token, timeout, None)
    }

    /// Build a client that trusts the given PEM certificate bundle in
    /// addition to the client's roots.
    ///
    /// The workspace `reqwest` uses `rustls-tls-manual-roots`, so the bundle
    /// is what makes a private Home Assistant certificate usable; public
    /// CAs are not compiled in.
    pub fn new_with_ca_pem(
        base_url: impl Into<String>,
        token: Token,
        timeout: Duration,
        ca_pem: &[u8],
    ) -> Result<Self, Error> {
        Self::build(base_url, token, timeout, Some(ca_pem))
    }

    /// Like [`Client::new_with_ca_pem`], but reads the PEM bundle from `path`.
    /// The value is never logged.
    pub fn new_with_ca_file(
        base_url: impl Into<String>,
        token: Token,
        timeout: Duration,
        path: impl AsRef<std::path::Path>,
    ) -> Result<Self, Error> {
        let pem = std::fs::read(path).map_err(|_| Error::Certificate { op: "read CA file" })?;
        Self::build(base_url, token, timeout, Some(&pem))
    }

    fn build(
        base_url: impl Into<String>,
        token: Token,
        timeout: Duration,
        ca_pem: Option<&[u8]>,
    ) -> Result<Self, Error> {
        let _already_installed = rustls::crypto::ring::default_provider().install_default();
        let ca_pem = ca_pem.map(<[u8]>::to_vec);
        let mut builder = reqwest::Client::builder()
            .timeout(timeout)
            // An authenticated client must not forward its bearer to a
            // redirect target; a 3xx is surfaced as a status error instead.
            .redirect(reqwest::redirect::Policy::none());
        if let Some(ca_pem) = ca_pem.as_deref() {
            let certificates = reqwest::Certificate::from_pem_bundle(ca_pem)
                .map_err(|_| Error::Certificate { op: "decode CA" })?;
            if certificates.is_empty() {
                return Err(Error::Certificate { op: "decode CA" });
            }
            for certificate in certificates {
                builder = builder.add_root_certificate(certificate);
            }
        }
        let http = builder.build().map_err(|source| Error::Transport {
            op: "build client",
            source,
        })?;
        Ok(Self {
            http,
            base_url: base_url.into().trim().trim_end_matches('/').to_owned(),
            token,
            timeout,
            ca_pem,
        })
    }

    /// Read the state string of one entity. An unknown entity, a non-object
    /// body or a missing/non-string state is [`Error::Decode`].
    pub async fn state(&self, entity: &str) -> Result<String, Error> {
        const OP: &str = "read state";
        let entity = entity.trim();
        if !valid_entity(entity) {
            return Err(Error::Invalid {
                op: OP,
                detail: "empty or invalid entity".to_owned(),
            });
        }
        let url = format!("{}{STATES_PATH}/{entity}", self.base_url);
        let request = self
            .http
            .get(url)
            .bearer_auth(self.token.expose())
            .build()
            .map_err(|source| Error::Transport { op: OP, source })?;
        let response = self.send(OP, request).await?;
        let body = response
            .bytes()
            .await
            .map_err(|source| Error::Transport { op: OP, source })?;
        if body.len() > MAX_STATE_BYTES {
            return Err(Error::Decode { op: OP });
        }
        let payload: StatePayload =
            serde_json::from_slice(&body).map_err(|_| Error::Decode { op: OP })?;
        payload.state.ok_or(Error::Decode { op: OP })
    }

    /// Read every entity with its state and attributes.
    ///
    /// The response is the Home Assistant `/api/states` array. Known
    /// attributes are typed in [`EntityAttributes`]; everything else is kept
    /// in [`EntityAttributes::other`] as raw JSON, so a caller never loses a
    /// field it does not model yet.
    pub async fn entities(&self) -> Result<Vec<Entity>, Error> {
        const OP: &str = "read entities";
        let request = self
            .http
            .get(format!("{}{STATES_PATH}", self.base_url))
            .bearer_auth(self.token.expose())
            .build()
            .map_err(|source| Error::Transport { op: OP, source })?;
        let response = self.send(OP, request).await?;
        let body = response
            .bytes()
            .await
            .map_err(|source| Error::Transport { op: OP, source })?;
        if body.len() > MAX_STATES_BYTES {
            return Err(Error::Decode { op: OP });
        }
        serde_json::from_slice(&body).map_err(|_| Error::Decode { op: OP })
    }

    /// Read the entity registry: entity id, owning device and entity category.
    ///
    /// The registry is WebSocket-only, so this opens one short-lived connection
    /// to `/api/websocket`, authenticates with the same bearer and sends
    /// `config/entity_registry/list`. The whole exchange is bounded by the
    /// client timeout. A failure is [`ErrorKind::Transient`]: the caller may
    /// keep what it has and retry on the next refresh.
    pub async fn entity_registry(&self) -> Result<Vec<RegistryEntity>, Error> {
        const OP: &str = "read entity registry";
        tokio::time::timeout(self.timeout, self.registry_exchange(OP))
            .await
            .unwrap_or(Err(Error::WebSocket { op: OP }))
    }

    async fn registry_exchange(&self, op: &'static str) -> Result<Vec<RegistryEntity>, Error> {
        let url = self.websocket_url(op)?;
        let connector = if url.starts_with("wss://") {
            self.tls_connector(op)?
        } else {
            None
        };
        let (mut socket, _response) = match connector {
            Some(connector) => {
                tokio_tungstenite::connect_async_tls_with_config(url, None, false, Some(connector))
                    .await
            }
            None => tokio_tungstenite::connect_async(url).await,
        }
        .map_err(|_| Error::WebSocket { op })?;

        let hello = read_text(&mut socket, op).await?;
        let hello: Value = serde_json::from_str(&hello).map_err(|_| Error::Decode { op })?;
        if hello.get("type").and_then(Value::as_str) != Some("auth_required") {
            return Err(Error::Decode { op });
        }

        let auth = serde_json::json!({
            "type": "auth",
            "access_token": self.token.expose(),
        });
        send_text(&mut socket, &auth.to_string(), op).await?;

        let reply = read_text(&mut socket, op).await?;
        let reply: Value = serde_json::from_str(&reply).map_err(|_| Error::Decode { op })?;
        match reply.get("type").and_then(Value::as_str) {
            Some("auth_ok") => {}
            Some("auth_invalid") => {
                return Err(Error::Status {
                    op,
                    status: 401,
                    message: "authentication failed".to_owned(),
                });
            }
            _ => return Err(Error::Decode { op }),
        }

        let request = serde_json::json!({
            "id": REGISTRY_MESSAGE_ID,
            "type": "config/entity_registry/list",
        });
        send_text(&mut socket, &request.to_string(), op).await?;
        loop {
            let text = read_text(&mut socket, op).await?;
            if let Some(entities) = registry_reply(&text).map_err(|()| Error::Decode { op })? {
                return Ok(entities);
            }
        }
    }

    /// `http(s)://host:port` → `ws(s)://host:port/api/websocket`.
    fn websocket_url(&self, op: &'static str) -> Result<String, Error> {
        let (scheme, rest) = if let Some(rest) = self.base_url.strip_prefix("https://") {
            ("wss://", rest)
        } else if let Some(rest) = self.base_url.strip_prefix("http://") {
            ("ws://", rest)
        } else {
            return Err(Error::Invalid {
                op,
                detail: "unsupported Home Assistant URL".to_owned(),
            });
        };
        Ok(format!("{scheme}{rest}{WEBSOCKET_PATH}"))
    }

    /// rustls connector trusting the configured CA bundle. Without a bundle,
    /// `connect_async` falls back to the WebPKI roots.
    fn tls_connector(&self, op: &'static str) -> Result<Option<Connector>, Error> {
        let Some(ca_pem) = self.ca_pem.as_deref() else {
            return Ok(None);
        };
        let mut roots = rustls::RootCertStore::empty();
        for certificate in CertificateDer::pem_slice_iter(ca_pem) {
            let certificate = certificate.map_err(|_| Error::Certificate { op })?;
            roots
                .add(certificate)
                .map_err(|_| Error::Certificate { op })?;
        }
        if roots.is_empty() {
            return Err(Error::Certificate { op });
        }
        let config = rustls::ClientConfig::builder()
            .with_root_certificates(roots)
            .with_no_client_auth();
        Ok(Some(Connector::Rustls(Arc::new(config))))
    }

    /// Invoke `<domain>.<service>` with any serializable JSON data. The caller
    /// decides which call is allowed; this client performs no allowlisting of
    /// its own.
    ///
    /// Typed request structs implement `Serialize`, so a caller does not have
    /// to build a [`Map`] by hand; `Map<String, Value>` still works.
    pub async fn call_service<D: Serialize>(
        &self,
        domain: &str,
        service: &str,
        data: D,
    ) -> Result<(), Error> {
        const OP: &str = "call service";
        let (domain, service) = validate_service(OP, domain, service)?;
        let body = serde_json::to_value(data).map_err(|_| Error::Invalid {
            op: OP,
            detail: "encode request".to_owned(),
        })?;
        if !body.is_object() {
            return Err(Error::Invalid {
                op: OP,
                detail: "service data must be a JSON object".to_owned(),
            });
        }
        let url = format!("{SERVICE_PATH}/{domain}/{service}");
        self.post_json(OP, &url, body).await
    }

    /// Invoke `<domain>.<service>` on one entity with typed extra data. The
    /// `data` object is merged with `entity_id`.
    pub async fn call_service_on<D: Serialize>(
        &self,
        domain: &str,
        service: &str,
        entity: &str,
        data: D,
    ) -> Result<(), Error> {
        const OP: &str = "call service";
        validate_service(OP, domain, service)?;
        let entity = entity.trim();
        if !valid_entity(entity) {
            return Err(Error::Invalid {
                op: OP,
                detail: "empty or invalid entity".to_owned(),
            });
        }
        let Value::Object(mut body) = serde_json::to_value(data).map_err(|_| Error::Invalid {
            op: OP,
            detail: "encode request".to_owned(),
        })?
        else {
            return Err(Error::Invalid {
                op: OP,
                detail: "service data must be a JSON object".to_owned(),
            });
        };
        body.insert("entity_id".to_owned(), Value::String(entity.to_owned()));
        self.call_service(domain, service, body).await
    }

    /// Run a script entity with variables. `script` may be a bare script name
    /// or a full `script.<name>` entity id.
    pub async fn call_script(
        &self,
        script: &str,
        variables: Map<String, Value>,
    ) -> Result<(), Error> {
        const OP: &str = "call script";
        let entity = script_entity(script);
        if entity == "script." {
            return Err(Error::Invalid {
                op: OP,
                detail: "empty script entity".to_owned(),
            });
        }
        let mut body = Map::new();
        body.insert("entity_id".to_owned(), Value::String(entity));
        if !variables.is_empty() {
            body.insert("variables".to_owned(), Value::Object(variables));
        }
        self.post_json(
            OP,
            &format!("{SERVICE_PATH}/script/turn_on"),
            Value::Object(body),
        )
        .await
    }

    /// Render a Home Assistant template and return the text. The template is
    /// always built by the caller, never taken from an untrusted client.
    pub async fn render_template(&self, template: &str) -> Result<String, Error> {
        const OP: &str = "render template";
        let mut body = Map::new();
        body.insert("template".to_owned(), Value::String(template.to_owned()));
        let payload = encode(OP, &Value::Object(body))?;
        let request = self
            .http
            .post(format!("{}{TEMPLATE_PATH}", self.base_url))
            .bearer_auth(self.token.expose())
            .header("content-type", "application/json")
            .body(payload)
            .build()
            .map_err(|source| Error::Transport { op: OP, source })?;
        let response = self.send(OP, request).await?;
        response
            .text()
            .await
            .map_err(|source| Error::Transport { op: OP, source })
    }

    /// Verify HTTP reachability and the token with the read-only `/api/`
    /// endpoint.
    pub async fn ping(&self) -> Result<(), Error> {
        const OP: &str = "ping HA";
        let request = self
            .http
            .get(format!("{}{PING_PATH}", self.base_url))
            .bearer_auth(self.token.expose())
            .build()
            .map_err(|source| Error::Transport { op: OP, source })?;
        self.send(OP, request).await.map(|_| ())
    }

    async fn post_json(&self, op: &'static str, path: &str, body: Value) -> Result<(), Error> {
        let payload = encode(op, &body)?;
        let request = self
            .http
            .post(format!("{}{path}", self.base_url))
            .bearer_auth(self.token.expose())
            .header("content-type", "application/json")
            .body(payload)
            .build()
            .map_err(|source| Error::Transport { op, source })?;
        self.send(op, request).await.map(|_| ())
    }

    async fn send(
        &self,
        op: &'static str,
        request: reqwest::Request,
    ) -> Result<reqwest::Response, Error> {
        // The body of a failed response is never read or logged: it may carry
        // entity attributes or other deployment detail.
        let response = self
            .http
            .execute(request)
            .await
            .map_err(|source| Error::Transport { op, source })?;
        if response.status().is_success() {
            return Ok(response);
        }
        let status = response.status().as_u16();
        let reason = response.status().canonical_reason().unwrap_or("HTTP error");
        Err(Error::Status {
            op,
            status,
            message: sanitize(reason, self.token.expose()),
        })
    }
}

/// One entity from `/api/states`.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct Entity {
    pub entity_id: String,
    #[serde(default)]
    pub state: String,
    #[serde(default)]
    pub attributes: EntityAttributes,
}

impl Entity {
    /// `attributes.friendly_name` or `""` when absent, matching the Go client.
    #[must_use]
    pub fn name(&self) -> &str {
        self.attributes.friendly_name.as_deref().unwrap_or("")
    }

    /// `attributes.icon` without the `mdi:` prefix, matching the Go client.
    #[must_use]
    pub fn icon(&self) -> &str {
        let icon = self.attributes.icon.as_deref().unwrap_or("");
        icon.strip_prefix("mdi:").unwrap_or(icon)
    }
}

/// One record of `config/entity_registry/list`.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct RegistryEntity {
    pub entity_id: String,
    #[serde(default)]
    pub device_id: Option<String>,
    /// `None` for a normal entity; `config` or `diagnostic` for a device
    /// setting.
    #[serde(default)]
    pub entity_category: Option<String>,
}

/// The Home Assistant attributes the catalog needs; every other attribute is
/// preserved verbatim in [`EntityAttributes::other`].
#[derive(Debug, Clone, PartialEq, Default, Deserialize)]
#[serde(default)]
pub struct EntityAttributes {
    pub friendly_name: Option<String>,
    pub icon: Option<String>,
    pub supported_color_modes: Vec<String>,
    pub min_color_temp_kelvin: Option<f64>,
    pub max_color_temp_kelvin: Option<f64>,
    pub brightness: Option<f64>,
    pub color_temp_kelvin: Option<f64>,
    pub hs_color: Option<Vec<f64>>,
    pub color_mode: Option<String>,
    #[serde(flatten)]
    pub other: Map<String, Value>,
}

fn validate_service<'a>(
    op: &'static str,
    domain: &'a str,
    service: &'a str,
) -> Result<(&'a str, &'a str), Error> {
    if valid_identifier(domain) && valid_identifier(service) {
        Ok((domain, service))
    } else {
        Err(Error::Invalid {
            op,
            detail: "empty or invalid service".to_owned(),
        })
    }
}

#[derive(Deserialize)]
struct StatePayload {
    #[serde(default)]
    state: Option<String>,
}

/// Read the next text frame, skipping keepalive and unrelated binary frames.
async fn read_text<S>(socket: &mut S, op: &'static str) -> Result<String, Error>
where
    S: futures_util::Stream<Item = Result<Message, tokio_tungstenite::tungstenite::Error>> + Unpin,
{
    loop {
        let Some(message) = socket.next().await else {
            return Err(Error::WebSocket { op });
        };
        let message = message.map_err(|_| Error::WebSocket { op })?;
        if message.is_close() {
            return Err(Error::WebSocket { op });
        }
        if let Ok(text) = message.to_text() {
            return Ok(text.to_owned());
        }
    }
}

async fn send_text<S>(socket: &mut S, text: &str, op: &'static str) -> Result<(), Error>
where
    S: futures_util::Sink<Message, Error = tokio_tungstenite::tungstenite::Error> + Unpin,
{
    socket
        .send(Message::text(text))
        .await
        .map_err(|_| Error::WebSocket { op })
}

/// Parse one answer of the registry request.
///
/// `Ok(None)` means another message kind (an event or an unrelated reply) and
/// the caller keeps reading; `Err(())` is a failed or malformed reply.
fn registry_reply(text: &str) -> Result<Option<Vec<RegistryEntity>>, ()> {
    let value: Value = serde_json::from_str(text).map_err(|_| ())?;
    if value.get("id").and_then(Value::as_i64) != Some(REGISTRY_MESSAGE_ID) {
        return Ok(None);
    }
    if value.get("success").and_then(Value::as_bool) != Some(true) {
        return Err(());
    }
    let result = value.get("result").ok_or(())?;
    serde_json::from_value(result.clone())
        .map(Some)
        .map_err(|_| ())
}

fn encode(op: &'static str, value: &Value) -> Result<Vec<u8>, Error> {
    serde_json::to_vec(value).map_err(|_| Error::Invalid {
        op,
        detail: "encode request".to_owned(),
    })
}

/// Normalize a configured script name into a Home Assistant entity id.
#[must_use]
pub fn script_entity(script: &str) -> String {
    let script = script.trim();
    if script.contains('.') {
        script.to_owned()
    } else {
        format!("script.{script}")
    }
}

/// Domain and service names are protocol identifiers, not free text; rejecting
/// anything else keeps the path unambiguous without URL-encoding.
fn valid_identifier(value: &str) -> bool {
    !value.is_empty()
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
}

/// Entity ids are `<domain>.<object_id>`; both parts use the Home Assistant
/// slug alphabet. Validating keeps the `/api/states/<entity>` path unambiguous
/// without URL-encoding.
fn valid_entity(entity: &str) -> bool {
    !entity.is_empty()
        && entity
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'.'))
}

/// Bound and redact a server-provided message.
fn sanitize(message: &str, token: &str) -> String {
    let mut message = message.trim().to_owned();
    if !token.is_empty() {
        message = message.replace(token, "***");
    }
    if message.len() > MAX_MESSAGE {
        message.truncate(MAX_MESSAGE);
        message.push('…');
    }
    message
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use axum::{
        Json, Router,
        extract::State,
        http::{HeaderMap, StatusCode},
        response::{IntoResponse, Response},
        routing::{get, post},
    };
    use home_secret::Token;
    use serde_json::{Value, json};

    use super::{Client, Error, ErrorKind, registry_reply, script_entity};

    async fn serve(app: Router) -> String {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind");
        let address = listener.local_addr().expect("address");
        tokio::spawn(async move { axum::serve(listener, app).await.expect("serve") });
        format!("http://{address}")
    }

    fn authorized(headers: &HeaderMap) -> bool {
        headers
            .get("authorization")
            .and_then(|value| value.to_str().ok())
            == Some("Bearer voice-token")
    }

    async fn state_route(headers: HeaderMap) -> Response {
        if !authorized(&headers) {
            return StatusCode::UNAUTHORIZED.into_response();
        }
        Json(json!({"entity_id": "sensor.station", "state": "idle"})).into_response()
    }

    async fn unauthorized() -> StatusCode {
        StatusCode::UNAUTHORIZED
    }

    async fn failure() -> StatusCode {
        StatusCode::INTERNAL_SERVER_ERROR
    }

    async fn slow() -> &'static str {
        tokio::time::sleep(Duration::from_millis(200)).await;
        "late"
    }

    async fn template(Json(body): Json<Value>) -> String {
        format!("rendered:{}", body["template"].as_str().unwrap_or(""))
    }

    async fn service(
        State(calls): State<std::sync::Arc<std::sync::Mutex<Vec<Value>>>>,
        Json(body): Json<Value>,
    ) -> StatusCode {
        calls.lock().expect("lock").push(body);
        StatusCode::OK
    }

    async fn entities_route(headers: HeaderMap) -> Response {
        if !authorized(&headers) {
            return StatusCode::UNAUTHORIZED.into_response();
        }
        Json(json!([
            {
                "entity_id": "light.kitchen",
                "state": "on",
                "attributes": {
                    "friendly_name": "Кухня",
                    "icon": "mdi:ceiling-light",
                    "supported_color_modes": ["color_temp", "hs"],
                    "min_color_temp_kelvin": 2000,
                    "max_color_temp_kelvin": 6500,
                    "brightness": 128,
                    "color_temp_kelvin": 3000,
                    "hs_color": [30.0, 80.0],
                    "color_mode": "color_temp",
                    "extra_field": {"nested": true}
                }
            },
            {"entity_id": "sensor.station", "state": "idle", "attributes": {}}
        ]))
        .into_response()
    }

    #[derive(serde::Serialize)]
    struct PlayMedia<'a> {
        media_content_id: &'a str,
        media_content_type: &'a str,
    }

    // Throwaway self-signed test CA (no private key in the tree).
    const TEST_CA_PEM: &str = include_str!("../testdata/test-ca.pem");

    fn client(base_url: &str) -> Client {
        Client::new(base_url, Token::new("voice-token"), Duration::from_secs(1)).expect("client")
    }

    #[tokio::test]
    async fn ping_accepts_an_authenticated_home_assistant() {
        let app = Router::new().route("/api/", get(|| async { "ok" }));
        let base = serve(app).await;
        client(&base).ping().await.expect("ping");
    }

    #[tokio::test]
    async fn state_reads_the_state_string() {
        let app = Router::new().route("/api/states/sensor.station", get(state_route));
        let base = serve(app).await;
        assert_eq!(
            client(&base).state("sensor.station").await.expect("state"),
            "idle"
        );
    }

    #[tokio::test]
    async fn entities_type_known_attributes_and_keep_the_rest() {
        let app = Router::new().route("/api/states", get(entities_route));
        let base = serve(app).await;
        let entities = client(&base).entities().await.expect("entities");
        assert_eq!(entities.len(), 2);
        let light = &entities[0];
        assert_eq!(light.entity_id, "light.kitchen");
        assert_eq!(light.state, "on");
        assert_eq!(light.name(), "Кухня");
        assert_eq!(light.icon(), "ceiling-light");
        assert_eq!(
            light.attributes.supported_color_modes,
            vec!["color_temp".to_owned(), "hs".to_owned()]
        );
        assert_eq!(light.attributes.min_color_temp_kelvin, Some(2000.0));
        assert_eq!(light.attributes.max_color_temp_kelvin, Some(6500.0));
        assert_eq!(light.attributes.brightness, Some(128.0));
        assert_eq!(light.attributes.color_temp_kelvin, Some(3000.0));
        assert_eq!(light.attributes.hs_color, Some(vec![30.0, 80.0]));
        assert_eq!(light.attributes.color_mode.as_deref(), Some("color_temp"));
        assert_eq!(light.attributes.other["extra_field"]["nested"], true);
        assert!(light.attributes.other.get("friendly_name").is_none());
        assert_eq!(entities[1].name(), "");
        assert_eq!(entities[1].icon(), "");
    }

    #[tokio::test]
    async fn typed_service_data_and_entity_helper_post_the_expected_body() {
        let calls = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
        let app = Router::new()
            .route("/api/services/media_player/play_media", post(service))
            .route(
                "/api/services/assist_satellite/start_conversation",
                post(service),
            )
            .with_state(calls.clone());
        let base = serve(app).await;
        let client = client(&base);
        client
            .call_service(
                "media_player",
                "play_media",
                PlayMedia {
                    media_content_id: "station",
                    media_content_type: "music",
                },
            )
            .await
            .expect("typed service");
        client
            .call_service_on(
                "assist_satellite",
                "start_conversation",
                "assist_satellite.kitchen",
                serde_json::Map::new(),
            )
            .await
            .expect("entity service");
        let calls = calls.lock().expect("lock");
        assert_eq!(calls[0]["media_content_id"], "station");
        assert_eq!(calls[0]["media_content_type"], "music");
        assert_eq!(calls[1]["entity_id"], "assist_satellite.kitchen");
    }

    #[tokio::test]
    async fn a_ca_bundle_is_accepted_from_bytes_and_a_file() {
        let app = Router::new().route("/api/", get(|| async { "ok" }));
        let base = serve(app).await;
        Client::new_with_ca_pem(
            &base,
            Token::new("voice-token"),
            Duration::from_secs(1),
            TEST_CA_PEM.as_bytes(),
        )
        .expect("client with CA bytes")
        .ping()
        .await
        .expect("ping");

        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("ca.pem");
        std::fs::write(&path, TEST_CA_PEM).expect("write CA");
        Client::new_with_ca_file(
            &base,
            Token::new("voice-token"),
            Duration::from_secs(1),
            &path,
        )
        .expect("client with CA file")
        .ping()
        .await
        .expect("ping");
    }

    #[test]
    fn an_unusable_ca_is_a_permanent_error() {
        let error = Client::new_with_ca_pem(
            "http://127.0.0.1:1",
            Token::new("voice-token"),
            Duration::from_secs(1),
            b"not a certificate",
        )
        .expect_err("bad CA");
        assert!(matches!(error, Error::Certificate { .. }));
        assert_eq!(error.kind(), ErrorKind::Permanent);
        let error = Client::new_with_ca_file(
            "http://127.0.0.1:1",
            Token::new("voice-token"),
            Duration::from_secs(1),
            "/definitely/not/a/ca.pem",
        )
        .expect_err("missing CA");
        assert!(matches!(error, Error::Certificate { .. }));
    }

    #[tokio::test]
    async fn unauthorized_is_permanent_and_names_only_the_status() {
        let app = Router::new().route("/api/states/sensor.station", get(unauthorized));
        let base = serve(app).await;
        let error = client(&base)
            .state("sensor.station")
            .await
            .expect_err("401");
        assert!(matches!(error, Error::Status { status: 401, .. }));
        assert_eq!(error.kind(), ErrorKind::Permanent);
        let rendered = error.to_string();
        assert!(rendered.contains("401"));
        assert!(!rendered.contains("voice-token"));
    }

    #[tokio::test]
    async fn server_error_is_transient() {
        let app = Router::new().route("/api/", get(failure));
        let base = serve(app).await;
        let error = client(&base).ping().await.expect_err("500");
        assert!(error.is_transient());
    }

    #[tokio::test]
    async fn timeout_is_transient_and_contains_no_token() {
        let app = Router::new().route("/api/", get(slow));
        let base = serve(app).await;
        let client = Client::new(&base, Token::new("voice-token"), Duration::from_millis(50))
            .expect("client");
        let error = client.ping().await.expect_err("timeout");
        assert_eq!(error.kind(), ErrorKind::Transient);
        assert!(!error.to_string().contains("voice-token"));
    }

    #[tokio::test]
    async fn malformed_state_is_permanent() {
        let app = Router::new().route(
            "/api/states/sensor.station",
            get(|| async { Json(json!({"state": 42})) }),
        );
        let base = serve(app).await;
        let error = client(&base)
            .state("sensor.station")
            .await
            .expect_err("decode");
        assert!(matches!(error, Error::Decode { .. }));
        assert_eq!(error.kind(), ErrorKind::Permanent);
    }

    #[tokio::test]
    async fn template_returns_the_rendered_text() {
        let app = Router::new().route("/api/template", post(template));
        let base = serve(app).await;
        assert_eq!(
            client(&base)
                .render_template("1 + 1")
                .await
                .expect("template"),
            "rendered:1 + 1"
        );
    }

    #[tokio::test]
    async fn service_call_posts_the_data_object() {
        let calls = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
        let app = Router::new()
            .route("/api/services/media_player/play_media", post(service))
            .with_state(calls.clone());
        let base = serve(app).await;
        let mut data = serde_json::Map::new();
        data.insert("entity_id".to_owned(), json!("media_player.station"));
        client(&base)
            .call_service("media_player", "play_media", data)
            .await
            .expect("service");
        let calls = calls.lock().expect("lock");
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0]["entity_id"], "media_player.station");
    }

    #[tokio::test]
    async fn script_call_normalizes_the_entity_and_keeps_variables() {
        let calls = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
        let app = Router::new()
            .route("/api/services/script/turn_on", post(service))
            .with_state(calls.clone());
        let base = serve(app).await;
        let mut variables = serde_json::Map::new();
        variables.insert("message".to_owned(), json!("Готово."));
        client(&base)
            .call_script("announce", variables)
            .await
            .expect("script");
        let calls = calls.lock().expect("lock");
        assert_eq!(calls[0]["entity_id"], "script.announce");
        assert_eq!(calls[0]["variables"]["message"], "Готово.");
    }

    #[tokio::test]
    async fn empty_script_or_invalid_service_is_rejected_before_io() {
        let app = Router::new();
        let base = serve(app).await;
        let client = client(&base);
        let error = client
            .call_script("  ", serde_json::Map::new())
            .await
            .expect_err("empty script");
        assert!(matches!(error, Error::Invalid { .. }));
        let error = client
            .call_service("media player", "play", serde_json::Map::new())
            .await
            .expect_err("invalid domain");
        assert!(matches!(error, Error::Invalid { .. }));
        let error = client
            .state("sensor/../secret")
            .await
            .expect_err("invalid entity");
        assert!(matches!(error, Error::Invalid { .. }));
    }

    #[test]
    fn script_entity_matches_the_go_helper() {
        assert_eq!(script_entity("announce"), "script.announce");
        assert_eq!(script_entity("script.announce"), "script.announce");
        assert_eq!(script_entity("  "), "script.");
    }

    #[test]
    fn entity_registry_reply_parses_categories_and_ignores_events() {
        let payload = json!({
            "id": 1,
            "type": "result",
            "success": true,
            "result": [
                {"entity_id": "light.lamp", "device_id": "dev-1", "entity_category": null},
                {"entity_id": "switch.delay", "device_id": "dev-1", "entity_category": "config"},
                {"entity_id": "switch.wake", "device_id": "dev-2", "entity_category": "diagnostic"},
                {"entity_id": "sensor.bare"}
            ]
        })
        .to_string();
        let entities = registry_reply(&payload)
            .expect("valid reply")
            .expect("registry reply");
        assert_eq!(entities.len(), 4);
        assert_eq!(entities[0].entity_category, None);
        assert_eq!(entities[1].entity_category.as_deref(), Some("config"));
        assert_eq!(entities[2].entity_category.as_deref(), Some("diagnostic"));
        assert_eq!(entities[3].device_id, None);
        assert_eq!(entities[3].entity_category, None);

        let event = json!({"type": "event", "event": {"event_type": "state_changed"}});
        assert_eq!(registry_reply(&event.to_string()), Ok(None));

        let failure = json!({"id": 1, "success": false, "error": {"code": "unknown_command"}});
        assert_eq!(registry_reply(&failure.to_string()), Err(()));
        assert_eq!(registry_reply("not json"), Err(()));
    }

    #[test]
    fn websocket_url_mirrors_the_http_scheme() {
        let http = Client::new(
            "http://192.0.2.28:8123",
            Token::new("token"),
            Duration::from_secs(1),
        )
        .expect("client");
        assert_eq!(
            http.websocket_url("read entity registry").expect("url"),
            "ws://192.0.2.28:8123/api/websocket"
        );
        let https = Client::new(
            "https://ha.example.test/",
            Token::new("token"),
            Duration::from_secs(1),
        )
        .expect("client");
        assert_eq!(
            https.websocket_url("read entity registry").expect("url"),
            "wss://ha.example.test/api/websocket"
        );
    }

    #[test]
    fn the_ca_bundle_builds_a_websocket_connector() {
        let client = Client::new_with_ca_pem(
            "https://ha.example.test",
            Token::new("token"),
            Duration::from_secs(1),
            TEST_CA_PEM.as_bytes(),
        )
        .expect("client");
        assert!(
            client
                .tls_connector("read entity registry")
                .expect("connector")
                .is_some()
        );
        let plain = Client::new(
            "https://ha.example.test",
            Token::new("token"),
            Duration::from_secs(1),
        )
        .expect("client");
        assert!(
            plain
                .tls_connector("read entity registry")
                .expect("connector")
                .is_none()
        );
    }

    #[test]
    fn debug_never_prints_the_token() {
        let client = Client::new(
            "http://127.0.0.1:1",
            Token::new("voice-token"),
            Duration::from_secs(1),
        )
        .expect("client");
        let rendered = format!("{client:?}");
        assert!(!rendered.contains("voice-token"));
        assert!(rendered.contains("Token(***)"));
    }
}
