//! Typed external catalog probes (`tcp`, `http`, `ha_state`, `ha_template`,
//! `ha_unavailable_summary`), ported from `internal/checks/checks.go`.
//!
//! A probe is bounded by [`DEFAULT_TIMEOUT`], never keeps a response body and
//! returns the fixed [`CheckResult`] shape the report and status details use.

use std::collections::BTreeSet;
use std::time::Duration;

use serde::Deserialize;

use crate::domain::catalog::{self, Check};
use crate::domain::status::CheckResult;

/// Bounds one external check.
pub const DEFAULT_TIMEOUT: Duration = Duration::from_secs(3);
const MAX_BODY: usize = 4 << 20;

/// Executes checks against the configured Home Assistant endpoint.
#[derive(Debug, Clone)]
pub struct Runner {
    base_url: String,
    token: String,
    http: reqwest::Client,
    timeout: Duration,
}

/// Build the shared HTTP client with the workspace rustls provider.
///
/// The workspace `reqwest` is compiled with manual roots, so an `https://`
/// Home Assistant or external endpoint verifies only when the operating
/// system's bundle is added explicitly (`home_tls`). Without this a probe that
/// looks healthy over HTTP would fail with `request failed` on TLS.
fn client(timeout: Duration) -> Result<reqwest::Client, String> {
    home_tls::public_https_client()
        .map_err(|error| format!("http TLS roots: {error}"))?
        .timeout(timeout)
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .map_err(|error| format!("http client: {error}"))
}

impl Runner {
    /// Build a runner. An empty base URL or token disables `ha_*` checks; they
    /// are reported as skipped rather than failed.
    pub fn new(base_url: &str, token: &str) -> Result<Self, String> {
        let timeout = DEFAULT_TIMEOUT;
        Ok(Self {
            base_url: base_url.trim().trim_end_matches('/').to_owned(),
            token: token.trim().to_owned(),
            http: client(timeout)?,
            timeout,
        })
    }

    /// Runs every declared check of every service and returns results in
    /// catalog order.
    pub async fn run_all(&self, services: &[catalog::Service]) -> Vec<CheckResult> {
        let mut results = Vec::new();
        for service in services {
            for check in &service.checks {
                results.push(self.run(&service.id, check).await);
            }
        }
        results
    }

    /// Runs one check.
    pub async fn run(&self, service_id: &str, check: &Check) -> CheckResult {
        let mut result = CheckResult {
            service: service_id.to_owned(),
            check_type: check.check_type.clone(),
            label: check.display_label(),
            ..CheckResult::default()
        };
        let call = tokio::time::timeout(self.timeout, self.run_inner(check, &mut result));
        if call.await.is_err() && result.error.is_empty() {
            "timeout".clone_into(&mut result.error);
        }
        result
    }

    async fn run_inner(&self, check: &Check, result: &mut CheckResult) {
        match check.check_type.as_str() {
            catalog::CHECK_TCP => self.run_tcp(check, result).await,
            catalog::CHECK_HTTP => self.run_http(check, result).await,
            catalog::CHECK_HA_STATE => self.run_ha_state(check, result).await,
            catalog::CHECK_HA_TEMPLATE => self.run_ha_template(check, result).await,
            catalog::CHECK_HA_UNAVAILABLE => self.run_ha_unavailable(check, result).await,
            _ => result.error = format!("неизвестный тип проверки: {}", check.check_type),
        }
    }

    async fn run_tcp(&self, check: &Check, result: &mut CheckResult) {
        let target = format!("{}:{}", check.host, check.port);
        match tokio::net::TcpStream::connect(&target).await {
            Ok(stream) => {
                drop(stream);
                result.ok = true;
                result.value = target;
            }
            Err(error) => result.error = error.to_string(),
        }
    }

    async fn run_http(&self, check: &Check, result: &mut CheckResult) {
        if check.auth == catalog::AUTH_HA && self.token.is_empty() {
            result.skipped = true;
            result.ok = true;
            "Home Assistant не настроен".clone_into(&mut result.value);
            return;
        }
        let mut request = self.http.get(&check.url);
        if check.auth == catalog::AUTH_HA {
            request = request.bearer_auth(&self.token);
        }
        let response = match request.send().await {
            Ok(response) => response,
            Err(error) => {
                result.error = error.to_string();
                return;
            }
        };
        let status = response.status().as_u16();
        let _ = response.bytes().await;
        let expected = if check.expect_status == 0 {
            200
        } else {
            check.expect_status
        };
        result.value = format!("HTTP {status}");
        if i64::from(status) != expected {
            result.error = format!("ожидался HTTP {expected}, получен {status}");
            return;
        }
        result.ok = true;
    }

    fn require_ha(&self, result: &mut CheckResult) -> bool {
        if self.base_url.is_empty() || self.token.is_empty() {
            result.skipped = true;
            result.ok = true;
            "Home Assistant не настроен".clone_into(&mut result.value);
            return false;
        }
        true
    }

    async fn run_ha_state(&self, check: &Check, result: &mut CheckResult) {
        if !self.require_ha(result) {
            return;
        }
        let path = format!("/api/states/{}", check.entity_id);
        let state: HaState = match self.get_json(&path).await {
            Ok(state) => state,
            Err(error) => {
                result.error = error;
                return;
            }
        };
        state.state.clone_into(&mut result.value);
        let bad = if check.bad_states.is_empty() {
            vec!["unavailable".to_owned(), "unknown".to_owned()]
        } else {
            check.bad_states.clone()
        };
        for candidate in bad {
            if candidate.trim().eq_ignore_ascii_case(&state.state) {
                result.error = format!("состояние {}", state.state);
                return;
            }
        }
        result.ok = true;
    }

    async fn run_ha_template(&self, check: &Check, result: &mut CheckResult) {
        if !self.require_ha(result) {
            return;
        }
        let body = match self.post_template(&check.template).await {
            Ok(body) => body,
            Err(error) => {
                result.error = error;
                return;
            }
        };
        let mut value = body.trim().to_owned();
        if value.chars().count() > 200 {
            value = format!("{}…", value.chars().take(200).collect::<String>());
        }
        if value.is_empty() {
            if check.required {
                "шаблон вернул пустое значение".clone_into(&mut result.error);
                return;
            }
            result.ok = true;
            "нет данных".clone_into(&mut result.value);
            return;
        }
        result.value = value;
        result.ok = true;
    }

    async fn run_ha_unavailable(&self, check: &Check, result: &mut CheckResult) {
        if !self.require_ha(result) {
            return;
        }
        let states: Vec<HaState> = match self.get_json("/api/states").await {
            Ok(states) => states,
            Err(error) => {
                result.error = error;
                return;
            }
        };
        let domains: BTreeSet<String> = check
            .domains
            .iter()
            .map(|domain| domain.trim().to_lowercase())
            .collect();
        let mut count = 0usize;
        for state in states {
            if !domains.is_empty() {
                let domain = state.entity_id.split('.').next().unwrap_or_default();
                if !domains.contains(&domain.to_lowercase()) {
                    continue;
                }
            }
            if state.state != "unavailable" {
                continue;
            }
            count += 1;
            if result.details.len() < 10 {
                let entity = if state.attributes.friendly_name.trim().is_empty() {
                    state.entity_id.clone()
                } else {
                    format!(
                        "{} ({})",
                        state.attributes.friendly_name.trim(),
                        state.entity_id
                    )
                };
                result.details.push(entity);
            }
        }
        result.value = count.to_string();
        if count > 0 {
            result.error = format!("недоступно сущностей: {count}");
            return;
        }
        result.ok = true;
    }

    async fn get_json<T: for<'de> Deserialize<'de>>(&self, path: &str) -> Result<T, String> {
        let response = self
            .http
            .get(format!("{}{path}", self.base_url))
            .bearer_auth(&self.token)
            .header(reqwest::header::ACCEPT, "application/json")
            .send()
            .await
            .map_err(|error| error.to_string())?;
        let status = response.status().as_u16();
        let body = response.bytes().await.map_err(|error| error.to_string())?;
        if status != 200 {
            return Err(format!("Home Assistant {path} вернул HTTP {status}"));
        }
        let body = &body[..body.len().min(MAX_BODY)];
        serde_json::from_slice(body)
            .map_err(|error| format!("ответ Home Assistant не JSON: {error}"))
    }

    async fn post_template(&self, template: &str) -> Result<String, String> {
        let body = serde_json::to_vec(&serde_json::json!({"template": template}))
            .map_err(|error| error.to_string())?;
        let response = self
            .http
            .post(format!("{}/api/template", self.base_url))
            .bearer_auth(&self.token)
            .header(reqwest::header::CONTENT_TYPE, "application/json")
            .body(body)
            .send()
            .await
            .map_err(|error| error.to_string())?;
        let status = response.status().as_u16();
        let body = response.bytes().await.map_err(|error| error.to_string())?;
        if status != 200 {
            return Err(format!("Home Assistant /api/template вернул HTTP {status}"));
        }
        Ok(serde_json::from_slice::<String>(&body)
            .unwrap_or_else(|_| String::from_utf8_lossy(&body).trim().to_owned()))
    }
}

#[derive(Debug, Deserialize)]
struct HaState {
    entity_id: String,
    state: String,
    #[serde(default)]
    attributes: HaAttributes,
}

#[derive(Debug, Default, Deserialize)]
struct HaAttributes {
    #[serde(default)]
    friendly_name: String,
}

#[cfg(test)]
mod tests {
    use axum::Router;
    use axum::routing::get;

    use super::*;

    async fn serve(app: Router) -> String {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind");
        let address = listener.local_addr().expect("addr");
        tokio::spawn(async move { axum::serve(listener, app).await.expect("serve") });
        format!("http://{address}")
    }

    #[tokio::test]
    async fn ha_checks_are_skipped_without_configuration() {
        let runner = Runner::new("", "").expect("runner");
        let check = Check {
            check_type: catalog::CHECK_HA_TEMPLATE.to_owned(),
            label: "Pulsar".to_owned(),
            template: "{{ 1 }}".to_owned(),
            ..Check::default()
        };
        let result = runner.run("ha", &check).await;
        assert!(result.skipped);
        assert!(result.ok);
        assert_eq!(result.value, "Home Assistant не настроен");
    }

    #[tokio::test]
    async fn tcp_and_template_checks_run() {
        let app = Router::new()
            .route("/api/template", axum::routing::post(|| async { "1.2.3" }))
            .route("/api/states", get(|| async { "[]" }));
        let base = serve(app).await;
        let runner = Runner::new(&base, "token").expect("runner");
        let tcp = Check {
            check_type: catalog::CHECK_TCP.to_owned(),
            host: "192.0.2.1".to_owned(),
            port: 9,
            ..Check::default()
        };
        let result = runner.run("svc", &tcp).await;
        assert!(!result.ok);
        assert!(!result.error.is_empty());
    }
}
