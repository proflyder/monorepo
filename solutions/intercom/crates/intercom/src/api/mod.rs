//! Connect transport and distroless probe routes.
//!
//! Connect JSON/binary over HTTP/1.1 serves every unary RPC; `Watch` is a
//! server-streaming RPC on the same listener. `/healthz`, `/readyz` and
//! `/metrics` are plain HTTP because Kubernetes probes and Prometheus must not
//! need a bearer.

mod auth;
mod service;

use std::collections::HashSet;
use std::convert::Infallible;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};

use axum::body::Body;
use axum::extract::State;
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::routing::get;
use connectrpc::{ConnectRpcService, Limits, Router as ConnectRouter};
use home_proto::v1::IntercomServiceExt;
use http::{Request, Response, header};

pub use auth::TokenAuth;
pub use service::Api;

/// Bounds every unary request and response message.
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

/// Builds the axum router: probes plus the Connect service behind one bearer
/// interceptor.
pub fn router(api: Api, auth: TokenAuth) -> axum::Router {
    let routes = ConnectRouter::new();
    let routes = <Api as IntercomServiceExt>::register(Arc::new(api.clone()), routes);
    let methods = Arc::new(routes.methods().map(str::to_owned).collect::<HashSet<_>>());
    let connect = ConnectRpcService::new(routes)
        .with_limits(
            Limits::default()
                .with_max_message_size(MAX_BODY_BYTES)
                .with_max_request_body_size(MAX_BODY_BYTES),
        )
        .with_interceptor_arc(Arc::new(auth));

    axum::Router::new()
        .route("/healthz", get(healthz))
        .route("/readyz", get(healthz))
        .route("/metrics", get(metrics))
        .with_state(api)
        .fallback_service(ConnectFallback {
            methods,
            service: connect,
        })
}

async fn healthz() -> impl IntoResponse {
    (
        [(header::CONTENT_TYPE, "text/plain; charset=utf-8")],
        "ok\n",
    )
}

async fn metrics(State(api): State<Api>) -> impl IntoResponse {
    (
        [(
            header::CONTENT_TYPE,
            "text/plain; version=0.0.4; charset=utf-8",
        )],
        api.intercom().render_metrics(),
    )
}
