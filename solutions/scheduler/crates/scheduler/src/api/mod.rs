mod access;
mod alarm;
mod auth;
mod convert;
mod error;
mod notifications;
pub mod push;
mod service;
#[cfg(test)]
mod sync_tests;
mod text;
pub mod vpn;
mod zone;

use std::collections::HashSet;
use std::convert::Infallible;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};

use axum::body::Body;
use connectrpc::{ConnectRpcService, Limits, Router as ConnectRouter};
use home_app_auth::AccessCache;
use home_proto::v1::{AlarmServiceExt, ScenarioServiceExt, SchedulerServiceExt};
use http::{Request, Response, StatusCode, header};

pub use access::AccessState;
pub use alarm::AlarmFacade;
pub use auth::AccessAuth;
pub use service::{EngineStatus, Service};

use crate::sync::SyncSink;
use auth::BearerAuth;

pub const MAX_BODY_BYTES: usize = 1 << 20;

#[must_use]
pub fn now_unix() -> i64 {
    jiff::Timestamp::now().as_second()
}

#[derive(Clone)]
struct ConnectFallback {
    methods: Arc<HashSet<String>>,
    service: ConnectRpcService<ConnectRouter>,
}

impl tower::Service<Request<Body>> for ConnectFallback {
    type Response = Response<Body>;
    type Error = Infallible;
    type Future = Pin<Box<dyn Future<Output = Result<Self::Response, Infallible>> + Send>>;

    fn poll_ready(&mut self, _context: &mut Context<'_>) -> Poll<Result<(), Infallible>> {
        Poll::Ready(Ok(()))
    }

    fn call(&mut self, request: Request<Body>) -> Self::Future {
        let path = request.uri().path();
        let known = self
            .methods
            .contains(path.strip_prefix('/').unwrap_or(path));
        let mut service = self.service.clone();
        Box::pin(async move {
            if !known {
                let response = Response::builder()
                    .status(StatusCode::NOT_FOUND)
                    .header(header::CONTENT_TYPE, "text/plain; charset=utf-8")
                    .body(Body::from("404"))
                    .expect("static response is valid");
                return Ok(response);
            }
            let response = service.call(request).await?;
            Ok(response.map(Body::new))
        })
    }
}

#[derive(Debug, Clone, Default)]
pub struct VpnAccess {
    pub settings: Option<Arc<vpn::VpnSettings>>,
    pub sync_token: Option<String>,
}

pub fn router(
    service: Service,
    alarms: AlarmFacade,
    auth: Arc<AccessAuth>,
    cache: AccessCache,
    vpn_access: VpnAccess,
    sync: Arc<dyn SyncSink>,
) -> axum::Router {
    let store = Arc::clone(service.store());
    let routes = ConnectRouter::new();
    let routes = <Service as SchedulerServiceExt>::register(Arc::new(service.clone()), routes);
    let routes = <Service as ScenarioServiceExt>::register(Arc::new(service), routes);
    let routes = Arc::new(alarms).register(routes);
    let methods = Arc::new(routes.methods().map(str::to_owned).collect::<HashSet<_>>());
    let service = ConnectRpcService::new(routes)
        .with_limits(
            Limits::default()
                .with_max_message_size(MAX_BODY_BYTES)
                .with_max_request_body_size(MAX_BODY_BYTES),
        )
        .with_interceptor_arc(auth.clone());
    let vpn = vpn::VpnState {
        store: Arc::clone(&store),
        app_auth: Arc::clone(&auth),
        sync_auth: vpn_access
            .sync_token
            .as_deref()
            .map(|sync| Arc::new(BearerAuth::new(sync))),
        settings: vpn_access.settings,
    };
    let vpn_routes = axum::Router::new()
        .route("/v1/vpn/profile", axum::routing::post(vpn::register))
        .route("/v1/vpn/profile/remove", axum::routing::post(vpn::remove))
        .route("/v1/vpn/peers", axum::routing::get(vpn::peers))
        .with_state(vpn);
    let notifications = notifications::NotificationState {
        store: Arc::clone(&store),
        auth: Arc::clone(&auth),
        sync: Arc::clone(&sync),
    };
    let history_routes = axum::Router::new()
        .route(
            "/v1/push/history",
            axum::routing::get(notifications::history).post(notifications::record),
        )
        .route(
            "/v1/push/history/unread-count",
            axum::routing::get(notifications::unread),
        )
        .route(
            "/v1/push/history/read",
            axum::routing::post(notifications::set_read),
        )
        .route(
            "/v1/push/history/read-all",
            axum::routing::post(notifications::read_all),
        )
        .route(
            "/v1/push/history/remove",
            axum::routing::post(notifications::remove),
        )
        .route(
            "/v1/push/history/clear",
            axum::routing::post(notifications::clear),
        )
        .with_state(notifications);
    let push = push::PushState {
        store: Arc::clone(&store),
        auth: Arc::clone(&auth),
        cache: cache.clone(),
    };
    let access = AccessState {
        store,
        auth,
        cache,
        sync,
    };
    let access_routes = axum::Router::new()
        .route("/v1/me", axum::routing::get(access::me))
        .route("/v1/access/modules", axum::routing::get(access::modules))
        .route(
            "/v1/access/tokens",
            axum::routing::get(access::list).post(access::create),
        )
        .route(
            "/v1/access/tokens/{id}",
            axum::routing::patch(access::update).delete(access::revoke),
        )
        .route("/v1/access/snapshot", axum::routing::get(access::snapshot))
        .with_state(access);
    axum::Router::new()
        .route(
            "/v1/push/devices",
            axum::routing::get(push::list).post(push::register),
        )
        .route("/v1/push/devices/remove", axum::routing::post(push::remove))
        .route("/v1/push/devices/rename", axum::routing::post(push::rename))
        .with_state(push)
        .merge(vpn_routes)
        .merge(history_routes)
        .merge(access_routes)
        .fallback_service(ConnectFallback { methods, service })
}
