//! Small HTTP client shared by preflight and the Assist smoke check.
//!
//! The behaviour mirrors the Go helpers: a transport error is reported as
//! status 0, a non-200 response drops the body, an empty body is `None`, and a
//! body that is not JSON is also reported as status 0 (the Python/Go contract).

use std::time::Duration;

use serde_json::Value;

/// Default request timeout used by preflight.
pub const PREFLIGHT_TIMEOUT: Duration = Duration::from_secs(15);

/// One JSON answer from a helper request.
pub struct Answer {
    /// HTTP status (0 for a transport or JSON error).
    pub status: u16,
    /// Parsed body, if any.
    pub value: Option<Value>,
}

/// HTTP JSON client.
pub struct Client {
    agent: ureq::Agent,
}

impl Client {
    /// Creates a client with a global timeout.
    #[must_use]
    pub fn new(timeout: Duration) -> Self {
        let config = ureq::Agent::config_builder()
            .proxy(None)
            .http_status_as_error(false)
            .timeout_global(Some(timeout))
            .build();
        Self {
            agent: ureq::Agent::new_with_config(config),
        }
    }

    /// Performs a GET with a bearer token.
    #[must_use]
    pub fn get(&self, url: &str, token: &str) -> Answer {
        let request = self
            .agent
            .get(url)
            .header("Authorization", &format!("Bearer {token}"));
        perform(request.call())
    }

    /// Performs a JSON POST with a bearer token.
    #[must_use]
    pub fn post_json(&self, url: &str, token: &str, payload: &str) -> Answer {
        let request = self
            .agent
            .post(url)
            .header("Authorization", &format!("Bearer {token}"))
            .header("Content-Type", "application/json");
        perform(request.send(payload))
    }
}

fn perform(result: Result<ureq::http::Response<ureq::Body>, ureq::Error>) -> Answer {
    let Ok(mut response) = result else {
        return Answer {
            status: 0,
            value: None,
        };
    };
    let status = response.status().as_u16();
    if status != 200 {
        return Answer {
            status,
            value: None,
        };
    }
    let Ok(text) = response.body_mut().read_to_string() else {
        return Answer {
            status,
            value: None,
        };
    };
    let text = text.trim();
    if text.is_empty() {
        return Answer {
            status,
            value: None,
        };
    }
    serde_json::from_str(text).map_or_else(
        |_| Answer {
            status: 0,
            value: None,
        },
        |value| Answer {
            status,
            value: Some(value),
        },
    )
}
