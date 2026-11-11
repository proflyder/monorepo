//! Minimal HTTP/1.1 endpoint: `GET /metrics` on the LAN interface.
//!
//! One thread, one connection at a time (Prometheus scrapes serially), no
//! dependencies beyond `std`. The socket is read with a timeout, so a stalled
//! client cannot keep the listener from the next scrape.

use std::io::{Read as _, Write as _};
use std::net::{TcpListener, TcpStream};
use std::path::PathBuf;
use std::time::Duration;

use tracing::{info, warn};

use crate::metrics::Metrics;
use crate::system::Collector;

const READ_TIMEOUT: Duration = Duration::from_secs(5);
const WRITE_TIMEOUT: Duration = Duration::from_secs(5);
const MAX_REQUEST: usize = 8192;

/// Binds `0.0.0.0:port` and serves `/metrics` in a detached thread.
///
/// Err when the port cannot be bound; the daemon then runs without metrics
/// (the caller logs it and continues).
pub fn spawn(port: u16, metrics: &'static Metrics) -> std::io::Result<()> {
    let listener = TcpListener::bind(("0.0.0.0", port))?;
    std::thread::Builder::new()
        .name("metrics-http".into())
        .spawn(move || serve(&listener, metrics))?;
    info!("metrics: http://0.0.0.0:{port}/metrics");
    Ok(())
}

fn serve(listener: &TcpListener, metrics: &'static Metrics) {
    let collector = Collector::new(PathBuf::from("/"));
    for stream in listener.incoming() {
        match stream {
            Ok(stream) => {
                if let Err(e) = handle(stream, metrics, &collector) {
                    warn!("metrics: request: {e}");
                }
            }
            Err(e) => warn!("metrics: accept: {e}"),
        }
    }
}

fn handle(mut stream: TcpStream, metrics: &Metrics, collector: &Collector) -> std::io::Result<()> {
    stream.set_read_timeout(Some(READ_TIMEOUT))?;
    stream.set_write_timeout(Some(WRITE_TIMEOUT))?;
    let mut request = Vec::new();
    let mut buf = [0_u8; 1024];
    while !request.windows(4).any(|w| w == b"\r\n\r\n") && request.len() < MAX_REQUEST {
        let n = stream.read(&mut buf)?;
        if n == 0 {
            break;
        }
        request.extend_from_slice(&buf[..n]);
    }
    let first = request.split(|&b| b == b'\n').next().unwrap_or_default();
    let mut parts = first.split(|&b| b == b' ');
    let method = parts.next().unwrap_or_default();
    let path = parts.next().unwrap_or_default();
    let (status, content_type, body) = if method == b"GET" && path == b"/metrics" {
        let sample = collector.sample();
        (
            "200 OK",
            "text/plain; version=0.0.4; charset=utf-8",
            metrics.render(&sample),
        )
    } else if method == b"GET" && path == b"/" {
        (
            "200 OK",
            "text/plain; charset=utf-8",
            "pulsar metrics: GET /metrics\n".to_owned(),
        )
    } else {
        ("404 Not Found", "text/plain; charset=utf-8", String::new())
    };
    let response = format!(
        "HTTP/1.1 {status}\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );
    stream.write_all(response.as_bytes())
}
