//! LAN-only HTTP control for Home Assistant voice intents.
//!
//! Five routes, one bearer token and one JSON status document. The routes and
//! field names are the contract consumed by `projects/home-assistant/ha-voice`.

use std::sync::Arc;

use anyhow::{Context, Result};
use axum::extract::State;
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use ricardo_runtime::Supervisor;
use ricardo_secret::Token;
use serde::Serialize;
use tokio::net::TcpListener;
use tracing::{info, warn};

use crate::cli::Config;
use crate::gateway::Gateway;

#[derive(Serialize)]
struct StatusBody {
    bluetooth: bool,
    manual: bool,
    status: String,
    wifi: String,
}

pub fn router(gateway: Arc<Gateway>, token: Token) -> Router {
    let router = Router::new()
        .route("/status", get(status_response))
        .route("/gateway/on", post(gateway_on))
        .route("/gateway/off", post(gateway_off))
        .route("/bluetooth/on", post(bluetooth_on))
        .route("/bluetooth/off", post(bluetooth_off))
        .with_state(gateway);
    ricardo_http::with_bearer(router, token)
}

/// Load the bearer token, bind and spawn the control server.
pub async fn start(gateway: &Arc<Gateway>, config: &Config, supervisor: &Supervisor) -> Result<()> {
    let token = ricardo_secret::load_or_create_hex(&config.control_token_file, 32)
        .with_context(|| format!("load control token {}", config.control_token_file.display()))?;
    let listener = TcpListener::bind(&config.control_addr)
        .await
        .with_context(|| format!("bind control HTTP {}", config.control_addr))?;
    info!(addr = %config.control_addr, "control HTTP listening");
    let router = router(gateway.clone(), token);
    let shutdown = supervisor.token();
    supervisor.spawn("control-http", async move {
        if let Err(error) = ricardo_http::serve(listener, router, shutdown).await {
            warn!(%error, "control HTTP stopped");
        }
    });
    Ok(())
}

macro_rules! action {
    ($name:ident, $method:ident) => {
        async fn $name(State(gateway): State<Arc<Gateway>>) -> Response {
            gateway.$method().await;
            status_response(State(gateway)).await.into_response()
        }
    };
}
action!(gateway_on, open_manual);
action!(gateway_off, close_manual);
action!(bluetooth_on, enable_bluetooth);
action!(bluetooth_off, disable_bluetooth);

async fn status_response(State(gateway): State<Arc<Gateway>>) -> Json<StatusBody> {
    Json(StatusBody {
        bluetooth: gateway.bluetooth.is_on(),
        manual: gateway.is_manual(),
        status: gateway.status().to_string(),
        wifi: gateway.wpa.state().await,
    })
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use tokio_util::sync::CancellationToken;

    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpStream;

    use super::*;
    use crate::test_support::TestGateway;

    const TOKEN: &str = "0123456789abcdef0123456789abcdef";

    async fn start(tag: &str) -> (TestGateway, std::net::SocketAddr, CancellationToken) {
        let context = TestGateway::new(tag, |command| match command {
            "STATUS" => "wpa_state=COMPLETED".to_string(),
            _ => "OK".to_string(),
        });
        let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
        let address = listener.local_addr().expect("address");
        let shutdown = CancellationToken::new();
        tokio::spawn(ricardo_http::serve(
            listener,
            router(context.gateway.clone(), Token::new(TOKEN)),
            shutdown.clone(),
        ));
        (context, address, shutdown)
    }

    async fn request(
        address: std::net::SocketAddr,
        method: &str,
        path: &str,
        authorization: Option<&str>,
    ) -> String {
        let mut stream = tokio::time::timeout(Duration::from_secs(2), TcpStream::connect(address))
            .await
            .expect("connect timeout")
            .expect("connect");
        let mut request = format!("{method} {path} HTTP/1.1\r\nHost: localhost\r\n");
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
    async fn status_requires_the_bearer_token() {
        let (_context, address, shutdown) = start("http-auth").await;
        let unauthorized = request(address, "GET", "/status", None).await;
        assert!(unauthorized.starts_with("HTTP/1.1 401"), "{unauthorized}");
        let wrong = request(address, "GET", "/status", Some("Bearer wrong")).await;
        assert!(wrong.starts_with("HTTP/1.1 401"), "{wrong}");
        shutdown.cancel();
    }

    #[tokio::test]
    async fn status_json_fields_and_methods() {
        let (_context, address, shutdown) = start("http-status").await;
        let auth = format!("Bearer {TOKEN}");
        let status = request(address, "GET", "/status", Some(&auth)).await;
        assert!(status.starts_with("HTTP/1.1 200"), "{status}");
        assert!(
            status.contains("content-type: application/json"),
            "{status}"
        );
        let body = status.split("\r\n\r\n").nth(1).expect("body");
        let json: serde_json::Value = serde_json::from_str(body).expect("JSON status");
        assert_eq!(json["bluetooth"], false);
        assert_eq!(json["manual"], false);
        assert_eq!(json["status"], "IDLE");
        assert_eq!(json["wifi"], "COMPLETED");

        let wrong_method = request(address, "POST", "/status", Some(&auth)).await;
        assert!(wrong_method.starts_with("HTTP/1.1 405"), "{wrong_method}");

        let gateway_get = request(address, "GET", "/gateway/on", Some(&auth)).await;
        assert!(gateway_get.starts_with("HTTP/1.1 405"), "{gateway_get}");

        let gateway_on = request(address, "POST", "/gateway/on", Some(&auth)).await;
        assert!(gateway_on.starts_with("HTTP/1.1 200"), "{gateway_on}");
        assert!(gateway_on.contains("\"manual\":true"), "{gateway_on}");
        let gateway_off = request(address, "POST", "/gateway/off", Some(&auth)).await;
        assert!(gateway_off.contains("\"manual\":false"), "{gateway_off}");

        assert!(
            request(address, "GET", "/bluetooth/on", Some(&auth))
                .await
                .starts_with("HTTP/1.1 405")
        );
        assert!(
            request(address, "GET", "/bluetooth/off", Some(&auth))
                .await
                .starts_with("HTTP/1.1 405")
        );
        let missing = request(address, "GET", "/nope", Some(&auth)).await;
        assert!(missing.starts_with("HTTP/1.1 404"), "{missing}");
        shutdown.cancel();
    }

    #[tokio::test]
    async fn bluetooth_routes_are_idempotent_when_already_in_that_state() {
        let (_context, address, shutdown) = start("http-bluetooth").await;
        let auth = format!("Bearer {TOKEN}");
        let off = request(address, "POST", "/bluetooth/off", Some(&auth)).await;
        assert!(off.starts_with("HTTP/1.1 200"), "{off}");
        assert!(off.contains("\"bluetooth\":false"), "{off}");
        shutdown.cancel();
    }
}
