//! Command-line entry point: serve `/metrics`, or fetch it with `-query` for the
//! rollout's exec probe. Everything reusable lives in the library.

use std::io::Write as _;
use std::sync::Arc;
use std::time::Duration;

use anyhow::Context as _;
use health_exporter::Exporter;
use home_telemetry::{LevelFilter, TelemetryBuilder};

const DEFAULT_LISTEN: &str = "0.0.0.0:9116";
const QUERY_TIMEOUT: Duration = Duration::from_secs(5);

struct Options {
    listen: String,
    query: Option<String>,
}

/// Parses the Go `flag`-compatible subset: `-name value`, `--name value`,
/// `-name=value` and `--name=value`. The rollout still calls `-query URL`.
fn parse_args(args: impl Iterator<Item = String>) -> Result<Options, String> {
    let mut options = Options {
        listen: DEFAULT_LISTEN.to_owned(),
        query: None,
    };
    let mut args = args.peekable();
    while let Some(argument) = args.next() {
        let (flag, inline) = match argument.split_once('=') {
            Some((flag, value)) => (flag.to_owned(), Some(value.to_owned())),
            None => (argument, None),
        };
        let name = flag.trim_start_matches('-');
        match name {
            "listen" => {
                options.listen = inline
                    .or_else(|| args.next())
                    .ok_or_else(|| "-listen needs a value".to_owned())?;
            }
            "query" => {
                options.query = Some(
                    inline
                        .or_else(|| args.next())
                        .ok_or_else(|| "-query needs a URL".to_owned())?,
                );
            }
            other => return Err(format!("unknown flag: {other}")),
        }
    }
    Ok(options)
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let options = parse_args(std::env::args().skip(1)).map_err(anyhow::Error::msg)?;
    match options.query {
        Some(url) if !url.is_empty() => query(&url).await,
        _ => serve(&options.listen).await,
    }
}

/// Fetches the metrics from `url` and prints them, then exits.
async fn query(url: &str) -> anyhow::Result<()> {
    let client = home_tls::public_https_client()
        .context("tls")?
        .timeout(QUERY_TIMEOUT)
        .build()
        .context("http client")?;
    let response = client
        .get(url)
        .send()
        .await
        .with_context(|| format!("query {url}"))?;
    let status = response.status();
    let body = response
        .bytes()
        .await
        .with_context(|| format!("read {url}"))?;
    if !status.is_success() {
        anyhow::bail!("query {url}: HTTP {}", status.as_u16());
    }
    let mut stdout = std::io::stdout().lock();
    stdout.write_all(&body).context("write metrics")?;
    if !body.ends_with(b"\n") {
        stdout.write_all(b"\n").context("write metrics")?;
    }
    stdout.flush().context("write metrics")?;
    Ok(())
}

/// Serves `/metrics` until SIGTERM/SIGINT.
async fn serve(listen: &str) -> anyhow::Result<()> {
    TelemetryBuilder::new()
        .with_max_level(LevelFilter::INFO)
        .init()
        .context("telemetry")?;
    let listener = tokio::net::TcpListener::bind(listen)
        .await
        .with_context(|| format!("bind {listen}"))?;
    tracing::info!(
        service = "health-exporter",
        version = env!("CARGO_PKG_VERSION"),
        listen = %listen,
        "health-exporter listening"
    );
    axum::serve(listener, health_exporter::router(Arc::new(Exporter::new())))
        .with_graceful_shutdown(home_runtime::shutdown_signal())
        .await
        .context("serve")?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(arguments: &[&str]) -> Result<Options, String> {
        parse_args(arguments.iter().map(ToString::to_string))
    }

    #[test]
    fn defaults_to_the_chart_listen_address() {
        let options = parse(&[]).expect("defaults");
        assert_eq!(options.listen, DEFAULT_LISTEN);
        assert!(options.query.is_none());
    }

    #[test]
    fn accepts_single_and_double_dash_query() {
        assert_eq!(
            parse(&["-query", "http://127.0.0.1:9116/metrics"])
                .expect("single dash")
                .query
                .as_deref(),
            Some("http://127.0.0.1:9116/metrics")
        );
        assert_eq!(
            parse(&["--query=http://127.0.0.1:9116/metrics"])
                .expect("double dash")
                .query
                .as_deref(),
            Some("http://127.0.0.1:9116/metrics")
        );
    }

    #[test]
    fn accepts_listen_before_query() {
        let options =
            parse(&["-listen", "127.0.0.1:9116", "-query", "http://x/metrics"]).expect("flags");
        assert_eq!(options.listen, "127.0.0.1:9116");
        assert_eq!(options.query.as_deref(), Some("http://x/metrics"));
    }

    #[test]
    fn rejects_unknown_flags() {
        assert!(parse(&["-bogus"]).is_err());
    }
}
