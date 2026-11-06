//! Rust runner for the shared golden HTTP fixtures.
//!
//! The normative format is `packages/go/contracttest/README.md`; this crate
//! replays the same `testdata/contract/*.json` against an [`axum::Router`]
//! through [`tower::ServiceExt::oneshot`]. There is deliberately no `-update`:
//! the Rust side only verifies committed fixtures.
//!
//! Supported: `capture`, `captureHeaders`, `{{name}}` substitution,
//! `<uuid>`, `<base64-32>` and `<any>` placeholders, structural JSON
//! comparison and byte-for-byte string bodies.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use axum::Router;
use axum::body::Body;
use base64::Engine as _;
use base64::alphabet;
use base64::engine::{DecodePaddingMode, GeneralPurpose, GeneralPurposeConfig};
use http::{HeaderName, HeaderValue, Method, Request, StatusCode, header};
use http_body_util::BodyExt as _;
use serde_json::{Map, Value};
use thiserror::Error;
use tower::ServiceExt as _;

const BASE64: GeneralPurpose = GeneralPurpose::new(
    &alphabet::STANDARD,
    GeneralPurposeConfig::new()
        .with_decode_padding_mode(DecodePaddingMode::RequireCanonical)
        .with_decode_allow_trailing_bits(true),
);

/// Fixture failure with the scenario and step that produced it.
#[derive(Debug, Error)]
#[error("{scenario} step {step}: {detail}")]
pub struct Error {
    pub scenario: String,
    pub step: usize,
    pub detail: String,
}

impl Error {
    fn new(scenario: &str, step: usize, detail: impl Into<String>) -> Self {
        Self {
            scenario: scenario.to_owned(),
            step,
            detail: detail.into(),
        }
    }
}

/// Optional hooks. The factory passed to [`run`] is called once per scenario
/// and is the place to seed state; `before_step` runs before every request
/// with the scenario name and the 1-based step number, exactly like Go's
/// `contracttest.WithBeforeStep` (for example to advance a fake clock).
type BeforeStep = Box<dyn Fn(&str, usize)>;
type Normalizer = Box<dyn Fn(&str, &mut Value)>;

#[derive(Default)]
pub struct Options {
    before_step: Option<BeforeStep>,
    normalizer: Option<Normalizer>,
}

impl Options {
    #[must_use]
    pub fn with_before_step(mut self, hook: impl Fn(&str, usize) + 'static) -> Self {
        self.before_step = Some(Box::new(hook));
        self
    }

    /// Rewrite random values in the actual response into placeholders before
    /// it is compared. The scenario name is provided.
    #[must_use]
    pub fn with_normalizer(mut self, hook: impl Fn(&str, &mut Value) + 'static) -> Self {
        self.normalizer = Some(Box::new(hook));
        self
    }
}

/// Replays every `*.json` fixture in `dir` and returns the number of
/// scenarios that passed.
pub async fn run<F>(dir: impl AsRef<Path>, new_router: F, options: Options) -> Result<usize, Error>
where
    F: Fn(&str) -> Router,
{
    let dir = locate(dir.as_ref());
    let mut files: Vec<PathBuf> = std::fs::read_dir(&dir)
        .map_err(|error| Error::new("<dir>", 0, format!("read {}: {error}", dir.display())))?
        .filter_map(|entry| entry.ok().map(|entry| entry.path()))
        .filter(|path| {
            path.extension()
                .is_some_and(|extension| extension == "json")
        })
        .collect();
    files.sort();
    if files.is_empty() {
        return Err(Error::new(
            "<dir>",
            0,
            format!("no *.json fixtures in {}", dir.display()),
        ));
    }

    let mut scenarios = 0;
    for file in files {
        let bytes = std::fs::read(&file)
            .map_err(|error| Error::new("<dir>", 0, format!("read {}: {error}", file.display())))?;
        let fixture: Value = serde_json::from_slice(&bytes)
            .map_err(|error| Error::new("<fixture>", 0, format!("{}: {error}", file.display())))?;
        let name = fixture
            .get("name")
            .and_then(Value::as_str)
            .ok_or_else(|| Error::new("<fixture>", 0, "fixture has no name"))?
            .to_owned();
        let steps = fixture
            .get("steps")
            .and_then(Value::as_array)
            .ok_or_else(|| Error::new(&name, 0, "fixture has no steps"))?;
        let router = new_router(&name);
        let compare_mode = fixture
            .get("compare")
            .and_then(Value::as_str)
            .unwrap_or("exact")
            .to_owned();
        validate_compare(&compare_mode).map_err(|detail| Error::new(&name, 0, detail))?;
        let mut captures: BTreeMap<String, String> = BTreeMap::new();

        for (index, step) in steps.iter().enumerate() {
            let number = index + 1;
            if let Some(hook) = &options.before_step {
                hook(&name, number);
            }
            replay_step(
                &name,
                number,
                step,
                router.clone(),
                &mut captures,
                &options,
                &compare_mode,
            )
            .await?;
        }
        scenarios += 1;
    }
    Ok(scenarios)
}

async fn replay_step(
    scenario: &str,
    number: usize,
    step: &Value,
    router: Router,
    captures: &mut BTreeMap<String, String>,
    options: &Options,
    file_compare: &str,
) -> Result<(), Error> {
    let request = step
        .get("request")
        .ok_or_else(|| Error::new(scenario, number, "step has no request"))?;
    let method = request
        .get("method")
        .and_then(Value::as_str)
        .ok_or_else(|| Error::new(scenario, number, "request has no method"))?;
    let path = request
        .get("path")
        .and_then(Value::as_str)
        .ok_or_else(|| Error::new(scenario, number, "request has no path"))?;
    let path = substitute_text(path, captures, scenario, number)?;
    let method = Method::from_bytes(method.as_bytes())
        .map_err(|_| Error::new(scenario, number, format!("invalid method {method:?}")))?;

    let mut builder = Request::builder().method(method).uri(path);
    if let Some(headers) = request.get("headers").and_then(Value::as_object) {
        for (name, value) in headers {
            let value = value.as_str().ok_or_else(|| {
                Error::new(scenario, number, format!("header {name} is not a string"))
            })?;
            let value = substitute_text(value, captures, scenario, number)?;
            builder = builder.header(
                HeaderName::from_bytes(name.as_bytes()).map_err(|_| {
                    Error::new(scenario, number, format!("invalid header name {name:?}"))
                })?,
                HeaderValue::from_str(&value).map_err(|_| {
                    Error::new(scenario, number, format!("invalid header value for {name}"))
                })?,
            );
        }
    }

    let body = match request.get("body") {
        None | Some(Value::Null) => Vec::new(),
        Some(Value::String(text)) => {
            substitute_text(text, captures, scenario, number)?.into_bytes()
        }
        Some(value) => {
            let mut value = value.clone();
            substitute_value(&mut value, captures, scenario, number)?;
            serde_json::to_vec(&value)
                .map_err(|error| Error::new(scenario, number, format!("request body: {error}")))?
        }
    };
    let response = router
        .oneshot(
            builder
                .body(Body::from(body))
                .map_err(|error| Error::new(scenario, number, format!("request: {error}")))?,
        )
        .await
        .map_err(|error| Error::new(scenario, number, format!("service: {error}")))?;

    let (parts, body) = response.into_parts();
    let bytes = body
        .collect()
        .await
        .map_err(|error| Error::new(scenario, number, format!("response body: {error}")))?
        .to_bytes();

    let expected = step
        .get("response")
        .ok_or_else(|| Error::new(scenario, number, "step has no response"))?;
    let expected_status = expected
        .get("status")
        .and_then(Value::as_u64)
        .ok_or_else(|| Error::new(scenario, number, "response has no status"))?;
    if parts.status != StatusCode::from_u16(expected_status as u16).expect("valid status") {
        return Err(Error::new(
            scenario,
            number,
            format!(
                "status {}, want {expected_status}; body {}",
                parts.status.as_u16(),
                String::from_utf8_lossy(&bytes)
            ),
        ));
    }

    if let Some(headers) = expected.get("headers").and_then(Value::as_object) {
        for (name, value) in headers {
            let value = value.as_str().ok_or_else(|| {
                Error::new(scenario, number, format!("header {name} is not a string"))
            })?;
            let actual = parts
                .headers
                .get(name)
                .and_then(|value| value.to_str().ok())
                .ok_or_else(|| {
                    Error::new(scenario, number, format!("missing response header {name}"))
                })?;
            if actual != value {
                return Err(Error::new(
                    scenario,
                    number,
                    format!("header {name} = {actual:?}, want {value:?}"),
                ));
            }
        }
    }

    let format = expected.get("format").and_then(Value::as_str).unwrap_or("");
    let payload: &[u8] = if format == "sse" {
        sse_payload(&bytes).map_err(|detail| Error::new(scenario, number, detail))?
    } else {
        &bytes
    };
    let compare_mode = step
        .get("compare")
        .and_then(Value::as_str)
        .unwrap_or(file_compare);
    validate_compare(compare_mode).map_err(|detail| Error::new(scenario, number, detail))?;
    let semantic = compare_mode == "semantic";

    let parsed = serde_json::from_slice::<Value>(payload).ok();
    if let Some(capture) = step.get("capture").and_then(Value::as_object) {
        let parsed = parsed
            .as_ref()
            .ok_or_else(|| Error::new(scenario, number, "capture needs a JSON response body"))?;
        for (name, path) in capture {
            let path = path.as_str().ok_or_else(|| {
                Error::new(scenario, number, format!("capture {name} has no path"))
            })?;
            let value = lookup(parsed, path).ok_or_else(|| {
                Error::new(
                    scenario,
                    number,
                    format!("capture {name}: {path} is absent from the response"),
                )
            })?;
            captures.insert(name.clone(), scalar_text(value));
        }
    }
    if let Some(capture) = step.get("captureHeaders").and_then(Value::as_object) {
        for (name, header_name) in capture {
            let header_name = header_name.as_str().ok_or_else(|| {
                Error::new(
                    scenario,
                    number,
                    format!("captureHeaders {name} is not a name"),
                )
            })?;
            let value = parts
                .headers
                .get(header_name)
                .and_then(|value| value.to_str().ok())
                .ok_or_else(|| {
                    Error::new(
                        scenario,
                        number,
                        format!("captureHeaders {name}: {header_name} is absent"),
                    )
                })?;
            captures.insert(name.clone(), value.to_owned());
        }
    }

    let Some(expected_body) = expected.get("body") else {
        return Ok(());
    };
    match expected_body {
        Value::String(text) => {
            let text = substitute_text(text, captures, scenario, number)?;
            if payload != text.as_bytes() {
                return Err(Error::new(
                    scenario,
                    number,
                    format!(
                        "body {:?}, want {:?}",
                        String::from_utf8_lossy(payload),
                        text
                    ),
                ));
            }
        }
        expected => {
            let mut actual = parsed.ok_or_else(|| {
                Error::new(
                    scenario,
                    number,
                    format!(
                        "response is not JSON: {:?}",
                        String::from_utf8_lossy(payload)
                    ),
                )
            })?;
            if let Some(normalizer) = &options.normalizer {
                normalizer(scenario, &mut actual);
            }
            let mut expected = expected.clone();
            substitute_value(&mut expected, captures, scenario, number)?;
            compare(&expected, &actual, "", scenario, number, semantic)?;
        }
    }
    Ok(())
}

const SSE_PREFIX: &str = "event: message\ndata: ";
const SSE_SUFFIX: &str = "\n\n";

fn sse_payload(bytes: &[u8]) -> Result<&[u8], String> {
    let text = std::str::from_utf8(bytes).map_err(|error| format!("SSE body: {error}"))?;
    let inner = text
        .strip_prefix(SSE_PREFIX)
        .and_then(|rest| rest.strip_suffix(SSE_SUFFIX))
        .ok_or_else(|| {
            format!("expected an SSE frame {SSE_PREFIX:?}…{SSE_SUFFIX:?}, got {text:?}")
        })?;
    Ok(inner.as_bytes())
}

fn validate_compare(mode: &str) -> Result<(), String> {
    match mode {
        "exact" | "semantic" => Ok(()),
        other => Err(format!("unknown compare mode {other:?}")),
    }
}

fn substitute_text(
    text: &str,
    captures: &BTreeMap<String, String>,
    scenario: &str,
    step: usize,
) -> Result<String, Error> {
    let mut result = text.to_owned();
    while let Some(start) = result.find("{{") {
        let Some(end) = result[start + 2..].find("}}") else {
            return Err(Error::new(scenario, step, "unterminated {{placeholder}}"));
        };
        let name = &result[start + 2..start + 2 + end];
        let value = captures
            .get(name)
            .ok_or_else(|| Error::new(scenario, step, format!("unknown capture {name:?}")))?;
        result.replace_range(start..start + 2 + end + 2, value);
    }
    Ok(result)
}

fn substitute_value(
    value: &mut Value,
    captures: &BTreeMap<String, String>,
    scenario: &str,
    step: usize,
) -> Result<(), Error> {
    match value {
        Value::String(text) => *text = substitute_text(text, captures, scenario, step)?,
        Value::Array(items) => {
            for item in items {
                substitute_value(item, captures, scenario, step)?;
            }
        }
        Value::Object(map) => {
            for item in map.values_mut() {
                substitute_value(item, captures, scenario, step)?;
            }
        }
        _ => {}
    }
    Ok(())
}

fn compare(
    expected: &Value,
    actual: &Value,
    path: &str,
    scenario: &str,
    step: usize,
    semantic: bool,
) -> Result<(), Error> {
    match expected {
        Value::String(text) if text == "<uuid>" => {
            if actual.as_str().is_some_and(is_uuid) {
                Ok(())
            } else {
                Err(Error::new(
                    scenario,
                    step,
                    format!("{}: {actual} is not a canonical UUID", display_path(path)),
                ))
            }
        }
        Value::String(text) if text == "<base64-32>" => {
            let valid = actual
                .as_str()
                .and_then(|text| BASE64.decode(text).ok())
                .is_some_and(|bytes| bytes.len() == 32);
            if valid {
                Ok(())
            } else {
                Err(Error::new(
                    scenario,
                    step,
                    format!("{}: {actual} is not base64 of 32 bytes", display_path(path)),
                ))
            }
        }
        Value::String(text) if text == "<any>" => {
            if is_non_empty(actual) {
                Ok(())
            } else {
                Err(Error::new(
                    scenario,
                    step,
                    format!("{}: value is empty", display_path(path)),
                ))
            }
        }
        Value::String(text) => match actual {
            Value::String(actual) if actual == text => Ok(()),
            _ => Err(Error::new(
                scenario,
                step,
                format!("{}: {actual}, want {text:?}", display_path(path)),
            )),
        },
        Value::Number(number) => {
            if actual.as_number() == Some(number) {
                Ok(())
            } else {
                Err(Error::new(
                    scenario,
                    step,
                    format!("{}: {actual}, want {number}", display_path(path)),
                ))
            }
        }
        Value::Bool(flag) => {
            if actual == &Value::Bool(*flag) {
                Ok(())
            } else {
                Err(Error::new(
                    scenario,
                    step,
                    format!("{}: {actual}, want {flag}", display_path(path)),
                ))
            }
        }
        Value::Null => {
            if actual.is_null() {
                Ok(())
            } else {
                Err(Error::new(
                    scenario,
                    step,
                    format!("{}: {actual}, want null", display_path(path)),
                ))
            }
        }
        Value::Array(expected_items) => {
            let Some(actual_items) = actual.as_array() else {
                return Err(Error::new(
                    scenario,
                    step,
                    format!("{}: {actual}, want an array", display_path(path)),
                ));
            };
            if expected_items.len() != actual_items.len() {
                return Err(Error::new(
                    scenario,
                    step,
                    format!(
                        "{}: array has {} items, want {}",
                        display_path(path),
                        actual_items.len(),
                        expected_items.len()
                    ),
                ));
            }
            for (index, (expected, actual)) in expected_items.iter().zip(actual_items).enumerate() {
                compare(
                    expected,
                    actual,
                    &format!("{path}[{index}]"),
                    scenario,
                    step,
                    semantic,
                )?;
            }
            Ok(())
        }
        Value::Object(expected_map) => {
            let Some(actual_map) = actual.as_object() else {
                return Err(Error::new(
                    scenario,
                    step,
                    format!("{}: {actual}, want an object", display_path(path)),
                ));
            };
            compare_objects(expected_map, actual_map, path, scenario, step, semantic)
        }
    }
}

fn compare_objects(
    expected: &Map<String, Value>,
    actual: &Map<String, Value>,
    path: &str,
    scenario: &str,
    step: usize,
    semantic: bool,
) -> Result<(), Error> {
    for key in expected.keys() {
        if !actual.contains_key(key) {
            return Err(Error::new(
                scenario,
                step,
                format!("{}: missing key {key:?}", display_path(path)),
            ));
        }
    }
    if !semantic {
        for key in actual.keys() {
            if !expected.contains_key(key) {
                return Err(Error::new(
                    scenario,
                    step,
                    format!("{}: unexpected key {key:?}", display_path(path)),
                ));
            }
        }
    }
    for (key, expected_value) in expected {
        let actual_value = &actual[key];
        compare(
            expected_value,
            actual_value,
            &format!("{path}.{key}"),
            scenario,
            step,
            semantic,
        )?;
    }
    Ok(())
}

fn display_path(path: &str) -> &str {
    if path.is_empty() { "<root>" } else { path }
}

fn is_non_empty(value: &Value) -> bool {
    match value {
        Value::Null => false,
        Value::String(text) => !text.is_empty(),
        Value::Array(items) => !items.is_empty(),
        Value::Object(map) => !map.is_empty(),
        Value::Bool(_) | Value::Number(_) => true,
    }
}

fn is_uuid(text: &str) -> bool {
    let bytes = text.as_bytes();
    bytes.len() == 36
        && bytes.iter().enumerate().all(|(index, byte)| {
            if matches!(index, 8 | 13 | 18 | 23) {
                *byte == b'-'
            } else {
                byte.is_ascii_hexdigit()
            }
        })
}

fn lookup<'a>(value: &'a Value, path: &str) -> Option<&'a Value> {
    let mut current = value;
    for segment in path.split('.') {
        let (name, indexes) = split_indexes(segment);
        if !name.is_empty() {
            current = current.get(name)?;
        }
        for index in indexes {
            current = current.get(index)?;
        }
    }
    Some(current)
}

fn split_indexes(segment: &str) -> (&str, Vec<usize>) {
    let mut indexes = Vec::new();
    let mut name = segment;
    while let Some(start) = name.find('[') {
        let Some(end) = name[start..].find(']') else {
            break;
        };
        let index = &name[start + 1..start + end];
        if let Ok(index) = index.parse() {
            indexes.push(index);
        }
        name = &name[..start];
    }
    (name, indexes)
}

fn scalar_text(value: &Value) -> String {
    match value {
        Value::String(text) => text.clone(),
        other => other.to_string(),
    }
}

fn locate(dir: &Path) -> PathBuf {
    if dir.is_absolute() || dir.is_dir() {
        return dir.to_owned();
    }
    for root in ["TEST_SRCDIR", "RUNFILES_DIR", "GULP_SRCDIR"] {
        if let Ok(root) = std::env::var(root) {
            let workspace = std::env::var("TEST_WORKSPACE").unwrap_or_default();
            let candidate = Path::new(&root).join(&workspace).join(dir);
            if candidate.is_dir() {
                return candidate;
            }
            let candidate = Path::new(&root).join(dir);
            if candidate.is_dir() {
                return candidate;
            }
        }
    }
    dir.to_owned()
}

/// Convenience for tests: the two header helpers used by fixtures.
#[must_use]
pub fn content_type() -> (&'static str, &'static str) {
    (header::CONTENT_TYPE.as_str(), "application/json")
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::Json;
    use axum::routing::{get, post};
    use serde_json::json;

    fn router() -> Router {
        Router::new()
            .route("/healthz", get(|| async { "ok\n" }))
            .route(
                "/items",
                post(|Json(body): Json<Value>| async move {
                    let id = body.get("name").and_then(Value::as_str).unwrap_or("item");
                    (
                        StatusCode::CREATED,
                        Json(json!({"id": id, "random": "00000000-0000-4000-8000-000000000001"})),
                    )
                }),
            )
    }

    fn temp_dir() -> tempfile::TempDir {
        tempfile::tempdir().expect("tempdir")
    }

    #[tokio::test]
    async fn semantic_compare_ignores_extra_fields_and_keeps_arrays_exact() {
        let dir = temp_dir();
        std::fs::write(
            dir.path().join("semantic.json"),
            serde_json::to_vec_pretty(&json!({
                "name": "semantic",
                "compare": "semantic",
                "steps": [{
                    "request": {"method": "POST", "path": "/mcp"},
                    "response": {
                        "status": 200,
                        "format": "sse",
                        "headers": {"Content-Type": "text/event-stream"},
                        "body": {"result": {"name": "mcp-gateway", "tools": [{"name": "infra_status"}]}}
                    }
                }]
            }))
            .expect("fixture"),
        )
        .expect("write");
        let count = run(
            dir.path(),
            |_| {
                Router::new().route(
                    "/mcp",
                    post(|| async {
                        (
                            [(header::CONTENT_TYPE.as_str(), "text/event-stream")],
                            "event: message\ndata: {\"result\":{\"name\":\"mcp-gateway\",\"ttlMs\":0,\"tools\":[{\"name\":\"infra_status\",\"inputSchema\":{\"type\":\"object\"}}]}}\n\n",
                        )
                    }),
                )
            },
            Options::default(),
        )
        .await
        .expect("fixtures");
        assert_eq!(count, 1);
    }

    #[test]
    fn sse_payload_requires_the_exact_envelope() {
        let payload = sse_payload(b"event: message\ndata: {\"id\":1}\n\n").expect("payload");
        assert_eq!(payload, b"{\"id\":1}");
        for broken in [
            &b"{\"id\":1}"[..],
            b"event: message\ndata: {}\n",
            b"event: ping\ndata: {}\n\n",
        ] {
            assert!(sse_payload(broken).is_err(), "{broken:?}");
        }
    }

    #[test]
    fn semantic_compare_rejects_missing_keys_and_changed_values() {
        let expected =
            json!({"result": {"name": "mcp-gateway", "tools": [{"name": "infra_status"}]}});
        let actual = json!({"result": {"name": "mcp-gateway", "tools": [{"name": "infra_status"}], "ttlMs": 0}});
        compare(&expected, &actual, "", "s", 1, true).expect("semantic");
        assert!(compare(&expected, &actual, "", "s", 1, false).is_err());
        let changed = json!({"result": {"name": "other", "tools": [{"name": "infra_status"}]}});
        assert!(compare(&expected, &changed, "", "s", 1, true).is_err());
        let shorter = json!({"result": {"name": "mcp-gateway", "tools": []}});
        assert!(compare(&expected, &shorter, "", "s", 1, true).is_err());
    }

    #[tokio::test]
    async fn replays_capture_placeholders_and_structure() {
        let dir = temp_dir();
        std::fs::write(
            dir.path().join("create.json"),
            serde_json::to_vec_pretty(&json!({
                "name": "create",
                "steps": [
                    {
                        "request": {
                            "method": "POST",
                            "path": "/items",
                            "headers": {"Content-Type": "application/json"},
                            "body": {"name": "wake"}
                        },
                        "capture": {"itemId": "id"},
                        "response": {
                            "status": 201,
                            "headers": {"Content-Type": "application/json"},
                            "body": {"id": "{{itemId}}", "random": "<uuid>"}
                        }
                    },
                    {
                        "request": {"method": "GET", "path": "/healthz"},
                        "response": {"status": 200, "body": "ok\n"}
                    }
                ]
            }))
            .expect("fixture"),
        )
        .expect("write");

        let count = run(dir.path(), |_| router(), Options::default())
            .await
            .expect("fixtures");
        assert_eq!(count, 1);
    }

    #[tokio::test]
    async fn the_before_step_hook_gets_the_scenario_and_one_based_step() {
        let dir = temp_dir();
        std::fs::write(
            dir.path().join("hooks.json"),
            br#"{
                "name": "hooks",
                "steps": [
                    {"request": {"method": "GET", "path": "/healthz"}, "response": {"status": 200}},
                    {"request": {"method": "GET", "path": "/healthz"}, "response": {"status": 200}}
                ]
            }"#,
        )
        .expect("write");
        let seen = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
        let recorder = seen.clone();
        let count = run(
            dir.path(),
            |_| router(),
            Options::default().with_before_step(move |scenario, step| {
                recorder
                    .lock()
                    .expect("lock")
                    .push((scenario.to_owned(), step));
            }),
        )
        .await
        .expect("fixtures");
        assert_eq!(count, 1);
        assert_eq!(
            *seen.lock().expect("lock"),
            vec![("hooks".to_owned(), 1), ("hooks".to_owned(), 2)]
        );
    }

    #[tokio::test]
    async fn reports_a_mismatch_with_step_context() {
        let dir = temp_dir();
        std::fs::write(
            dir.path().join("bad.json"),
            br#"{
                "name": "bad",
                "steps": [{
                    "request": {"method": "GET", "path": "/healthz"},
                    "response": {"status": 201, "body": "ok\n"}
                }]
            }"#,
        )
        .expect("write");
        let error = run(dir.path(), |_| router(), Options::default())
            .await
            .expect_err("mismatch");
        assert!(error.to_string().starts_with("bad step 1:"), "{error}");
    }

    #[test]
    fn lookups_and_uuid() {
        let value = json!({"a": {"b": [{"id": "x"}]}});
        assert_eq!(
            lookup(&value, "a.b[0].id").and_then(Value::as_str),
            Some("x")
        );
        assert!(is_uuid("00000000-0000-4000-8000-000000000001"));
        assert!(!is_uuid("00000000-0000-4000-8000-00000000000"));
    }
}
