//! Read-only Hermes config drift exporter.
//!
//! The exporter rebuilds the render plan on a timer and publishes the result as
//! Prometheus text on `GET /metrics`. It never writes to `HERMES_HOME`: the plan
//! is the same one `hermes render-config` applies, only inspected. A failed
//! plan (placeholder refusal, unreadable private values) keeps the process up
//! and reports `hermes_config_check_success 0`, so the alert fires instead of
//! the sidecar crash-looping on a bad overlay.

use std::fmt::Write as _;
use std::fs;
use std::io::{BufRead, BufReader, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use sha2::{Digest, Sha256};

use crate::hermesconfig::{self, Options};

/// One completed drift check, ready to render as Prometheus text.
#[derive(Clone, Debug, Default)]
pub struct CheckResult {
    /// Paths (relative to `HERMES_HOME`) the plan would rewrite.
    pub drifted: Vec<String>,
    /// Whether the plan was built without an error.
    pub success: bool,
    /// Unix time of the check.
    pub timestamp_seconds: u64,
    /// Digest of the overlay inputs the plan was built from.
    pub overlay_digest: String,
}

/// Escapes a Prometheus label value.
fn escape_label(value: &str) -> String {
    let mut escaped = String::with_capacity(value.len());
    for character in value.chars() {
        match character {
            '\\' => escaped.push_str("\\\\"),
            '"' => escaped.push_str("\\\""),
            '\n' => escaped.push_str("\\n"),
            other => escaped.push(other),
        }
    }
    escaped
}

/// Renders one check result as Prometheus text exposition.
#[must_use]
pub fn render_metrics(result: &CheckResult) -> String {
    let mut out = String::new();
    let _ = writeln!(
        out,
        "# HELP hermes_config_drift 1 when the render plan would rewrite the file."
    );
    let _ = writeln!(out, "# TYPE hermes_config_drift gauge");
    for file in &result.drifted {
        let _ = writeln!(
            out,
            "hermes_config_drift{{file=\"{}\"}} 1",
            escape_label(file)
        );
    }
    let _ = writeln!(
        out,
        "# HELP hermes_config_drift_files Number of files the render plan would rewrite."
    );
    let _ = writeln!(out, "# TYPE hermes_config_drift_files gauge");
    let _ = writeln!(out, "hermes_config_drift_files {}", result.drifted.len());
    let _ = writeln!(
        out,
        "# HELP hermes_config_check_success 1 when the last drift check succeeded."
    );
    let _ = writeln!(out, "# TYPE hermes_config_check_success gauge");
    let _ = writeln!(
        out,
        "hermes_config_check_success {}",
        u8::from(result.success)
    );
    let _ = writeln!(
        out,
        "# HELP hermes_config_last_check_timestamp_seconds Unix time of the last drift check."
    );
    let _ = writeln!(
        out,
        "# TYPE hermes_config_last_check_timestamp_seconds gauge"
    );
    let _ = writeln!(
        out,
        "hermes_config_last_check_timestamp_seconds {}",
        result.timestamp_seconds
    );
    let _ = writeln!(
        out,
        "# HELP hermes_config_overlay_info Identifies the overlay inputs by digest."
    );
    let _ = writeln!(out, "# TYPE hermes_config_overlay_info gauge");
    let _ = writeln!(
        out,
        "hermes_config_overlay_info{{digest=\"{}\"}} 1",
        escape_label(&result.overlay_digest)
    );
    out
}

/// Appends every file below `directory` to `inputs`, labelled by its relative
/// path under `prefix`.
fn collect_files(directory: &Path, prefix: &str, inputs: &mut Vec<(String, PathBuf)>) {
    let Ok(entries) = fs::read_dir(directory) else {
        return;
    };
    let mut paths: Vec<PathBuf> = entries.flatten().map(|entry| entry.path()).collect();
    paths.sort();
    for path in paths {
        let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
            continue;
        };
        if path.is_dir() {
            collect_files(&path, &format!("{prefix}/{name}"), inputs);
        } else {
            inputs.push((format!("{prefix}/{name}"), path));
        }
    }
}

/// Returns the SHA-256 hex digest of the reviewed overlay inputs.
///
/// The digest covers the files baked into the `hermes-config` image (fragment,
/// context, identity, prompts and skills), not the private values Secret, which
/// changes independently of the image.
#[must_use]
pub fn overlay_digest(options: &Options) -> String {
    let mut inputs: Vec<(String, PathBuf)> = [
        ("config.fragment.yaml", &options.fragment),
        ("HERMES.md", &options.context),
        ("SOUL.md", &options.soul),
        ("voice_prompt.txt", &options.voice_prompt),
        ("telegram_prompt.txt", &options.telegram_prompt),
    ]
    .into_iter()
    .map(|(label, path)| (label.to_string(), PathBuf::from(path)))
    .collect();
    collect_files(Path::new(&options.skills), "skills", &mut inputs);
    inputs.sort_by(|(left, _), (right, _)| left.cmp(right));

    let mut hasher = Sha256::new();
    for (label, path) in inputs {
        hasher.update(label.as_bytes());
        hasher.update([0]);
        match fs::read(&path) {
            Ok(bytes) => {
                hasher.update(bytes.len().to_le_bytes());
                hasher.update(&bytes);
            }
            Err(_) => hasher.update(b"missing"),
        }
    }
    hex::encode(hasher.finalize())
}

/// Returns the path relative to `home`, with forward slashes.
fn relative_path(home: &str, path: &Path) -> String {
    path.strip_prefix(home)
        .unwrap_or(path)
        .to_string_lossy()
        .replace('\\', "/")
}

/// Builds one read-only plan and converts it into a metric result.
///
/// The second element carries the unredacted error text for the caller to log;
/// metrics never include it.
#[must_use]
pub fn check(options: &Options) -> (CheckResult, Option<String>) {
    let timestamp_seconds = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |duration| duration.as_secs());
    let overlay_digest = overlay_digest(options);
    match hermesconfig::build_plan(options) {
        Ok(plan) => (
            CheckResult {
                drifted: plan
                    .iter()
                    .map(|file| relative_path(&options.home, &file.path))
                    .collect(),
                success: true,
                timestamp_seconds,
                overlay_digest,
            },
            None,
        ),
        Err(error) => (
            CheckResult {
                drifted: Vec::new(),
                success: false,
                timestamp_seconds,
                overlay_digest,
            },
            Some(error),
        ),
    }
}

/// Locks the shared state, recovering from a poisoned mutex.
fn lock(state: &Mutex<CheckResult>) -> std::sync::MutexGuard<'_, CheckResult> {
    match state.lock() {
        Ok(guard) => guard,
        Err(poisoned) => poisoned.into_inner(),
    }
}

/// Answers one HTTP request; `/metrics` and `/healthz` are served, the rest is
/// a 404. The connection is closed after the response.
pub fn handle_connection(mut stream: TcpStream, state: &Mutex<CheckResult>) -> std::io::Result<()> {
    let mut reader = BufReader::new(stream.try_clone()?);
    let mut request_line = String::new();
    reader.read_line(&mut request_line)?;
    loop {
        let mut header = String::new();
        if reader.read_line(&mut header)? == 0 || header == "\r\n" || header == "\n" {
            break;
        }
    }
    let path = request_line
        .split_whitespace()
        .nth(1)
        .unwrap_or("/")
        .split('?')
        .next()
        .unwrap_or("/");
    let (status, content_type, body) = match path {
        "/metrics" => (
            "200 OK",
            "text/plain; version=0.0.4; charset=utf-8",
            render_metrics(&lock(state)),
        ),
        "/healthz" => ("200 OK", "text/plain; charset=utf-8", "ok\n".to_string()),
        _ => (
            "404 Not Found",
            "text/plain; charset=utf-8",
            "not found\n".to_string(),
        ),
    };
    write!(
        stream,
        "HTTP/1.1 {status}\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    )?;
    stream.flush()
}

/// Serves the metrics endpoint forever, one short-lived thread per connection.
fn serve(listener: &TcpListener, state: &Arc<Mutex<CheckResult>>) {
    for stream in listener.incoming().flatten() {
        let state = Arc::clone(state);
        std::thread::spawn(move || {
            let _ = handle_connection(stream, &state);
        });
    }
}

/// Runs the periodic check forever. The first check happens immediately.
pub fn run(
    options: &Options,
    listen: &str,
    interval: Duration,
    stderr: &mut dyn Write,
) -> Result<(), String> {
    if listen.is_empty() {
        return Err("hermes drift-exporter: --listen is required".to_string());
    }
    let listener = TcpListener::bind(listen)
        .map_err(|error| format!("hermes drift-exporter: bind {listen}: {error}"))?;
    let (result, error) = check(options);
    log_error(stderr, &error);
    let state = Arc::new(Mutex::new(result));
    let server_state = Arc::clone(&state);
    std::thread::spawn(move || serve(&listener, &server_state));

    let interval = if interval.is_zero() {
        Duration::from_secs(300)
    } else {
        interval
    };
    let mut next = Instant::now() + interval;
    loop {
        let now = Instant::now();
        if now < next {
            std::thread::sleep(next - now);
        }
        let (result, error) = check(options);
        log_error(stderr, &error);
        *lock(&state) = result;
        next += interval;
        if next <= Instant::now() {
            next = Instant::now() + interval;
        }
    }
}

fn log_error(stderr: &mut dyn Write, error: &Option<String>) {
    if let Some(error) = error {
        let _ = writeln!(
            stderr,
            "hermes drift-exporter: {}",
            hermesconfig::redact(error)
        );
    }
}

// `Read` is imported for tests that drain a response body.
#[allow(unused_imports)]
use std::io::Read as _;

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_result() -> CheckResult {
        CheckResult {
            drifted: vec![
                "config.yaml".to_string(),
                "skills/time/SKILL.md".to_string(),
            ],
            success: true,
            timestamp_seconds: 1_700_000_000,
            overlay_digest: "abc123".to_string(),
        }
    }

    #[test]
    fn metrics_include_every_documented_series() {
        let text = render_metrics(&sample_result());
        assert!(text.contains(
            "# TYPE hermes_config_drift gauge\nhermes_config_drift{file=\"config.yaml\"} 1\n"
        ));
        assert!(text.contains("hermes_config_drift{file=\"skills/time/SKILL.md\"} 1\n"));
        assert!(text.contains("hermes_config_drift_files 2\n"));
        assert!(text.contains("hermes_config_check_success 1\n"));
        assert!(text.contains("hermes_config_last_check_timestamp_seconds 1700000000\n"));
        assert!(text.contains("hermes_config_overlay_info{digest=\"abc123\"} 1\n"));
        assert!(text.contains("# HELP hermes_config_drift "));
    }

    #[test]
    fn failed_checks_report_zero_and_no_drift_series() {
        let mut result = sample_result();
        result.drifted.clear();
        result.success = false;
        let text = render_metrics(&result);
        assert!(!text.contains("hermes_config_drift{"));
        assert!(text.contains("hermes_config_drift_files 0\n"));
        assert!(text.contains("hermes_config_check_success 0\n"));
    }

    #[test]
    fn label_values_are_escaped() {
        let result = CheckResult {
            drifted: vec!["a\"b\\c\nd".to_string()],
            ..sample_result()
        };
        let text = render_metrics(&result);
        assert!(text.contains("hermes_config_drift{file=\"a\\\"b\\\\c\\nd\"} 1"));
    }

    fn round_trip(path: &str) -> String {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
        let address = listener.local_addr().expect("address");
        let state = Arc::new(Mutex::new(sample_result()));
        let server_state = Arc::clone(&state);
        let server = std::thread::spawn(move || {
            let (stream, _) = listener.accept().expect("accept");
            handle_connection(stream, &server_state).expect("response");
        });
        let mut client = TcpStream::connect(address).expect("connect");
        write!(client, "GET {path} HTTP/1.1\r\nHost: localhost\r\n\r\n").expect("request");
        let mut body = Vec::new();
        client.read_to_end(&mut body).expect("read");
        server.join().expect("server");
        String::from_utf8(body).expect("utf8")
    }

    #[test]
    fn serves_metrics_over_http() {
        let response = round_trip("/metrics");
        assert!(response.starts_with("HTTP/1.1 200 OK\r\n"));
        assert!(response.contains("hermes_config_drift{file=\"config.yaml\"} 1"));
        assert!(response.contains("hermes_config_overlay_info{digest=\"abc123\"} 1"));
    }

    #[test]
    fn serves_healthz_over_http() {
        let response = round_trip("/healthz");
        assert!(response.starts_with("HTTP/1.1 200 OK\r\n"));
        assert!(response.ends_with("ok\n"));
    }

    #[test]
    fn unknown_paths_are_not_found() {
        let response = round_trip("/nope");
        assert!(response.starts_with("HTTP/1.1 404 Not Found\r\n"));
    }

    #[test]
    fn digest_changes_with_the_overlay() {
        let directory = tempfile::tempdir().expect("tempdir");
        let fragment = directory.path().join("config.fragment.yaml");
        fs::write(&fragment, "agent: {}\n").expect("fragment");
        let mut options = Options {
            home: directory.path().display().to_string(),
            fragment: fragment.display().to_string(),
            context: directory.path().join("HERMES.md").display().to_string(),
            soul: directory.path().join("SOUL.md").display().to_string(),
            voice_prompt: directory.path().join("voice.txt").display().to_string(),
            telegram_prompt: directory.path().join("telegram.txt").display().to_string(),
            skills: directory.path().join("skills").display().to_string(),
            private_values: String::new(),
            allow_placeholders: true,
        };
        let first = overlay_digest(&options);
        fs::write(&fragment, "agent:\n  name: hermes\n").expect("fragment");
        let second = overlay_digest(&options);
        assert_ne!(first, second);
        fs::write(&fragment, "agent: {}\n").expect("fragment");
        assert_eq!(first, overlay_digest(&options));
        options.skills.clear();
        // An empty skills path is simply absent from the digest.
        let _ = overlay_digest(&options);
    }
}
