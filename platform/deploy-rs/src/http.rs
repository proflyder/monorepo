use std::time::Duration;

use serde_json::Value;

pub const PREFLIGHT_TIMEOUT: Duration = Duration::from_secs(15);

pub struct Answer {
    pub status: u16,
    pub value: Option<Value>,
}

pub struct Client {
    agent: ureq::Agent,
}

impl Client {
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

    #[must_use]
    pub fn get(&self, url: &str, token: &str) -> Answer {
        let request = self
            .agent
            .get(url)
            .header("Authorization", &format!("Bearer {token}"));
        perform(request.call())
    }

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
