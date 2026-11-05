//! Bearer authentication and graceful serving for axum control APIs.
//!
//! [`with_bearer`] wraps a [`Router`] with a constant-time `Authorization:
//! Bearer <token>` check; [`serve`] owns the `axum::serve` call and stops
//! accepting connections when the supplied [`CancellationToken`] fires.

use axum::Router;
use axum::extract::Request;
use axum::http::{StatusCode, header};
use axum::middleware::{self, Next};
use axum::response::{IntoResponse, Response};
use ricardo_secret::Token;
use tokio::net::TcpListener;
use tokio_util::sync::CancellationToken;
use tracing::debug;

/// Reject requests whose `Authorization` header is not `Bearer <token>`.
pub async fn require_bearer(token: Token, request: Request, next: Next) -> Response {
    let authorization = request
        .headers()
        .get(header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok());
    let authorized = authorization
        .and_then(|value| value.strip_prefix("Bearer "))
        .is_some_and(|candidate| token.ct_eq(candidate));
    if !authorized {
        debug!("rejected control request: missing or invalid bearer token");
        return StatusCode::UNAUTHORIZED.into_response();
    }
    next.run(request).await
}

/// Protect every route of `router` with [`require_bearer`].
pub fn with_bearer(router: Router, token: Token) -> Router {
    router.layer(middleware::from_fn(move |request, next| {
        let token = token.clone();
        async move { require_bearer(token, request, next).await }
    }))
}

/// Serve `router` on `listener` until `shutdown` is cancelled.
///
/// The listener is closed before the future resolves; in-flight requests are
/// allowed to finish (axum's graceful shutdown).
pub async fn serve(
    listener: TcpListener,
    router: Router,
    shutdown: CancellationToken,
) -> std::io::Result<()> {
    axum::serve(listener, router)
        .with_graceful_shutdown(async move { shutdown.cancelled().await })
        .await
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use axum::routing::get;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpStream;

    use super::*;

    const TOKEN: &str = "0123456789abcdef0123456789abcdef";

    fn router() -> Router {
        Router::new().route("/status", get(|| async { "ok" }))
    }

    async fn round_trip(address: std::net::SocketAddr, authorization: Option<&str>) -> String {
        let mut stream = tokio::time::timeout(Duration::from_secs(2), TcpStream::connect(address))
            .await
            .expect("connect timeout")
            .expect("connect");
        let mut request = String::from("GET /status HTTP/1.1\r\nHost: localhost\r\n");
        if let Some(value) = authorization {
            request.push_str("Authorization: ");
            request.push_str(value);
            request.push_str("\r\n");
        }
        request.push_str("Connection: close\r\n\r\n");
        stream.write_all(request.as_bytes()).await.expect("write");
        let mut response = Vec::new();
        tokio::time::timeout(Duration::from_secs(2), stream.read_to_end(&mut response))
            .await
            .expect("read timeout")
            .expect("read");
        String::from_utf8_lossy(&response).into_owned()
    }

    #[tokio::test]
    async fn bearer_accepts_only_the_exact_token() {
        let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
        let address = listener.local_addr().expect("address");
        let shutdown = CancellationToken::new();
        let serve_shutdown = shutdown.clone();
        let server = tokio::spawn(serve(
            listener,
            with_bearer(router(), Token::new(TOKEN)),
            serve_shutdown,
        ));

        let accepted = round_trip(address, Some(&format!("Bearer {TOKEN}"))).await;
        assert!(accepted.starts_with("HTTP/1.1 200"), "{accepted}");

        let missing = round_trip(address, None).await;
        assert!(missing.starts_with("HTTP/1.1 401"), "{missing}");

        let wrong = round_trip(address, Some("Bearer nope")).await;
        assert!(wrong.starts_with("HTTP/1.1 401"), "{wrong}");

        let scheme = round_trip(address, Some(TOKEN)).await;
        assert!(scheme.starts_with("HTTP/1.1 401"), "{scheme}");

        shutdown.cancel();
        tokio::time::timeout(Duration::from_secs(2), server)
            .await
            .expect("server stops")
            .expect("server task")
            .expect("serve");
    }
}
