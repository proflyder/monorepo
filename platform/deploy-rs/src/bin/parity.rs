//! Parity runner for the frozen contract fixtures.
//!
//! Reads `testdata/frozen/manifest.json`, runs the Rust `home-deploy` binary on
//! every case marked `implemented` and compares stdout, stderr, the exit code
//! and every declared output file byte for byte with the values recorded from
//! the Go binary. Cases whose command is not ported yet are reported as skips
//! with the reason from the manifest, so the remaining work stays visible.
//!
//! Usage: `parity_runner <home-deploy binary> <frozen directory>`

use std::collections::HashMap;
use std::fs;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::Path;
use std::process::{Command, ExitCode};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread::JoinHandle;
use std::time::Duration;

use serde_json::Value;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().collect();
    let Some(binary) = args.get(1) else {
        eprintln!("usage: parity_runner <home-deploy> <frozen-dir>");
        return ExitCode::from(2);
    };
    let Some(root) = args.get(2) else {
        eprintln!("usage: parity_runner <home-deploy> <frozen-dir>");
        return ExitCode::from(2);
    };
    let binary = fs::canonicalize(binary).unwrap_or_else(|_| Path::new(binary).to_path_buf());
    match run(&binary, Path::new(root)) {
        Ok(true) => ExitCode::SUCCESS,
        Ok(false) => ExitCode::FAILURE,
        Err(error) => {
            eprintln!("parity: {error}");
            ExitCode::FAILURE
        }
    }
}

fn run(binary: &Path, root: &Path) -> Result<bool, String> {
    let manifest_text = fs::read_to_string(root.join("manifest.json"))
        .map_err(|error| format!("read manifest: {error}"))?;
    let manifest: Value =
        serde_json::from_str(&manifest_text).map_err(|error| format!("parse manifest: {error}"))?;
    let cases = manifest
        .get("cases")
        .and_then(Value::as_array)
        .ok_or_else(|| "manifest has no cases".to_string())?;

    let mut passed = 0usize;
    let mut skipped = 0usize;
    let mut failed = 0usize;
    let mut skipped_names = Vec::new();

    for case in cases {
        let name = string(case, "name")?;
        if string(case, "status")? != "implemented" {
            skipped += 1;
            let reason = case
                .get("reason")
                .and_then(Value::as_str)
                .unwrap_or("not ported");
            skipped_names.push(format!("{name} ({reason})"));
            continue;
        }
        if let Err(error) = run_case(binary, root, case) {
            failed += 1;
            println!("FAIL {name}: {error}");
        } else {
            passed += 1;
            println!("PASS {name}");
        }
    }

    if !skipped_names.is_empty() {
        println!("skipped {} case(s):", skipped_names.len());
        for name in &skipped_names {
            println!("  - {name}");
        }
    }
    println!("parity: {passed} passed, {failed} failed, {skipped} skipped");
    Ok(failed == 0)
}

fn run_case(binary: &Path, root: &Path, case: &Value) -> Result<(), String> {
    let source = case
        .get("cwd")
        .and_then(Value::as_str)
        .map_or_else(|| root.to_path_buf(), |relative| root.join(relative));
    let name = string(case, "name")?.replace(['/', '\\'], "_");
    let cwd =
        std::env::temp_dir().join(format!("home-deploy-parity-{}-{name}", std::process::id()));
    if cwd.exists() {
        fs::remove_dir_all(&cwd).map_err(|error| format!("clean {name}: {error}"))?;
    }
    copy_dir(&source, &cwd)?;
    let argv: Vec<String> = case
        .get("argv")
        .and_then(Value::as_array)
        .ok_or_else(|| "case has no argv".to_string())?
        .iter()
        .map(|value| value.as_str().unwrap_or_default().to_string())
        .collect();

    let _server = match local_port(&argv) {
        Some(port) if cwd.join("http.json").is_file() => Some(start_server(&cwd, port)?),
        _ => None,
    };

    let mut command = Command::new(binary);
    command.args(&argv).current_dir(&cwd);
    if let Some(env) = case.get("env").and_then(Value::as_object) {
        if let Some(prefix) = env.get("PATH_PREFIX").and_then(Value::as_str) {
            let current = std::env::var("PATH").unwrap_or_default();
            command.env(
                "PATH",
                format!("{}:{}", cwd.join(prefix).display(), current),
            );
        }
        for (key, value) in env {
            if key == "PATH_PREFIX" {
                continue;
            }
            if let Some(value) = value.as_str() {
                command.env(key, value);
            }
        }
    }

    let output = command
        .output()
        .map_err(|error| format!("spawn: {error}"))?;
    let expected_exit = case.get("exit").and_then(Value::as_i64).unwrap_or(0);
    let actual_exit = i64::from(output.status.code().unwrap_or(-1));
    if actual_exit != expected_exit {
        return Err(format!(
            "exit {actual_exit}, expected {expected_exit}; stderr: {}",
            String::from_utf8_lossy(&output.stderr)
        ));
    }
    compare_bytes(&cwd, case, "stdout", &normalize_smoke(&output.stdout))?;
    compare_bytes(&cwd, case, "stderr", &normalize_smoke(&output.stderr))?;

    if let Some(outputs) = case.get("outputs").and_then(Value::as_array) {
        for item in outputs {
            let path = string(item, "path")?;
            let golden = string(item, "golden")?;
            let actual =
                fs::read(cwd.join(&path)).map_err(|error| format!("read {path}: {error}"))?;
            let expected = fs::read(cwd.join(&golden))
                .map_err(|error| format!("read golden {golden}: {error}"))?;
            if normalize_smoke(&actual) != expected {
                return Err(format!("{path} differs from {golden}"));
            }
        }
    }
    let _ = fs::remove_dir_all(&cwd);
    Ok(())
}

/// Replaces the clock-derived smoke Job name with the fixture placeholder.
fn normalize_smoke(bytes: &[u8]) -> Vec<u8> {
    const MARKER: &[u8] = b"hermes-smoke-";
    let mut output = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index..].starts_with(MARKER) {
            output.extend_from_slice(MARKER);
            index += MARKER.len();
            let start = index;
            while index < bytes.len() && bytes[index].is_ascii_digit() {
                index += 1;
            }
            if index > start {
                output.extend_from_slice(b"<id>");
                continue;
            }
            continue;
        }
        output.push(bytes[index]);
        index += 1;
    }
    output
}

/// Returns the local port of a preflight URL in argv.
fn local_port(argv: &[String]) -> Option<u16> {
    const PREFIX: &str = "http://127.0.0.1:";
    for argument in argv {
        let Some(position) = argument.find(PREFIX) else {
            continue;
        };
        let rest = &argument[position + PREFIX.len()..];
        let digits: String = rest.chars().take_while(char::is_ascii_digit).collect();
        if let Ok(port) = digits.parse() {
            return Some(port);
        }
    }
    None
}

/// Serves the case's `http.json` map while the command runs.
struct Server {
    stop: Arc<AtomicBool>,
    handle: Option<JoinHandle<()>>,
}

impl Drop for Server {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(handle) = self.handle.take() {
            let _ = handle.join();
        }
    }
}

fn start_server(cwd: &Path, port: u16) -> Result<Server, String> {
    let text = fs::read_to_string(cwd.join("http.json"))
        .map_err(|error| format!("read http.json: {error}"))?;
    let parsed: Value =
        serde_json::from_str(&text).map_err(|error| format!("parse http.json: {error}"))?;
    let mut routes: HashMap<String, (u16, String)> = HashMap::new();
    if let Some(entries) = parsed.as_object() {
        for (path, value) in entries {
            let Some(pair) = value.as_array() else {
                continue;
            };
            let status = pair.first().and_then(Value::as_u64).unwrap_or(500) as u16;
            let body = pair
                .get(1)
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string();
            routes.insert(path.clone(), (status, body));
        }
    }
    let listener = TcpListener::bind(("127.0.0.1", port))
        .map_err(|error| format!("bind preflight server: {error}"))?;
    listener
        .set_nonblocking(true)
        .map_err(|error| format!("preflight server: {error}"))?;
    let stop = Arc::new(AtomicBool::new(false));
    let thread_stop = Arc::clone(&stop);
    let handle = std::thread::spawn(move || {
        while !thread_stop.load(Ordering::Relaxed) {
            match listener.accept() {
                Ok((stream, _)) => serve_connection(stream, &routes),
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                    std::thread::sleep(Duration::from_millis(1));
                }
                Err(_) => break,
            }
        }
    });
    Ok(Server {
        stop,
        handle: Some(handle),
    })
}

fn serve_connection(mut stream: TcpStream, routes: &HashMap<String, (u16, String)>) {
    // Accepted sockets can inherit non-blocking mode from the listener on
    // macOS; reads must block until the request headers arrive.
    let _ = stream.set_nonblocking(false);
    let _ = stream.set_read_timeout(Some(Duration::from_secs(5)));
    let mut request = Vec::new();
    let mut buffer = [0u8; 1024];
    while !request.windows(4).any(|window| window == b"\r\n\r\n") {
        match stream.read(&mut buffer) {
            Ok(0) => break,
            Ok(count) => request.extend_from_slice(&buffer[..count]),
            Err(error) if error.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(_) => break,
        }
        if request.len() > 16 * 1024 {
            break;
        }
    }
    let text = String::from_utf8_lossy(&request);
    let path = text
        .lines()
        .next()
        .and_then(|line| line.split_whitespace().nth(1))
        .unwrap_or("/");
    let path = path.split('?').next().unwrap_or(path);
    let (status, body) = routes.get(path).cloned().unwrap_or((404, String::new()));
    let response = format!(
        "HTTP/1.1 {status} OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );
    let _ = stream.write_all(response.as_bytes());
}

/// Recursively copies a fixture case so a test run never writes into the
/// checkout (render outputs and fake command traces are generated files).
fn copy_dir(source: &Path, target: &Path) -> Result<(), String> {
    fs::create_dir_all(target).map_err(|error| format!("create {}: {error}", target.display()))?;
    let entries =
        fs::read_dir(source).map_err(|error| format!("read {}: {error}", source.display()))?;
    for entry in entries {
        let entry = entry.map_err(|error| format!("read entry: {error}"))?;
        let path = entry.path();
        let destination = target.join(entry.file_name());
        if path.is_dir() {
            copy_dir(&path, &destination)?;
        } else {
            fs::copy(&path, &destination)
                .map_err(|error| format!("copy {}: {error}", path.display()))?;
        }
    }
    Ok(())
}

fn compare_bytes(cwd: &Path, case: &Value, field: &str, actual: &[u8]) -> Result<(), String> {
    let Some(relative) = case.get(field).and_then(Value::as_str) else {
        return Ok(());
    };
    let expected =
        fs::read(cwd.join(relative)).map_err(|error| format!("read {relative}: {error}"))?;
    if actual != expected {
        return Err(format!(
            "{field} differs:\n--- expected ---\n{}\n--- actual ---\n{}",
            String::from_utf8_lossy(&expected),
            String::from_utf8_lossy(actual)
        ));
    }
    Ok(())
}

fn string(case: &Value, field: &str) -> Result<String, String> {
    case.get(field)
        .and_then(Value::as_str)
        .map(str::to_string)
        .ok_or_else(|| format!("case has no {field}"))
}
