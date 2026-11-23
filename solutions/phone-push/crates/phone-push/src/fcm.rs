use std::sync::Mutex;
use std::time::Duration;

use ring::rand::SystemRandom;
use ring::signature::{RSA_PKCS1_SHA256, RsaKeyPair};
use serde::Serialize;
use serde_json::{Value, json};

pub const FIND_TYPE: &str = "phone_find";
pub const NOTIFY_TYPE: &str = "phone_notify";
pub const SYNC_TYPE: &str = "sync";

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("service account: {0}")]
    ServiceAccount(&'static str),
    #[error("oauth: {0}")]
    OAuth(String),
    #[error("fcm: {0}")]
    Request(String),
    #[error("fcm rejected the token")]
    Unregistered,
    #[error("fcm unavailable: HTTP {0}")]
    Unavailable(u16),
}

#[derive(Debug, Clone)]
pub struct ServiceAccount {
    pub project_id: String,
    pub client_email: String,
    private_key_pem: String,
}

impl ServiceAccount {
    pub fn parse(json: &str) -> Result<Self, Error> {
        let document: Value =
            serde_json::from_str(json).map_err(|_| Error::ServiceAccount("invalid JSON"))?;
        let field = |name: &str| {
            document
                .get(name)
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|value| !value.is_empty())
        };
        let project_id =
            field("project_id").ok_or(Error::ServiceAccount("project_id is missing"))?;
        let client_email =
            field("client_email").ok_or(Error::ServiceAccount("client_email is missing"))?;
        let private_key = field("private_key")
            .ok_or(Error::ServiceAccount("private_key is missing"))?
            .replace("\\n", "\n");
        Ok(Self {
            project_id: project_id.to_owned(),
            client_email: client_email.to_owned(),
            private_key_pem: private_key,
        })
    }

    fn private_key_der(&self) -> Result<Vec<u8>, Error> {
        let body: String = self
            .private_key_pem
            .lines()
            .filter(|line| !line.starts_with("-----"))
            .collect();
        let der = base64::Engine::decode(&base64::engine::general_purpose::STANDARD, body.trim())
            .map_err(|_| Error::ServiceAccount("private_key is not valid base64"))?;
        Ok(der)
    }

    fn signer(&self) -> Result<RsaKeyPair, Error> {
        let der = self.private_key_der()?;
        RsaKeyPair::from_pkcs8(&der)
            .map_err(|_| Error::ServiceAccount("private_key is not a PKCS#8 RSA key"))
    }

    pub fn public_key_der(&self) -> Result<Vec<u8>, Error> {
        Ok(self.signer()?.public().as_ref().to_vec())
    }

    pub fn assertion(&self, token_url: &str, issued_at: i64) -> Result<String, Error> {
        let header = json!({"alg": "RS256", "typ": "JWT"});
        let claims = json!({
            "iss": self.client_email,
            "scope": super::config::FCM_SCOPE,
            "aud": token_url,
            "iat": issued_at,
            "exp": issued_at + 3600,
        });
        let signing_input = format!("{}.{}", b64_json(&header), b64_json(&claims));
        let key = self.signer()?;
        let mut signature = vec![0_u8; key.public().modulus_len()];
        key.sign(
            &RSA_PKCS1_SHA256,
            &SystemRandom::new(),
            signing_input.as_bytes(),
            &mut signature,
        )
        .map_err(|_| Error::ServiceAccount("signing failed"))?;
        Ok(format!(
            "{signing_input}.{}",
            base64::Engine::encode(&base64::engine::general_purpose::URL_SAFE_NO_PAD, signature)
        ))
    }
}

fn b64_json(value: &Value) -> String {
    let bytes = serde_json::to_vec(value).unwrap_or_default();
    base64::Engine::encode(&base64::engine::general_purpose::URL_SAFE_NO_PAD, bytes)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Message {
    Find {
        reason: String,
    },
    Notification {
        title: String,
        body: String,
        deep_link: Option<String>,
    },
    Sync {
        entity: String,
        op: String,
        id: String,
    },
}

impl Message {
    #[must_use]
    pub const fn kind(&self) -> &'static str {
        match self {
            Self::Find { .. } => FIND_TYPE,
            Self::Notification { .. } => NOTIFY_TYPE,
            Self::Sync { .. } => SYNC_TYPE,
        }
    }

    #[must_use]
    pub fn payload(&self, token: &str, record_id: &str) -> Value {
        match self {
            Self::Find { reason } => json!({
                "message": {
                    "token": token,
                    "android": {"priority": "high"},
                    "data": {"type": FIND_TYPE, "reason": reason, "notificationId": record_id},
                }
            }),
            Self::Sync { entity, op, id } => json!({
                "message": {
                    "token": token,
                    "android": {"priority": "normal", "collapse_key": format!("{SYNC_TYPE}-{entity}")},
                    "data": {"type": SYNC_TYPE, "entity": entity, "op": op, "id": id},
                }
            }),
            Self::Notification {
                title,
                body,
                deep_link,
            } => {
                let mut data = json!({
                    "type": NOTIFY_TYPE,
                    "title": title,
                    "body": body,
                    "notificationId": record_id,
                });
                if let Some(link) = deep_link {
                    data["deepLink"] = json!(link);
                }
                json!({
                    "message": {
                        "token": token,
                        "android": {"priority": "high"},
                        "data": data,
                    }
                })
            }
        }
    }
}

#[derive(Debug, Serialize)]
struct TokenRequest<'a> {
    grant_type: &'a str,
    assertion: &'a str,
}

struct CachedToken {
    value: String,
    expires_at: i64,
}

pub struct Sender {
    http: reqwest::Client,
    account: ServiceAccount,
    token_url: String,
    fcm_url: String,
    cache: Mutex<Option<CachedToken>>,
}

impl std::fmt::Debug for Sender {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("Sender")
            .field("project_id", &self.account.project_id)
            .field("token_url", &self.token_url)
            .field("fcm_url", &self.fcm_url)
            .finish_non_exhaustive()
    }
}

impl Sender {
    pub fn new(
        service_account: &str,
        token_url: &str,
        fcm_url: &str,
        timeout: Duration,
    ) -> Result<Self, Error> {
        let account = ServiceAccount::parse(service_account)?;
        let http = home_tls::public_https_client()
            .map_err(|_| Error::OAuth("TLS client".to_owned()))?
            .timeout(timeout)
            .build()
            .map_err(|_| Error::OAuth("HTTP client".to_owned()))?;
        Ok(Self {
            http,
            account,
            token_url: token_url.to_owned(),
            fcm_url: fcm_url.trim_end_matches('/').to_owned(),
            cache: Mutex::new(None),
        })
    }

    #[must_use]
    pub fn project_id(&self) -> &str {
        &self.account.project_id
    }

    pub async fn send(
        &self,
        token: &str,
        message: &Message,
        record_id: &str,
        now: i64,
    ) -> Result<(), Error> {
        let access = self.access_token(now).await?;
        let url = format!(
            "{}/v1/projects/{}/messages:send",
            self.fcm_url, self.account.project_id
        );
        let payload = serde_json::to_vec(&message.payload(token, record_id))
            .map_err(|_| Error::Request("body".to_owned()))?;
        let response = self
            .http
            .post(url)
            .bearer_auth(access)
            .header(reqwest::header::CONTENT_TYPE, "application/json")
            .body(payload)
            .send()
            .await
            .map_err(|_| Error::Unavailable(0))?;
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        if status.is_success() {
            return Ok(());
        }
        if is_unregistered(&body) {
            return Err(Error::Unregistered);
        }
        let code = status.as_u16();
        if code == 401 {
            self.clear_cache();
            return Err(Error::OAuth("access token rejected".to_owned()));
        }
        if code >= 500 || code == 429 {
            return Err(Error::Unavailable(code));
        }
        Err(Error::Request(format!("HTTP {code}")))
    }

    async fn access_token(&self, now: i64) -> Result<String, Error> {
        if let Some(cached) = self.cached_token(now) {
            return Ok(cached);
        }
        let assertion = self.account.assertion(&self.token_url, now)?;
        let response = self
            .http
            .post(&self.token_url)
            .form(&TokenRequest {
                grant_type: "urn:ietf:params:oauth:grant-type:jwt-bearer",
                assertion: &assertion,
            })
            .send()
            .await
            .map_err(|_| Error::OAuth("token request failed".to_owned()))?;
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        if !status.is_success() {
            return Err(Error::OAuth(format!(
                "token endpoint HTTP {}",
                status.as_u16()
            )));
        }
        let document: Value = serde_json::from_str(&body)
            .map_err(|_| Error::OAuth("invalid token response".to_owned()))?;
        let value = document
            .get("access_token")
            .and_then(Value::as_str)
            .filter(|value| !value.is_empty())
            .ok_or_else(|| Error::OAuth("token response has no access_token".to_owned()))?
            .to_owned();
        let expires_in = document
            .get("expires_in")
            .and_then(Value::as_i64)
            .unwrap_or(3600);
        self.store_token(&value, now + expires_in);
        Ok(value)
    }

    fn cached_token(&self, now: i64) -> Option<String> {
        let cache = self
            .cache
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        cache
            .as_ref()
            .filter(|cached| cached.expires_at - 60 > now)
            .map(|cached| cached.value.clone())
    }

    fn store_token(&self, value: &str, expires_at: i64) {
        let mut cache = self
            .cache
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        *cache = Some(CachedToken {
            value: value.to_owned(),
            expires_at,
        });
    }

    fn clear_cache(&self) {
        let mut cache = self
            .cache
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        *cache = None;
    }
}

fn is_unregistered(body: &str) -> bool {
    body.contains("UNREGISTERED") || body.contains("registration-token-not-registered")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_find_message_is_data_only_and_high_priority() {
        let payload = Message::Find {
            reason: "Рикардо".to_owned(),
        }
        .payload("token", "entry-1");
        let message = &payload["message"];
        assert!(message.get("notification").is_none());
        assert_eq!(message["android"]["priority"], "high");
        assert_eq!(message["data"]["type"], FIND_TYPE);
        assert_eq!(message["data"]["reason"], "Рикардо");
        assert_eq!(message["token"], "token");
    }

    #[test]
    fn a_sync_message_is_data_only_and_carries_no_content() {
        let payload = Message::Sync {
            entity: "alarms".to_owned(),
            op: "upsert".to_owned(),
            id: "alarm-1".to_owned(),
        }
        .payload("token", "entry-1");
        let message = &payload["message"];
        assert!(message.get("notification").is_none());
        assert!(message["android"].get("notification").is_none());
        assert_eq!(
            message["data"],
            json!({"type": SYNC_TYPE, "entity": "alarms", "op": "upsert", "id": "alarm-1"})
        );
    }

    #[test]
    fn a_notification_is_data_only_so_the_app_draws_it_in_its_theme() {
        let payload = Message::Notification {
            title: "Дом".to_owned(),
            body: "Готово".to_owned(),
            deep_link: Some("home://alarms".to_owned()),
        }
        .payload("token", "entry-1");
        let message = &payload["message"];
        assert!(message.get("notification").is_none());
        assert!(message["android"].get("notification").is_none());
        assert_eq!(message["android"]["priority"], "high");
        assert_eq!(message["data"]["type"], NOTIFY_TYPE);
        assert_eq!(message["data"]["title"], "Дом");
        assert_eq!(message["data"]["body"], "Готово");
        assert_eq!(message["data"]["deepLink"], "home://alarms");
    }

    #[test]
    fn unregistered_errors_are_recognised() {
        assert!(is_unregistered(
            r#"{"error":{"details":[{"errorCode":"UNREGISTERED"}]}}"#
        ));
        assert!(!is_unregistered(r#"{"error":{"status":"INTERNAL"}}"#));
    }
}
