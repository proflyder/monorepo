//! `phone` tool group: find the phone and send it a notification.
//!
//! `phone_find` answers «где телефон» / «найди телефон» with a data-only
//! high-priority FCM message: the app rings like an alarm even when the screen
//! is off, Do Not Disturb is on or the app was killed, and stops on its own
//! after a minute or when the owner presses «Нашёл».
//!
//! `phone_notify(title, body)` is the ordinary channel: a normal notification
//! with a title, a body and an optional deep link, rendered from the app's own
//! resources.

use std::sync::Arc;

use serde::Serialize;
use serde_json::{Value, json};

use crate::push::Delivery;
use crate::tools::{
    Annotations, BoxFuture, Deps, ToolGroup, ToolOutcome, ToolSpec, failure, respond,
};

/// Longest accepted reason (the phone speaks it, so it stays short).
pub const MAX_REASON_CHARS: usize = 200;
/// Longest notification title.
pub const MAX_TITLE_CHARS: usize = 120;
/// Longest notification body.
pub const MAX_BODY_CHARS: usize = 500;
/// Longest deep link accepted from the model.
pub const MAX_DEEP_LINK_CHARS: usize = 512;

/// The `phone` group.
pub struct PhoneGroup {
    deps: Arc<Deps>,
}

impl PhoneGroup {
    /// Build the group.
    #[must_use]
    pub fn new(deps: Arc<Deps>) -> Self {
        Self { deps }
    }
}

impl ToolGroup for PhoneGroup {
    fn name(&self) -> &'static str {
        "phone"
    }

    fn tools(&self) -> Vec<ToolSpec> {
        vec![
            ToolSpec {
                name: "phone_find",
                description: PHONE_FIND_DESCRIPTION,
                input_schema: json!({
                    "type": "object",
                    "additionalProperties": false,
                    "properties": {
                        "reason": {
                            "type": "string",
                            "description": "Необязательная короткая причина, которую телефон покажет на экране, например «Рикардо». Не передавай сюда текст команды."
                        }
                    },
                }),
                annotations: Annotations::destructive(),
            },
            ToolSpec {
                name: "phone_notify",
                description: PHONE_NOTIFY_DESCRIPTION,
                input_schema: json!({
                    "type": "object",
                    "additionalProperties": false,
                    "properties": {
                        "title": {"type": "string", "description": "Заголовок уведомления, короткий, до 120 символов."},
                        "body": {"type": "string", "description": "Текст уведомления, до 500 символов."},
                        "deep_link": {"type": "string", "description": "Необязательный внутренний адрес экрана приложения, например home://alarms. Только из явной просьбы владельца."}
                    },
                    "required": ["title", "body"],
                }),
                annotations: Annotations::destructive(),
            },
        ]
    }

    fn call<'a>(
        &'a self,
        name: &'a str,
        arguments: Value,
    ) -> BoxFuture<'a, Result<ToolOutcome, String>> {
        Box::pin(async move {
            match name {
                "phone_find" => Ok(phone_find(&self.deps, arguments).await),
                "phone_notify" => Ok(phone_notify(&self.deps, arguments).await),
                other => Err(format!("unknown tool {other:?}")),
            }
        })
    }
}

#[derive(Debug, Serialize)]
struct DeliveryDetail {
    sent: usize,
    failed: usize,
    devices: usize,
}

impl From<Delivery> for DeliveryDetail {
    fn from(delivery: Delivery) -> Self {
        Self {
            sent: delivery.sent,
            failed: delivery.failed,
            devices: delivery.devices,
        }
    }
}

async fn phone_find(deps: &Deps, arguments: Value) -> ToolOutcome {
    let Some(push) = deps.push() else {
        return failure(SPEECH_NOT_CONFIGURED, None::<Value>);
    };
    let reason = arguments
        .get("reason")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .trim();
    if reason.chars().count() > MAX_REASON_CHARS {
        return failure(
            "Причина слишком длинная. Скажи короче, до двухсот символов.",
            None::<Value>,
        );
    }
    let Ok(delivery) = push.find(reason).await else {
        deps.audit("phone.find", "phone", "failure");
        return failure(SPEECH_PUSH_FAILED, None::<Value>);
    };
    let outcome = if delivery.sent > 0 {
        "success"
    } else {
        "failure"
    };
    deps.audit("phone.find", "phone", outcome);
    respond(find_speech(delivery), Some(DeliveryDetail::from(delivery)))
}

async fn phone_notify(deps: &Deps, arguments: Value) -> ToolOutcome {
    let Some(push) = deps.push() else {
        return failure(SPEECH_NOT_CONFIGURED, None::<Value>);
    };
    let title = arguments
        .get("title")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .trim();
    let body = arguments
        .get("body")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .trim();
    if title.is_empty() || body.is_empty() {
        return failure(
            "Нужны заголовок и текст уведомления. Назови оба.",
            None::<Value>,
        );
    }
    if title.chars().count() > MAX_TITLE_CHARS || body.chars().count() > MAX_BODY_CHARS {
        return failure("Уведомление слишком длинное. Скажи короче.", None::<Value>);
    }
    let deep_link = arguments
        .get("deep_link")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty());
    if deep_link.is_some_and(|value| value.chars().count() > MAX_DEEP_LINK_CHARS) {
        return failure("Ссылка на экран слишком длинная.", None::<Value>);
    }
    let Ok(delivery) = push.notify(title, body, deep_link).await else {
        deps.audit("phone.notify", "phone", "failure");
        return failure(SPEECH_PUSH_FAILED, None::<Value>);
    };
    let outcome = if delivery.sent > 0 {
        "success"
    } else {
        "failure"
    };
    deps.audit("phone.notify", "phone", outcome);
    let speech = if delivery.sent == 0 {
        SPEECH_NO_PHONE.to_owned()
    } else {
        "Отправил уведомление на телефон.".to_owned()
    };
    respond(speech, Some(DeliveryDetail::from(delivery)))
}

fn find_speech(delivery: Delivery) -> String {
    if delivery.sent > 0 {
        return "Звоню на телефон.".to_owned();
    }
    if delivery.devices == 0 {
        return SPEECH_NO_PHONE.to_owned();
    }
    "Не удалось достучаться до телефона. Попробуй ещё раз.".to_owned()
}

const SPEECH_NOT_CONFIGURED: &str = "Уведомления на телефон не настроены.";
const SPEECH_PUSH_FAILED: &str = "Не удалось отправить на телефон. Попробуй ещё раз.";
const SPEECH_NO_PHONE: &str = "Телефон не зарегистрирован. Открой приложение на телефоне.";

const PHONE_FIND_DESCRIPTION: &str = "\
Найти телефон: громко звонит как будильник, в обход беззвучного режима и режима «Не беспокоить», \
даже если экран выключен или приложение закрыто. \
Вызывай на «где телефон», «найди телефон», «позвони на телефон», «где мой мобильник». \
reason — необязательная короткая фраза для экрана телефона, не текст команды. \
Звонок останавливается кнопкой «Нашёл» и сам через минуту.";

const PHONE_NOTIFY_DESCRIPTION: &str = "\
Отправить обычное уведомление на телефон: title и body обязательны, deep_link — только по явной \
просьбе владельца (например home://alarms). \
Используй, когда просят «напомни на телефон», «пришли уведомление», «напиши на телефон». \
Это не звонок: чтобы найти телефон, используй phone_find.";

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::clock::Clock;
    use crate::domain::state::{AgentMode, KeyedStore, PendingStore};
    use crate::metrics::Metrics;
    use crate::push::PushApi;
    use std::sync::Mutex;
    use std::time::Duration;

    struct FixedClock;

    impl Clock for FixedClock {
        fn now(&self) -> jiff::Timestamp {
            jiff::Timestamp::from_second(1_700_000_000).expect("instant")
        }
    }

    #[derive(Default)]
    struct FakePush {
        calls: Mutex<Vec<String>>,
        devices: Mutex<usize>,
        fail: Mutex<bool>,
    }

    impl PushApi for FakePush {
        fn find<'a>(&'a self, reason: &'a str) -> BoxFuture<'a, Result<Delivery, String>> {
            Box::pin(async move {
                self.calls
                    .lock()
                    .expect("lock")
                    .push(format!("find:{reason}"));
                if *self.fail.lock().expect("lock") {
                    return Err("down".to_owned());
                }
                let devices = *self.devices.lock().expect("lock");
                Ok(Delivery {
                    sent: devices,
                    failed: 0,
                    devices,
                })
            })
        }

        fn notify<'a>(
            &'a self,
            title: &'a str,
            body: &'a str,
            deep_link: Option<&'a str>,
        ) -> BoxFuture<'a, Result<Delivery, String>> {
            Box::pin(async move {
                self.calls.lock().expect("lock").push(format!(
                    "notify:{title}:{body}:{}",
                    deep_link.unwrap_or("-")
                ));
                let devices = *self.devices.lock().expect("lock");
                Ok(Delivery {
                    sent: devices,
                    failed: 0,
                    devices,
                })
            })
        }
    }

    fn deps(push: Option<Arc<FakePush>>) -> Arc<Deps> {
        let push: Option<Arc<dyn PushApi>> =
            push.map(|fake| Arc::new(Shared(fake)) as Arc<dyn PushApi>);
        Arc::new(Deps {
            catalog: None,
            prom: None,
            alerts: None,
            loki: None,
            kube: None,
            report: None,
            checks: None,
            pending: Arc::new(PendingStore::new(
                Duration::from_secs(90),
                Arc::new(FixedClock),
            )),
            timers: Arc::new(crate::tools::TimerScheduler),
            metrics: Arc::new(Metrics::new()),
            clock: Arc::new(FixedClock),
            location: jiff::tz::TimeZone::UTC,
            timezone_name: "UTC".to_owned(),
            scheduler: None,
            push,
            cooldown: Duration::from_secs(60),
            self_restart_delay: Duration::from_secs(1),
            query_timeout: Duration::from_secs(1),
            status_timeout: Duration::from_secs(1),
            report_timeout: Duration::from_secs(1),
            log_byte_cap: Arc::new(std::sync::atomic::AtomicUsize::new(1024)),
            bb: None,
            bb_targets: None,
            bb_pending: Arc::new(KeyedStore::new(
                Duration::from_secs(90),
                Arc::new(FixedClock),
            )),
            bb_mode: Arc::new(AgentMode::new()),
        })
    }

    fn fixture(devices: usize) -> (PhoneGroup, Arc<FakePush>) {
        let fake = Arc::new(FakePush::default());
        *fake.devices.lock().expect("lock") = devices;
        (PhoneGroup::new(deps(Some(Arc::clone(&fake)))), fake)
    }

    /// `Arc<FakePush>` does not implement `PushApi`; keep the fake behind one
    /// shared handle so a test can inspect the calls.
    struct Shared(Arc<FakePush>);

    impl PushApi for Shared {
        fn find<'a>(&'a self, reason: &'a str) -> BoxFuture<'a, Result<Delivery, String>> {
            self.0.find(reason)
        }

        fn notify<'a>(
            &'a self,
            title: &'a str,
            body: &'a str,
            deep_link: Option<&'a str>,
        ) -> BoxFuture<'a, Result<Delivery, String>> {
            self.0.notify(title, body, deep_link)
        }
    }

    fn call(group: &PhoneGroup, name: &str, arguments: Value) -> ToolOutcome {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("runtime");
        runtime
            .block_on(group.call(name, arguments))
            .expect("tool call")
    }

    #[test]
    fn the_group_exposes_exactly_two_tools() {
        let group = PhoneGroup::new(deps(None));
        let tools = group.tools();
        assert_eq!(
            tools.iter().map(|tool| tool.name).collect::<Vec<_>>(),
            ["phone_find", "phone_notify"]
        );
        assert!(tools.iter().all(|tool| {
            !tool.description.is_empty()
                && tool.input_schema["type"] == "object"
                && tool.input_schema["additionalProperties"] == false
        }));
        assert!(tools.iter().all(|tool| !tool.annotations.read_only));
        let notify = &tools[1];
        assert_eq!(notify.input_schema["required"], json!(["title", "body"]));
    }

    #[test]
    fn find_speaks_that_it_is_ringing() {
        let (group, fake) = fixture(1);
        let outcome = call(&group, "phone_find", json!({"reason": "Рикардо"}));
        assert_eq!(outcome.speech, "Звоню на телефон.");
        let calls = fake.calls.lock().expect("lock").clone();
        assert_eq!(calls, ["find:Рикардо"]);
    }

    #[test]
    fn find_without_a_registered_phone_says_so() {
        let (group, _fake) = fixture(0);
        let outcome = call(&group, "phone_find", json!({}));
        assert_eq!(
            outcome.speech,
            "Телефон не зарегистрирован. Открой приложение на телефоне."
        );
    }

    #[test]
    fn notify_requires_a_title_and_a_body() {
        let (group, _fake) = fixture(1);
        let outcome = call(&group, "phone_notify", json!({"title": "Дом"}));
        assert!(outcome.speech.contains("заголовок"), "{}", outcome.speech);
    }

    #[test]
    fn notify_passes_the_deep_link_through() {
        let (group, fake) = fixture(1);
        let outcome = call(
            &group,
            "phone_notify",
            json!({"title": "Дом", "body": "Дверь", "deep_link": "home://alarms"}),
        );
        assert_eq!(outcome.speech, "Отправил уведомление на телефон.");
        let calls = fake.calls.lock().expect("lock").clone();
        assert_eq!(calls, ["notify:Дом:Дверь:home://alarms"]);
    }
}
