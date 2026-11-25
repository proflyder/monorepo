use std::collections::BTreeMap;
use std::time::Duration;

use home_secret::Token;
use serde::Serialize;

const MAX_PUSH_TIMEOUT: Duration = Duration::from_secs(5);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Action {
    StepUp,
    Enroll,
    DeviceRevoke,
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
            Self::Enroll => "device_enroll",
            Self::DeviceRevoke => "device_revoke",
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
        if !should_push(event) {
            return;
        }
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
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub data: BTreeMap<String, String>,
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
        Action::StepUp => "Подтверждение телефона",
        Action::Enroll => "Привязка телефона",
        Action::DeviceRevoke => "Отзыв телефона",
    };
    let outcome = outcome_label(event.outcome);
    let body = if path.is_empty() {
        format!("{action}{outcome}")
    } else {
        format!("{action}{outcome}: {path}")
    };
    let mut data = BTreeMap::from([("entity".to_owned(), "secrets".to_owned())]);
    if !path.is_empty() {
        data.insert("path".to_owned(), path.to_owned());
    }
    PushBody {
        module: "secrets".to_owned(),
        title: "Секреты".to_owned(),
        body,
        deep_link: Some(deep_link(event)),
        data,
    }
}

fn should_push(event: &Event<'_>) -> bool {
    match event.action {
        Action::List | Action::Metadata => false,
        Action::StepUp => event.outcome != "ok",
        Action::Reveal
        | Action::Update
        | Action::Rollback
        | Action::Delete
        | Action::Enroll
        | Action::DeviceRevoke => true,
    }
}

fn deep_link(event: &Event<'_>) -> String {
    match event.action {
        Action::Enroll | Action::DeviceRevoke => "home://secrets/device".to_owned(),
        _ => event.path.filter(|path| !path.is_empty()).map_or_else(
            || "home://secrets".to_owned(),
            |path| format!("home://secrets/path/{}", path.replace('/', "%2F")),
        ),
    }
}

fn outcome_label(outcome: &str) -> &'static str {
    match outcome {
        "ok" => "",
        "step_up_required" => " — нужно подтверждение телефона",
        "step_up_invalid" => " — неверная подпись",
        "step_up_expired" => " — подтверждение истекло",
        "device_not_enrolled" => " — телефон не привязан",
        "device_unavailable" => " — устройство недоступно",
        "enrollment_invalid" => " отклонена",
        "enrollment_closed" => " — код привязки не выдан",
        "enrollment_expired" => " — код привязки истёк",
        "enrollment_conflict" => " — привязка изменилась",
        "attestation_challenge_invalid" => " — неверный challenge аттестации",
        "attestation_invalid" => " — неверная цепочка аттестации",
        "public_key_invalid" => " — неверный публичный ключ",
        "rate_limited" => " — слишком много попыток",
        "conflict" => " — конфликт версий",
        "path_forbidden" => " — путь запрещён",
        "invalid_path" => " — неверный путь",
        "invalid_key" => " — неверный ключ",
        "value_too_large" => " — значение слишком велико",
        "version_required" => " — нужна версия",
        "version_not_found" => " — версия не найдена",
        "version_unavailable" => " — версия недоступна",
        "version_not_current" => " — версия не последняя",
        "state_unavailable" => " — состояние недоступно",
        "key_not_found" => " — ключ не найден",
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
        assert_eq!(
            body.deep_link.as_deref(),
            Some("home://secrets/path/k8s%2Ftest")
        );
        assert_eq!(body.data["entity"], "secrets");
        assert_eq!(body.data["path"], "k8s/test");
        assert!(!body.body.contains("password"));
        assert!(
            !serde_json::to_string(&body)
                .expect("json")
                .contains("value")
        );
    }

    #[test]
    fn a_rejected_enrollment_does_not_blame_the_code() {
        let body = push_body(&Event {
            action: Action::Enroll,
            principal: "owner",
            path: None,
            key: None,
            version: None,
            outcome: "enrollment_invalid",
            detail: Some("attestation_invalid"),
        });
        assert_eq!(body.body, "Привязка телефона отклонена");
    }

    #[test]
    fn list_and_metadata_never_push_but_secret_access_does() {
        let event = |action, outcome| Event {
            action,
            principal: "owner",
            path: Some("k8s/test"),
            key: None,
            version: None,
            outcome,
            detail: None,
        };
        assert!(!should_push(&event(Action::List, "ok")));
        assert!(!should_push(&event(Action::Metadata, "ok")));
        assert!(should_push(&event(Action::Reveal, "ok")));
        assert!(should_push(&event(Action::Update, "ok")));
        assert!(should_push(&event(Action::Rollback, "ok")));
        assert!(should_push(&event(Action::Delete, "ok")));
        assert!(should_push(&event(Action::Enroll, "ok")));
        assert!(should_push(&event(Action::DeviceRevoke, "ok")));
        assert!(!should_push(&event(Action::StepUp, "ok")));
        assert!(should_push(&event(Action::StepUp, "step_up_invalid")));
        assert!(should_push(&event(Action::StepUp, "rate_limited")));
        assert!(should_push(&event(Action::Reveal, "step_up_required")));
    }

    #[test]
    fn a_catalog_attempt_links_to_the_catalog() {
        let body = push_body(&Event {
            action: Action::List,
            principal: "owner",
            path: None,
            key: None,
            version: None,
            outcome: "ok",
            detail: None,
        });
        assert_eq!(body.deep_link.as_deref(), Some("home://secrets"));
        assert_eq!(body.body, "Просмотр каталога секретов");
    }

    #[test]
    fn a_nested_secret_path_becomes_a_path_link() {
        let body = push_body(&Event {
            action: Action::Metadata,
            principal: "owner",
            path: Some("infra/backup/ha"),
            key: None,
            version: None,
            outcome: "ok",
            detail: None,
        });
        assert_eq!(
            body.deep_link.as_deref(),
            Some("home://secrets/path/infra%2Fbackup%2Fha")
        );
    }

    #[test]
    fn a_device_event_opens_the_binding_screen() {
        let body = push_body(&Event {
            action: Action::DeviceRevoke,
            principal: "owner",
            path: None,
            key: None,
            version: None,
            outcome: "ok",
            detail: None,
        });
        assert_eq!(body.deep_link.as_deref(), Some("home://secrets/device"));
        assert_eq!(body.data["entity"], "secrets");
        assert!(!body.data.contains_key("path"));
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
        assert!(body.body.contains("неверная подпись"));
        assert!(body.body.contains("k8s/test"));
        assert!(!body.body.contains("password"));
    }

    #[test]
    fn every_action_has_a_name() {
        for action in [
            Action::StepUp,
            Action::Enroll,
            Action::DeviceRevoke,
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
