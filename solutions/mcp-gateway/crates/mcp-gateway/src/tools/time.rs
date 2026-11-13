//! `time` tool group: alarms, timers and reminders through the home scheduler.
//!
//! Hermes never parses spoken clock times into HA scripts: it asks `time_now`,
//! computes one absolute RFC3339 instant and calls `alarm_set` / `timer_set`.
//! The group talks to the durable `home.v1.SchedulerService`, exactly like the
//! HA voice scripts and the Android app, so the existing `ha-bridge` executor
//! still rings the station through `script.alarm_start`.

use std::sync::Arc;

use jiff::Timestamp;
use serde::Serialize;
use serde_json::{Value, json};

use crate::domain::time as time_domain;
use crate::scheduler::{Schedule, SchedulerApi};
use crate::tools::{
    Annotations, BoxFuture, Deps, ToolGroup, ToolOutcome, ToolSpec, empty_schema, failure, respond,
};

/// Scheduler kind for alarms.
pub const KIND_ALARM: &str = "SCHEDULE_KIND_ALARM";
/// Scheduler kind for reminders.
pub const KIND_REMINDER: &str = "SCHEDULE_KIND_REMINDER";
/// Hard scheduler limit for one timer.
pub const MAX_TIMER_SECONDS: i64 = 604_800;
/// Hard scheduler limit for a one-shot alarm horizon.
pub const MAX_ONCE_SECONDS: i64 = 7 * 24 * 3600;
/// Snooze duration used by the HA client.
pub const SNOOZE_MINUTES: i64 = 9;

/// The `time` group.
pub struct TimeGroup {
    deps: Arc<Deps>,
}

impl TimeGroup {
    /// Build the group.
    #[must_use]
    pub fn new(deps: Arc<Deps>) -> Self {
        Self { deps }
    }
}

impl ToolGroup for TimeGroup {
    fn name(&self) -> &'static str {
        "time"
    }

    fn tools(&self) -> Vec<ToolSpec> {
        vec![
            ToolSpec {
                name: "time_now",
                description: TIME_NOW_DESCRIPTION,
                input_schema: empty_schema(),
                annotations: Annotations::read_only(),
            },
            ToolSpec {
                name: "alarm_set",
                description: ALARM_SET_DESCRIPTION,
                input_schema: json!({
                    "type": "object",
                    "additionalProperties": false,
                    "properties": {
                        "at": {"type": "string", "description": "Абсолютное время RFC3339 с офсетом, например 2026-11-12T22:00:00+05:00. Не присылай локальное время без пояса."},
                        "label": {"type": "string", "description": "Необязательная короткая метка, названная владельцем (например, «Работа»). Не передавай сюда текст команды."},
                        "repeat": {"type": "string", "enum": ["once", "daily", "weekdays", "weekends"], "description": "once — один раз (по умолчанию), daily — каждый день, weekdays — по будням, weekends — по выходным."}
                    },
                    "required": ["at"],
                }),
                annotations: Annotations::destructive(),
            },
            ToolSpec {
                name: "alarm_list",
                description: ALARM_LIST_DESCRIPTION,
                input_schema: json!({
                    "type": "object",
                    "additionalProperties": false,
                    "properties": {
                        "include_disabled": {"type": "boolean", "description": "Показать и выключенные будильники. По умолчанию false."}
                    },
                }),
                annotations: Annotations::read_only(),
            },
            ToolSpec {
                name: "alarm_cancel",
                description: ALARM_CANCEL_DESCRIPTION,
                input_schema: json!({
                    "type": "object",
                    "additionalProperties": false,
                    "properties": {
                        "id": {"type": "string", "description": "id будильника из alarm_list."},
                        "all": {"type": "boolean", "description": "Выключить все будильники. Используй только при явном «все»."}
                    },
                }),
                annotations: Annotations::destructive(),
            },
            ToolSpec {
                name: "alarm_delete",
                description: ALARM_DELETE_DESCRIPTION,
                input_schema: json!({
                    "type": "object",
                    "additionalProperties": false,
                    "properties": {
                        "id": {"type": "string", "description": "id будильника из alarm_list."},
                        "all": {"type": "boolean", "description": "Удалить все будильники. Используй только при явном «все»."}
                    },
                }),
                annotations: Annotations::destructive(),
            },
            ToolSpec {
                name: "alarm_update",
                description: ALARM_UPDATE_DESCRIPTION,
                input_schema: json!({
                    "type": "object",
                    "additionalProperties": false,
                    "properties": {
                        "id": {"type": "string", "description": "id будильника из alarm_list."},
                        "at": {"type": "string", "description": "Новое абсолютное время RFC3339 с офсетом, например 2026-11-12T07:30:00+05:00."}
                    },
                    "required": ["id", "at"],
                }),
                annotations: Annotations::destructive(),
            },
            ToolSpec {
                name: "alarm_snooze",
                description: ALARM_SNOOZE_DESCRIPTION,
                input_schema: json!({
                    "type": "object",
                    "additionalProperties": false,
                    "properties": {
                        "id": {"type": "string", "description": "id звонящего будильника. Можно не указывать: инструмент сам найдёт активный звонок."}
                    },
                }),
                annotations: Annotations::destructive(),
            },
            ToolSpec {
                name: "alarm_stop",
                description: ALARM_STOP_DESCRIPTION,
                input_schema: json!({
                    "type": "object",
                    "additionalProperties": false,
                    "properties": {
                        "id": {"type": "string", "description": "id звонящего будильника. Можно не указывать: инструмент сам найдёт активный звонок."}
                    },
                }),
                annotations: Annotations::destructive(),
            },
            ToolSpec {
                name: "timer_set",
                description: TIMER_SET_DESCRIPTION,
                input_schema: json!({
                    "type": "object",
                    "additionalProperties": false,
                    "properties": {
                        "seconds": {"type": "integer", "minimum": 1, "maximum": 604800, "description": "Длительность таймера в секундах, от 1 до 604800 (7 дней). 10 минут — это 600."},
                        "label": {"type": "string", "description": "Необязательная короткая метка, например «Пицца»."}
                    },
                    "required": ["seconds"],
                }),
                annotations: Annotations::destructive(),
            },
            ToolSpec {
                name: "timer_list",
                description: TIMER_LIST_DESCRIPTION,
                input_schema: json!({
                    "type": "object",
                    "additionalProperties": false,
                    "properties": {
                        "include_finished": {"type": "boolean", "description": "Показать и завершённые таймеры. По умолчанию false."}
                    },
                }),
                annotations: Annotations::read_only(),
            },
            ToolSpec {
                name: "timer_cancel",
                description: TIMER_CANCEL_DESCRIPTION,
                input_schema: json!({
                    "type": "object",
                    "additionalProperties": false,
                    "properties": {
                        "id": {"type": "string", "description": "id таймера из timer_list."},
                        "all": {"type": "boolean", "description": "Отменить все активные таймеры. Используй только при явном «все»."}
                    },
                }),
                annotations: Annotations::destructive(),
            },
            ToolSpec {
                name: "timer_pause",
                description: TIMER_PAUSE_DESCRIPTION,
                input_schema: json!({
                    "type": "object",
                    "additionalProperties": false,
                    "properties": {
                        "id": {"type": "string", "description": "id таймера. Можно не указывать, если активен ровно один таймер."}
                    },
                }),
                annotations: Annotations::destructive(),
            },
            ToolSpec {
                name: "timer_resume",
                description: TIMER_RESUME_DESCRIPTION,
                input_schema: json!({
                    "type": "object",
                    "additionalProperties": false,
                    "properties": {
                        "id": {"type": "string", "description": "id таймера. Можно не указывать, если на паузе ровно один таймер."}
                    },
                }),
                annotations: Annotations::destructive(),
            },
            ToolSpec {
                name: "reminder_set",
                description: REMINDER_SET_DESCRIPTION,
                input_schema: json!({
                    "type": "object",
                    "additionalProperties": false,
                    "properties": {
                        "at": {"type": "string", "description": "Абсолютное время RFC3339 с офсетом, например 2026-11-13T09:00:00+05:00."},
                        "text": {"type": "string", "description": "О чём напомнить, коротко и по делу."}
                    },
                    "required": ["at", "text"],
                }),
                annotations: Annotations::destructive(),
            },
            ToolSpec {
                name: "reminder_list",
                description: REMINDER_LIST_DESCRIPTION,
                input_schema: json!({
                    "type": "object",
                    "additionalProperties": false,
                    "properties": {
                        "include_disabled": {"type": "boolean", "description": "Показать и выключенные напоминания. По умолчанию false."}
                    },
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
                "time_now" => Ok(time_now(&self.deps)),
                "alarm_set" => Ok(alarm_set(&self.deps, arguments).await),
                "alarm_list" => Ok(alarm_list(&self.deps, arguments).await),
                "alarm_cancel" => Ok(alarm_cancel(&self.deps, arguments).await),
                "alarm_delete" => Ok(alarm_delete(&self.deps, arguments).await),
                "alarm_update" => Ok(alarm_update(&self.deps, arguments).await),
                "alarm_snooze" => Ok(alarm_snooze_stop(&self.deps, arguments, true).await),
                "alarm_stop" => Ok(alarm_snooze_stop(&self.deps, arguments, false).await),
                "timer_set" => Ok(timer_set(&self.deps, arguments).await),
                "timer_list" => Ok(timer_list(&self.deps, arguments).await),
                "timer_cancel" => Ok(timer_cancel(&self.deps, arguments).await),
                "timer_pause" => Ok(timer_pause_resume(&self.deps, arguments, true).await),
                "timer_resume" => Ok(timer_pause_resume(&self.deps, arguments, false).await),
                "reminder_set" => Ok(reminder_set(&self.deps, arguments).await),
                "reminder_list" => Ok(reminder_list(&self.deps, arguments).await),
                other => Err(format!("unknown tool {other:?}")),
            }
        })
    }
}

// --- time_now -------------------------------------------------------------

#[derive(Debug, Serialize)]
struct NowDetail {
    rfc3339: String,
    human: String,
    timezone: String,
    unix: i64,
}

fn time_now(deps: &Deps) -> ToolOutcome {
    let now = deps.now();
    respond(
        time_domain::now_text(now, &deps.location, &deps.timezone_name),
        Some(NowDetail {
            rfc3339: time_domain::rfc3339_local(now, &deps.location),
            human: time_domain::now_text(now, &deps.location, &deps.timezone_name),
            timezone: deps.timezone_name.clone(),
            unix: now.as_second(),
        }),
    )
}

// --- alarms ---------------------------------------------------------------

#[derive(Debug, Serialize)]
struct ScheduleDetail {
    id: String,
    label: String,
    kind: String,
    local: String,
    repeat: String,
    enabled: bool,
}

fn schedule_detail(schedule: &Schedule, deps: &Deps) -> ScheduleDetail {
    ScheduleDetail {
        id: schedule.id.clone(),
        label: schedule.label.clone(),
        kind: schedule.kind.clone(),
        local: if schedule.next_due_unix > 0 {
            Timestamp::from_second(schedule.next_due_unix)
                .ok()
                .map_or_else(String::new, |instant| {
                    time_domain::human_instant(instant, &deps.location)
                })
        } else {
            String::new()
        },
        repeat: time_domain::repeat_human(&schedule.repeat).to_owned(),
        enabled: schedule.enabled,
    }
}

async fn alarm_set(deps: &Deps, arguments: Value) -> ToolOutcome {
    let Some(api) = deps.scheduler() else {
        return failure(SPEECH_SCHEDULER_MISSING, None::<Value>);
    };
    let Some(at) = arguments.get("at").and_then(Value::as_str) else {
        return failure(
            "Не указано время будильника. Назови дату и время.",
            None::<Value>,
        );
    };
    let repeat = arguments.get("repeat").and_then(Value::as_str);
    let now = deps.now();
    let target = match time_domain::alarm_target(at, repeat, now) {
        Ok(target) => target,
        Err(error) => return failure(error, None::<Value>),
    };
    let (wall, once_date) = time_domain::local_parts(target.instant, &deps.location);
    let label = match label(&arguments, "label", "") {
        Ok(label) => label,
        Err(error) => return failure(error, None::<Value>),
    };
    let label = if label.is_empty() {
        time_domain::default_alarm_label(target.instant, &deps.location)
    } else {
        label
    };
    let repeat_mode = repeat.unwrap_or("once");
    let request = json!({
        "label": label,
        "kind": KIND_ALARM,
        "timezone": deps.timezone_name,
        "at": {"hour": wall.hour, "minute": wall.minute, "second": wall.second},
        "onceDate": if repeat_mode == "once" { once_date } else { String::new() },
        "repeat": target.repeat,
        "enabled": true,
        "payloadJson": "{}",
        "idempotencyKey": idempotency_key("alarm", now),
    });
    let Ok(schedule) = api.create_schedule(request).await else {
        deps.audit("time.alarm_set", "alarm", "failure");
        return failure(SPEECH_SCHEDULER_FAILED, None::<Value>);
    };
    deps.audit("time.alarm_set", "alarm", "success");
    let speech = if repeat_mode == "once" || schedule.repeat == "REPEAT_KIND_ONCE" {
        format!(
            "Будильник на {}.",
            time_domain::human_instant(target.instant, &deps.location)
        )
    } else {
        let when = next_human(&schedule, deps)
            .unwrap_or_else(|| time_domain::clock_text(target.instant, &deps.location));
        format!(
            "Будильник на {when}, {}.",
            time_domain::repeat_human(&schedule.repeat)
        )
    };
    respond(speech, Some(schedule_detail(&schedule, deps)))
}

async fn alarm_list(deps: &Deps, arguments: Value) -> ToolOutcome {
    let Some(api) = deps.scheduler() else {
        return failure(SPEECH_SCHEDULER_MISSING, None::<Value>);
    };
    let include_disabled = arguments
        .get("include_disabled")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let Ok(schedules) = api.list_schedules(KIND_ALARM, include_disabled).await else {
        return failure(SPEECH_SCHEDULER_FAILED, None::<Value>);
    };
    let details: Vec<ScheduleDetail> = schedules.iter().map(|s| schedule_detail(s, deps)).collect();
    let speech = if schedules.is_empty() {
        "Будильников нет.".to_owned()
    } else {
        let parts: Vec<String> = schedules
            .iter()
            .take(4)
            .map(|schedule| {
                let when = next_human(schedule, deps).unwrap_or_else(|| "без времени".to_owned());
                let repeat = if schedule.repeat == "REPEAT_KIND_ONCE" {
                    String::new()
                } else {
                    format!(", {}", time_domain::repeat_human(&schedule.repeat))
                };
                let enabled = if schedule.enabled {
                    String::new()
                } else {
                    ", выключен".to_owned()
                };
                format!("{} на {when}{repeat}{enabled}", schedule.label)
            })
            .collect();
        let mut speech = format!("Будильники: {}.", parts.join("; "));
        if schedules.len() > parts.len() {
            speech.push_str(" И ещё есть.");
        }
        speech
    };
    respond(
        speech,
        Some(json!({"schedules": details, "total": schedules.len()})),
    )
}

async fn alarm_cancel(deps: &Deps, arguments: Value) -> ToolOutcome {
    let Some(api) = deps.scheduler() else {
        return failure(SPEECH_SCHEDULER_MISSING, None::<Value>);
    };
    let all = arguments
        .get("all")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    if all {
        let Ok(schedules) = api.list_schedules(KIND_ALARM, false).await else {
            return failure(SPEECH_SCHEDULER_FAILED, None::<Value>);
        };
        if schedules.is_empty() {
            return respond("Будильников нет.", Some(json!({"changed": 0})));
        }
        let mut changed = 0;
        for schedule in &schedules {
            if api.set_schedule_enabled(&schedule.id, false).await.is_ok() {
                changed += 1;
            }
        }
        if changed == 0 {
            return failure(SPEECH_SCHEDULER_FAILED, None::<Value>);
        }
        return respond(
            "Выключил все будильники.",
            Some(json!({"changed": changed})),
        );
    }
    let Some(id) = arguments.get("id").and_then(Value::as_str) else {
        return failure(
            "Не понял, какой будильник выключить. Назови метку или время.",
            None::<Value>,
        );
    };
    match api.set_schedule_enabled(id, false).await {
        Ok(schedule) => respond(
            format!("Выключил будильник «{}».", schedule.label),
            Some(schedule_detail(&schedule, deps)),
        ),
        Err(_) => failure(SPEECH_SCHEDULER_FAILED, None::<Value>),
    }
}

async fn alarm_delete(deps: &Deps, arguments: Value) -> ToolOutcome {
    let Some(api) = deps.scheduler() else {
        return failure(SPEECH_SCHEDULER_MISSING, None::<Value>);
    };
    let all = arguments
        .get("all")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    if all {
        let Ok(schedules) = api.list_schedules(KIND_ALARM, true).await else {
            return failure(SPEECH_SCHEDULER_FAILED, None::<Value>);
        };
        if schedules.is_empty() {
            return respond("Будильников нет.", Some(json!({"deleted": 0})));
        }
        let mut deleted = 0;
        for schedule in &schedules {
            if api.delete_schedule(&schedule.id).await.is_ok() {
                deleted += 1;
            }
        }
        if deleted == 0 {
            return failure(SPEECH_SCHEDULER_FAILED, None::<Value>);
        }
        return respond("Удалил все будильники.", Some(json!({"deleted": deleted})));
    }
    let Some(id) = arguments.get("id").and_then(Value::as_str) else {
        return failure(
            "Не понял, какой будильник удалить. Назови метку или время.",
            None::<Value>,
        );
    };
    match api.delete_schedule(id).await {
        Ok(()) => respond("Удалил будильник.", Some(json!({"deleted": id}))),
        Err(_) => failure(SPEECH_SCHEDULER_FAILED, None::<Value>),
    }
}

async fn alarm_update(deps: &Deps, arguments: Value) -> ToolOutcome {
    let Some(api) = deps.scheduler() else {
        return failure(SPEECH_SCHEDULER_MISSING, None::<Value>);
    };
    let Some(id) = arguments.get("id").and_then(Value::as_str) else {
        return failure(
            "Не понял, какой будильник изменить. Назови метку или время.",
            None::<Value>,
        );
    };
    let Some(at) = arguments.get("at").and_then(Value::as_str) else {
        return failure("Не указано новое время будильника.", None::<Value>);
    };
    let Ok(schedule) = api.get_schedule(id).await else {
        return failure(SPEECH_SCHEDULER_FAILED, None::<Value>);
    };
    let now = deps.now();
    let target = match time_domain::alarm_target(at, Some(repeat_name(&schedule.repeat)), now) {
        Ok(target) => target,
        Err(error) => return failure(error, None::<Value>),
    };
    let (wall, once_date) = time_domain::local_parts(target.instant, &deps.location);
    let request = json!({
        "id": schedule.id,
        "label": schedule.label,
        "kind": schedule.kind,
        "timezone": deps.timezone_name,
        "at": {"hour": wall.hour, "minute": wall.minute, "second": wall.second},
        "onceDate": if schedule.repeat == "REPEAT_KIND_ONCE" { once_date } else { schedule.once_date.clone() },
        "repeat": schedule.repeat,
        "enabled": schedule.enabled,
        "payloadJson": schedule.payload_json,
    });
    match api.update_schedule(request, schedule.revision).await {
        Ok(updated) => {
            let when = if updated.repeat == "REPEAT_KIND_ONCE" {
                time_domain::human_instant(target.instant, &deps.location)
            } else {
                next_human(&updated, deps)
                    .unwrap_or_else(|| time_domain::clock_text(target.instant, &deps.location))
            };
            respond(
                format!("Перенёс будильник «{}» на {when}.", updated.label),
                Some(schedule_detail(&updated, deps)),
            )
        }
        Err(_) => failure(SPEECH_SCHEDULER_FAILED, None::<Value>),
    }
}

async fn alarm_snooze_stop(deps: &Deps, arguments: Value, snooze: bool) -> ToolOutcome {
    let Some(api) = deps.scheduler() else {
        return failure(SPEECH_SCHEDULER_MISSING, None::<Value>);
    };
    let id = arguments.get("id").and_then(Value::as_str);
    let id = match id {
        Some(id) if !id.trim().is_empty() => id.to_owned(),
        _ => match active_alarm(api, deps.now()).await {
            Ok(Some(schedule)) => schedule.id,
            Ok(None) => {
                return failure(
                    if snooze {
                        "Будильник сейчас не звонит."
                    } else {
                        "Будильник не звонит."
                    },
                    None::<Value>,
                );
            }
            Err(()) => return failure(SPEECH_SCHEDULER_FAILED, None::<Value>),
        },
    };
    if snooze {
        match api.snooze_schedule(&id, SNOOZE_MINUTES).await {
            Ok(_) => respond(
                "Отложил будильник на девять минут.",
                Some(json!({"id": id})),
            ),
            Err(_) => failure(SPEECH_SCHEDULER_FAILED, None::<Value>),
        }
    } else {
        match api.stop_schedule(&id).await {
            Ok(_) => respond("Остановил будильник.", Some(json!({"id": id}))),
            Err(_) => failure(SPEECH_SCHEDULER_FAILED, None::<Value>),
        }
    }
}

async fn active_alarm(api: &dyn SchedulerApi, now: Timestamp) -> Result<Option<Schedule>, ()> {
    let now = now.as_second();
    let schedules = api.list_schedules(KIND_ALARM, true).await.map_err(|_| ())?;
    Ok(schedules
        .into_iter()
        .find(|schedule| schedule.active_occurrence_unix > 0 && schedule.active_until_unix > now))
}

fn next_human(schedule: &Schedule, deps: &Deps) -> Option<String> {
    if schedule.next_due_unix <= 0 {
        return None;
    }
    Timestamp::from_second(schedule.next_due_unix)
        .ok()
        .map(|instant| time_domain::human_instant(instant, &deps.location))
}

fn repeat_name(kind: &str) -> &'static str {
    match kind {
        "REPEAT_KIND_DAILY" => time_domain::REPEAT_DAILY,
        "REPEAT_KIND_WEEKDAYS" => time_domain::REPEAT_WEEKDAYS,
        "REPEAT_KIND_WEEKENDS" => time_domain::REPEAT_WEEKENDS,
        _ => time_domain::REPEAT_ONCE,
    }
}

// --- timers ---------------------------------------------------------------

#[derive(Debug, Serialize)]
struct TimerDetail {
    id: String,
    label: String,
    duration_seconds: i64,
    remaining_seconds: i64,
    due_unix: i64,
    state: String,
    human: String,
}

fn timer_detail(timer: &crate::scheduler::Timer, deps: &Deps) -> TimerDetail {
    let remaining = if timer.state == "TIMER_STATE_RUNNING" && timer.due_unix > 0 {
        (timer.due_unix - deps.now().as_second()).max(0)
    } else {
        timer.remaining_seconds
    };
    TimerDetail {
        id: timer.id.clone(),
        label: timer.label.clone(),
        duration_seconds: timer.duration_seconds,
        remaining_seconds: remaining,
        due_unix: timer.due_unix,
        state: timer.state.clone(),
        human: time_domain::duration_text(remaining),
    }
}

async fn timer_set(deps: &Deps, arguments: Value) -> ToolOutcome {
    let Some(api) = deps.scheduler() else {
        return failure(SPEECH_SCHEDULER_MISSING, None::<Value>);
    };
    let Some(seconds) = arguments.get("seconds").and_then(Value::as_i64) else {
        return failure(
            "Не понял длительность таймера. Скажи, например, на десять минут.",
            None::<Value>,
        );
    };
    if seconds <= 0 {
        return failure(
            "Длительность таймера должна быть больше нуля. Скажи, например, на десять минут.",
            None::<Value>,
        );
    }
    if seconds > MAX_TIMER_SECONDS {
        return failure(
            "Таймер можно поставить не дольше чем на семь дней.",
            None::<Value>,
        );
    }
    let label = match label(&arguments, "label", "") {
        Ok(label) => label,
        Err(error) => return failure(error, None::<Value>),
    };
    let label = if label.is_empty() {
        "Таймер".to_owned()
    } else {
        label
    };
    let now = deps.now();
    let Ok(timer) = api
        .start_timer(&label, seconds, &idempotency_key("timer", now))
        .await
    else {
        deps.audit("time.timer_set", "timer", "failure");
        return failure(SPEECH_SCHEDULER_FAILED, None::<Value>);
    };
    deps.audit("time.timer_set", "timer", "success");
    let text = time_domain::duration_text(seconds);
    respond(
        if label == "Таймер" {
            format!("Таймер на {text}.")
        } else {
            format!("Таймер «{label}» на {text}.")
        },
        Some(timer_detail(&timer, deps)),
    )
}

async fn timer_list(deps: &Deps, arguments: Value) -> ToolOutcome {
    let Some(api) = deps.scheduler() else {
        return failure(SPEECH_SCHEDULER_MISSING, None::<Value>);
    };
    let include_finished = arguments
        .get("include_finished")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let Ok(timers) = api.list_timers(include_finished).await else {
        return failure(SPEECH_SCHEDULER_FAILED, None::<Value>);
    };
    let speech = if timers.is_empty() {
        "Активных таймеров нет.".to_owned()
    } else {
        let parts: Vec<String> = timers
            .iter()
            .take(4)
            .map(|timer| {
                let detail = timer_detail(timer, deps);
                if timer.state == "TIMER_STATE_PAUSED" {
                    format!("{} на паузе, осталось {}", timer.label, detail.human)
                } else {
                    format!("{}: {}", timer.label, detail.human)
                }
            })
            .collect();
        format!("Таймеры: {}.", parts.join("; "))
    };
    respond(
        speech,
        Some(json!({
            "timers": timers.iter().map(|timer| timer_detail(timer, deps)).collect::<Vec<_>>(),
            "total": timers.len(),
        })),
    )
}

async fn timer_cancel(deps: &Deps, arguments: Value) -> ToolOutcome {
    let Some(api) = deps.scheduler() else {
        return failure(SPEECH_SCHEDULER_MISSING, None::<Value>);
    };
    let all = arguments
        .get("all")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    if all {
        let Ok(timers) = api.list_timers(false).await else {
            return failure(SPEECH_SCHEDULER_FAILED, None::<Value>);
        };
        if timers.is_empty() {
            return respond("Активных таймеров нет.", Some(json!({"cancelled": 0})));
        }
        let mut cancelled = 0;
        for timer in &timers {
            if api.cancel_timer(&timer.id).await.is_ok() {
                cancelled += 1;
            }
        }
        if cancelled == 0 {
            return failure(SPEECH_SCHEDULER_FAILED, None::<Value>);
        }
        return respond(
            "Отменил все таймеры.",
            Some(json!({"cancelled": cancelled})),
        );
    }
    let id = match timer_id(api, arguments).await {
        Ok(Some(id)) => id,
        Ok(None) => {
            return failure(
                "Не понял, какой таймер отменить. Скажи метку или сколько осталось.",
                None::<Value>,
            );
        }
        Err(()) => return failure(SPEECH_SCHEDULER_FAILED, None::<Value>),
    };
    match api.cancel_timer(&id).await {
        Ok(timer) => respond(
            format!("Отменил таймер «{}».", timer.label),
            Some(timer_detail(&timer, deps)),
        ),
        Err(_) => failure(SPEECH_SCHEDULER_FAILED, None::<Value>),
    }
}

async fn timer_pause_resume(deps: &Deps, arguments: Value, pause: bool) -> ToolOutcome {
    let Some(api) = deps.scheduler() else {
        return failure(SPEECH_SCHEDULER_MISSING, None::<Value>);
    };
    let id = match timer_id(api, arguments).await {
        Ok(Some(id)) => id,
        Ok(None) => {
            return failure(
                if pause {
                    "Активный таймер не найден."
                } else {
                    "Таймер на паузе не найден."
                },
                None::<Value>,
            );
        }
        Err(()) => return failure(SPEECH_SCHEDULER_FAILED, None::<Value>),
    };
    let result = if pause {
        api.pause_timer(&id).await
    } else {
        api.resume_timer(&id).await
    };
    match result {
        Ok(timer) => respond(
            if pause {
                format!("Поставил таймер «{}» на паузу.", timer.label)
            } else {
                format!("Продолжил таймер «{}».", timer.label)
            },
            Some(timer_detail(&timer, deps)),
        ),
        Err(_) => failure(SPEECH_SCHEDULER_FAILED, None::<Value>),
    }
}

async fn timer_id(api: &dyn SchedulerApi, arguments: Value) -> Result<Option<String>, ()> {
    if let Some(id) = arguments.get("id").and_then(Value::as_str)
        && !id.trim().is_empty()
    {
        return Ok(Some(id.to_owned()));
    }
    let timers = api.list_timers(false).await.map_err(|_| ())?;
    if timers.len() == 1 {
        return Ok(Some(timers[0].id.clone()));
    }
    Ok(None)
}

// --- reminders ------------------------------------------------------------

async fn reminder_set(deps: &Deps, arguments: Value) -> ToolOutcome {
    let Some(api) = deps.scheduler() else {
        return failure(SPEECH_SCHEDULER_MISSING, None::<Value>);
    };
    let Some(at) = arguments.get("at").and_then(Value::as_str) else {
        return failure(
            "Не указано время напоминания. Назови дату и время.",
            None::<Value>,
        );
    };
    let text = match label(&arguments, "text", "Напоминание") {
        Ok(text) => text,
        Err(error) => return failure(error, None::<Value>),
    };
    let now = deps.now();
    let target = match time_domain::alarm_target(at, Some("once"), now) {
        Ok(target) => target,
        Err(error) => return failure(error, None::<Value>),
    };
    let (wall, once_date) = time_domain::local_parts(target.instant, &deps.location);
    let request = json!({
        "label": text,
        "kind": KIND_REMINDER,
        "timezone": deps.timezone_name,
        "at": {"hour": wall.hour, "minute": wall.minute, "second": wall.second},
        "onceDate": once_date,
        "repeat": "REPEAT_KIND_ONCE",
        "enabled": true,
        "payloadJson": "{}",
        "idempotencyKey": idempotency_key("reminder", now),
    });
    match api.create_schedule(request).await {
        Ok(schedule) => {
            let when = next_human(&schedule, deps).unwrap_or_else(|| "указанное время".to_owned());
            respond(
                format!("Напомню «{}» {when}.", schedule.label),
                Some(schedule_detail(&schedule, deps)),
            )
        }
        Err(_) => failure(SPEECH_SCHEDULER_FAILED, None::<Value>),
    }
}

async fn reminder_list(deps: &Deps, arguments: Value) -> ToolOutcome {
    let Some(api) = deps.scheduler() else {
        return failure(SPEECH_SCHEDULER_MISSING, None::<Value>);
    };
    let include_disabled = arguments
        .get("include_disabled")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let Ok(schedules) = api.list_schedules(KIND_REMINDER, include_disabled).await else {
        return failure(SPEECH_SCHEDULER_FAILED, None::<Value>);
    };
    let speech = if schedules.is_empty() {
        "Напоминаний нет.".to_owned()
    } else {
        let parts: Vec<String> = schedules
            .iter()
            .take(4)
            .map(|schedule| {
                let when = next_human(schedule, deps).unwrap_or_else(|| "без времени".to_owned());
                format!("«{}» на {when}", schedule.label)
            })
            .collect();
        format!("Напоминания: {}.", parts.join("; "))
    };
    respond(
        speech,
        Some(json!({
            "reminders": schedules.iter().map(|s| schedule_detail(s, deps)).collect::<Vec<_>>(),
            "total": schedules.len(),
        })),
    )
}

// --- shared helpers -------------------------------------------------------

fn label(arguments: &Value, key: &str, fallback: &str) -> Result<String, String> {
    let value = arguments
        .get(key)
        .and_then(Value::as_str)
        .unwrap_or(fallback)
        .trim();
    if value.len() > 80 {
        return Err("Метка слишком длинная. Скажи короче, до 80 байт.".to_owned());
    }
    Ok(value.to_owned())
}

fn idempotency_key(kind: &str, now: Timestamp) -> String {
    format!(
        "mcp:time:{kind}:{}:{}",
        now.as_nanosecond(),
        rand::random::<u16>()
    )
}

const SPEECH_SCHEDULER_MISSING: &str = "Планировщик времени не настроен.";
const SPEECH_SCHEDULER_FAILED: &str =
    "Не удалось выполнить запрос к планировщику. Попробуй ещё раз.";

const TIME_NOW_DESCRIPTION: &str = "\
Текущее локальное время дома и часовой пояс. Вызывай первым для любой фразы про время: \
«который час», «поставь будильник через час», «напомни завтра в девять». \
Возвращает RFC3339 с офсетом, человеческую строку и IANA-пояс дома (например Asia/Almaty). \
Никогда не вычисляй время сам: возьми now() здесь, прибавь длительность и передай alarm_set/timer_set. \
Пример: «поставь будильник через час» → time_now, затем alarm_set(at = now + 1 час).";

const ALARM_SET_DESCRIPTION: &str = "\
Поставить будильник на абсолютное время. at — только RFC3339 с офсетом (2026-11-12T22:00:00+05:00); \
локальное время без пояса отклоняется. Сначала вызови time_now и вычисли at из текущего времени. \
repeat: once (по умолчанию), daily, weekdays (по будням), weekends (по выходным). \
Однократный будильник — не дальше 7 дней вперёд; прошедшее время отклоняется без создания. \
label — только явная метка владельца («Работа»), не текст команды. \
Примеры: «поставь будильник через час» → at = now + 1h; «на двадцать два ноль ноль» → at = сегодня 22:00; \
«разбуди в семь утра по будням» → at = ближайшие 07:00, repeat=weekdays; «через полчаса» → at = now + 30m. \
Ответ всегда содержит итоговое локальное время для озвучки.";

const ALARM_LIST_DESCRIPTION: &str = "\
Список будильников с ближайшим срабатыванием, меткой и повтором. \
Используй для «какие будильники», «когда будильник», «во сколько разбудишь», а также чтобы найти id \
перед alarm_cancel, alarm_update, alarm_snooze или alarm_stop. \
Пустой список — нормальный ответ «Будильников нет.», не ошибка.";

const ALARM_CANCEL_DESCRIPTION: &str = "\
Выключить будильник, не удаляя его. Нужен id из alarm_list или all=true для явного «выключи все будильники». \
Не угадывай id по времени: сначала alarm_list. Ответ подтверждает, какой будильник выключен.";

const ALARM_DELETE_DESCRIPTION: &str = "\
Удалить будильник навсегда. Нужен id из alarm_list или all=true только при явном «удали все будильники». \
Не угадывай id по времени: сначала alarm_list.";

const ALARM_UPDATE_DESCRIPTION: &str = "\
Перенести существующий будильник на новое абсолютное время RFC3339 с офсетом. \
Сначала alarm_list, чтобы взять id; повтор и метка сохраняются. \
Прошедшее время и горизонт больше 7 дней для однократного будильника отклоняются.";

const ALARM_SNOOZE_DESCRIPTION: &str = "\
Отложить звонящий будильник на девять минут. Если id не указан, инструмент сам находит активный звонок. \
Используй для «отложи», «ещё девять минут», «отложи будильник». Если будильник не звонит — так и скажи.";

const ALARM_STOP_DESCRIPTION: &str = "\
Остановить звонящий будильник. Если id не указан, инструмент сам находит активный звонок. \
Используй для «останови будильник», «выключи звонок», «хватит», когда звонок уже идёт. \
Если звонка нет, ответ говорит об этом; музыку этот инструмент не трогает.";

const TIMER_SET_DESCRIPTION: &str = "\
Поставить таймер на длительность в секундах (1..604800, то есть до 7 дней). \
Сначала time_now не обязателен: длительность считается от текущего момента. \
Примеры: «таймер на 10 минут» → seconds=600; «на полтора часа» → seconds=5400; \
«на 30 секунд» → seconds=30; «таймер пицца на 20 минут» → seconds=1200, label=\"Пицца\". \
Ноль, отрицательное значение и больше 7 дней отклоняются без создания. Ответ проговаривает длительность.";

const TIMER_LIST_DESCRIPTION: &str = "\
Активные таймеры: метка, остаток и состояние (идёт/пауза). Используй для «какие таймеры», \
«сколько осталось», а также чтобы найти id перед timer_cancel, timer_pause или timer_resume.";

const TIMER_CANCEL_DESCRIPTION: &str = "\
Отменить таймер по id из timer_list; all=true — только при явном «отмени все таймеры». \
Если id не указан и активен ровно один таймер, он и отменяется.";

const TIMER_PAUSE_DESCRIPTION: &str = "\
Поставить таймер на паузу по id из timer_list; если активен ровно один таймер, id можно не указывать. \
Используй для «поставь таймер на паузу».";

const TIMER_RESUME_DESCRIPTION: &str = "\
Продолжить таймер с паузы по id из timer_list; если на паузе ровно один таймер, id можно не указывать. \
Используй для «продолжи таймер».";

const REMINDER_SET_DESCRIPTION: &str = "\
Создать однократное напоминание на абсолютное время RFC3339 с офсетом. \
Сначала time_now, затем at; text — короткая суть напоминания. \
Примеры: «напомни завтра в девять позвонить маме» → at = завтра 09:00, text=\"позвонить маме\"; \
«напомни через два часа выключить духовку» → at = now + 2h. \
Прошедшее время и горизонт больше 7 дней отклоняются.";

const REMINDER_LIST_DESCRIPTION: &str = "\
Список напоминаний с временем и текстом. Используй для «какие напоминания», «что я просил напомнить». \
Пустой список — нормальное «Напоминаний нет.».";

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::clock::Clock;
    use crate::domain::state::{AgentMode, KeyedStore, PendingStore};
    use crate::metrics::Metrics;
    use crate::scheduler::WallTime;
    use std::collections::BTreeMap;
    use std::sync::Mutex;
    use std::time::Duration;

    struct FixedClock(Timestamp);
    impl Clock for FixedClock {
        fn now(&self) -> Timestamp {
            self.0
        }
    }

    #[derive(Default)]
    struct FakeScheduler {
        created: Mutex<Vec<Value>>,
        schedules: Mutex<Vec<Schedule>>,
        timers: Mutex<Vec<crate::scheduler::Timer>>,
        fail: Mutex<bool>,
    }

    impl FakeScheduler {
        fn check(&self) -> Result<(), String> {
            if *self.fail.lock().unwrap() {
                Err("down".to_owned())
            } else {
                Ok(())
            }
        }
        fn schedule_from(request: &Value) -> Schedule {
            Schedule {
                id: "s1".to_owned(),
                label: request["label"].as_str().unwrap_or_default().to_owned(),
                kind: request["kind"].as_str().unwrap_or_default().to_owned(),
                timezone: request["timezone"].as_str().unwrap_or_default().to_owned(),
                at: WallTime {
                    hour: request["at"]["hour"].as_i64().unwrap_or(0) as i32,
                    minute: request["at"]["minute"].as_i64().unwrap_or(0) as i32,
                    second: request["at"]["second"].as_i64().unwrap_or(0) as i32,
                },
                once_date: request["onceDate"].as_str().unwrap_or_default().to_owned(),
                repeat: request["repeat"].as_str().unwrap_or_default().to_owned(),
                next_due_unix: 1_763_070_000,
                enabled: true,
                revision: 1,
                payload_json: "{}".to_owned(),
                ..Schedule::default()
            }
        }
    }

    impl SchedulerApi for FakeScheduler {
        fn create_schedule<'a>(
            &'a self,
            request: Value,
        ) -> BoxFuture<'a, Result<Schedule, String>> {
            Box::pin(async move {
                self.check()?;
                self.created.lock().unwrap().push(request.clone());
                let schedule = Self::schedule_from(&request);
                self.schedules.lock().unwrap().push(schedule.clone());
                Ok(schedule)
            })
        }
        fn get_schedule<'a>(&'a self, _id: &'a str) -> BoxFuture<'a, Result<Schedule, String>> {
            Box::pin(async move {
                self.check()?;
                self.schedules
                    .lock()
                    .unwrap()
                    .first()
                    .cloned()
                    .ok_or_else(|| "not found".to_owned())
            })
        }
        fn list_schedules<'a>(
            &'a self,
            kind: &'a str,
            _include_disabled: bool,
        ) -> BoxFuture<'a, Result<Vec<Schedule>, String>> {
            Box::pin(async move {
                self.check()?;
                Ok(self
                    .schedules
                    .lock()
                    .unwrap()
                    .iter()
                    .filter(|schedule| schedule.kind == kind)
                    .cloned()
                    .collect())
            })
        }
        fn update_schedule<'a>(
            &'a self,
            schedule: Value,
            _expected_revision: i64,
        ) -> BoxFuture<'a, Result<Schedule, String>> {
            Box::pin(async move {
                self.check()?;
                let updated = Self::schedule_from(&schedule);
                *self.schedules.lock().unwrap() = vec![updated.clone()];
                Ok(updated)
            })
        }
        fn delete_schedule<'a>(&'a self, _id: &'a str) -> BoxFuture<'a, Result<(), String>> {
            Box::pin(async move {
                self.check()?;
                self.schedules.lock().unwrap().clear();
                Ok(())
            })
        }
        fn set_schedule_enabled<'a>(
            &'a self,
            _id: &'a str,
            enabled: bool,
        ) -> BoxFuture<'a, Result<Schedule, String>> {
            Box::pin(async move {
                self.check()?;
                let mut schedule = self
                    .schedules
                    .lock()
                    .unwrap()
                    .first()
                    .cloned()
                    .ok_or_else(|| "not found".to_owned())?;
                schedule.enabled = enabled;
                Ok(schedule)
            })
        }
        fn snooze_schedule<'a>(
            &'a self,
            _id: &'a str,
            _minutes: i64,
        ) -> BoxFuture<'a, Result<Schedule, String>> {
            Box::pin(async move {
                self.check()?;
                Ok(self
                    .schedules
                    .lock()
                    .unwrap()
                    .first()
                    .cloned()
                    .unwrap_or_default())
            })
        }
        fn stop_schedule<'a>(&'a self, _id: &'a str) -> BoxFuture<'a, Result<Schedule, String>> {
            Box::pin(async move {
                self.check()?;
                Ok(self
                    .schedules
                    .lock()
                    .unwrap()
                    .first()
                    .cloned()
                    .unwrap_or_default())
            })
        }
        fn start_timer<'a>(
            &'a self,
            label: &'a str,
            seconds: i64,
            _idempotency_key: &'a str,
        ) -> BoxFuture<'a, Result<crate::scheduler::Timer, String>> {
            Box::pin(async move {
                self.check()?;
                Ok(crate::scheduler::Timer {
                    id: "t1".to_owned(),
                    label: label.to_owned(),
                    duration_seconds: seconds,
                    remaining_seconds: seconds,
                    due_unix: 0,
                    state: "TIMER_STATE_RUNNING".to_owned(),
                })
            })
        }
        fn list_timers<'a>(
            &'a self,
            _include_finished: bool,
        ) -> BoxFuture<'a, Result<Vec<crate::scheduler::Timer>, String>> {
            Box::pin(async move {
                self.check()?;
                Ok(self.timers.lock().unwrap().clone())
            })
        }
        fn cancel_timer<'a>(
            &'a self,
            _id: &'a str,
        ) -> BoxFuture<'a, Result<crate::scheduler::Timer, String>> {
            Box::pin(async move {
                self.check()?;
                Ok(crate::scheduler::Timer {
                    id: "t1".to_owned(),
                    label: "Таймер".to_owned(),
                    ..crate::scheduler::Timer::default()
                })
            })
        }
        fn pause_timer<'a>(
            &'a self,
            _id: &'a str,
        ) -> BoxFuture<'a, Result<crate::scheduler::Timer, String>> {
            Box::pin(async move { self.check().map(|()| crate::scheduler::Timer::default()) })
        }
        fn resume_timer<'a>(
            &'a self,
            _id: &'a str,
        ) -> BoxFuture<'a, Result<crate::scheduler::Timer, String>> {
            Box::pin(async move { self.check().map(|()| crate::scheduler::Timer::default()) })
        }
    }

    fn deps(scheduler: Arc<dyn SchedulerApi>) -> Deps {
        Deps {
            catalog: None,
            prom: None,
            alerts: None,
            loki: None,
            kube: None,
            report: None,
            checks: None,
            pending: Arc::new(PendingStore::new(
                Duration::from_secs(60),
                Arc::new(FixedClock("2026-11-12T12:00:00+05:00".parse().unwrap())),
            )),
            timers: Arc::new(crate::tools::TimerScheduler),
            metrics: Arc::new(Metrics::new()),
            clock: Arc::new(FixedClock("2026-11-12T12:00:00+05:00".parse().unwrap())),
            location: jiff::tz::TimeZone::get("Asia/Almaty").unwrap(),
            timezone_name: "Asia/Almaty".to_owned(),
            cooldown: Duration::from_secs(1),
            self_restart_delay: Duration::ZERO,
            query_timeout: Duration::from_secs(1),
            status_timeout: Duration::from_secs(1),
            report_timeout: Duration::from_secs(1),
            log_byte_cap: Arc::new(std::sync::atomic::AtomicUsize::new(1)),
            bb: None,
            bb_targets: None,
            bb_pending: Arc::new(KeyedStore::new(
                Duration::from_secs(60),
                Arc::new(FixedClock("2026-11-12T12:00:00+05:00".parse().unwrap())),
            )),
            bb_mode: Arc::new(AgentMode::new()),
            scheduler: Some(scheduler),
            push: None,
        }
    }

    fn group(scheduler: Arc<dyn SchedulerApi>) -> TimeGroup {
        TimeGroup::new(Arc::new(deps(scheduler)))
    }

    #[tokio::test]
    async fn alarm_set_builds_the_same_ha_request_and_speaks_local_time() {
        let fake = Arc::new(FakeScheduler::default());
        let outcome = group(fake.clone())
            .call(
                "alarm_set",
                json!({"at": "2026-11-12T22:00:00+05:00", "label": "Работа"}),
            )
            .await
            .expect("call");
        assert!(outcome.speech.contains("22:00"), "{}", outcome.speech);
        let created = fake.created.lock().unwrap().clone();
        assert_eq!(created.len(), 1);
        assert_eq!(created[0]["kind"], KIND_ALARM);
        assert_eq!(created[0]["timezone"], "Asia/Almaty");
        assert_eq!(created[0]["at"]["hour"], 22);
        assert_eq!(created[0]["onceDate"], "2026-11-12");
        assert_eq!(created[0]["repeat"], "REPEAT_KIND_ONCE");
        assert_eq!(created[0]["payloadJson"], "{}");
    }

    #[tokio::test]
    async fn alarm_set_rejects_past_and_too_far_without_calling_scheduler() {
        let fake = Arc::new(FakeScheduler::default());
        let group = group(fake.clone());
        for at in ["2026-11-12T11:00:00+05:00", "2026-11-21T12:00:00+05:00"] {
            let outcome = group
                .call("alarm_set", json!({"at": at}))
                .await
                .expect("call");
            assert!(outcome.speech.contains("прошло") || outcome.speech.contains("7 дней"));
        }
        assert!(fake.created.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn timer_set_rejects_zero_and_above_a_week() {
        let fake = Arc::new(FakeScheduler::default());
        let group = group(fake.clone());
        for seconds in [0, -5, MAX_TIMER_SECONDS + 1] {
            let outcome = group
                .call("timer_set", json!({"seconds": seconds}))
                .await
                .expect("call");
            assert!(
                outcome.speech.contains("больше нуля") || outcome.speech.contains("семь дней"),
                "{}",
                outcome.speech
            );
        }
        let outcome = group
            .call("timer_set", json!({"seconds": 600, "label": "Пицца"}))
            .await
            .expect("call");
        assert!(outcome.speech.contains("10 минут"), "{}", outcome.speech);
        assert!(outcome.speech.contains("Пицца"));
    }

    #[tokio::test]
    async fn alarm_update_keeps_repeat_and_label_and_speaks_the_new_time() {
        let fake = Arc::new(FakeScheduler::default());
        fake.schedules.lock().unwrap().push(Schedule {
            id: "s1".to_owned(),
            label: "Работа".to_owned(),
            kind: KIND_ALARM.to_owned(),
            repeat: "REPEAT_KIND_WEEKDAYS".to_owned(),
            enabled: true,
            revision: 4,
            ..Schedule::default()
        });
        let outcome = group(fake.clone())
            .call(
                "alarm_update",
                json!({"id": "s1", "at": "2026-11-13T08:15:00+05:00"}),
            )
            .await
            .expect("call");
        assert!(outcome.speech.contains("Работа"));
        let schedule = fake.schedules.lock().unwrap().first().cloned().unwrap();
        assert_eq!(schedule.repeat, "REPEAT_KIND_WEEKDAYS");
        assert_eq!(schedule.at.hour, 8);
        assert_eq!(schedule.at.minute, 15);
    }

    #[tokio::test]
    async fn alarm_list_says_none_without_creating_anything() {
        let fake = Arc::new(FakeScheduler::default());
        let outcome = group(fake.clone())
            .call("alarm_list", json!({}))
            .await
            .expect("call");
        assert_eq!(outcome.speech, "Будильников нет.");
        assert!(fake.created.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn scheduler_failures_are_spoken_not_panicked() {
        let fake = Arc::new(FakeScheduler::default());
        *fake.fail.lock().unwrap() = true;
        let outcome = group(fake)
            .call("alarm_set", json!({"at": "2026-11-12T22:00:00+05:00"}))
            .await
            .expect("call");
        assert!(outcome.speech.contains("планировщику"));
    }

    #[test]
    fn tool_group_has_the_required_tools() {
        let group = group(Arc::new(FakeScheduler::default()));
        let names: BTreeMap<_, _> = group
            .tools()
            .into_iter()
            .map(|spec| (spec.name, spec.description))
            .collect();
        for name in [
            "time_now",
            "alarm_set",
            "alarm_list",
            "alarm_cancel",
            "alarm_update",
            "alarm_snooze",
            "alarm_stop",
            "timer_set",
            "timer_list",
            "timer_cancel",
            "reminder_set",
            "reminder_list",
        ] {
            assert!(names.contains_key(name), "{name}");
        }
        assert!(names["alarm_set"].contains("RFC3339"));
        assert!(names["alarm_set"].contains("через час"));
    }
}
