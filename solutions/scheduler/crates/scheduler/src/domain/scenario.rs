//! Durable scenario definitions.
//!
//! A scenario is a named, ordered list of steps gated by triggers and
//! conditions. The scheduler owns definitions and emits `scenario.fire`
//! events; a separate executor evaluates conditions and runs the steps.
//!
//! The stored body is the [`ScenarioDefinition`] serialized as JSON. Server
//! fields (`id`, `revision`, timestamps and the computed `next_due_unix`) live
//! in SQLite columns and never in the body.

use jiff::tz::TimeZone;
use serde::{Deserialize, Serialize};

use crate::domain::rule::{Repeat, Rule, RuleError, WallTime, parse_iso_date};
use crate::domain::validation::DomainError;

/// Maximum label length in bytes.
pub const MAX_SCENARIO_LABEL_BYTES: usize = 80;
/// Maximum description length in bytes.
pub const MAX_SCENARIO_DESCRIPTION_BYTES: usize = 512;
/// Maximum number of triggers.
pub const MAX_SCENARIO_TRIGGERS: usize = 8;
/// Maximum number of conditions.
pub const MAX_SCENARIO_CONDITIONS: usize = 8;
/// Maximum number of steps.
pub const MAX_SCENARIO_STEPS: usize = 32;
/// Maximum spoken text length in characters.
pub const MAX_SPEAK_CHARS: usize = 500;
/// Maximum delay step.
pub const MAX_DELAY_SECONDS: i32 = 3600;
/// Maximum entity id length.
pub const MAX_ENTITY_ID_BYTES: usize = 255;
/// Maximum entity state length.
pub const MAX_ENTITY_STATE_BYTES: usize = 255;
/// Maximum catalog entry id length.
pub const MAX_ENTRY_ID_BYTES: usize = 200;
/// Maximum catalog action length.
pub const MAX_ACTION_BYTES: usize = 64;

/// What starts a scenario.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ScenarioTrigger {
    /// Only RunScenario starts it.
    Manual,
    /// A wall-clock trigger with the same recurrence policy as a Schedule.
    Time(ScenarioTimeTrigger),
}

/// Local wall-clock recurrence of a time trigger.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ScenarioTimeTrigger {
    pub timezone: String,
    pub at: WallTime,
    pub repeat: Repeat,
    pub once_date: String,
    pub weekdays: Vec<i32>,
}

impl ScenarioTimeTrigger {
    /// The trigger's recurrence translated to the scheduler's UTC rule.
    ///
    /// The definition keeps its IANA zone because the executor evaluates
    /// local time windows; the materialized schedule is UTC-only like every
    /// other schedule. The weekday mask is shifted to the UTC weekdays the
    /// local occurrences land on.
    #[must_use]
    pub fn rule(&self) -> Rule {
        crate::domain::rule::local_to_utc(
            &self.timezone,
            self.at,
            self.repeat,
            &self.weekdays,
            &self.once_date,
        )
    }
}

/// Validates the local recurrence shape of a scenario time trigger.
fn validate_time_trigger(time: &ScenarioTimeTrigger) -> Result<(), RuleError> {
    if time.timezone.is_empty() {
        return Err(RuleError::new("timezone is required"));
    }
    if TimeZone::get(&time.timezone).is_err() {
        return Err(RuleError::new(format!(
            "unknown timezone {:?}",
            time.timezone
        )));
    }
    if !(0..=23).contains(&time.at.hour) {
        return Err(RuleError::new(format!(
            "hour {} outside 0..23",
            time.at.hour
        )));
    }
    if !(0..=59).contains(&time.at.minute) {
        return Err(RuleError::new(format!(
            "minute {} outside 0..59",
            time.at.minute
        )));
    }
    if !(0..=59).contains(&time.at.second) {
        return Err(RuleError::new(format!(
            "second {} outside 0..59",
            time.at.second
        )));
    }
    match time.repeat {
        Repeat::Once => {
            if time.once_date.is_empty() {
                return Err(RuleError::new("once_date is required for REPEAT_KIND_ONCE"));
            }
            if parse_iso_date(&time.once_date).is_none() {
                return Err(RuleError::new("once_date must be YYYY-MM-DD"));
            }
            if !time.weekdays.is_empty() {
                return Err(RuleError::new(
                    "weekdays are only valid for REPEAT_KIND_CUSTOM",
                ));
            }
        }
        Repeat::Daily | Repeat::Weekdays | Repeat::Weekends => {
            if !time.once_date.is_empty() {
                return Err(RuleError::new(
                    "once_date is only valid for REPEAT_KIND_ONCE",
                ));
            }
            if !time.weekdays.is_empty() {
                return Err(RuleError::new(
                    "weekdays are only valid for REPEAT_KIND_CUSTOM",
                ));
            }
        }
        Repeat::Custom => {
            if !time.once_date.is_empty() {
                return Err(RuleError::new(
                    "once_date is only valid for REPEAT_KIND_ONCE",
                ));
            }
            if time.weekdays.is_empty() {
                return Err(RuleError::new("REPEAT_KIND_CUSTOM needs weekdays"));
            }
            let mut seen: Vec<i32> = Vec::with_capacity(time.weekdays.len());
            for &weekday in &time.weekdays {
                if !(1..=7).contains(&weekday) {
                    return Err(RuleError::new(format!("weekday {weekday} outside 1..7")));
                }
                if seen.contains(&weekday) {
                    return Err(RuleError::new(format!("duplicate weekday {weekday}")));
                }
                seen.push(weekday);
            }
        }
        Repeat::Unspecified => {
            return Err(RuleError::new(format!(
                "unsupported repeat {}",
                time.repeat.name()
            )));
        }
    }
    if time.repeat == Repeat::Once && time.rule().once_unix <= 0 {
        return Err(RuleError::new("once_date must be YYYY-MM-DD"));
    }
    Ok(())
}

/// A condition the executor evaluates before running steps.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ScenarioCondition {
    /// Matches a Home Assistant entity state.
    EntityState { entity_id: String, state: String },
    /// Matches a local time window; `end < start` wraps over midnight.
    TimeWindow {
        start: WallTime,
        end: WallTime,
        weekdays: Vec<i32>,
    },
}

/// One ordered scenario step.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ScenarioStep {
    /// A `home.control`-shaped catalog call.
    Catalog {
        entry_id: String,
        action: String,
        params_json: String,
    },
    /// Speak text on the station.
    Speak { text: String },
    /// Wait before the next step.
    Delay { seconds: i32 },
}

/// Client-controlled part of a scenario; this is the stored JSON body.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ScenarioDefinition {
    pub label: String,
    pub description: String,
    pub continue_on_error: bool,
    pub triggers: Vec<ScenarioTrigger>,
    pub conditions: Vec<ScenarioCondition>,
    pub steps: Vec<ScenarioStep>,
}

/// A durable scenario row plus its computed next occurrence.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Scenario {
    pub id: String,
    pub label: String,
    pub description: String,
    pub enabled: bool,
    pub revision: i64,
    pub created_unix: i64,
    pub updated_unix: i64,
    pub triggers: Vec<ScenarioTrigger>,
    pub conditions: Vec<ScenarioCondition>,
    pub steps: Vec<ScenarioStep>,
    pub continue_on_error: bool,
    pub next_due_unix: i64,
}

impl Scenario {
    /// Client-controlled definition stored as JSON.
    #[must_use]
    pub fn definition(&self) -> ScenarioDefinition {
        ScenarioDefinition {
            label: self.label.clone(),
            description: self.description.clone(),
            continue_on_error: self.continue_on_error,
            triggers: self.triggers.clone(),
            conditions: self.conditions.clone(),
            steps: self.steps.clone(),
        }
    }

    /// Rebuilds a scenario from its stored body and server columns.
    #[must_use]
    pub fn from_definition(
        id: String,
        definition: ScenarioDefinition,
        enabled: bool,
        revision: i64,
        created_unix: i64,
        updated_unix: i64,
    ) -> Self {
        Self {
            id,
            label: definition.label,
            description: definition.description,
            enabled,
            revision,
            created_unix,
            updated_unix,
            triggers: definition.triggers,
            conditions: definition.conditions,
            steps: definition.steps,
            continue_on_error: definition.continue_on_error,
            next_due_unix: 0,
        }
    }

    /// Time triggers in declaration order.
    pub fn time_triggers(&self) -> impl Iterator<Item = (usize, &ScenarioTimeTrigger)> {
        self.triggers
            .iter()
            .enumerate()
            .filter_map(|(index, trigger)| match trigger {
                ScenarioTrigger::Time(time) => Some((index, time)),
                ScenarioTrigger::Manual => None,
            })
    }

    /// Validates a client definition. `max_body_bytes` bounds the serialized
    /// body, matching the payload limit of the rest of the scheduler.
    pub fn validate(
        definition: &ScenarioDefinition,
        max_body_bytes: usize,
    ) -> Result<(), DomainError> {
        if definition.label.trim().is_empty() {
            return Err(DomainError::invalid("label is required"));
        }
        if definition.label.len() > MAX_SCENARIO_LABEL_BYTES {
            return Err(DomainError::invalid(format!(
                "label exceeds {MAX_SCENARIO_LABEL_BYTES} bytes"
            )));
        }
        if definition.description.len() > MAX_SCENARIO_DESCRIPTION_BYTES {
            return Err(DomainError::invalid(format!(
                "description exceeds {MAX_SCENARIO_DESCRIPTION_BYTES} bytes"
            )));
        }
        if definition.triggers.len() > MAX_SCENARIO_TRIGGERS {
            return Err(DomainError::invalid(format!(
                "at most {MAX_SCENARIO_TRIGGERS} triggers are allowed"
            )));
        }
        if definition.conditions.len() > MAX_SCENARIO_CONDITIONS {
            return Err(DomainError::invalid(format!(
                "at most {MAX_SCENARIO_CONDITIONS} conditions are allowed"
            )));
        }
        if definition.steps.len() > MAX_SCENARIO_STEPS {
            return Err(DomainError::invalid(format!(
                "at most {MAX_SCENARIO_STEPS} steps are allowed"
            )));
        }
        for trigger in &definition.triggers {
            if let ScenarioTrigger::Time(time) = trigger {
                validate_time_trigger(time).map_err(|error| DomainError::invalid_rule(&error))?;
            }
        }
        for condition in &definition.conditions {
            validate_condition(condition)?;
        }
        for step in &definition.steps {
            validate_step(step)?;
        }
        let body = serde_json::to_string(definition)
            .map_err(|_| DomainError::invalid("scenario body is not serializable"))?;
        if body.len() > max_body_bytes {
            return Err(DomainError::invalid(format!(
                "scenario body exceeds {max_body_bytes} bytes"
            )));
        }
        Ok(())
    }

    /// Serialized definition used as the stored body and idempotency input.
    pub fn body_json(&self) -> Result<String, DomainError> {
        serde_json::to_string(&self.definition())
            .map_err(|_| DomainError::invalid("scenario body is not serializable"))
    }
}

fn validate_condition(condition: &ScenarioCondition) -> Result<(), DomainError> {
    match condition {
        ScenarioCondition::EntityState { entity_id, state } => {
            validate_entity_id(entity_id)?;
            if state.is_empty() || state.len() > MAX_ENTITY_STATE_BYTES {
                return Err(DomainError::invalid(format!(
                    "entity state must be 1..{MAX_ENTITY_STATE_BYTES} bytes"
                )));
            }
            if state.chars().any(char::is_control) {
                return Err(DomainError::invalid(
                    "entity state must not contain control characters",
                ));
            }
        }
        ScenarioCondition::TimeWindow {
            start,
            end,
            weekdays,
        } => {
            validate_wall_time(*start)?;
            validate_wall_time(*end)?;
            validate_weekdays(weekdays)?;
        }
    }
    Ok(())
}

fn validate_step(step: &ScenarioStep) -> Result<(), DomainError> {
    match step {
        ScenarioStep::Catalog {
            entry_id,
            action,
            params_json,
        } => {
            if entry_id.trim().is_empty() || entry_id.len() > MAX_ENTRY_ID_BYTES {
                return Err(DomainError::invalid(format!(
                    "catalog entry_id must be 1..{MAX_ENTRY_ID_BYTES} bytes"
                )));
            }
            if action.trim().is_empty() || action.len() > MAX_ACTION_BYTES {
                return Err(DomainError::invalid(format!(
                    "catalog action must be 1..{MAX_ACTION_BYTES} bytes"
                )));
            }
            if !params_json.trim().is_empty()
                && serde_json::from_str::<serde_json::Value>(params_json).is_err()
            {
                return Err(DomainError::invalid(
                    "catalog params_json must be valid JSON",
                ));
            }
        }
        ScenarioStep::Speak { text } => {
            let chars = text.chars().count();
            if !(1..=MAX_SPEAK_CHARS).contains(&chars) {
                return Err(DomainError::invalid(format!(
                    "speak text must be 1..{MAX_SPEAK_CHARS} characters"
                )));
            }
            if text.chars().any(char::is_control) {
                return Err(DomainError::invalid(
                    "speak text must not contain control characters",
                ));
            }
        }
        ScenarioStep::Delay { seconds } => {
            if !(1..=MAX_DELAY_SECONDS).contains(seconds) {
                return Err(DomainError::invalid(format!(
                    "delay seconds must be 1..{MAX_DELAY_SECONDS}"
                )));
            }
        }
    }
    Ok(())
}

fn validate_entity_id(entity_id: &str) -> Result<(), DomainError> {
    if entity_id.is_empty() || entity_id.len() > MAX_ENTITY_ID_BYTES {
        return Err(DomainError::invalid(format!(
            "entity_id must be 1..{MAX_ENTITY_ID_BYTES} bytes"
        )));
    }
    let Some((domain, object)) = entity_id.split_once('.') else {
        return Err(DomainError::invalid(
            "entity_id must be <domain>.<object_id>",
        ));
    };
    let valid_part = |part: &str| {
        let mut chars = part.chars();
        chars.next().is_some_and(|first| first.is_ascii_lowercase())
            && chars.all(|character| {
                character.is_ascii_lowercase() || character.is_ascii_digit() || character == '_'
            })
    };
    if !valid_part(domain) || !valid_part(object) {
        return Err(DomainError::invalid(
            "entity_id must be <domain>.<object_id> using [a-z0-9_]",
        ));
    }
    Ok(())
}

fn validate_wall_time(at: WallTime) -> Result<(), DomainError> {
    if !(0..=23).contains(&at.hour)
        || !(0..=59).contains(&at.minute)
        || !(0..=59).contains(&at.second)
    {
        return Err(DomainError::invalid(
            "wall time must be hour 0..23, minute 0..59, second 0..59",
        ));
    }
    Ok(())
}

fn validate_weekdays(weekdays: &[i32]) -> Result<(), DomainError> {
    let mut seen = Vec::with_capacity(weekdays.len());
    for &weekday in weekdays {
        if !(1..=7).contains(&weekday) {
            return Err(DomainError::invalid(format!(
                "weekday {weekday} outside 1..7"
            )));
        }
        if seen.contains(&weekday) {
            return Err(DomainError::invalid(format!("duplicate weekday {weekday}")));
        }
        seen.push(weekday);
    }
    Ok(())
}
