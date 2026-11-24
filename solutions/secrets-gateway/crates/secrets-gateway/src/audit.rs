use std::time::Duration;

use home_secret::Token;
use serde::Serialize;

const MAX_PUSH_TIMEOUT: Duration = Duration::from_secs(5);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Action {
    StepUp,
    List,
    Metadata,
    Reveal,
    Update,
    Rollback,
    Delete,
}

impl Action {
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::StepUp => "step_up",
            Self::List => "list",
            Self::Metadata => "metadata",
            Self::Reveal => "reveal",
            Self::Update => "update",
            Self::Rollback => "rollback",
            Self::Delete => "delete",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Event<'a> {
    pub action: Action,
    pub principal: &'a str,
    pub path: Option<&'a str>,
    pub key: Option<&'a str>,
    pub version: Option<u64>,
    pub outcome: &'a str,
    pub detail: Option<&'a str>,
}

#[derive(Clone)]
pub struct Auditor {
    push: Option<PushClient>,
}

impl Auditor {
    #[must_use]
    pub fn disabled() -> Self {
        Self { push: None }
    }

    #[must_use]
    pub fn new(push: PushClient) -> Self {
        Self { push: Some(push) }
    }

    pub fn record(&self, event: &Event<'_>) {
        tracing::info!(
            event = "secrets.access",
            action = event.action.as_str(),
            principal = event.principal,
            path = event.path.unwrap_or(""),
            key = event.key.unwrap_or(""),
            version = event.version.unwrap_or(0),
            outcome = event.outcome,
            detail = event.detail.unwrap_or(""),
            "secrets access"
        );
        let Some(push) = self.push.clone() else {
            return;
        };
        let body = push_body(event);
        tokio::spawn(async move {
            if let Err(error) = push.notify(&body).await {
                tracing::warn!(error = %error, "secrets audit push failed");
            }
        });
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PushBody {
    pub module: String,
    pub title: String,
    pub body: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub deep_link: Option<String>,
}

fn push_body(event: &Event<'_>) -> PushBody {
    let path = event.path.unwrap_or_default();
    let action = match event.action {
        Action::Reveal => "Показ секрета",
        Action::Update => "Обновление секрета",
        Action::Rollback => "Откат секрета",
        Action::Delete => "Удаление последней версии",
        Action::List => "Просмотр каталога секретов",
        Action::Metadata => "Просмотр версий секрета",
        Action::StepUp => "Step-up",
    };
    let outcome = outcome_label(event.outcome);
    let body = if path.is_empty() {
        format!("{action}{outcome}")
    } else {
        format!("{action}{outcome}: {path}")
    };
    PushBody {
        module: "secrets".to_owned(),
        title: "Секреты".to_owned(),
        body,
        deep_link: Some("home://secrets".to_owned()),
    }
}

fn outcome_label(outcome: &str) -> &'static str {
    match outcome {
        "ok" => "",
        "step_up_required" => " — нужен step-up",
        "step_up_invalid" => " — неверный код",
        "rate_limited" => " — слишком много попыток",
        "conflict" => " — конфликт версий",
        "error" => " — ошибка",
        _ => " — отказ",
    }
}

#[derive(Clone)]
pub struct PushClient {
    http: reqwest::Client,
    url: String,
    token: Token,
}

impl PushClient {
    pub fn new(base_url: &str, token: Token, timeout: Duration) -> Result<Self, String> {
        let http = home_tls::public_https_client()
            .map_err(|error| format!("phone-push TLS: {error}"))?
            .timeout(timeout.min(MAX_PUSH_TIMEOUT))
            .build()
            .map_err(|error| format!("phone-push HTTP client: {error}"))?;
        Ok(Self {
            http,
            url: format!("{}/v1/push/notify", base_url.trim_end_matches('/')),
            token,
        })
    }

    pub async fn notify(&self, body: &PushBody) -> Result<(), String> {
        let payload = serde_json::to_vec(body).map_err(|_| "push request body".to_owned())?;
        let response = self
            .http
            .post(&self.url)
            .bearer_auth(self.token.expose())
            .header(reqwest::header::CONTENT_TYPE, "application/json")
            .body(payload)
            .send()
            .await
            .map_err(|_| "push request failed".to_owned())?;
        if !response.status().is_success() {
            return Err(format!("push returned HTTP {}", response.status().as_u16()));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_push_never_carries_a_secret_value() {
        let body = push_body(&Event {
            action: Action::Reveal,
            principal: "owner",
            path: Some("k8s/test"),
            key: Some("password"),
            version: Some(3),
            outcome: "ok",
            detail: None,
        });
        assert_eq!(body.module, "secrets");
        assert_eq!(body.body, "Показ секрета: k8s/test");
        assert_eq!(body.deep_link.as_deref(), Some("home://secrets"));
        assert!(!body.body.contains("password"));
        assert!(
            !serde_json::to_string(&body)
                .expect("json")
                .contains("value")
        );
    }

    #[test]
    fn a_failed_attempt_is_not_reported_as_success() {
        let body = push_body(&Event {
            action: Action::Update,
            principal: "owner",
            path: Some("k8s/test"),
            key: Some("password"),
            version: None,
            outcome: "step_up_invalid",
            detail: None,
        });
        assert!(body.body.contains("неверный код"));
        assert!(body.body.contains("k8s/test"));
        assert!(!body.body.contains("password"));
    }

    #[test]
    fn every_action_has_a_name() {
        for action in [
            Action::StepUp,
            Action::List,
            Action::Metadata,
            Action::Reveal,
            Action::Update,
            Action::Rollback,
            Action::Delete,
        ] {
            assert!(!action.as_str().is_empty());
        }
    }
}
