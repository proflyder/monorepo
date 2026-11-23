use std::sync::Arc;

use axum::Json;
use axum::extract::State;
use axum::http::{HeaderMap, StatusCode, header};
use axum::response::{IntoResponse, Response};
use home_app_auth::Module;
use serde::{Deserialize, Serialize};

use crate::store::{Store, StoreError, VpnPeer, VpnPeerInput, VpnPool, vpn_key_is_valid};

use super::auth::{AccessAuth, BearerAuth, error_response};

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct OcservSettings {
    pub url: String,
    pub group: String,
    #[serde(default)]
    pub split_group: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct VpnSettingsFile {
    pub pool: String,
    pub endpoint: String,
    #[serde(default)]
    pub lan_endpoint: String,
    pub server_public_key: String,
    pub dns: Vec<String>,
    pub allowed_ips: Vec<String>,
    #[serde(default)]
    pub split_allowed_ips: Vec<String>,
    #[serde(default = "default_mtu")]
    pub mtu: u16,
    #[serde(default = "default_keepalive")]
    pub persistent_keepalive: u16,
    pub ocserv: OcservSettings,
}

const fn default_mtu() -> u16 {
    1380
}

const fn default_keepalive() -> u16 {
    25
}

#[derive(Debug, Clone)]
pub struct VpnSettings {
    pub pool: VpnPool,
    pub file: VpnSettingsFile,
}

impl VpnSettings {
    pub fn parse(json: &[u8]) -> Result<Self, String> {
        let file: VpnSettingsFile =
            serde_json::from_slice(json).map_err(|error| format!("VPN settings: {error}"))?;
        let pool = VpnPool::parse(&file.pool)?;
        if !vpn_key_is_valid(&file.server_public_key) {
            return Err("VPN settings: serverPublicKey is not a WireGuard key".into());
        }
        if file.endpoint.trim().is_empty() || !file.endpoint.contains(':') {
            return Err("VPN settings: endpoint must be host:port".into());
        }
        if !file.lan_endpoint.is_empty() && !file.lan_endpoint.contains(':') {
            return Err("VPN settings: lanEndpoint must be host:port".into());
        }
        if file.allowed_ips.is_empty() {
            return Err("VPN settings: allowedIps is empty".into());
        }
        if !(1280..=1420).contains(&file.mtu) {
            return Err("VPN settings: mtu must be within 1280..1420".into());
        }
        if !file.ocserv.url.starts_with("https://") || file.ocserv.group.trim().is_empty() {
            return Err("VPN settings: ocserv needs an https url and a group".into());
        }
        Ok(Self { pool, file })
    }
}

#[derive(Clone, Debug)]
pub struct VpnState {
    pub store: Arc<Store>,
    pub app_auth: Arc<AccessAuth>,
    pub sync_auth: Option<Arc<BearerAuth>>,
    pub settings: Option<Arc<VpnSettings>>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProfileBody {
    #[serde(default)]
    pub device_id: String,
    #[serde(default)]
    pub public_key: String,
    #[serde(default)]
    pub device_name: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RemoveBody {
    #[serde(default)]
    pub device_id: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct InterfaceView {
    address: String,
    dns: Vec<String>,
    mtu: u16,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct ServerPeerView {
    public_key: String,
    endpoint: String,
    lan_endpoint: String,
    allowed_ips: Vec<String>,
    persistent_keepalive: u16,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct WireguardView {
    interface: InterfaceView,
    peer: ServerPeerView,
    split_allowed_ips: Vec<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct OcservView {
    url: String,
    group: String,
    split_group: String,
    authentication: Vec<&'static str>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct ProfileView {
    device_id: String,
    device_name: String,
    wireguard: WireguardView,
    ocserv: OcservView,
    created_unix: String,
    updated_unix: String,
}

#[derive(Debug, Serialize)]
struct ProfileResponse {
    profile: ProfileView,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct SyncPeerView {
    public_key: String,
    allowed_ip: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct SyncResponse {
    address: String,
    peers: Vec<SyncPeerView>,
}

#[derive(Debug, Serialize)]
struct RemovedResponse {
    removed: bool,
}

#[derive(Debug, Serialize)]
struct ErrorResponse {
    error: &'static str,
}

fn profile_view(settings: &VpnSettings, peer: &VpnPeer) -> ProfileView {
    let file = &settings.file;
    ProfileView {
        device_id: peer.device_id.clone(),
        device_name: peer.device_name.clone(),
        wireguard: WireguardView {
            interface: InterfaceView {
                address: format!("{}/32", settings.pool.address(peer.host)),
                dns: file.dns.clone(),
                mtu: file.mtu,
            },
            peer: ServerPeerView {
                public_key: file.server_public_key.clone(),
                endpoint: file.endpoint.clone(),
                lan_endpoint: file.lan_endpoint.clone(),
                allowed_ips: file.allowed_ips.clone(),
                persistent_keepalive: file.persistent_keepalive,
            },
            split_allowed_ips: file.split_allowed_ips.clone(),
        },
        ocserv: OcservView {
            url: file.ocserv.url.clone(),
            group: file.ocserv.group.clone(),
            split_group: file.ocserv.split_group.clone(),
            authentication: vec!["certificate", "password+totp"],
        },
        created_unix: peer.created_unix.to_string(),
        updated_unix: peer.updated_unix.to_string(),
    }
}

pub async fn register(
    State(state): State<VpnState>,
    headers: HeaderMap,
    Json(body): Json<ProfileBody>,
) -> Response {
    let principal = match require_vpn(&state, &headers) {
        Ok(principal) => principal,
        Err(response) => return *response,
    };
    let Some(settings) = state.settings.clone() else {
        return error(StatusCode::SERVICE_UNAVAILABLE, "vpn is not configured");
    };
    let input = VpnPeerInput {
        device_id: body.device_id,
        public_key: body.public_key,
        device_name: body.device_name,
    };
    let pool = settings.pool;
    let now = super::now_unix();
    let result = blocking(Arc::clone(&state.store), move |store| {
        store.register_vpn_peer_for(pool, &input, &principal.id, principal.owner, now)
    })
    .await;
    match result {
        Ok(peer) => Json(ProfileResponse {
            profile: profile_view(&settings, &peer),
        })
        .into_response(),
        Err(response) => *response,
    }
}

pub async fn remove(
    State(state): State<VpnState>,
    headers: HeaderMap,
    Json(body): Json<RemoveBody>,
) -> Response {
    let principal = match require_vpn(&state, &headers) {
        Ok(principal) => principal,
        Err(response) => return *response,
    };
    let device_id = body.device_id;
    let scope = (!principal.owner).then(|| principal.id.clone());
    match blocking(Arc::clone(&state.store), move |store| {
        store.remove_vpn_peer_for(&device_id, scope.as_deref())
    })
    .await
    {
        Ok(removed) => Json(RemovedResponse { removed }).into_response(),
        Err(response) => *response,
    }
}

fn require_vpn(
    state: &VpnState,
    headers: &HeaderMap,
) -> Result<home_app_auth::Principal, Box<Response>> {
    state
        .app_auth
        .app(headers.get(header::AUTHORIZATION), Module::Vpn)
        .map_err(|forbidden| {
            let (status, headers, body) = error_response(forbidden);
            Box::new((status, headers, body).into_response())
        })
}

pub async fn peers(State(state): State<VpnState>, headers: HeaderMap) -> Response {
    let authorized = state
        .sync_auth
        .as_ref()
        .is_some_and(|auth| auth.authorized(headers.get(header::AUTHORIZATION)));
    if !authorized {
        return error(StatusCode::UNAUTHORIZED, "unauthenticated");
    }
    let Some(settings) = state.settings.clone() else {
        return error(StatusCode::SERVICE_UNAVAILABLE, "vpn is not configured");
    };
    match blocking(Arc::clone(&state.store), Store::list_vpn_peers).await {
        Ok(peers) => Json(SyncResponse {
            address: format!(
                "{}/{}",
                settings.pool.server_address(),
                settings.pool.prefix()
            ),
            peers: peers
                .iter()
                .map(|peer| SyncPeerView {
                    public_key: peer.public_key.clone(),
                    allowed_ip: format!("{}/32", settings.pool.address(peer.host)),
                })
                .collect(),
        })
        .into_response(),
        Err(response) => *response,
    }
}

async fn blocking<T, F>(store: Arc<Store>, call: F) -> Result<T, Box<Response>>
where
    T: Send + 'static,
    F: FnOnce(&Store) -> Result<T, StoreError> + Send + 'static,
{
    match tokio::task::spawn_blocking(move || call(&store)).await {
        Ok(Ok(value)) => Ok(value),
        Ok(Err(failure)) => Err(Box::new(store_error(&failure))),
        Err(_) => Err(Box::new(error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "internal error",
        ))),
    }
}

fn store_error(failure: &StoreError) -> Response {
    match failure.kind() {
        Some(crate::domain::ErrorKind::InvalidArgument) => {
            error(StatusCode::BAD_REQUEST, "invalid argument")
        }
        Some(crate::domain::ErrorKind::Precondition) => {
            error(StatusCode::CONFLICT, "public key belongs to another device")
        }
        Some(crate::domain::ErrorKind::LimitExceeded) => {
            error(StatusCode::TOO_MANY_REQUESTS, "limit exceeded")
        }
        Some(crate::domain::ErrorKind::NotFound) => error(StatusCode::NOT_FOUND, "not found"),
        _ => error(StatusCode::INTERNAL_SERVER_ERROR, "internal error"),
    }
}

fn error(status: StatusCode, message: &'static str) -> Response {
    (status, Json(ErrorResponse { error: message })).into_response()
}

#[cfg(test)]
mod tests {
    use axum::Router;
    use axum::body::Body;
    use axum::http::Request;
    use axum::routing::{get, post};
    use tower::ServiceExt;

    use super::*;
    use crate::store::Config;

    const APP_TOKEN: &str = "app-test-token";
    const MEMBER_TOKEN: &str = "member-test-token";
    const SYNC_TOKEN: &str = "sync-test-token";
    const SERVER_KEY: &str = "c2VydmVyLXNlcnZlci1zZXJ2ZXItc2VydmVyLXNlcnY=";
    const PHONE_KEY: &str = "aGVsbG8td29ybGQtaGVsbG8td29ybGQtaGVsbG8tdzA=";

    fn settings_json() -> String {
        format!(
            r#"{{"pool":"10.77.2.0/24","endpoint":"vpn.example.com:51820",
            "lanEndpoint":"192.0.2.29:51820","serverPublicKey":"{SERVER_KEY}","dns":["192.0.2.1"],
            "allowedIps":["0.0.0.0/0"],"splitAllowedIps":["192.0.2.0/24"],
            "ocserv":{{"url":"https://vpn.example.com/","group":"full","splitGroup":"lan"}}}}"#
        )
    }

    fn app(configured: bool) -> Router {
        let store = Arc::new(Store::open_in_memory(Config::default()).expect("store"));
        let settings = configured
            .then(|| Arc::new(VpnSettings::parse(settings_json().as_bytes()).expect("settings")));
        let snapshot = home_app_auth::AccessSnapshot {
            revision: 1,
            tokens: vec![
                home_app_auth::TokenRecord {
                    id: "owner".to_owned(),
                    name: "owner".to_owned(),
                    sha256: home_app_auth::sha256_hex(APP_TOKEN),
                    owner: true,
                    modules: home_app_auth::Modules::owner()
                        .ids()
                        .into_iter()
                        .map(str::to_owned)
                        .collect(),
                },
                home_app_auth::TokenRecord {
                    id: "member".to_owned(),
                    name: "member".to_owned(),
                    sha256: home_app_auth::sha256_hex(MEMBER_TOKEN),
                    owner: false,
                    modules: home_app_auth::Modules::member()
                        .ids()
                        .into_iter()
                        .map(str::to_owned)
                        .collect(),
                },
            ],
        };
        let auth = AccessAuth::new(
            home_app_auth::AccessCache::new(snapshot),
            home_secret::Token::new(SYNC_TOKEN),
        );
        let state = VpnState {
            store,
            app_auth: Arc::new(auth),
            sync_auth: Some(Arc::new(BearerAuth::new(SYNC_TOKEN))),
            settings,
        };
        Router::new()
            .route("/v1/vpn/profile", post(register))
            .route("/v1/vpn/profile/remove", post(remove))
            .route("/v1/vpn/peers", get(peers))
            .with_state(state)
    }

    async fn call(router: Router, request: Request<Body>) -> (StatusCode, serde_json::Value) {
        let response = router.oneshot(request).await.expect("response");
        let status = response.status();
        let body = axum::body::to_bytes(response.into_body(), 64 * 1024)
            .await
            .expect("body");
        (status, serde_json::from_slice(&body).expect("json"))
    }

    fn post_json(uri: &str, token: &str, body: &str) -> Request<Body> {
        Request::builder()
            .method("POST")
            .uri(uri)
            .header(header::AUTHORIZATION, format!("Bearer {token}"))
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(body.to_owned()))
            .expect("request")
    }

    fn peers_request(token: &str) -> Request<Body> {
        Request::builder()
            .uri("/v1/vpn/peers")
            .header(header::AUTHORIZATION, format!("Bearer {token}"))
            .body(Body::empty())
            .expect("request")
    }

    fn profile_body() -> String {
        format!(
            r#"{{"deviceId":"device-aaaaaaaa","publicKey":"{PHONE_KEY}","deviceName":"Pixel"}}"#
        )
    }

    #[tokio::test]
    async fn a_profile_carries_the_wireguard_and_ocserv_settings() {
        let router = app(true);
        let (status, body) = call(
            router.clone(),
            post_json("/v1/vpn/profile", APP_TOKEN, &profile_body()),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{body}");
        let profile = &body["profile"];
        assert_eq!(profile["deviceId"], "device-aaaaaaaa");
        assert_eq!(profile["wireguard"]["interface"]["address"], "10.77.2.2/32");
        assert_eq!(profile["wireguard"]["interface"]["dns"][0], "192.0.2.1");
        assert_eq!(profile["wireguard"]["interface"]["mtu"], 1380);
        assert_eq!(profile["wireguard"]["peer"]["publicKey"], SERVER_KEY);
        assert_eq!(
            profile["wireguard"]["peer"]["endpoint"],
            "vpn.example.com:51820"
        );
        assert_eq!(
            profile["wireguard"]["peer"]["lanEndpoint"],
            "192.0.2.29:51820"
        );
        assert_eq!(profile["wireguard"]["peer"]["allowedIps"][0], "0.0.0.0/0");
        assert_eq!(profile["wireguard"]["peer"]["persistentKeepalive"], 25);
        assert_eq!(profile["wireguard"]["splitAllowedIps"][0], "192.0.2.0/24");
        assert_eq!(profile["ocserv"]["url"], "https://vpn.example.com/");
        assert_eq!(profile["ocserv"]["group"], "full");
        assert!(body.to_string().find(PHONE_KEY).is_none());

        let (status, synced) = call(router, peers_request(SYNC_TOKEN)).await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(synced["address"], "10.77.2.1/24");
        assert_eq!(synced["peers"][0]["publicKey"], PHONE_KEY);
        assert_eq!(synced["peers"][0]["allowedIp"], "10.77.2.2/32");
    }

    #[tokio::test]
    async fn removing_a_device_drops_its_peer() {
        let router = app(true);
        call(
            router.clone(),
            post_json("/v1/vpn/profile", APP_TOKEN, &profile_body()),
        )
        .await;
        let remove_body = r#"{"deviceId":"device-aaaaaaaa"}"#;
        let (status, body) = call(
            router.clone(),
            post_json("/v1/vpn/profile/remove", APP_TOKEN, remove_body),
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(body["removed"], true);
        let (_, synced) = call(router.clone(), peers_request(SYNC_TOKEN)).await;
        assert_eq!(synced["peers"].as_array().map(Vec::len), Some(0));
        let (_, again) = call(
            router,
            post_json("/v1/vpn/profile/remove", APP_TOKEN, remove_body),
        )
        .await;
        assert_eq!(again["removed"], false);
    }

    #[tokio::test]
    async fn members_cannot_manage_vpn_profiles() {
        let router = app(true);
        let (status, _) = call(
            router.clone(),
            post_json("/v1/vpn/profile", MEMBER_TOKEN, &profile_body()),
        )
        .await;
        assert_eq!(status, StatusCode::FORBIDDEN);
        let (status, _) = call(
            router,
            post_json(
                "/v1/vpn/profile/remove",
                MEMBER_TOKEN,
                r#"{"deviceId":"device-aaaaaaaa"}"#,
            ),
        )
        .await;
        assert_eq!(status, StatusCode::FORBIDDEN);
    }

    #[tokio::test]
    async fn the_tokens_are_not_interchangeable() {
        let router = app(true);
        let (status, _) = call(
            router.clone(),
            post_json("/v1/vpn/profile", SYNC_TOKEN, &profile_body()),
        )
        .await;
        assert_eq!(status, StatusCode::UNAUTHORIZED);
        let (status, _) = call(router, peers_request(APP_TOKEN)).await;
        assert_eq!(status, StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn a_bad_key_is_a_bad_request() {
        let router = app(true);
        let body = r#"{"deviceId":"device-aaaaaaaa","publicKey":"not-a-key"}"#;
        let (status, _) = call(router, post_json("/v1/vpn/profile", APP_TOKEN, body)).await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn an_unconfigured_vpn_is_unavailable() {
        let router = app(false);
        let (status, body) = call(
            router.clone(),
            post_json("/v1/vpn/profile", APP_TOKEN, &profile_body()),
        )
        .await;
        assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
        assert_eq!(body["error"], "vpn is not configured");
        let (status, _) = call(router, peers_request(SYNC_TOKEN)).await;
        assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
    }

    #[test]
    fn settings_are_validated() {
        assert!(VpnSettings::parse(settings_json().as_bytes()).is_ok());
        let bad_key = settings_json().replace(SERVER_KEY, "short");
        assert!(VpnSettings::parse(bad_key.as_bytes()).is_err());
        let bad_pool = settings_json().replace("10.77.2.0/24", "10.77.2.5/24");
        assert!(VpnSettings::parse(bad_pool.as_bytes()).is_err());
        let plain = settings_json().replace("https://vpn", "http://vpn");
        assert!(VpnSettings::parse(plain.as_bytes()).is_err());
        let bad_lan = settings_json().replace("192.0.2.29:51820", "192.0.2.29");
        assert!(VpnSettings::parse(bad_lan.as_bytes()).is_err());
    }

    #[test]
    fn the_lan_endpoint_is_optional() {
        let without = settings_json().replace(r#""lanEndpoint":"192.0.2.29:51820","#, "");
        let settings = VpnSettings::parse(without.as_bytes()).expect("settings");
        assert_eq!(settings.file.lan_endpoint, "");
    }
}
