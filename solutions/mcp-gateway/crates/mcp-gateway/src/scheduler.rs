use std::time::Duration;

use serde::{Deserialize, Deserializer, Serialize};
use serde_json::{Value, json};

use crate::tools::BoxFuture;

const SERVICE: &str = "home.v1.SchedulerService";
const MAX_BODY_BYTES: usize = 8 << 20;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct WallTime {
    #[serde(default)]
    pub hour: i32,
    #[serde(default)]
    pub minute: i32,
    #[serde(default)]
    pub second: i32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct Schedule {
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub label: String,
    #[serde(default)]
    pub kind: String,
    #[serde(default)]
    pub at: WallTime,
    #[serde(default, rename = "onceDate")]
    pub once_date: String,
    #[serde(default)]
    pub repeat: String,
    #[serde(default, rename = "nextDueUnix", deserialize_with = "int64")]
    pub next_due_unix: i64,
    #[serde(default, deserialize_with = "bool_or_default")]
    pub enabled: bool,
    #[serde(default, rename = "snoozeUntilUnix", deserialize_with = "int64")]
    pub snooze_until_unix: i64,
    #[serde(default, rename = "activeOccurrenceUnix", deserialize_with = "int64")]
    pub active_occurrence_unix: i64,
    #[serde(default, rename = "activeUntilUnix", deserialize_with = "int64")]
    pub active_until_unix: i64,
    #[serde(default, deserialize_with = "int64")]
    pub revision: i64,
    #[serde(default, rename = "payloadJson")]
    pub payload_json: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct Timer {
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub label: String,
    #[serde(default, rename = "durationSeconds", deserialize_with = "int64")]
    pub duration_seconds: i64,
    #[serde(default, rename = "remainingSeconds", deserialize_with = "int64")]
    pub remaining_seconds: i64,
    #[serde(default, rename = "dueUnix", deserialize_with = "int64")]
    pub due_unix: i64,
    #[serde(default)]
    pub state: String,
}

pub trait SchedulerApi: Send + Sync {
    fn configured(&self) -> bool {
        true
    }

    fn create_schedule<'a>(&'a self, request: Value) -> BoxFuture<'a, Result<Schedule, String>>;

    fn get_schedule<'a>(&'a self, id: &'a str) -> BoxFuture<'a, Result<Schedule, String>>;

    fn list_schedules<'a>(
        &'a self,
        kind: &'a str,
        include_disabled: bool,
    ) -> BoxFuture<'a, Result<Vec<Schedule>, String>>;

    fn update_schedule<'a>(
        &'a self,
        schedule: Value,
        expected_revision: i64,
    ) -> BoxFuture<'a, Result<Schedule, String>>;

    fn delete_schedule<'a>(&'a self, id: &'a str) -> BoxFuture<'a, Result<(), String>>;

    fn set_schedule_enabled<'a>(
        &'a self,
        id: &'a str,
        enabled: bool,
    ) -> BoxFuture<'a, Result<Schedule, String>>;

    fn snooze_schedule<'a>(
        &'a self,
        id: &'a str,
        minutes: i64,
    ) -> BoxFuture<'a, Result<Schedule, String>>;

    fn stop_schedule<'a>(&'a self, id: &'a str) -> BoxFuture<'a, Result<Schedule, String>>;

    fn start_timer<'a>(
        &'a self,
        label: &'a str,
        seconds: i64,
        idempotency_key: &'a str,
    ) -> BoxFuture<'a, Result<Timer, String>>;

    fn list_timers<'a>(
        &'a self,
        include_finished: bool,
    ) -> BoxFuture<'a, Result<Vec<Timer>, String>>;

    fn cancel_timer<'a>(&'a self, id: &'a str) -> BoxFuture<'a, Result<Timer, String>>;

    fn pause_timer<'a>(&'a self, id: &'a str) -> BoxFuture<'a, Result<Timer, String>>;

    fn resume_timer<'a>(&'a self, id: &'a str) -> BoxFuture<'a, Result<Timer, String>>;
}

#[derive(Clone)]
pub struct Client {
    http: reqwest::Client,
    base_url: String,
    token: String,
    timezone: String,
}

impl std::fmt::Debug for Client {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("scheduler::Client")
            .field("base_url", &self.base_url)
            .finish_non_exhaustive()
    }
}

impl Client {
    pub fn new(
        base_url: &str,
        token: String,
        timezone: &str,
        timeout: Duration,
    ) -> Result<Self, String> {
        let http = home_tls::public_https_client()
            .map_err(|error| format!("scheduler TLS: {error}"))?
            .timeout(timeout)
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .map_err(|error| format!("scheduler client: {error}"))?;
        Ok(Self {
            http,
            base_url: base_url.trim().trim_end_matches('/').to_owned(),
            token,
            timezone: timezone.to_owned(),
        })
    }

    async fn call(&self, method: &str, body: Value) -> Result<Value, String> {
        let payload = serde_json::to_vec(&body)
            .map_err(|_| format!("scheduler {method}: invalid request"))?;
        let response = self
            .http
            .post(format!("{}/{SERVICE}/{method}", self.base_url))
            .header(
                reqwest::header::AUTHORIZATION,
                format!("Bearer {}", self.token),
            )
            .header(reqwest::header::CONTENT_TYPE, "application/json")
            .header("Connect-Protocol-Version", "1")
            .header("Time-Zone", &self.timezone)
            .body(payload)
            .send()
            .await
            .map_err(|_| format!("scheduler {method}: request failed"))?;
        let status = response.status().as_u16();
        let bytes = response
            .bytes()
            .await
            .map_err(|_| format!("scheduler {method}: request failed"))?;
        let bytes = &bytes[..bytes.len().min(MAX_BODY_BYTES)];
        let value: Value = serde_json::from_slice(bytes)
            .map_err(|_| format!("scheduler {method}: invalid JSON response"))?;
        if status != 200 {
            return Err(connect_error(method, status, &value));
        }
        Ok(value)
    }

    async fn typed<T: serde::de::DeserializeOwned>(
        &self,
        method: &str,
        body: Value,
    ) -> Result<T, String> {
        let value = self.call(method, body).await?;
        serde_json::from_value(value)
            .map_err(|_| format!("scheduler {method}: invalid JSON response"))
    }
}

fn connect_error(method: &str, status: u16, body: &Value) -> String {
    let code = body
        .get("code")
        .and_then(Value::as_str)
        .unwrap_or("unknown");
    let message = body
        .get("message")
        .and_then(Value::as_str)
        .unwrap_or("request rejected");
    format!("scheduler {method}: {code} ({status}): {message}")
}

impl SchedulerApi for Client {
    fn create_schedule<'a>(&'a self, request: Value) -> BoxFuture<'a, Result<Schedule, String>> {
        Box::pin(async move {
            #[derive(Deserialize)]
            struct Response {
                schedule: Schedule,
            }
            let response: Response = self.typed("CreateSchedule", request).await?;
            Ok(response.schedule)
        })
    }

    fn get_schedule<'a>(&'a self, id: &'a str) -> BoxFuture<'a, Result<Schedule, String>> {
        Box::pin(async move {
            #[derive(Deserialize)]
            struct Response {
                schedule: Schedule,
            }
            let response: Response = self.typed("GetSchedule", json!({"id": id})).await?;
            Ok(response.schedule)
        })
    }

    fn list_schedules<'a>(
        &'a self,
        kind: &'a str,
        include_disabled: bool,
    ) -> BoxFuture<'a, Result<Vec<Schedule>, String>> {
        Box::pin(async move {
            #[derive(Deserialize)]
            struct Response {
                #[serde(default)]
                schedules: Vec<Schedule>,
            }
            let response: Response = self
                .typed(
                    "ListSchedules",
                    json!({"kind": kind, "includeDisabled": include_disabled}),
                )
                .await?;
            Ok(response.schedules)
        })
    }

    fn update_schedule<'a>(
        &'a self,
        schedule: Value,
        expected_revision: i64,
    ) -> BoxFuture<'a, Result<Schedule, String>> {
        Box::pin(async move {
            #[derive(Deserialize)]
            struct Response {
                schedule: Schedule,
            }
            let response: Response = self
                .typed(
                    "UpdateSchedule",
                    json!({
                        "schedule": schedule,
                        "expectedRevision": expected_revision.to_string(),
                    }),
                )
                .await?;
            Ok(response.schedule)
        })
    }

    fn delete_schedule<'a>(&'a self, id: &'a str) -> BoxFuture<'a, Result<(), String>> {
        Box::pin(async move {
            self.call("DeleteSchedule", json!({"id": id})).await?;
            Ok(())
        })
    }

    fn set_schedule_enabled<'a>(
        &'a self,
        id: &'a str,
        enabled: bool,
    ) -> BoxFuture<'a, Result<Schedule, String>> {
        Box::pin(async move {
            #[derive(Deserialize)]
            struct Response {
                schedule: Schedule,
            }
            let response: Response = self
                .typed("SetScheduleEnabled", json!({"id": id, "enabled": enabled}))
                .await?;
            Ok(response.schedule)
        })
    }

    fn snooze_schedule<'a>(
        &'a self,
        id: &'a str,
        minutes: i64,
    ) -> BoxFuture<'a, Result<Schedule, String>> {
        Box::pin(async move {
            #[derive(Deserialize)]
            struct Response {
                schedule: Schedule,
            }
            let response: Response = self
                .typed("SnoozeSchedule", json!({"id": id, "minutes": minutes}))
                .await?;
            Ok(response.schedule)
        })
    }

    fn stop_schedule<'a>(&'a self, id: &'a str) -> BoxFuture<'a, Result<Schedule, String>> {
        Box::pin(async move {
            #[derive(Deserialize)]
            struct Response {
                schedule: Schedule,
            }
            let response: Response = self.typed("StopSchedule", json!({"id": id})).await?;
            Ok(response.schedule)
        })
    }

    fn start_timer<'a>(
        &'a self,
        label: &'a str,
        seconds: i64,
        idempotency_key: &'a str,
    ) -> BoxFuture<'a, Result<Timer, String>> {
        Box::pin(async move {
            #[derive(Deserialize)]
            struct Response {
                timer: Timer,
            }
            let response: Response = self
                .typed(
                    "StartTimer",
                    json!({
                        "label": label,
                        "durationSeconds": seconds.to_string(),
                        "idempotencyKey": idempotency_key,
                    }),
                )
                .await?;
            Ok(response.timer)
        })
    }

    fn list_timers<'a>(
        &'a self,
        include_finished: bool,
    ) -> BoxFuture<'a, Result<Vec<Timer>, String>> {
        Box::pin(async move {
            #[derive(Deserialize)]
            struct Response {
                #[serde(default)]
                timers: Vec<Timer>,
            }
            let response: Response = self
                .typed("ListTimers", json!({"includeFinished": include_finished}))
                .await?;
            Ok(response.timers)
        })
    }

    fn cancel_timer<'a>(&'a self, id: &'a str) -> BoxFuture<'a, Result<Timer, String>> {
        Box::pin(async move {
            #[derive(Deserialize)]
            struct Response {
                timer: Timer,
            }
            let response: Response = self.typed("CancelTimer", json!({"id": id})).await?;
            Ok(response.timer)
        })
    }

    fn pause_timer<'a>(&'a self, id: &'a str) -> BoxFuture<'a, Result<Timer, String>> {
        Box::pin(async move {
            #[derive(Deserialize)]
            struct Response {
                timer: Timer,
            }
            let response: Response = self.typed("PauseTimer", json!({"id": id})).await?;
            Ok(response.timer)
        })
    }

    fn resume_timer<'a>(&'a self, id: &'a str) -> BoxFuture<'a, Result<Timer, String>> {
        Box::pin(async move {
            #[derive(Deserialize)]
            struct Response {
                timer: Timer,
            }
            let response: Response = self.typed("ResumeTimer", json!({"id": id})).await?;
            Ok(response.timer)
        })
    }
}

fn int64<'de, D: Deserializer<'de>>(deserializer: D) -> Result<i64, D::Error> {
    let value = Option::<Value>::deserialize(deserializer)?;
    Ok(match value {
        Some(Value::String(text)) => text.parse().unwrap_or(0),
        Some(Value::Number(number)) => number.as_i64().unwrap_or(0),
        _ => 0,
    })
}

fn bool_or_default<'de, D: Deserializer<'de>>(deserializer: D) -> Result<bool, D::Error> {
    let value = Option::<Value>::deserialize(deserializer)?;
    Ok(matches!(value, Some(Value::Bool(true))))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn connect_json_int64_and_bool_variants_decode() {
        let schedule: Schedule = serde_json::from_value(json!({
            "id": "s1",
            "label": "Работа",
            "kind": "SCHEDULE_KIND_ALARM",
            "repeat": "REPEAT_KIND_WEEKDAYS",
            "at": {"hour": 7, "minute": 30, "second": 0},
            "nextDueUnix": "1767339000",
            "revision": 3,
            "enabled": true
        }))
        .expect("schedule");
        assert_eq!(schedule.next_due_unix, 1_767_339_000);
        assert_eq!(schedule.revision, 3);
        assert!(schedule.enabled);
        assert_eq!(schedule.at.hour, 7);
    }

    #[test]
    fn missing_completed_response_fields_stay_default() {
        let timer: Timer = serde_json::from_value(json!({"id": "t1"})).expect("timer");
        assert_eq!(timer.id, "t1");
        assert_eq!(timer.duration_seconds, 0);
        assert!(!timer.label.contains("Bearer"));
    }

    #[test]
    fn errors_never_include_the_request_token() {
        let error = connect_error("CreateSchedule", 401, &json!({"code": "unauthenticated"}));
        assert!(error.contains("unauthenticated"));
        assert!(!error.contains("Bearer"));
    }
}
