mod alarm;
mod auth;
mod convert;
mod error;
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
use home_proto::v1::{AlarmServiceExt, ScenarioServiceExt, SchedulerServiceExt};
use http::{Request, Response, StatusCode, header};

pub use alarm::AlarmFacade;
pub use service::{EngineStatus, Service};

use auth::BearerAuth;

pub const MAX_BODY_BYTES: usize = 1 << 20;

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
    token: &str,
    vpn_access: VpnAccess,
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
        .with_interceptor_arc(Arc::new(BearerAuth::new(token)));
    let vpn = vpn::VpnState {
        store: Arc::clone(&store),
        app_auth: Arc::new(BearerAuth::new(token)),
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
    let push = push::PushState {
        store,
        auth: Arc::new(BearerAuth::new(token)),
    };
    axum::Router::new()
        .route(
            "/v1/push/devices",
            axum::routing::get(push::list).post(push::register),
        )
        .route("/v1/push/devices/remove", axum::routing::post(push::remove))
        .route("/v1/push/devices/rename", axum::routing::post(push::rename))
        .with_state(push)
        .merge(vpn_routes)
        .fallback_service(ConnectFallback { methods, service })
}
