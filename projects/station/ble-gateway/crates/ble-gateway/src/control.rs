//! LAN-only HTTP control for Home Assistant voice intents.
//!
//! The endpoint is deliberately tiny: five routes, a bearer token and a JSON
//! status document. A hand-written HTTP/1.1 handler keeps the dependency set
//! small; every response closes the connection, so clients never depend on
//! keep-alive semantics.

use std::fmt::Write as _;
use std::sync::Arc;
use std::time::Duration;

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};

use crate::gateway::{Gateway, random_bytes};
use crate::logf;

const MAX_HEADER_BYTES: usize = 64 * 1024;

pub struct Response {
    pub code: u16,
    pub content_type: &'static str,
    pub nosniff: bool,
    pub body: String,
}

impl Response {
    fn text(code: u16, body: &str) -> Self {
        Self {
            code,
            content_type: "text/plain; charset=utf-8",
            nosniff: true,
            body: format!("{body}\n"),
        }
    }

    fn json(body: String) -> Self {
        Self {
            code: 200,
            content_type: "application/json",
            nosniff: false,
            body,
        }
    }

    fn reason(&self) -> &'static str {
        match self.code {
            200 => "OK",
            401 => "Unauthorized",
            404 => "Not Found",
            405 => "Method Not Allowed",
            _ => "Internal Server Error",
        }
    }

    fn to_bytes(&self) -> Vec<u8> {
        let mut head = String::new();
        let _ = write!(
            head,
            "HTTP/1.1 {} {}\r\nContent-Type: {}\r\nContent-Length: {}\r\nConnection: close\r\n",
            self.code,
            self.reason(),
            self.content_type,
            self.body.len()
        );
        if self.nosniff {
            head.push_str("X-Content-Type-Options: nosniff\r\n");
        }
        head.push_str("\r\n");
        let mut bytes = head.into_bytes();
        bytes.extend_from_slice(self.body.as_bytes());
        bytes
    }
}

fn constant_time_eq(left: &[u8], right: &[u8]) -> bool {
    if left.len() != right.len() {
        return false;
    }
    let mut difference = 0u8;
    for (a, b) in left.iter().zip(right) {
        difference |= a ^ b;
    }
    difference == 0
}

fn hex(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        let _ = write!(out, "{byte:02x}");
    }
    out
}

/// Escape a string the way Go's encoding/json does by default (including
/// HTML-safe escapes), so the document matches byte for byte.
fn json_escape(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for ch in value.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            '\u{08}' => out.push_str("\\b"),
            '\u{0c}' => out.push_str("\\f"),
            '<' => out.push_str("\\u003c"),
            '>' => out.push_str("\\u003e"),
            '&' => out.push_str("\\u0026"),
            '\u{2028}' => out.push_str("\\u2028"),
            '\u{2029}' => out.push_str("\\u2029"),
            c if (c as u32) < 0x20 => {
                let _ = write!(out, "\\u{:04x}", c as u32);
            }
            c => out.push(c),
        }
    }
    out
}

impl Gateway {
    pub fn load_control_token(&self, path: &std::path::Path) -> std::io::Result<()> {
        if let Ok(bytes) = std::fs::read(path) {
            let token = String::from_utf8_lossy(&bytes).trim().to_string();
            if token.len() >= 16 {
                self.lock_state().control_token = token;
                return Ok(());
            }
        }
        let token = hex(&random_bytes(32)?);
        let dir = crate::gateway::directory_of(path);
        crate::gateway::create_private_dir(&dir)?;
        crate::gateway::write_mode(path, format!("{token}\n").as_bytes(), 0o600)?;
        self.lock_state().control_token = token;
        Ok(())
    }

    pub async fn control_route(
        self: &Arc<Self>,
        method: &str,
        target: &str,
        authorization: Option<&str>,
    ) -> Response {
        let expected = format!("Bearer {}", self.lock_state().control_token);
        let authorized = authorization
            .is_some_and(|value| constant_time_eq(value.as_bytes(), expected.as_bytes()));
        if !authorized {
            return Response::text(401, "unauthorized");
        }

        let path = target.split('?').next().unwrap_or(target);
        match path {
            "/status" => {
                if method != "GET" {
                    return Response::text(405, "method not allowed");
                }
                Response::json(self.control_status_json().await)
            }
            "/gateway/on" => {
                if method != "POST" {
                    return Response::text(405, "method not allowed");
                }
                self.open_manual().await;
                Response::json(self.control_status_json().await)
            }
            "/gateway/off" => {
                if method != "POST" {
                    return Response::text(405, "method not allowed");
                }
                self.close_manual().await;
                Response::json(self.control_status_json().await)
            }
            "/bluetooth/on" => {
                if method != "POST" {
                    return Response::text(405, "method not allowed");
                }
                self.enable_bluetooth().await;
                Response::json(self.control_status_json().await)
            }
            "/bluetooth/off" => {
                if method != "POST" {
                    return Response::text(405, "method not allowed");
                }
                self.disable_bluetooth().await;
                Response::json(self.control_status_json().await)
            }
            _ => Response::text(404, "404 page not found"),
        }
    }

    async fn control_status_json(&self) -> String {
        let snapshot = self.snapshot();
        let wifi = self.wpa.state().await;
        format!(
            "{{\"bluetooth\":{},\"manual\":{},\"status\":\"{}\",\"wifi\":\"{}\"}}\n",
            snapshot.bluetooth,
            snapshot.manual,
            json_escape(&snapshot.status),
            json_escape(&wifi)
        )
    }

    pub async fn start_control(
        self: &Arc<Self>,
        addr: &str,
        token_path: &std::path::Path,
    ) -> Result<(), String> {
        self.load_control_token(token_path)
            .map_err(|err| err.to_string())?;
        let listener = TcpListener::bind(addr)
            .await
            .map_err(|err| format!("bind {addr}: {err}"))?;
        logf!(self.logger, "control HTTP listening on {addr}");
        self.serve_control(listener);
        Ok(())
    }

    pub fn serve_control(self: &Arc<Self>, listener: TcpListener) {
        let gateway = self.clone();
        tokio::spawn(async move {
            loop {
                match listener.accept().await {
                    Ok((stream, _)) => {
                        let gateway = gateway.clone();
                        tokio::spawn(async move {
                            if let Err(err) = gateway.handle_control_connection(stream).await {
                                logf!(gateway.logger, "control HTTP: {err}");
                            }
                        });
                    }
                    Err(err) => {
                        logf!(gateway.logger, "control HTTP stopped: {err}");
                        break;
                    }
                }
            }
        });
    }

    async fn handle_control_connection(
        self: &Arc<Self>,
        mut stream: TcpStream,
    ) -> std::io::Result<()> {
        let _ = tokio::time::timeout(Duration::from_secs(5), async {
            let Some((method, target, headers)) = read_request(&mut stream).await? else {
                return Ok::<(), std::io::Error>(());
            };
            let authorization = headers
                .iter()
                .find(|(name, _)| name == "authorization")
                .map(|(_, value)| value.as_str());
            let response = self.control_route(&method, &target, authorization).await;
            stream.write_all(&response.to_bytes()).await?;
            stream.shutdown().await
        })
        .await;
        Ok(())
    }
}

async fn read_request(
    stream: &mut TcpStream,
) -> std::io::Result<Option<(String, String, Vec<(String, String)>)>> {
    let mut buffer = Vec::with_capacity(1024);
    let mut chunk = [0u8; 1024];
    let header_end = loop {
        let read = stream.read(&mut chunk).await?;
        if read == 0 {
            return Ok(None);
        }
        buffer.extend_from_slice(&chunk[..read]);
        if let Some(position) = buffer.windows(4).position(|window| window == b"\r\n\r\n") {
            break position;
        }
        if buffer.len() > MAX_HEADER_BYTES {
            return Ok(None);
        }
    };

    let text = String::from_utf8_lossy(&buffer[..header_end]).into_owned();
    let mut lines = text.split("\r\n");
    let Some(request_line) = lines.next() else {
        return Ok(None);
    };
    let mut parts = request_line.split_whitespace();
    let (Some(method), Some(target)) = (parts.next(), parts.next()) else {
        return Ok(None);
    };
    let mut headers = Vec::new();
    for line in lines {
        if line.is_empty() {
            continue;
        }
        if let Some((name, value)) = line.split_once(':') {
            headers.push((name.trim().to_ascii_lowercase(), value.trim().to_string()));
        }
    }
    Ok(Some((method.to_string(), target.to_string(), headers)))
}

#[cfg(test)]
mod tests {
    use std::path::Path;
    use std::time::Duration;

    use super::*;
    use crate::test_support::{fake_wpa, new_gateway, read_file, temp_dir};

    const TOKEN: &str = "0123456789abcdef0123456789abcdef";

    #[test]
    fn json_escape_matches_go() {
        assert_eq!(json_escape("a\"b\\c\n"), "a\\\"b\\\\c\\n");
        assert_eq!(json_escape("<&>"), "\\u003c\\u0026\\u003e");
        assert_eq!(json_escape("\u{2028}"), "\\u2028");
        assert_eq!(json_escape("\u{01}"), "\\u0001");
    }

    #[test]
    fn constant_time_comparison() {
        assert!(constant_time_eq(b"Bearer abc", b"Bearer abc"));
        assert!(!constant_time_eq(b"Bearer abc", b"Bearer abd"));
        assert!(!constant_time_eq(b"Bearer abc", b"Bearer ab"));
    }

    #[tokio::test]
    async fn control_auth() {
        let context = new_gateway("control-auth", Path::new("/nonexistent/wpa_cli"));
        let gateway = &context.gateway;
        gateway.lock_state().control_token = TOKEN.to_string();

        let missing = gateway.control_route("GET", "/status", None).await;
        assert_eq!(missing.code, 401);
        let wrong = gateway
            .control_route("GET", "/status", Some("Bearer wrong"))
            .await;
        assert_eq!(wrong.code, 401);
        let correct = gateway
            .control_route("GET", "/status", Some(&format!("Bearer {TOKEN}")))
            .await;
        assert_eq!(correct.code, 200);
    }

    #[tokio::test]
    async fn control_status_and_methods() {
        let dir = temp_dir("control-status-script");
        let path = fake_wpa(
            &dir,
            r#"case "$*" in *" status"*) echo wpa_state=COMPLETED ;; *) echo OK ;; esac"#,
        );
        let context = new_gateway("control-status", &path);
        let gateway = &context.gateway;
        gateway.lock_state().control_token = TOKEN.to_string();
        let auth = format!("Bearer {TOKEN}");

        let status = gateway.control_route("GET", "/status", Some(&auth)).await;
        assert_eq!(status.code, 200);
        assert!(status.body.contains("\"wifi\":\"COMPLETED\""));
        assert!(status.body.contains("\"status\":\"IDLE\",\"wifi\""));
        let status_post = gateway.control_route("POST", "/status", Some(&auth)).await;
        assert_eq!(status_post.code, 405);

        let on_get = gateway
            .control_route("GET", "/gateway/on", Some(&auth))
            .await;
        assert_eq!(on_get.code, 405);
        let on_post = gateway
            .control_route("POST", "/gateway/on", Some(&auth))
            .await;
        assert_eq!(on_post.code, 200);
        assert!(on_post.body.contains("\"manual\":true"));
        let off_post = gateway
            .control_route("POST", "/gateway/off", Some(&auth))
            .await;
        assert_eq!(off_post.code, 200);
        assert!(off_post.body.contains("\"manual\":false"));

        assert_eq!(
            gateway
                .control_route("GET", "/bluetooth/on", Some(&auth))
                .await
                .code,
            405
        );
        assert_eq!(
            gateway
                .control_route("GET", "/bluetooth/off", Some(&auth))
                .await
                .code,
            405
        );
        assert_eq!(
            gateway
                .control_route("GET", "/nope", Some(&auth))
                .await
                .code,
            404
        );
        let _ = std::fs::remove_dir_all(dir);
    }

    #[tokio::test]
    async fn load_control_token_creates_private_file_and_reuses() {
        use std::os::unix::fs::PermissionsExt;

        let context = new_gateway("control-token", Path::new("/nonexistent/wpa_cli"));
        let gateway = &context.gateway;
        let path = context.dir.join("control-token");
        gateway.load_control_token(&path).expect("load token");
        assert_eq!(gateway.lock_state().control_token.len(), 64);
        let mode = std::fs::metadata(&path)
            .expect("stat token")
            .permissions()
            .mode()
            & 0o777;
        assert_eq!(mode, 0o600);
        assert_eq!(read_file(&path).trim(), gateway.lock_state().control_token);

        std::fs::write(&path, "reuse-me-0123456789\n").expect("write token");
        gateway.load_control_token(&path).expect("reload token");
        assert_eq!(gateway.lock_state().control_token, "reuse-me-0123456789");
    }

    #[tokio::test]
    async fn http_round_trip() {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};

        let dir = temp_dir("control-http-script");
        let path = fake_wpa(
            &dir,
            r#"case "$*" in *" status"*) echo wpa_state=COMPLETED ;; *) echo OK ;; esac"#,
        );
        let context = new_gateway("control-http", &path);
        let gateway = &context.gateway;
        gateway.lock_state().control_token = TOKEN.to_string();
        let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
        let address = listener.local_addr().expect("addr");
        gateway.serve_control(listener);

        let mut stream = tokio::time::timeout(Duration::from_secs(2), TcpStream::connect(address))
            .await
            .expect("connect timeout")
            .expect("connect");
        let request = format!(
            "GET /status HTTP/1.1\r\nHost: 127.0.0.1\r\nAuthorization: Bearer {TOKEN}\r\n\r\n"
        );
        stream.write_all(request.as_bytes()).await.expect("write");
        let mut response = Vec::new();
        tokio::time::timeout(Duration::from_secs(2), stream.read_to_end(&mut response))
            .await
            .expect("read timeout")
            .expect("read");
        let text = String::from_utf8_lossy(&response);
        assert!(text.starts_with("HTTP/1.1 200 OK\r\n"), "{text}");
        assert!(text.contains("Content-Type: application/json"), "{text}");
        assert!(text.contains("\"wifi\":\"COMPLETED\""), "{text}");

        let mut unauthorized = TcpStream::connect(address).await.expect("connect");
        unauthorized
            .write_all(b"GET /status HTTP/1.1\r\nHost: x\r\n\r\n")
            .await
            .expect("write");
        let mut response = Vec::new();
        unauthorized.read_to_end(&mut response).await.expect("read");
        let text = String::from_utf8_lossy(&response);
        assert!(text.starts_with("HTTP/1.1 401 Unauthorized\r\n"), "{text}");
        assert!(text.ends_with("unauthorized\n"), "{text}");
        let _ = std::fs::remove_dir_all(dir);
    }
}
