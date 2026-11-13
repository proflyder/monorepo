//! FCM HTTP v1 sender with service-account OAuth.
//!
//! A service-account JSON is exchanged for a short-lived access token with a
//! signed RS256 JWT assertion, exactly like the official client libraries:
//! `POST {token_url}` with `grant_type=…jwt-bearer&assertion=<jwt>`. The token
//! is cached in memory until shortly before it expires; nothing is written to
//! disk.
//!
//! Two message shapes:
//!
//! * [`Message::notification`] carries a `notification` block, so the system
//!   tray shows the text even if the app process is gone;
//! * [`Message::find`] is data-only with `android.priority=high`, so the
//!   device wakes the app instead of rendering a tray entry.
//!
//! Both set the Android channel id: the app owns the channels and their
//! importance, the server only names them.

use std::sync::Mutex;
use std::time::Duration;

use ring::rand::SystemRandom;
use ring::signature::{RSA_PKCS1_SHA256, RsaKeyPair};
use serde::Serialize;
use serde_json::{Value, json};

/// Data key that marks a find-phone message.
pub const FIND_TYPE: &str = "phone_find";
/// Data key that marks a normal notification message.
pub const NOTIFY_TYPE: &str = "phone_notify";
/// Notification channel the app registers for normal notifications.
pub const NOTIFICATION_CHANNEL: &str = "home_notifications";

/// Why a send failed.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// The service account cannot be used (malformed or unsupported key).
    #[error("service account: {0}")]
    ServiceAccount(&'static str),
    /// The OAuth assertion was rejected.
    #[error("oauth: {0}")]
    OAuth(String),
    /// FCM rejected the message body.
    #[error("fcm: {0}")]
    Request(String),
    /// FCM answered with a permanent per-token error; the caller should forget
    /// the token.
    #[error("fcm rejected the token")]
    Unregistered,
    /// FCM answered with a transient failure.
    #[error("fcm unavailable: HTTP {0}")]
    Unavailable(u16),
}

/// One parsed service account.
#[derive(Debug, Clone)]
pub struct ServiceAccount {
    pub project_id: String,
    pub client_email: String,
    private_key_pem: String,
}

impl ServiceAccount {
    /// Parses the JSON a GCP service-account key file contains.
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

    /// The PKCS#8 DER bytes behind the PEM block.
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

    /// The PKCS#1 RSA public key (DER) of the service account. Exposed so a
    /// test can verify the signed assertion, and so a future rotation check can
    /// compare the key that is actually mounted.
    pub fn public_key_der(&self) -> Result<Vec<u8>, Error> {
        Ok(self.signer()?.public().as_ref().to_vec())
    }

    /// Builds the signed JWT assertion for one OAuth exchange.
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

/// One message to deliver.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Message {
    /// Data-only find-phone request.
    Find { reason: String },
    /// Notification with an optional deep link.
    Notification {
        title: String,
        body: String,
        deep_link: Option<String>,
    },
}

impl Message {
    /// `type` value the app routes on.
    #[must_use]
    pub const fn kind(&self) -> &'static str {
        match self {
            Self::Find { .. } => FIND_TYPE,
            Self::Notification { .. } => NOTIFY_TYPE,
        }
    }

    /// The FCM `message` object for one device token.
    #[must_use]
    pub fn payload(&self, token: &str) -> Value {
        match self {
            Self::Find { reason } => json!({
                "message": {
                    "token": token,
                    "android": {"priority": "high"},
                    "data": {"type": FIND_TYPE, "reason": reason},
                }
            }),
            Self::Notification {
                title,
                body,
                deep_link,
            } => {
                let mut data = json!({"type": NOTIFY_TYPE});
                if let Some(link) = deep_link {
                    data["deepLink"] = json!(link);
                }
                json!({
                    "message": {
                        "token": token,
                        "android": {
                            "priority": "high",
                            "notification": {"channel_id": NOTIFICATION_CHANNEL},
                        },
                        "notification": {"title": title, "body": body},
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

/// The FCM sender. Cloning is cheap: one HTTP client and a token cache.
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
    /// Builds a sender from the service-account JSON.
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

    /// The project id the API path needs.
    #[must_use]
    pub fn project_id(&self) -> &str {
        &self.account.project_id
    }

    /// Sends one message to one token.
    pub async fn send(&self, token: &str, message: &Message, now: i64) -> Result<(), Error> {
        let access = self.access_token(now).await?;
        let url = format!(
            "{}/v1/projects/{}/messages:send",
            self.fcm_url, self.account.project_id
        );
        let payload = serde_json::to_vec(&message.payload(token))
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
            // The cached token may have been revoked; drop it for the next call.
            self.clear_cache();
            return Err(Error::OAuth("access token rejected".to_owned()));
        }
        if code >= 500 || code == 429 {
            return Err(Error::Unavailable(code));
        }
        Err(Error::Request(format!("HTTP {code}")))
    }

    /// Returns a cached access token or exchanges a fresh assertion.
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

/// Whether an FCM error body means "this token is dead".
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
        .payload("token");
        let message = &payload["message"];
        assert!(message.get("notification").is_none());
        assert_eq!(message["android"]["priority"], "high");
        assert_eq!(message["data"]["type"], FIND_TYPE);
        assert_eq!(message["data"]["reason"], "Рикардо");
        assert_eq!(message["token"], "token");
    }

    #[test]
    fn a_notification_names_the_app_channel_and_carries_the_deep_link() {
        let payload = Message::Notification {
            title: "Дом".to_owned(),
            body: "Готово".to_owned(),
            deep_link: Some("home://alarms".to_owned()),
        }
        .payload("token");
        let message = &payload["message"];
        assert_eq!(message["notification"]["title"], "Дом");
        assert_eq!(
            message["android"]["notification"]["channel_id"],
            NOTIFICATION_CHANNEL
        );
        assert_eq!(message["data"]["deepLink"], "home://alarms");
        assert_eq!(message["data"]["type"], NOTIFY_TYPE);
    }

    #[test]
    fn unregistered_errors_are_recognised() {
        assert!(is_unregistered(
            r#"{"error":{"details":[{"errorCode":"UNREGISTERED"}]}}"#
        ));
        assert!(!is_unregistered(r#"{"error":{"status":"INTERNAL"}}"#));
    }
}
