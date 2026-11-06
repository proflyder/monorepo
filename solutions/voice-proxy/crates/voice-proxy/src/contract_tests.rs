//! Replays the golden HTTP fixtures from `solutions/voice-proxy/testdata/contract`
//! through the Rust proxy. The fixtures are the Go implementation's files,
//! unchanged.
//!
//! The upstream double is a tiny blocking HTTP/1.1 server on loopback: the
//! contract runner calls its router factory from inside an async runtime, so a
//! `tokio` server cannot be spawned there (`block_on` would panic).

use std::io::{Read as _, Write as _};
use std::net::TcpListener;
use std::sync::{Arc, OnceLock};

use axum::Router;
use home_contracttest::{Options as ContractOptions, run};
use serde_json::Value;

use crate::api::{Options, Proxy};
use crate::domain::clock::SystemClock;
use crate::relay::Upstream;

fn contract_dir() -> String {
    // Bazel runs with TEST_SRCDIR/runfiles; cargo runs from the package dir and
    // exports CARGO_MANIFEST_DIR at compile time.
    if std::env::var_os("TEST_SRCDIR").is_some() {
        "solutions/voice-proxy/testdata/contract".to_owned()
    } else {
        option_env!("CARGO_MANIFEST_DIR").map_or_else(
            || "solutions/voice-proxy/testdata/contract".to_owned(),
            |root| format!("{root}/../../testdata/contract"),
        )
    }
}

/// Starts the deterministic Hermes double once and returns its base URL.
fn upstream_url() -> &'static str {
    static URL: OnceLock<String> = OnceLock::new();
    URL.get_or_init(|| {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind contract upstream");
        let address = listener.local_addr().expect("address");
        std::thread::spawn(move || {
            for stream in listener.incoming() {
                let Ok(mut stream) = stream else {
                    return;
                };
                let _ = serve_one(&mut stream);
            }
        });
        format!("http://{address}")
    })
}

fn new_router(_scenario: &str) -> Router {
    let upstream =
        Upstream::new(upstream_url(), std::time::Duration::from_secs(5)).expect("upstream client");
    let proxy = Arc::new(Proxy::new(Options {
        upstream,
        reasoning_effort: "low".to_owned(),
        max_body: 10 << 20,
        cancel_phrases: crate::domain::cancel::DEFAULT_CANCEL_PHRASES
            .iter()
            .map(|phrase| (*phrase).to_owned())
            .collect(),
        router: None,
        clock: Arc::new(SystemClock),
    }));
    proxy.router()
}

fn serve_one(stream: &mut std::net::TcpStream) -> std::io::Result<()> {
    let request = read_request(stream)?;
    let response = respond(&request);
    stream.write_all(response.as_bytes())?;
    stream.flush()
}

struct Request {
    method: String,
    body: Vec<u8>,
}

fn read_request(stream: &mut std::net::TcpStream) -> std::io::Result<Request> {
    let mut data = Vec::new();
    let mut buffer = [0u8; 4096];
    loop {
        if let Some(index) = find_headers_end(&data) {
            let head = String::from_utf8_lossy(&data[..index]).into_owned();
            let mut lines = head.lines();
            let request_line = lines.next().unwrap_or_default();
            let mut parts = request_line.split_whitespace();
            let method = parts.next().unwrap_or_default().to_owned();
            let _path = parts.next().unwrap_or_default();
            let length = lines
                .filter_map(|line| line.split_once(':'))
                .find(|(name, _)| name.eq_ignore_ascii_case("content-length"))
                .and_then(|(_, value)| value.trim().parse::<usize>().ok())
                .unwrap_or(0);
            let body_start = index + 4;
            while data.len() < body_start + length {
                let read = stream.read(&mut buffer)?;
                if read == 0 {
                    break;
                }
                data.extend_from_slice(&buffer[..read]);
            }
            let body = data[body_start..data.len().min(body_start + length)].to_vec();
            return Ok(Request { method, body });
        }
        let read = stream.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        data.extend_from_slice(&buffer[..read]);
    }
    Ok(Request {
        method: "GET".to_owned(),
        body: Vec::new(),
    })
}

fn find_headers_end(data: &[u8]) -> Option<usize> {
    data.windows(4).position(|window| window == b"\r\n\r\n")
}

fn respond(request: &Request) -> String {
    if request.method == "GET" {
        return http_response(
            "200 OK",
            "application/json",
            r#"{"object":"list","data":[{"id":"hermes-agent","object":"model"}]}"#,
        );
    }
    let payload: Value = serde_json::from_slice(&request.body).unwrap_or(Value::Null);
    if payload["model"] == "boom" {
        return http_response(
            "502 Bad Gateway",
            "application/json",
            r#"{"error":{"message":"upstream exploded","type":"upstream_error"}}"#,
        );
    }
    if payload["stream"] == true {
        let body = [
            r#"data: {"id":"chatcmpl-contract","object":"chat.completion.chunk","created":1000,"model":"hermes-agent","choices":[{"index":0,"delta":{"role":"assistant"},"finish_reason":null}]}"#,
            "",
            r#"data: {"id":"chatcmpl-contract","object":"chat.completion.chunk","created":1000,"model":"hermes-agent","choices":[{"index":0,"delta":{"content":"Привет."},"finish_reason":null}]}"#,
            "",
            r#"data: {"id":"chatcmpl-contract","object":"chat.completion.chunk","created":1000,"model":"hermes-agent","choices":[{"index":0,"delta":{},"finish_reason":"stop"}]}"#,
            "",
            "data: [DONE]",
            "",
            "",
        ]
        .join("\n");
        return http_response("200 OK", "text/event-stream", &body);
    }
    http_response(
        "200 OK",
        "application/json",
        r#"{"id":"chatcmpl-contract","object":"chat.completion","created":1000,"model":"hermes-agent","choices":[{"index":0,"message":{"role":"assistant","content":"Привет."},"finish_reason":"stop"}],"usage":{"prompt_tokens":1,"completion_tokens":2,"total_tokens":3}}"#,
    )
}

fn http_response(status: &str, content_type: &str, body: &str) -> String {
    format!(
        "HTTP/1.1 {status}\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    )
}

/// Replaces the only non-deterministic field of a local completion: its
/// wall-clock `created` timestamp.
fn normalize_created(_scenario: &str, value: &mut Value) {
    match value {
        Value::Array(items) => {
            for item in items {
                normalize_created("", item);
            }
        }
        Value::Object(object) => {
            for (key, item) in object.iter_mut() {
                if key == "created" {
                    *item = Value::String("<any>".to_owned());
                } else {
                    normalize_created("", item);
                }
            }
        }
        _ => {}
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn contract_fixtures() {
    let count = run(
        contract_dir(),
        new_router,
        ContractOptions::default().with_normalizer(normalize_created),
    )
    .await
    .expect("all voice-proxy contract fixtures pass");
    assert_eq!(count, 5);
}
