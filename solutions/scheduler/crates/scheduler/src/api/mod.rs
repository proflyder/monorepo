//! Connect transport: one axum fallback serving both protobuf services.
//!
//! Only the `json` feature of `connectrpc` is enabled: the crate builds the
//! Tower service itself and does not pull `hyper/http2` into the station
//! firmware. gRPC and gRPC-Web over HTTP/1.1 still work; gRPC over h2c is not
//! offered yet (no live client used it).

mod alarm;
mod auth;
mod convert;
mod error;
mod service;

use std::collections::HashSet;
use std::convert::Infallible;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};

use axum::body::Body;
use connectrpc::{ConnectRpcService, Limits, Router as ConnectRouter};
use home_proto::v1::{AlarmServiceExt, SchedulerServiceExt};
use http::{Request, Response, StatusCode, header};

pub use alarm::AlarmFacade;
pub use service::{EngineStatus, Service};

use auth::BearerAuth;

/// Bounds every unary request and response message, matching the Go server's
/// `MaxBodyBytes`.
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
                // connect-go's mux answers an unknown procedure with the
                // plain-text `404`, not a Connect envelope.
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

/// Builds the axum router with both services behind one bearer interceptor.
pub fn router(service: Service, alarms: AlarmFacade, token: &str) -> axum::Router {
    let routes = ConnectRouter::new();
    let routes = Arc::new(service).register(routes);
    let routes = Arc::new(alarms).register(routes);
    let methods = Arc::new(routes.methods().map(str::to_owned).collect::<HashSet<_>>());
    let service = ConnectRpcService::new(routes)
        .with_limits(
            Limits::default()
                .with_max_message_size(MAX_BODY_BYTES)
                .with_max_request_body_size(MAX_BODY_BYTES),
        )
        .with_interceptor_arc(Arc::new(BearerAuth::new(token)));
    axum::Router::new().fallback_service(ConnectFallback { methods, service })
}
