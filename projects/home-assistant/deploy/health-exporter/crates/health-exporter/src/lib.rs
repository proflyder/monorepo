//! Runtime chart's TCP health exporter.
//!
//! It is the Rust replacement for the former Go binary and keeps its contract:
//! operational TCP checks only, no commands, no credentials, no conversation
//! content. Metric names, labels, ports and the `ENDPOINT_HOST_*` environment
//! variables are unchanged, so the existing dashboards and alerts keep working.
//!
//! The same binary is its own in-pod client (`-query URL`), because the
//! distroless image has no shell, curl or python for the rollout's exec check.

use std::future::Future;
use std::io;
use std::pin::Pin;
use std::sync::Arc;
use std::time::Duration;

use axum::Router;
use axum::extract::State;
use axum::http::header;
use axum::response::IntoResponse;
use axum::routing::get;
use tokio::net::TcpStream;

/// TCP connect timeout for one endpoint, matching the Python/Go exporter.
pub const CONNECT_TIMEOUT: Duration = Duration::from_secs(1);

/// One TCP check. The order is the Python exporter's dict order, so the metrics
/// text is stable for scraping and diffing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Endpoint {
    pub name: &'static str,
    pub port: u16,
}

/// The services the home runtime exposes on the host network.
pub const ENDPOINTS: [Endpoint; 7] = [
    Endpoint {
        name: "ha",
        port: 8123,
    },
    Endpoint {
        name: "music_assistant",
        port: 8095,
    },
    Endpoint {
        name: "piper",
        port: 10200,
    },
    Endpoint {
        name: "whisper",
        port: 10300,
    },
    Endpoint {
        name: "gigaam",
        port: 10305,
    },
    Endpoint {
        name: "hermes",
        port: 8642,
    },
    Endpoint {
        name: "voice_proxy",
        port: 8643,
    },
];

type DialFuture = Pin<Box<dyn Future<Output = io::Result<()>> + Send>>;
type DialFn = Box<dyn Fn(String, u16) -> DialFuture + Send + Sync>;
type HostFn = Box<dyn Fn(&str) -> String + Send + Sync>;
type NowFn = Box<dyn Fn() -> u64 + Send + Sync>;

/// Renders the metrics document.
pub struct Exporter {
    /// Connects to `host:port`. Injectable for tests.
    dial: DialFn,
    /// Resolves the address for an endpoint name (`ENDPOINT_HOST_<NAME>`).
    host: HostFn,
    /// Clock in nanoseconds, injectable so a test can assert formatting without
    /// depending on wall time.
    now: NowFn,
}

impl Exporter {
    /// Returns an exporter that reads `ENDPOINT_HOST_*` and dials with a 1s
    /// timeout, matching the Python implementation.
    #[must_use]
    pub fn new() -> Self {
        let start = std::time::Instant::now();
        Self {
            dial: Box::new(|host, port| {
                Box::pin(async move {
                    let address = format!("{host}:{port}");
                    match tokio::time::timeout(CONNECT_TIMEOUT, TcpStream::connect(&address)).await
                    {
                        Ok(Ok(stream)) => {
                            drop(stream);
                            Ok(())
                        }
                        Ok(Err(error)) => Err(error),
                        Err(_) => Err(io::Error::new(
                            io::ErrorKind::TimedOut,
                            "tcp connect timed out",
                        )),
                    }
                })
            }),
            host: Box::new(|name| {
                let variable = format!("ENDPOINT_HOST_{}", name.to_uppercase());
                std::env::var(variable)
                    .ok()
                    .filter(|value| !value.is_empty())
                    .unwrap_or_else(|| "127.0.0.1".to_owned())
            }),
            now: Box::new(move || u64::try_from(start.elapsed().as_nanos()).unwrap_or(u64::MAX)),
        }
    }

    /// Renders the Prometheus text exposition.
    pub async fn metrics(&self) -> String {
        use std::fmt::Write as _;

        // `home_endpoint_*` is the current name. The `ricardo_endpoint_*`
        // series stay for one rollout as deprecated aliases so an older
        // dashboard or alert rule keeps working while the new exporter lands.
        let mut out = String::from(
            "# HELP home_endpoint_up Whether a configured TCP endpoint accepts connections.\n\
             # TYPE home_endpoint_up gauge\n\
             # HELP home_endpoint_connect_seconds TCP connection time, not voice latency.\n\
             # TYPE home_endpoint_connect_seconds gauge\n\
             # HELP ricardo_endpoint_up Deprecated alias of home_endpoint_up.\n\
             # TYPE ricardo_endpoint_up gauge\n\
             # HELP ricardo_endpoint_connect_seconds Deprecated alias of home_endpoint_connect_seconds.\n\
             # TYPE ricardo_endpoint_connect_seconds gauge\n",
        );
        for endpoint in ENDPOINTS {
            let started = (self.now)();
            let up = if (self.dial)((self.host)(endpoint.name), endpoint.port)
                .await
                .is_ok()
            {
                1
            } else {
                0
            };
            let elapsed = (self.now)().saturating_sub(started);
            let seconds = elapsed as f64 / 1_000_000_000.0;
            let _ = writeln!(
                out,
                "home_endpoint_up{{endpoint=\"{}\"}} {}",
                endpoint.name, up
            );
            let _ = writeln!(
                out,
                "home_endpoint_connect_seconds{{endpoint=\"{}\"}} {seconds:.6}",
                endpoint.name
            );
            let _ = writeln!(
                out,
                "ricardo_endpoint_up{{endpoint=\"{}\"}} {}",
                endpoint.name, up
            );
            let _ = writeln!(
                out,
                "ricardo_endpoint_connect_seconds{{endpoint=\"{}\"}} {seconds:.6}",
                endpoint.name
            );
        }
        out
    }
}

impl Default for Exporter {
    fn default() -> Self {
        Self::new()
    }
}

/// Serves `/metrics` and nothing else.
pub fn router(exporter: Arc<Exporter>) -> Router {
    Router::new()
        .route("/metrics", get(metrics_handler))
        .with_state(exporter)
}

async fn metrics_handler(State(exporter): State<Arc<Exporter>>) -> impl IntoResponse {
    (
        [(header::CONTENT_TYPE, "text/plain; version=0.0.4")],
        exporter.metrics().await,
    )
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;
    use std::sync::atomic::{AtomicU64, Ordering};

    use axum::body::Body;
    use http::{Request, StatusCode};
    use http_body_util::BodyExt as _;
    use tower::ServiceExt as _;

    use super::*;

    fn golden_path() -> std::path::PathBuf {
        let relative = "projects/home-assistant/deploy/health-exporter/crates/health-exporter/testdata/metrics.golden";
        if let Ok(root) = std::env::var("TEST_SRCDIR") {
            let workspace = std::env::var("TEST_WORKSPACE").unwrap_or_default();
            let candidate = std::path::Path::new(&root).join(&workspace).join(relative);
            if candidate.exists() {
                return candidate;
            }
        }
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("testdata")
            .join("metrics.golden")
    }

    /// A clock that advances exactly one millisecond per call, mirroring the Go
    /// golden generator.
    fn step_clock() -> NowFn {
        let counter = AtomicU64::new(0);
        Box::new(move || counter.fetch_add(1_000_000, Ordering::SeqCst))
    }

    fn all_up() -> DialFn {
        Box::new(|_host, _port| Box::pin(async { Ok(()) }))
    }

    fn loopback() -> HostFn {
        Box::new(|_name| "127.0.0.1".to_owned())
    }

    #[tokio::test]
    async fn golden_metrics_document_matches_the_go_exporter() {
        let exporter = Exporter {
            dial: Box::new(|_host, port| {
                Box::pin(async move {
                    if port == 8643 {
                        Err(io::Error::new(io::ErrorKind::ConnectionRefused, "refused"))
                    } else {
                        Ok(())
                    }
                })
            }),
            host: loopback(),
            now: step_clock(),
        };
        let expected = std::fs::read_to_string(golden_path()).expect("golden metrics file");
        assert_eq!(exporter.metrics().await, expected);
    }

    #[tokio::test]
    async fn every_endpoint_has_both_metrics() {
        let exporter = Exporter {
            dial: all_up(),
            host: loopback(),
            now: step_clock(),
        };
        let body = exporter.metrics().await;
        assert!(body.contains("# TYPE home_endpoint_up gauge"));
        assert!(body.contains("# TYPE home_endpoint_connect_seconds gauge"));
        for endpoint in ENDPOINTS {
            assert!(
                body.contains(&format!(
                    "home_endpoint_up{{endpoint=\"{}\"}} 1",
                    endpoint.name
                )),
                "{} up metric is missing",
                endpoint.name
            );
            assert!(
                body.contains(&format!(
                    "home_endpoint_connect_seconds{{endpoint=\"{}\"}} ",
                    endpoint.name
                )),
                "{} duration metric is missing",
                endpoint.name
            );
        }
    }

    #[tokio::test]
    async fn failed_dial_is_down() {
        let exporter = Exporter {
            dial: Box::new(|_host, port| {
                Box::pin(async move {
                    if port == 8643 {
                        Err(io::Error::new(io::ErrorKind::ConnectionRefused, "refused"))
                    } else {
                        Ok(())
                    }
                })
            }),
            host: loopback(),
            now: step_clock(),
        };
        let body = exporter.metrics().await;
        assert!(body.contains("home_endpoint_up{endpoint=\"voice_proxy\"} 0"));
    }

    #[tokio::test]
    async fn ha_can_be_checked_on_a_separate_host() {
        let seen = Arc::new(Mutex::new(Vec::new()));
        let recorder = Arc::clone(&seen);
        let exporter = Exporter {
            dial: Box::new(move |host, port| {
                recorder
                    .lock()
                    .expect("dialled addresses")
                    .push(format!("{host}:{port}"));
                Box::pin(async { Ok(()) })
            }),
            host: Box::new(|name| {
                if name == "ha" {
                    "haos.home.arpa".to_owned()
                } else {
                    "127.0.0.1".to_owned()
                }
            }),
            now: step_clock(),
        };
        let body = exporter.metrics().await;
        assert!(body.contains("home_endpoint_up{endpoint=\"ha\"} 1"));
        let dialled = seen.lock().expect("dialled addresses").clone();
        assert!(dialled.contains(&"haos.home.arpa:8123".to_owned()));
        assert!(dialled.contains(&"127.0.0.1:8095".to_owned()));
    }

    #[tokio::test]
    async fn handler_serves_metrics_and_rejects_other_paths() {
        let exporter = Arc::new(Exporter {
            dial: all_up(),
            host: loopback(),
            now: step_clock(),
        });
        let response = router(exporter)
            .oneshot(
                Request::builder()
                    .uri("/metrics")
                    .body(Body::empty())
                    .expect("request"),
            )
            .await
            .expect("response");
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(
            response
                .headers()
                .get(header::CONTENT_TYPE)
                .and_then(|value| value.to_str().ok()),
            Some("text/plain; version=0.0.4")
        );
        let body = response
            .into_body()
            .collect()
            .await
            .expect("body")
            .to_bytes();
        assert!(std::str::from_utf8(&body).expect("utf-8").contains(
            "# HELP home_endpoint_up Whether a configured TCP endpoint accepts connections."
        ));

        let other = router(Arc::new(Exporter {
            dial: all_up(),
            host: loopback(),
            now: step_clock(),
        }))
        .oneshot(
            Request::builder()
                .uri("/healthz")
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("response");
        assert_eq!(other.status(), StatusCode::NOT_FOUND);
    }
}
