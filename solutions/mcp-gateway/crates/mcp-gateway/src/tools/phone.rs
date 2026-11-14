//! `phone` tool group: find a phone, send it a notification, list the phones.
//!
//! `phone_find` answers «где телефон» / «найди телефон» with a data-only
//! high-priority FCM message: the app rings like an alarm even when the screen
//! is off, Do Not Disturb is on or the app was killed, and stops on its own
//! after a minute or when the owner presses «Нашёл».
//!
//! `phone_notify(title, body)` is the ordinary channel: a normal notification
//! with a title, a body and an optional deep link, rendered from the app's own
//! resources.
//!
//! `phone_list` shows the registered phones. A family can have several: the
//! owner says a name («Стас», «телефон Стаса», «Стасику»), Hermes forwards it
//! verbatim, and this module matches it deterministically against device names.
//! A single phone is rung without a question; with several phones and no
//! unambiguous name the tool returns the names so Hermes asks exactly one
//! question. The push service never exposes an FCM token.

use std::sync::Arc;

use serde::Serialize;
use serde_json::{Value, json};

use crate::push::{Delivery, Device};
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
/// Longest spoken device name forwarded to the registry.
pub const MAX_NAME_CHARS: usize = 64;

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
                        },
                        "name": {
                            "type": "string",
                            "description": "Имя телефона так, как его назвал владелец, дословно: «Стас», «Стаса», «телефон Стаса». Шлюз сам сопоставит падеж и уменьшительное. Не передавай, если имя не названо."
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
                        "deep_link": {"type": "string", "description": "Необязательный внутренний адрес экрана приложения, например home://alarms. Только из явной просьбы владельца."},
                        "name": {
                            "type": "string",
                            "description": "Имя телефона дословно, как назвал владелец: «Стас», «телефон Стаса». Не передавай, если имя не названо."
                        }
                    },
                    "required": ["title", "body"],
                }),
                annotations: Annotations::destructive(),
            },
            ToolSpec {
                name: "phone_list",
                description: PHONE_LIST_DESCRIPTION,
                input_schema: json!({
                    "type": "object",
                    "additionalProperties": false,
                    "properties": {},
                }),
                annotations: Annotations::read_only(),
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
                "phone_list" => Ok(phone_list(&self.deps).await),
                other => Err(format!("unknown tool {other:?}")),
            }
        })
    }
}

/// What the owner asked for: exactly one phone, or a question with names.
enum Choice<'a> {
    One(&'a Device),
    Ask(Vec<&'a Device>),
}

#[derive(Debug, Serialize)]
struct DeviceDetail {
    id: String,
    name: String,
}

impl From<&Device> for DeviceDetail {
    fn from(device: &Device) -> Self {
        Self {
            id: device.id.clone(),
            name: display_name(device),
        }
    }
}

#[derive(Debug, Serialize)]
struct DeliveryDetail {
    device: DeviceDetail,
    sent: usize,
    failed: usize,
    devices: usize,
}

impl DeliveryDetail {
    fn new(device: &Device, delivery: Delivery) -> Self {
        Self {
            device: DeviceDetail::from(device),
            sent: delivery.sent,
            failed: delivery.failed,
            devices: delivery.devices,
        }
    }
}

#[derive(Debug, Serialize)]
struct AskDetail {
    devices: Vec<DeviceDetail>,
    need_name: bool,
}

impl AskDetail {
    fn new(devices: &[&Device]) -> Self {
        Self {
            devices: devices
                .iter()
                .map(|device| DeviceDetail::from(*device))
                .collect(),
            need_name: true,
        }
    }
}

#[derive(Debug, Serialize)]
struct ListDetail {
    devices: Vec<DeviceDetail>,
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
    let Some(name) = requested_name(&arguments) else {
        return failure(SPEECH_NAME_TOO_LONG, None::<Value>);
    };
    let Ok(devices) = push.devices().await else {
        deps.audit("phone.find", "phone", "failure");
        return failure(SPEECH_PUSH_FAILED, None::<Value>);
    };
    let device = match choose(&devices, name) {
        Choice::One(device) => device,
        Choice::Ask(candidates) => {
            deps.audit("phone.find", "phone", "needs_name");
            return respond(
                ask_speech(&candidates, name),
                Some(AskDetail::new(&candidates)),
            );
        }
    };
    let Ok(delivery) = push.find(reason, Some(device.id.as_str())).await else {
        deps.audit("phone.find", "phone", "failure");
        return failure(SPEECH_PUSH_FAILED, None::<Value>);
    };
    let outcome = if delivery.sent > 0 {
        "success"
    } else {
        "failure"
    };
    deps.audit("phone.find", "phone", outcome);
    respond(
        find_speech(device, delivery),
        Some(DeliveryDetail::new(device, delivery)),
    )
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
    let Some(name) = requested_name(&arguments) else {
        return failure(SPEECH_NAME_TOO_LONG, None::<Value>);
    };
    let Ok(devices) = push.devices().await else {
        deps.audit("phone.notify", "phone", "failure");
        return failure(SPEECH_PUSH_FAILED, None::<Value>);
    };
    let device = match choose(&devices, name) {
        Choice::One(device) => device,
        Choice::Ask(candidates) => {
            deps.audit("phone.notify", "phone", "needs_name");
            return respond(
                ask_speech(&candidates, name),
                Some(AskDetail::new(&candidates)),
            );
        }
    };
    let Ok(delivery) = push
        .notify(title, body, deep_link, Some(device.id.as_str()))
        .await
    else {
        deps.audit("phone.notify", "phone", "failure");
        return failure(SPEECH_PUSH_FAILED, None::<Value>);
    };
    let outcome = if delivery.sent > 0 {
        "success"
    } else {
        "failure"
    };
    deps.audit("phone.notify", "phone", outcome);
    respond(
        notify_speech(device, delivery),
        Some(DeliveryDetail::new(device, delivery)),
    )
}

async fn phone_list(deps: &Deps) -> ToolOutcome {
    let Some(push) = deps.push() else {
        return failure(SPEECH_NOT_CONFIGURED, None::<Value>);
    };
    let Ok(devices) = push.devices().await else {
        deps.audit("phone.list", "phone", "failure");
        return failure(SPEECH_PUSH_FAILED, None::<Value>);
    };
    deps.audit("phone.list", "phone", "success");
    let detail = ListDetail {
        devices: devices.iter().map(DeviceDetail::from).collect(),
    };
    respond(list_speech(&devices), Some(detail))
}

/// The optional name as Hermes heard it, trimmed. `None` means it was too long.
fn requested_name(arguments: &Value) -> Option<&str> {
    let name = arguments
        .get("name")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .trim();
    if name.chars().count() > MAX_NAME_CHARS {
        return None;
    }
    Some(name)
}

/// Deterministic resolution: one phone never needs a name, several phones need
/// an unambiguous match. Unmatched or ambiguous input returns the candidates so
/// Hermes can ask one question instead of guessing.
fn choose<'a>(devices: &'a [Device], name: &str) -> Choice<'a> {
    if devices.len() == 1 {
        return Choice::One(&devices[0]);
    }
    let query = tokens(name);
    if query.is_empty() {
        return Choice::Ask(devices.iter().collect());
    }
    let matched: Vec<&Device> = devices
        .iter()
        .filter(|device| matches_name(&query, &device.name))
        .collect();
    match matched.len() {
        1 => Choice::One(matched[0]),
        _ => Choice::Ask(if matched.is_empty() {
            devices.iter().collect()
        } else {
            matched
        }),
    }
}

fn find_speech(device: &Device, delivery: Delivery) -> String {
    let name = display_name(device);
    if delivery.sent > 0 {
        return format!("Звоню на телефон «{name}».");
    }
    if delivery.devices == 0 {
        return SPEECH_NO_PHONE.to_owned();
    }
    format!("Не удалось достучаться до телефона «{name}». Попробуй ещё раз.")
}

fn notify_speech(device: &Device, delivery: Delivery) -> String {
    let name = display_name(device);
    if delivery.sent > 0 {
        return format!("Отправил уведомление на телефон «{name}».");
    }
    if delivery.devices == 0 {
        return SPEECH_NO_PHONE.to_owned();
    }
    format!("Не удалось доставить уведомление на телефон «{name}».")
}

fn list_speech(devices: &[Device]) -> String {
    match devices.len() {
        0 => SPEECH_NO_PHONE.to_owned(),
        1 => format!("Зарегистрирован телефон «{}».", display_name(&devices[0])),
        _ => format!(
            "Телефоны: {}. Чтобы позвонить или написать, назови имя.",
            quoted_names(&devices.iter().collect::<Vec<_>>())
        ),
    }
}

fn ask_speech(candidates: &[&Device], name: &str) -> String {
    if candidates.is_empty() {
        return SPEECH_NO_PHONE.to_owned();
    }
    let names = quoted_names(candidates);
    if name.is_empty() {
        return format!("Телефонов несколько: {names}. На какой звонить?");
    }
    let matched = candidates
        .iter()
        .any(|device| matches_name(&tokens(name), &device.name));
    if matched {
        return format!("Под это имя подходит несколько телефонов: {names}. Какой выбрать?");
    }
    format!("Не нашёл телефон «{name}». Есть: {names}. На какой звонить?")
}

fn quoted_names(devices: &[&Device]) -> String {
    devices
        .iter()
        .map(|device| format!("«{}»", display_name(device)))
        .collect::<Vec<_>>()
        .join(", ")
}

fn display_name(device: &Device) -> String {
    let name = device.name.trim();
    if name.is_empty() {
        "Без имени".to_owned()
    } else {
        name.to_owned()
    }
}

/// Names that refer to the handset itself rather than its owner. They are
/// dropped before matching, so «телефон Стаса» matches the phone named «Стас».
const PHONE_WORDS: &[&str] = &[
    "телефон",
    "телефона",
    "телефону",
    "телефоном",
    "телефоне",
    "телефоны",
    "телефонов",
    "мобильник",
    "мобильника",
    "мобильнику",
    "мобильником",
    "мобильнике",
    "мобильники",
    "мобильный",
    "мобильного",
    "мобильному",
    "мобильным",
    "сотовый",
    "сотового",
    "сотовому",
    "аппарат",
    "аппарата",
    "трубка",
    "трубку",
    "трубки",
    "на",
    "к",
    "у",
    "для",
    "это",
    "мой",
    "моего",
    "моему",
    "моя",
    "мою",
];

/// Russian case endings, longest first, so «Стаса»/«Стасу»/«Стасом» share the
/// stem «стас» with the nominative «Стас».
const CASE_ENDINGS: &[&str] = &[
    "ями", "ами", "его", "ого", "ему", "ому", "ими", "ыми", "ей", "ой", "ый", "ий", "ая", "яя",
    "ое", "ее", "ые", "ие", "ов", "ев", "ах", "ях", "ом", "ем", "ам", "ям", "ью", "ию", "а", "я",
    "у", "ю", "е", "и", "ы", "о", "ь",
];

/// Diminutive suffixes: «Стасик», «Машенька», «Сашенька» reduce to the same
/// stem as «Стас», «Маша», «Саша». A strip that would leave fewer than three
/// characters is skipped, so «Вика» never collapses to «в».
const DIMINUTIVES: &[&str] = &[
    "еньк", "оньк", "ечк", "очк", "ик", "чик", "ок", "ек", "ша", "ня", "уля", "юля", "уня", "юша",
];

/// Splits a spoken name into lowercase meaningful words.
fn tokens(value: &str) -> Vec<String> {
    value
        .to_lowercase()
        .replace('ё', "е")
        .split(|character: char| !character.is_alphabetic())
        .filter(|word| !word.is_empty() && !PHONE_WORDS.contains(word))
        .map(str::to_owned)
        .collect()
}

/// Whether one spoken name refers to one device name.
fn matches_name(query: &[String], device_name: &str) -> bool {
    let device_tokens = tokens(device_name);
    query.iter().any(|spoken| {
        let spoken = stem(spoken);
        device_tokens
            .iter()
            .any(|stored| same_stem(&spoken, &stem(stored)))
    })
}

fn same_stem(left: &str, right: &str) -> bool {
    if left == right {
        return true;
    }
    let short = left.chars().count().min(right.chars().count());
    short >= 2 && (left.starts_with(right) || right.starts_with(left))
}

fn stem(word: &str) -> String {
    let without_case = strip(word, CASE_ENDINGS, 2);
    strip(&without_case, DIMINUTIVES, 3)
}

fn strip(word: &str, suffixes: &[&str], keep: usize) -> String {
    for suffix in suffixes {
        if let Some(remainder) = word.strip_suffix(suffix)
            && remainder.chars().count() >= keep
        {
            return remainder.to_owned();
        }
    }
    word.to_owned()
}

const SPEECH_NOT_CONFIGURED: &str = "Уведомления на телефон не настроены.";
const SPEECH_PUSH_FAILED: &str = "Не удалось отправить на телефон. Попробуй ещё раз.";
const SPEECH_NO_PHONE: &str = "Телефон не зарегистрирован. Открой приложение на телефоне.";
const SPEECH_NAME_TOO_LONG: &str = "Имя телефона слишком длинное. Назови имя короче.";

const PHONE_FIND_DESCRIPTION: &str = "\
Найти телефон: громко звонит как будильник, в обход беззвучного режима и режима «Не беспокоить», \
даже если экран выключен или приложение закрыто. \
Вызывай на «где телефон», «найди телефон», «позвони на телефон», «где мой мобильник». \
reason — необязательная короткая фраза для экрана телефона, не текст команды. \
Если владелец назвал телефон, передай name дословно, как услышал: «Стас», «Стаса», «телефон Стаса»; \
падеж и уменьшительное сопоставит шлюз. Если телефонов несколько и имя не названо или не совпало, \
инструмент вернёт список имён — задай один короткий вопрос. \
Звонок останавливается кнопкой «Нашёл» и сам через минуту.";

const PHONE_NOTIFY_DESCRIPTION: &str = "\
Отправить обычное уведомление на телефон: title и body обязательны, deep_link — только по явной \
просьбе владельца (например home://alarms). name — имя телефона дословно, если владелец его назвал; \
при нескольких телефонах без имени инструмент вернёт список имён и нужно задать один короткий вопрос. \
Используй, когда просят «напомни на телефон», «пришли уведомление», «напиши на телефон». \
Это не звонок: чтобы найти телефон, используй phone_find.";

const PHONE_LIST_DESCRIPTION: &str = "\
Список телефонов дома: имя, заданное в приложении, и идентификатор. \
Используй на «какие телефоны подключены», «сколько телефонов», «чей телефон зарегистрирован» \
и когда непонятно, на какой телефон звонить или писать. Никогда не выдумывай имена: бери их из ответа.";

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
        devices: Mutex<Vec<Device>>,
        fail: Mutex<bool>,
    }

    impl FakePush {
        fn with_devices(names: &[(&str, &str)]) -> Self {
            let fake = Self::default();
            *fake.devices.lock().expect("lock") = names
                .iter()
                .map(|(id, name)| Device {
                    id: (*id).to_owned(),
                    name: (*name).to_owned(),
                    platform: "android".to_owned(),
                })
                .collect();
            fake
        }
    }

    impl PushApi for FakePush {
        fn devices<'a>(&'a self) -> BoxFuture<'a, Result<Vec<Device>, String>> {
            Box::pin(async move {
                if *self.fail.lock().expect("lock") {
                    return Err("down".to_owned());
                }
                self.calls.lock().expect("lock").push("devices".to_owned());
                Ok(self.devices.lock().expect("lock").clone())
            })
        }

        fn find<'a>(
            &'a self,
            reason: &'a str,
            device_id: Option<&'a str>,
        ) -> BoxFuture<'a, Result<Delivery, String>> {
            Box::pin(async move {
                self.calls
                    .lock()
                    .expect("lock")
                    .push(format!("find:{reason}:{}", device_id.unwrap_or("-")));
                if *self.fail.lock().expect("lock") {
                    return Err("down".to_owned());
                }
                Ok(Delivery {
                    sent: 1,
                    failed: 0,
                    devices: 1,
                })
            })
        }

        fn notify<'a>(
            &'a self,
            title: &'a str,
            body: &'a str,
            deep_link: Option<&'a str>,
            device_id: Option<&'a str>,
        ) -> BoxFuture<'a, Result<Delivery, String>> {
            Box::pin(async move {
                self.calls.lock().expect("lock").push(format!(
                    "notify:{title}:{body}:{}:{}",
                    deep_link.unwrap_or("-"),
                    device_id.unwrap_or("-")
                ));
                Ok(Delivery {
                    sent: 1,
                    failed: 0,
                    devices: 1,
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

    fn fixture(devices: &[(&str, &str)]) -> (PhoneGroup, Arc<FakePush>) {
        let fake = Arc::new(FakePush::with_devices(devices));
        (PhoneGroup::new(deps(Some(Arc::clone(&fake)))), fake)
    }

    /// `Arc<FakePush>` does not implement `PushApi`; keep the fake behind one
    /// shared handle so a test can inspect the calls.
    struct Shared(Arc<FakePush>);

    impl PushApi for Shared {
        fn devices<'a>(&'a self) -> BoxFuture<'a, Result<Vec<Device>, String>> {
            self.0.devices()
        }

        fn find<'a>(
            &'a self,
            reason: &'a str,
            device_id: Option<&'a str>,
        ) -> BoxFuture<'a, Result<Delivery, String>> {
            self.0.find(reason, device_id)
        }

        fn notify<'a>(
            &'a self,
            title: &'a str,
            body: &'a str,
            deep_link: Option<&'a str>,
            device_id: Option<&'a str>,
        ) -> BoxFuture<'a, Result<Delivery, String>> {
            self.0.notify(title, body, deep_link, device_id)
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

    fn calls(fake: &FakePush) -> Vec<String> {
        fake.calls.lock().expect("lock").clone()
    }

    #[test]
    fn the_group_exposes_find_notify_and_list() {
        let group = PhoneGroup::new(deps(None));
        let tools = group.tools();
        assert_eq!(
            tools.iter().map(|tool| tool.name).collect::<Vec<_>>(),
            ["phone_find", "phone_notify", "phone_list"]
        );
        assert!(tools.iter().all(|tool| {
            !tool.description.is_empty()
                && tool.input_schema["type"] == "object"
                && tool.input_schema["additionalProperties"] == false
        }));
        assert!(tools[0..2].iter().all(|tool| !tool.annotations.read_only));
        assert!(tools[2].annotations.read_only);
        let notify = &tools[1];
        assert_eq!(notify.input_schema["required"], json!(["title", "body"]));
    }

    #[test]
    fn find_speaks_that_it_is_ringing() {
        let (group, fake) = fixture(&[("device-a", "Стас")]);
        let outcome = call(&group, "phone_find", json!({"reason": "Рикардо"}));
        assert_eq!(outcome.speech, "Звоню на телефон «Стас».");
        assert_eq!(calls(&fake), ["devices", "find:Рикардо:device-a"]);
    }

    #[test]
    fn a_single_phone_rings_without_a_name_even_when_a_name_was_said() {
        let (group, fake) = fixture(&[("device-a", "Стас")]);
        let outcome = call(&group, "phone_find", json!({"name": "Аня"}));
        assert_eq!(outcome.speech, "Звоню на телефон «Стас».");
        assert_eq!(calls(&fake), ["devices", "find::device-a"]);
    }

    #[test]
    fn find_without_a_registered_phone_says_so() {
        let (group, _fake) = fixture(&[]);
        let outcome = call(&group, "phone_find", json!({}));
        assert_eq!(
            outcome.speech,
            "Телефон не зарегистрирован. Открой приложение на телефоне."
        );
    }

    #[test]
    fn several_phones_without_a_name_ask_one_question_with_the_names() {
        let (group, fake) = fixture(&[("device-a", "Стас"), ("device-b", "Аня")]);
        let outcome = call(&group, "phone_find", json!({}));
        assert_eq!(
            outcome.speech,
            "Телефонов несколько: «Стас», «Аня». На какой звонить?"
        );
        assert!(calls(&fake).contains(&"devices".to_owned()));
        assert!(!calls(&fake).iter().any(|call| call.starts_with("find:")));
    }

    #[test]
    fn an_unmatched_name_returns_the_registered_names() {
        let (group, fake) = fixture(&[("device-a", "Стас"), ("device-b", "Аня")]);
        let outcome = call(&group, "phone_find", json!({"name": "Петя"}));
        assert_eq!(
            outcome.speech,
            "Не нашёл телефон «Петя». Есть: «Стас», «Аня». На какой звонить?"
        );
        assert!(!calls(&fake).iter().any(|call| call.starts_with("find:")));
    }

    #[test]
    fn russian_cases_and_possessives_match_the_phone_name() {
        for name in [
            "Стас",
            "Стаса",
            "Стасу",
            "Стасом",
            "телефон Стаса",
            "на телефон Стаса",
        ] {
            let (group, fake) = fixture(&[("device-a", "Стас"), ("device-b", "Аня")]);
            let outcome = call(&group, "phone_find", json!({"name": name}));
            assert_eq!(outcome.speech, "Звоню на телефон «Стас».", "{name}");
            assert_eq!(calls(&fake), ["devices", "find::device-a"], "{name}");
        }
    }

    #[test]
    fn diminutives_match_the_same_stem() {
        for name in ["Стасик", "Стасика", "Машенька", "Маши"] {
            let (group, _fake) = fixture(&[("device-a", "Стас"), ("device-b", "Маша")]);
            let outcome = call(&group, "phone_find", json!({"name": name}));
            let expected = if name.starts_with("Маш") {
                "Звоню на телефон «Маша»."
            } else {
                "Звоню на телефон «Стас»."
            };
            assert_eq!(outcome.speech, expected, "{name}");
        }
    }

    #[test]
    fn two_matching_names_ask_which_one() {
        let (group, _fake) = fixture(&[("device-a", "Стас"), ("device-b", "Стася")]);
        let outcome = call(&group, "phone_find", json!({"name": "Стас"}));
        assert!(
            outcome.speech.contains("несколько телефонов"),
            "{}",
            outcome.speech
        );
        assert!(outcome.speech.contains("«Стас»"));
        assert!(outcome.speech.contains("«Стася»"));
    }

    #[test]
    fn phone_list_names_every_registered_phone() {
        let (group, _fake) = fixture(&[("device-a", "Стас"), ("device-b", "Аня")]);
        let outcome = call(&group, "phone_list", json!({}));
        assert_eq!(
            outcome.speech,
            "Телефоны: «Стас», «Аня». Чтобы позвонить или написать, назови имя."
        );
        let details = outcome.details.clone().unwrap_or_default();
        assert!(
            details.contains("«Стас»") || details.contains("Стас"),
            "{details}"
        );
        assert!(!details.contains("token"), "{details}");
        let (group, _fake) = fixture(&[("device-a", "Стас")]);
        assert_eq!(
            call(&group, "phone_list", json!({})).speech,
            "Зарегистрирован телефон «Стас»."
        );
    }

    #[test]
    fn notify_requires_a_title_and_a_body() {
        let (group, _fake) = fixture(&[("device-a", "Стас")]);
        let outcome = call(&group, "phone_notify", json!({"title": "Дом"}));
        assert!(outcome.speech.contains("заголовок"), "{}", outcome.speech);
    }

    #[test]
    fn notify_uses_the_named_phone_and_passes_the_deep_link() {
        let (group, fake) = fixture(&[("device-a", "Стас"), ("device-b", "Аня")]);
        let outcome = call(
            &group,
            "phone_notify",
            json!({
                "title": "Дом",
                "body": "Дверь",
                "deep_link": "home://alarms",
                "name": "Ане"
            }),
        );
        assert_eq!(outcome.speech, "Отправил уведомление на телефон «Аня».");
        assert_eq!(
            calls(&fake),
            ["devices", "notify:Дом:Дверь:home://alarms:device-b"]
        );
    }

    #[test]
    fn notify_with_several_phones_and_no_name_asks_first() {
        let (group, fake) = fixture(&[("device-a", "Стас"), ("device-b", "Аня")]);
        let outcome = call(
            &group,
            "phone_notify",
            json!({"title": "Дом", "body": "Дверь"}),
        );
        assert_eq!(
            outcome.speech,
            "Телефонов несколько: «Стас», «Аня». На какой звонить?"
        );
        assert!(!calls(&fake).iter().any(|call| call.starts_with("notify:")));
    }
}
