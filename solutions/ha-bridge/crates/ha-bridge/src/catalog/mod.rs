//! The phone-facing control catalog: pure rules over discovered Home Assistant
//! data.
//!
//! Nothing is listed by hand. Entities arrive in [`Raw`], [`build`] applies the
//! [`Rules`] and [`Entry::resolve`] maps a catalog action back to one Home
//! Assistant service call. Only entries that survive the rules can be shown or
//! driven, so neither the phone nor a task payload can reach an arbitrary
//! service or entity.
//!
//! The rules and the user-visible strings match the Go implementation in
//! `internal/catalog`, with one deliberate fix: display text is bounded by
//! characters, not bytes, so a Cyrillic title never splits into U+FFFD.

mod light;

pub use light::{Light, LightInput, Params};

use std::collections::{HashMap, HashSet};

use serde_json::{Map, Value};

/// The control model of an entry.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    /// A one-shot action: an HA script or scene. Action `run`.
    Scenario,
    Light,
    Switch,
    /// The assistant's named profile; a plain switch where on is the
    /// non-default profile.
    Profile,
    /// A device-level one-shot action that is not an entity of its own, for
    /// example speaking a text on the speaker. It carries the owning device
    /// and resolves to its own allowlisted service call.
    Action,
}

impl Kind {
    /// Wire name used by `GET /v1/catalog`.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Scenario => "scenario",
            Self::Light => "light",
            Self::Switch => "switch",
            Self::Profile => "profile",
            Self::Action => "action",
        }
    }
}

/// Actions understood by the executor.
pub const ACTION_RUN: &str = "run";
pub const ACTION_TURN_ON: &str = "turn_on";
pub const ACTION_TURN_OFF: &str = "turn_off";
pub const ACTION_SET: &str = "set";
pub const ACTION_SPEAK: &str = "speak";

/// The scheduler task type of a control command.
pub const TASK_TYPE: &str = "home.control";

/// Synthetic catalog id of the "speak on the speaker" action.
pub const SPEAK_ENTRY_ID: &str = "action.station_speak";
/// The Home Assistant script that performs the speech. The action entry only
/// exists while Home Assistant exposes this script, so a server without the
/// script never advertises the action.
pub const SPEAK_SCRIPT: &str = "script.speak_text";
/// The satellite entity whose device/room the action belongs to.
const SPEAK_SATELLITE_DOMAIN: &str = "assist_satellite";
/// Longest text the action accepts, in characters.
pub const SPEAK_MAX_TEXT: usize = 500;

const MAX_ENTRIES: usize = 200;
const MAX_TEXT: usize = 80;
const MAX_ICON: usize = 32;
const DEFAULT_MIN_KELVIN: i32 = 2000;
const DEFAULT_MAX_KELVIN: i32 = 6500;

/// One discovered Home Assistant entity before filtering.
#[derive(Debug, Clone, Default)]
pub struct Raw {
    pub entity: String,
    pub state: String,
    pub name: String,
    pub icon: String,
    pub room: String,
    /// Home Assistant device registry id, empty when the entity has no device
    /// or the registry could not be read.
    pub device_id: String,
    /// Display name of the owning device, empty when unknown.
    pub device: String,
    /// `"primary"` for a normal entity, `"setting"` for a config/diagnostic
    /// entity of a device or for a device's switch next to a light, empty when
    /// the entity registry could not be read.
    pub role: String,
    pub hidden: bool,
    pub scenario: bool,
    pub profile: bool,
    pub light: Option<Light>,
}

/// Which entities discovery exposes and under which role labels.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Rules {
    pub scenario_label: String,
    pub profile_label: String,
    /// `path.Match` globs on the entity id, e.g. `switch.*_child_lock`.
    pub exclude: Vec<String>,
}

impl Rules {
    /// Check rule syntax so a misconfiguration fails at startup.
    pub fn validate(&self) -> Result<(), RulesError> {
        if !valid_label(&self.scenario_label) {
            return Err(RulesError::ScenarioLabel);
        }
        if !valid_label(&self.profile_label) {
            return Err(RulesError::ProfileLabel);
        }
        for pattern in &self.exclude {
            validate_glob(pattern).map_err(|()| RulesError::Exclude(pattern.clone()))?;
        }
        Ok(())
    }
}

/// A rejected rule set.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum RulesError {
    #[error("scenario label must be an HA label id: [a-z0-9_-]{{1,64}}")]
    ScenarioLabel,
    #[error("profile label must be an HA label id: [a-z0-9_-]{{1,64}}")]
    ProfileLabel,
    #[error("invalid exclude pattern {0}")]
    Exclude(String),
}

/// One controllable thing with its live state at discovery time.
///
/// Entries are flat: one Home Assistant entity is one entry. A device groups
/// its entries in the client through [`Entry::device_id`]. Device-level
/// actions (for example "speak this text on the speaker" or "call the
/// speaker"), once implemented, are expected to be further entries with
/// `kind: "action"` that carry the same `device_id` and `device`, keep a
/// human-readable `title`, and resolve to their own allowlisted service call.
/// The executor contract (`{"entry", "action", ...}` in the `control` queue)
/// does not change, so adding actions stays additive for the clients.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    pub id: String,
    pub kind: Kind,
    pub title: String,
    /// Stable localization key of a server-generated title. Empty for titles
    /// that come from Home Assistant user data (they are already the user's
    /// words and must not be translated).
    pub title_key: String,
    /// Typed arguments for [`Entry::title_key`].
    pub title_args: std::collections::BTreeMap<String, String>,
    pub room: String,
    pub icon: String,
    pub entity: String,
    pub state: String,
    pub available: bool,
    /// Home Assistant device registry id; empty when unknown.
    pub device_id: String,
    /// Display name of the owning device; empty when unknown.
    pub device: String,
    /// `"primary"`, `"setting"` or empty when unknown.
    pub role: String,
    pub light: Option<Light>,
}

/// A resolved Home Assistant service invocation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServiceCall {
    pub domain: String,
    pub service: String,
    pub entity: String,
    pub data: Map<String, Value>,
}

/// An entry with an immutable display order and id lookup.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Catalog {
    entries: Vec<Entry>,
    by_id: HashMap<String, usize>,
}

impl Catalog {
    /// Wrap already-validated entries (see [`build`]).
    #[must_use]
    pub fn new(entries: Vec<Entry>) -> Self {
        let by_id = entries
            .iter()
            .enumerate()
            .map(|(index, entry)| (entry.id.clone(), index))
            .collect();
        Self { entries, by_id }
    }

    #[must_use]
    pub fn entries(&self) -> &[Entry] {
        &self.entries
    }

    #[must_use]
    pub fn lookup(&self, id: &str) -> Option<&Entry> {
        self.by_id.get(id).map(|index| &self.entries[*index])
    }

    /// Every entry flagged unavailable and stripped of state. The stale
    /// fallback never pretends Home Assistant is reachable.
    #[must_use]
    pub fn mark_unavailable(&self) -> Self {
        let entries = self
            .entries
            .iter()
            .map(|entry| Entry {
                available: false,
                state: String::new(),
                ..entry.clone()
            })
            .collect();
        Self::new(entries)
    }
}

impl Entry {
    /// Actions this entry supports.
    #[must_use]
    pub fn actions(&self) -> Vec<&'static str> {
        if self.kind == Kind::Scenario {
            return vec![ACTION_RUN];
        }
        if self.kind == Kind::Action {
            return vec![ACTION_SPEAK];
        }
        if self.light.is_some() {
            return vec![ACTION_TURN_ON, ACTION_TURN_OFF, ACTION_SET];
        }
        vec![ACTION_TURN_ON, ACTION_TURN_OFF]
    }

    /// Map `(entry, action)` to a Home Assistant service call. Unsupported
    /// actions are rejected here, so the executor can only perform what the
    /// catalog allows.
    pub fn resolve(&self, action: &str, params: &Params) -> Result<ServiceCall, ResolveError> {
        let domain = domain_of(&self.entity).to_owned();
        match (self.kind, action) {
            (Kind::Light, ACTION_SET) => {
                let light = self
                    .light
                    .as_ref()
                    .ok_or_else(|| ResolveError::NotAdjustable {
                        id: self.id.clone(),
                    })?;
                let data = light.service_data(params)?;
                Ok(ServiceCall {
                    domain,
                    service: ACTION_TURN_ON.to_owned(),
                    entity: self.entity.clone(),
                    data,
                })
            }
            (Kind::Light, ACTION_TURN_ON) => {
                let data = match self.light.as_ref() {
                    Some(light) => light.turn_on_data(params)?,
                    None if params.is_empty() => Map::new(),
                    None => {
                        return Err(ResolveError::NotAdjustable {
                            id: self.id.clone(),
                        });
                    }
                };
                Ok(ServiceCall {
                    domain,
                    service: ACTION_TURN_ON.to_owned(),
                    entity: self.entity.clone(),
                    data,
                })
            }
            (Kind::Light, ACTION_TURN_OFF) => {
                let data = match self.light.as_ref() {
                    Some(light) => light.turn_off_data(params)?,
                    None if params.is_empty() => Map::new(),
                    None => {
                        return Err(ResolveError::NotAdjustable {
                            id: self.id.clone(),
                        });
                    }
                };
                Ok(ServiceCall {
                    domain,
                    service: ACTION_TURN_OFF.to_owned(),
                    entity: self.entity.clone(),
                    data,
                })
            }
            (Kind::Scenario, ACTION_RUN) => Ok(ServiceCall {
                domain,
                service: ACTION_TURN_ON.to_owned(),
                entity: self.entity.clone(),
                data: Map::new(),
            }),
            (Kind::Action, ACTION_SPEAK) => {
                let text = speak_text(params)?;
                let mut variables = Map::new();
                variables.insert("message".to_owned(), Value::String(text));
                let mut data = Map::new();
                data.insert("variables".to_owned(), Value::Object(variables));
                Ok(ServiceCall {
                    domain: "script".to_owned(),
                    service: ACTION_TURN_ON.to_owned(),
                    entity: SPEAK_SCRIPT.to_owned(),
                    data,
                })
            }
            (Kind::Switch | Kind::Profile, ACTION_TURN_ON | ACTION_TURN_OFF) => Ok(ServiceCall {
                domain,
                service: action.to_owned(),
                entity: self.entity.clone(),
                data: Map::new(),
            }),
            _ => Err(ResolveError::Unsupported {
                action: action.to_owned(),
                kind: self.kind,
                id: self.id.clone(),
            }),
        }
    }
}

/// A rejected catalog action.
#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum ResolveError {
    #[error("light {id:?} cannot be adjusted")]
    NotAdjustable { id: String },
    #[error("action {action:?} is not supported for {kind:?} {id:?}")]
    Unsupported {
        action: String,
        kind: Kind,
        id: String,
    },
    #[error("{0}")]
    Light(#[from] light::LightError),
    #[error("{0}")]
    Speak(#[from] SpeakError),
}

/// Why a `speak` text was rejected. Messages never contain the text itself:
/// it is user content and must not reach logs or ack details.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum SpeakError {
    #[error("speak needs a text")]
    Missing,
    #[error("speak text is empty")]
    Empty,
    #[error("speak text is longer than {SPEAK_MAX_TEXT} characters")]
    TooLong,
    #[error("speak text contains control characters")]
    ControlCharacters,
}

/// Validate and trim the text of a `speak` action.
///
/// The text is bounded by characters, must contain something after trimming
/// and must not contain control characters (`\n`, `\t`, escape/format
/// characters), so a task cannot smuggle terminal or log control sequences.
fn speak_text(params: &Params) -> Result<String, SpeakError> {
    let text = params.text.as_deref().ok_or(SpeakError::Missing)?.trim();
    if text.is_empty() {
        return Err(SpeakError::Empty);
    }
    if text.chars().count() > SPEAK_MAX_TEXT {
        return Err(SpeakError::TooLong);
    }
    if text.chars().any(char::is_control) {
        return Err(SpeakError::ControlCharacters);
    }
    Ok(text.to_owned())
}

/// Home Assistant domain of an entity id.
#[must_use]
pub fn domain_of(entity: &str) -> &str {
    entity.split_once('.').map_or(
        "",
        |(domain, _)| if domain.is_empty() { "" } else { domain },
    )
}

/// Apply the rules to discovered entities.
///
/// Output order is stable: profile, then scenarios, then devices; within each,
/// by room (unassigned last), title, id.
#[must_use]
pub fn build(raws: &[Raw], rules: &Rules) -> Vec<Entry> {
    let mut entries: Vec<Entry> = Vec::with_capacity(raws.len());
    // Devices that expose a light control. Home Assistant reports a `null`
    // category for Yeelight-style setting switches; the light sibling is the
    // only signal that they are settings rather than independent switches.
    let light_devices: HashSet<&str> = raws.iter().filter_map(device_with_light).collect();
    // Devices on which the integration itself marks categories. A device that
    // already has a config/diagnostic entity is fully annotated in the
    // registry, so its null-category switches are ordinary controls (the
    // station has a "Ring" light but its Microphones/Clock display switches
    // are still controls). Yeelight marks nothing, so the light heuristic
    // applies there.
    let categorized_devices: HashSet<&str> = raws
        .iter()
        .filter(|raw| raw.role.trim().eq_ignore_ascii_case("setting"))
        .filter_map(device_key)
        .collect();
    for raw in raws {
        if !valid_entity(&raw.entity) {
            continue;
        }
        let domain = domain_of(&raw.entity);
        let Some(mut kind) = kind_of(domain) else {
            continue;
        };
        if raw.hidden || excluded(&rules.exclude, &raw.entity) {
            continue;
        }
        // Scripts are exposed only as declared scenarios.
        if domain == "script" && !raw.scenario {
            continue;
        }
        // A switch carrying the profile label is the profile selector, not a
        // device. The role only means something for an on/off entity.
        if raw.profile && domain == "switch" {
            kind = Kind::Profile;
        }

        let state = raw.state.trim().to_lowercase();
        let available = !state.is_empty() && state != "unavailable" && state != "unknown";
        let mut entry = Entry {
            id: raw.entity.clone(),
            kind,
            title: title_of(raw),
            title_key: String::new(),
            title_args: std::collections::BTreeMap::new(),
            room: truncate_chars(raw.room.trim(), MAX_TEXT),
            icon: icon_of(&raw.icon),
            entity: raw.entity.clone(),
            state: String::new(),
            available,
            device_id: truncate_chars(raw.device_id.trim(), MAX_TEXT),
            device: truncate_chars(raw.device.trim(), MAX_TEXT),
            role: role_of(
                raw,
                device_key(raw).is_some_and(|key| light_devices.contains(key)),
                device_key(raw).is_some_and(|key| categorized_devices.contains(key)),
            ),
            light: None,
        };
        if kind == Kind::Light && available {
            entry.light.clone_from(&raw.light);
        }
        if kind != Kind::Scenario && available && (state == "on" || state == "off") {
            entry.state = state;
        }
        entries.push(entry);
    }

    if let Some(entry) = speak_entry(raws) {
        entries.push(entry);
    }

    entries.sort_by(|a, b| {
        let rank_a = rank(a.kind);
        let rank_b = rank(b.kind);
        rank_a
            .cmp(&rank_b)
            .then_with(|| (a.room.is_empty()).cmp(&b.room.is_empty()))
            .then_with(|| a.room.cmp(&b.room))
            .then_with(|| a.title.to_lowercase().cmp(&b.title.to_lowercase()))
            .then_with(|| a.id.cmp(&b.id))
    });
    entries.truncate(MAX_ENTRIES);
    entries
}

fn rank(kind: Kind) -> u8 {
    match kind {
        Kind::Profile => 0,
        Kind::Scenario => 1,
        Kind::Action => 2,
        Kind::Light | Kind::Switch => 3,
    }
}

/// Title and icon of the synthetic speak action. They are fixed, because the
/// action has no Home Assistant entity of its own to take a name from.
/// Legacy English title of the synthetic speak action. Current clients render
/// [`SPEAK_TITLE_KEY`] instead.
const SPEAK_TITLE: &str = "Speak on the station";
/// Stable localization key of the synthetic speak action title.
pub const SPEAK_TITLE_KEY: &str = "catalog.action.speak_title";
const SPEAK_ICON: &str = "message-text";

/// The synthetic "speak this text on the speaker" entry.
///
/// It exists only while Home Assistant exposes [`SPEAK_SCRIPT`]. Its device
/// and room come from the satellite entity when discovery saw one; without
/// that metadata the action is still offered, just ungrouped. The script is
/// looked up by its fixed id, so neither the payload nor an attacker-chosen
/// entity can redirect the call.
fn speak_entry(raws: &[Raw]) -> Option<Entry> {
    let script = raws
        .iter()
        .find(|raw| raw.entity == SPEAK_SCRIPT && !raw.hidden)?;
    let anchor = raws
        .iter()
        .find(|raw| domain_of(&raw.entity) == SPEAK_SATELLITE_DOMAIN && valid_entity(&raw.entity));
    let state = script.state.trim().to_lowercase();
    Some(Entry {
        id: SPEAK_ENTRY_ID.to_owned(),
        kind: Kind::Action,
        title: SPEAK_TITLE.to_owned(),
        title_key: SPEAK_TITLE_KEY.to_owned(),
        title_args: std::collections::BTreeMap::new(),
        room: anchor.map_or_else(String::new, |raw| truncate_chars(raw.room.trim(), MAX_TEXT)),
        icon: SPEAK_ICON.to_owned(),
        entity: SPEAK_SCRIPT.to_owned(),
        state: String::new(),
        available: !state.is_empty() && state != "unavailable" && state != "unknown",
        device_id: anchor.map_or_else(String::new, |raw| {
            truncate_chars(raw.device_id.trim(), MAX_TEXT)
        }),
        device: anchor.map_or_else(String::new, |raw| {
            truncate_chars(raw.device.trim(), MAX_TEXT)
        }),
        role: "primary".to_owned(),
        light: None,
    })
}

fn kind_of(domain: &str) -> Option<Kind> {
    match domain {
        "script" | "scene" => Some(Kind::Scenario),
        "light" => Some(Kind::Light),
        "switch" => Some(Kind::Switch),
        _ => None,
    }
}

fn excluded(patterns: &[String], entity: &str) -> bool {
    patterns
        .iter()
        .any(|pattern| glob_match(pattern, entity).unwrap_or(false))
}

/// Whether the entity is a device setting rather than a primary control.
///
/// Discovery fills [`Raw::role`] from the Home Assistant entity registry
/// category (`config`/`diagnostic` → `setting`, a known `null` → `primary`,
/// an unreadable registry → empty).
///
/// A `primary` switch whose device also has a light entity is a setting only
/// when the integration marks no categories on that device at all: the light
/// is then the control and the switches are its options (Yeelight). If the
/// device already has a `config`/`diagnostic` entity, the registry is
/// authoritative and its `null` switches stay primary (the station's
/// Microphones and Clock display despite the "Ring" light). Unknown roles stay
/// empty, so a registry outage never claims a category.
fn role_of(raw: &Raw, device_has_light: bool, device_has_categories: bool) -> String {
    let role = raw.role.trim();
    if role.eq_ignore_ascii_case("setting") {
        return "setting".to_owned();
    }
    if role.eq_ignore_ascii_case("primary") {
        if device_has_light && !device_has_categories && domain_of(&raw.entity) == "switch" {
            return "setting".to_owned();
        }
        return "primary".to_owned();
    }
    String::new()
}

/// Grouping key of a device: its registry id, or the display name as fallback.
fn device_key(raw: &Raw) -> Option<&str> {
    let id = raw.device_id.trim();
    if !id.is_empty() {
        return Some(id);
    }
    let name = raw.device.trim();
    (!name.is_empty()).then_some(name)
}

fn device_with_light(raw: &Raw) -> Option<&str> {
    if domain_of(&raw.entity) == "light" {
        device_key(raw)
    } else {
        None
    }
}

/// Human title of an entity.
///
/// Home Assistant builds `friendly_name` as `<device name> <entity name>`, so
/// the device prefix is removed when it is present. The remainder then loses
/// leading separators (`*`, `-`, `:` …) and repeated whitespace. When only the
/// device prefix was there, the device name itself becomes the title.
fn title_of(raw: &Raw) -> String {
    let name = raw.name.trim();
    let device = raw.device.trim();
    let title = if device.is_empty() {
        clean_spaces(name)
    } else {
        strip_device_prefix(name, device).map_or_else(
            || clean_spaces(name),
            |rest| {
                let rest = strip_leading_separators(rest);
                if rest.is_empty() {
                    clean_spaces(device)
                } else {
                    rest
                }
            },
        )
    };
    if !title.is_empty() {
        return truncate_chars(&title, MAX_TEXT);
    }
    // "light.bedside_lamp" -> "Bedside lamp"
    let object = raw
        .entity
        .split_once('.')
        .map_or(raw.entity.as_str(), |(_, object)| object)
        .replace('_', " ");
    if object.is_empty() {
        return raw.entity.clone();
    }
    let mut characters = object.chars();
    let first = characters.next().unwrap_or_default();
    format!("{}{rest}", first.to_uppercase(), rest = characters.as_str())
}

/// Split a `friendly_name` that starts with the device name.
///
/// The comparison is case-insensitive and returns the remainder untouched, or
/// `None` when the name does not start with the device.
fn strip_device_prefix<'a>(name: &'a str, device: &str) -> Option<&'a str> {
    let head = name.get(..device.len())?;
    if head.to_lowercase() == device.to_lowercase() {
        Some(&name[device.len()..])
    } else {
        None
    }
}

/// Drop leading separators left after the device prefix and collapse spaces.
fn strip_leading_separators(text: &str) -> String {
    clean_spaces(text.trim_start_matches(is_separator))
}

fn is_separator(character: char) -> bool {
    character.is_whitespace()
        || matches!(
            character,
            '*' | '-' | '–' | '—' | ':' | ';' | '·' | '•' | '|' | '/' | '\\' | ','
        )
}

fn clean_spaces(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn icon_of(icon: &str) -> String {
    let icon = icon.trim();
    let icon = icon.strip_prefix("mdi:").unwrap_or(icon).to_lowercase();
    let filtered: String = icon
        .chars()
        .filter(|character| {
            character.is_ascii_lowercase()
                || character.is_ascii_digit()
                || *character == '_'
                || *character == '-'
        })
        .collect();
    truncate_chars(&filtered, MAX_ICON)
}

fn truncate_chars(text: &str, limit: usize) -> String {
    if text.chars().count() <= limit {
        return text.to_owned();
    }
    text.chars().take(limit).collect()
}

fn valid_entity(entity: &str) -> bool {
    let Some((domain, object)) = entity.split_once('.') else {
        return false;
    };
    !domain.is_empty()
        && !object.is_empty()
        && domain
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'_')
        && object
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'_')
}

fn valid_label(label: &str) -> bool {
    !label.is_empty()
        && label.len() <= 64
        && label.bytes().all(|byte| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'_' || byte == b'-'
        })
}

/// Validate a `path.Match` pattern the way Go does before matching.
fn validate_glob(pattern: &str) -> Result<(), ()> {
    let characters: Vec<char> = pattern.chars().collect();
    let mut index = 0;
    while index < characters.len() {
        match characters[index] {
            '\\' => {
                if index + 1 >= characters.len() {
                    return Err(());
                }
                index += 2;
            }
            '[' => {
                let mut cursor = index + 1;
                if characters.get(cursor) == Some(&'^') {
                    cursor += 1;
                }
                let mut closed = false;
                while cursor < characters.len() {
                    match characters[cursor] {
                        '\\' => {
                            if cursor + 1 >= characters.len() {
                                return Err(());
                            }
                            cursor += 2;
                        }
                        ']' if cursor > index + 1 => {
                            closed = true;
                            break;
                        }
                        _ => cursor += 1,
                    }
                }
                if !closed {
                    return Err(());
                }
                index = cursor + 1;
            }
            _ => index += 1,
        }
    }
    Ok(())
}

/// Go `path.Match` for the entity ids this catalog uses. Entity ids never
/// contain `/`, so the separator rule collapses to "`*` and `?` match any
/// character". Malformed patterns are reported, matching `path.Match`'s
/// `ErrBadPattern`.
fn glob_match(pattern: &str, name: &str) -> Result<bool, ()> {
    validate_glob(pattern)?;
    let pattern: Vec<char> = pattern.chars().collect();
    let name: Vec<char> = name.chars().collect();
    match_pattern(&pattern, &name)
}

fn match_pattern(pattern: &[char], name: &[char]) -> Result<bool, ()> {
    let mut pattern = pattern;
    let mut name = name;
    loop {
        match pattern.first() {
            None => return Ok(name.is_empty()),
            Some('*') => {
                while pattern.first() == Some(&'*') {
                    pattern = &pattern[1..];
                }
                if pattern.is_empty() {
                    return Ok(true);
                }
                for start in 0..=name.len() {
                    if match_pattern(pattern, &name[start..])? {
                        return Ok(true);
                    }
                }
                return Ok(false);
            }
            Some('?') => {
                let Some((_, rest)) = name.split_first() else {
                    return Ok(false);
                };
                name = rest;
                pattern = &pattern[1..];
            }
            Some('[') => {
                let Some((matched, consumed)) = match_class(pattern, name)? else {
                    return Ok(false);
                };
                pattern = &pattern[consumed..];
                name = &name[1..];
                debug_assert!(matched);
            }
            Some('\\') => {
                let Some(&literal) = pattern.get(1) else {
                    return Err(());
                };
                let Some((&character, rest)) = name.split_first() else {
                    return Ok(false);
                };
                if character != literal {
                    return Ok(false);
                }
                name = rest;
                pattern = &pattern[2..];
            }
            Some(&literal) => {
                let Some((&character, rest)) = name.split_first() else {
                    return Ok(false);
                };
                if character != literal {
                    return Ok(false);
                }
                name = rest;
                pattern = &pattern[1..];
            }
        }
    }
}

/// Match one character class; returns whether the first name character matched
/// and how many pattern characters the class spans. `None` means the class does
/// not match.
fn match_class(pattern: &[char], name: &[char]) -> Result<Option<(bool, usize)>, ()> {
    let Some(&character) = name.first() else {
        return Ok(None);
    };
    let mut index = 1;
    let mut negate = false;
    if pattern.get(index) == Some(&'^') {
        negate = true;
        index += 1;
    }
    let mut matched = false;
    let mut first_in_class = true;
    loop {
        let Some(&current) = pattern.get(index) else {
            return Err(());
        };
        if current == ']' && !first_in_class {
            index += 1;
            break;
        }
        first_in_class = false;
        let (low, next) = class_char(pattern, index)?;
        index = next;
        if pattern.get(index) == Some(&'-') {
            let (high, next) = class_char(pattern, index + 1)?;
            index = next;
            if low <= character && character <= high {
                matched = true;
            }
        } else if low == character {
            matched = true;
        }
    }
    if matched == negate {
        Ok(None)
    } else {
        Ok(Some((true, index)))
    }
}

fn class_char(pattern: &[char], index: usize) -> Result<(char, usize), ()> {
    match pattern.get(index) {
        Some('\\') => pattern
            .get(index + 1)
            .map(|&character| (character, index + 2))
            .ok_or(()),
        Some(&character) => Ok((character, index + 1)),
        None => Err(()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rules() -> Rules {
        Rules {
            scenario_label: "scenario".to_owned(),
            profile_label: "voice_profile".to_owned(),
            exclude: vec!["switch.*_child_lock".to_owned()],
        }
    }

    fn ids(entries: &[Entry]) -> Vec<&str> {
        entries.iter().map(|entry| entry.id.as_str()).collect()
    }

    #[test]
    fn build_exposes_profile_scenarios_and_devices_only() {
        let raws = vec![
            Raw {
                entity: "switch.voice_profile".to_owned(),
                state: "off".to_owned(),
                name: "Voice profile".to_owned(),
                profile: true,
                ..Raw::default()
            },
            Raw {
                entity: "light.bedside_lamp".to_owned(),
                state: "on".to_owned(),
                name: "Bedside lamp".to_owned(),
                room: "Bedroom".to_owned(),
                ..Raw::default()
            },
            Raw {
                entity: "switch.desk_fan".to_owned(),
                state: "unavailable".to_owned(),
                name: "Desk fan".to_owned(),
                room: "Workspace".to_owned(),
                ..Raw::default()
            },
            Raw {
                entity: "scene.movie".to_owned(),
                state: "x".to_owned(),
                name: "Movie".to_owned(),
                ..Raw::default()
            },
            Raw {
                entity: "script.blackout".to_owned(),
                state: "off".to_owned(),
                name: "Blackout".to_owned(),
                scenario: true,
                ..Raw::default()
            },
            Raw {
                entity: "script.alarm_start".to_owned(),
                state: "off".to_owned(),
                name: "Alarm start".to_owned(),
                icon: "mdi:alarm".to_owned(),
                ..Raw::default()
            },
            Raw {
                entity: "light.secret".to_owned(),
                state: "on".to_owned(),
                hidden: true,
                ..Raw::default()
            },
            Raw {
                entity: "sensor.temp".to_owned(),
                state: "21".to_owned(),
                ..Raw::default()
            },
            Raw {
                entity: "switch.router_child_lock".to_owned(),
                state: "off".to_owned(),
                ..Raw::default()
            },
        ];
        assert_eq!(
            ids(&build(&raws, &rules())),
            vec![
                "switch.voice_profile",
                "script.blackout",
                "scene.movie",
                "light.bedside_lamp",
                "switch.desk_fan"
            ]
        );
    }

    #[test]
    fn profile_role_applies_to_switches_only() {
        let raws = vec![
            Raw {
                entity: "light.labelled".to_owned(),
                state: "on".to_owned(),
                profile: true,
                ..Raw::default()
            },
            Raw {
                entity: "switch.profile".to_owned(),
                state: "on".to_owned(),
                profile: true,
                ..Raw::default()
            },
        ];
        let entries = build(&raws, &rules());
        assert_eq!(entries[0].id, "switch.profile");
        assert_eq!(entries[0].kind, Kind::Profile);
        assert_eq!(entries[1].kind, Kind::Light);
    }

    #[test]
    fn titles_drop_the_device_prefix_and_leading_separators() {
        let cases = [
            ("Светильник  Light", "Светильник", "Light"),
            (
                "Светильник * yl-light delay-off",
                "Светильник",
                "yl-light delay-off",
            ),
            (
                "Яндекс Станция Миди Wake: разбуди меня",
                "Яндекс Станция Миди",
                "Wake: разбуди меня",
            ),
            ("Лампа", "Светильник", "Лампа"),
            ("Светильник", "Светильник", "Светильник"),
            ("Светильник - ", "Светильник", "Светильник"),
            ("  Просто   лампа  ", "", "Просто лампа"),
            ("Светильник  ", "Светильник", "Светильник"),
            ("светильник lower", "Светильник", "lower"),
        ];
        for (name, device, expected) in cases {
            let raw = Raw {
                entity: "light.x".to_owned(),
                name: name.to_owned(),
                device: device.to_owned(),
                ..Raw::default()
            };
            assert_eq!(title_of(&raw), expected, "{name:?} / {device:?}");
        }
    }

    #[test]
    fn device_metadata_and_role_reach_the_entry() {
        let raws = [
            Raw {
                entity: "switch.delay".to_owned(),
                state: "off".to_owned(),
                name: "Светильник * yl-light delay-off".to_owned(),
                device_id: "dev-1".to_owned(),
                device: "Светильник".to_owned(),
                role: "setting".to_owned(),
                ..Raw::default()
            },
            Raw {
                entity: "light.lamp".to_owned(),
                state: "on".to_owned(),
                name: "Светильник Light".to_owned(),
                device_id: "dev-1".to_owned(),
                device: "Светильник".to_owned(),
                ..Raw::default()
            },
        ];
        let entries = build(&raws, &rules());
        let setting = entries
            .iter()
            .find(|entry| entry.id == "switch.delay")
            .expect("setting");
        assert_eq!(setting.title, "yl-light delay-off");
        assert_eq!(setting.device_id, "dev-1");
        assert_eq!(setting.device, "Светильник");
        assert_eq!(setting.role, "setting");
        // Without metadata the fields stay empty and nothing is lost.
        let lamp = entries
            .iter()
            .find(|entry| entry.id == "light.lamp")
            .expect("lamp");
        assert_eq!(lamp.title, "Light");
        assert_eq!(lamp.role, "");
    }

    #[test]
    fn switches_next_to_a_light_are_settings_and_other_switches_stay_primary() {
        let raws = [
            // Yeelight: the light is the control, null-category switches are
            // its settings.
            Raw {
                entity: "light.yeelight".to_owned(),
                state: "on".to_owned(),
                name: "Светильник Light".to_owned(),
                device_id: "yeelight-1".to_owned(),
                device: "Светильник".to_owned(),
                role: "primary".to_owned(),
                ..Raw::default()
            },
            Raw {
                entity: "switch.yeelight_delay_off".to_owned(),
                state: "off".to_owned(),
                name: "Светильник yl-light delay-off".to_owned(),
                device_id: "yeelight-1".to_owned(),
                device: "Светильник".to_owned(),
                role: "primary".to_owned(),
                ..Raw::default()
            },
            // Station: config entities are settings, null-category controls
            // stay primary because the device has no light entity.
            Raw {
                entity: "switch.station_wake".to_owned(),
                state: "off".to_owned(),
                name: "Яндекс Станция Миди Wake".to_owned(),
                device_id: "station-1".to_owned(),
                device: "Яндекс Станция Миди".to_owned(),
                role: "setting".to_owned(),
                ..Raw::default()
            },
            Raw {
                entity: "switch.station_mute".to_owned(),
                state: "off".to_owned(),
                name: "Яндекс Станция Миди Mute microphones".to_owned(),
                device_id: "station-1".to_owned(),
                device: "Яндекс Станция Миди".to_owned(),
                role: "primary".to_owned(),
                ..Raw::default()
            },
            // Registry outage: the role stays unknown even next to a light.
            Raw {
                entity: "switch.yeelight_pomodoro".to_owned(),
                state: "off".to_owned(),
                device_id: "yeelight-1".to_owned(),
                device: "Светильник".to_owned(),
                ..Raw::default()
            },
        ];
        let entries = build(&raws, &rules());
        let role = |id: &str| {
            entries
                .iter()
                .find(|entry| entry.id == id)
                .unwrap_or_else(|| panic!("missing {id}"))
                .role
                .as_str()
        };
        assert_eq!(role("light.yeelight"), "primary");
        assert_eq!(role("switch.yeelight_delay_off"), "setting");
        assert_eq!(role("switch.station_wake"), "setting");
        assert_eq!(role("switch.station_mute"), "primary");
        assert_eq!(role("switch.yeelight_pomodoro"), "");
    }

    #[test]
    fn a_categorized_device_trusts_the_registry_even_with_a_light() {
        // Live station: it has a "Ring" light and a few config entities, but
        // the remaining null-category switches are ordinary controls.
        let raws = [
            Raw {
                entity: "light.station_ring".to_owned(),
                state: "on".to_owned(),
                name: "Яндекс Станция Миди Ring".to_owned(),
                device_id: "station-1".to_owned(),
                device: "Яндекс Станция Миди".to_owned(),
                role: "primary".to_owned(),
                ..Raw::default()
            },
            Raw {
                entity: "switch.station_wake".to_owned(),
                state: "off".to_owned(),
                name: "Яндекс Станция Миди Wake".to_owned(),
                device_id: "station-1".to_owned(),
                device: "Яндекс Станция Миди".to_owned(),
                role: "setting".to_owned(),
                ..Raw::default()
            },
            Raw {
                entity: "switch.station_mute".to_owned(),
                state: "off".to_owned(),
                name: "Яндекс Станция Миди Microphones".to_owned(),
                device_id: "station-1".to_owned(),
                device: "Яндекс Станция Миди".to_owned(),
                role: "primary".to_owned(),
                ..Raw::default()
            },
            Raw {
                entity: "switch.station_clock_display".to_owned(),
                state: "on".to_owned(),
                name: "Яндекс Станция Миди Clock display".to_owned(),
                device_id: "station-1".to_owned(),
                device: "Яндекс Станция Миди".to_owned(),
                role: "primary".to_owned(),
                ..Raw::default()
            },
        ];
        let entries = build(&raws, &rules());
        let role = |id: &str| {
            entries
                .iter()
                .find(|entry| entry.id == id)
                .unwrap_or_else(|| panic!("missing {id}"))
                .role
                .as_str()
        };
        assert_eq!(role("light.station_ring"), "primary");
        assert_eq!(role("switch.station_wake"), "setting");
        assert_eq!(role("switch.station_mute"), "primary");
        assert_eq!(role("switch.station_clock_display"), "primary");
    }

    #[test]
    fn truncation_counts_characters() {
        let text = "ж".repeat(MAX_TEXT + 10);
        let title = truncate_chars(&text, MAX_TEXT);
        assert_eq!(title.chars().count(), MAX_TEXT);
        assert!(title.chars().all(|character| character == 'ж'));
    }

    #[test]
    fn resolve_returns_the_go_service_calls() {
        let entries = build(
            &[
                Raw {
                    entity: "scene.focus".to_owned(),
                    state: "x".to_owned(),
                    ..Raw::default()
                },
                Raw {
                    entity: "light.lamp".to_owned(),
                    state: "on".to_owned(),
                    light: Some(
                        Light::new(LightInput {
                            color_modes: vec!["color_temp".to_owned()],
                            min_kelvin: 2700,
                            max_kelvin: 6500,
                            brightness: -1,
                            ..LightInput::default()
                        })
                        .expect("light"),
                    ),
                    ..Raw::default()
                },
            ],
            &rules(),
        );
        let catalog = Catalog::new(entries);
        let scene = catalog.lookup("scene.focus").expect("scene");
        let call = scene.resolve(ACTION_RUN, &Params::default()).expect("run");
        assert_eq!(
            (
                call.domain.as_str(),
                call.service.as_str(),
                call.entity.as_str()
            ),
            ("scene", "turn_on", "scene.focus")
        );
        assert!(call.data.is_empty());

        let lamp = catalog.lookup("light.lamp").expect("lamp");
        let call = lamp
            .resolve(
                ACTION_SET,
                &Params {
                    brightness: Some(40),
                    kelvin: Some(3000),
                    ..Params::default()
                },
            )
            .expect("set");
        assert_eq!(call.data["brightness_pct"], Value::from(40));
        assert_eq!(call.data["color_temp_kelvin"], Value::from(3000));

        assert!(
            lamp.resolve(ACTION_RUN, &Params::default())
                .expect_err("run is not a light action")
                .to_string()
                .contains("not supported")
        );
    }

    #[test]
    fn resolve_forwards_effect_and_transition_and_rejects_unknown_effects() {
        let entries = build(
            &[Raw {
                entity: "light.lamp".to_owned(),
                state: "on".to_owned(),
                light: Some(Light {
                    brightness: true,
                    color_temp: true,
                    min_kelvin: 2700,
                    max_kelvin: 6500,
                    effects: vec!["Day".to_owned()],
                    ..Light::default()
                }),
                ..Raw::default()
            }],
            &rules(),
        );
        let catalog = Catalog::new(entries);
        let lamp = catalog.lookup("light.lamp").expect("lamp");
        let call = lamp
            .resolve(
                ACTION_SET,
                &Params {
                    brightness: Some(100),
                    kelvin: Some(2800),
                    effect: Some("Day".to_owned()),
                    transition: Some(0.5),
                    ..Params::default()
                },
            )
            .expect("set");
        assert_eq!(
            (
                call.domain.as_str(),
                call.service.as_str(),
                call.entity.as_str()
            ),
            ("light", "turn_on", "light.lamp")
        );
        assert_eq!(call.data["effect"], Value::from("Day"));
        assert_eq!(call.data["transition"], Value::from(0.5));

        // turn_on carries the same parameters.
        let call = lamp
            .resolve(
                ACTION_TURN_ON,
                &Params {
                    effect: Some("Day".to_owned()),
                    transition: Some(2.0),
                    ..Params::default()
                },
            )
            .expect("turn_on");
        assert_eq!(call.service, "turn_on");
        assert_eq!(call.data["effect"], Value::from("Day"));

        // turn_off accepts a transition and nothing else.
        let call = lamp
            .resolve(
                ACTION_TURN_OFF,
                &Params {
                    transition: Some(2.0),
                    ..Params::default()
                },
            )
            .expect("turn_off");
        assert_eq!(call.service, "turn_off");
        assert_eq!(call.data["transition"], Value::from(2.0));
        assert!(
            lamp.resolve(
                ACTION_TURN_OFF,
                &Params {
                    brightness: Some(10),
                    ..Params::default()
                }
            )
            .is_err()
        );

        // An effect the bulb does not advertise is rejected.
        assert_eq!(
            lamp.resolve(
                ACTION_SET,
                &Params {
                    effect: Some("Party".to_owned()),
                    ..Params::default()
                }
            )
            .expect_err("unknown effect"),
            ResolveError::Light(light::LightError::EffectUnknown)
        );
    }

    fn speak_raws(script_state: &str) -> Vec<Raw> {
        vec![
            Raw {
                entity: SPEAK_SCRIPT.to_owned(),
                state: script_state.to_owned(),
                name: "Speak text".to_owned(),
                ..Raw::default()
            },
            Raw {
                entity: "assist_satellite.station_satellite".to_owned(),
                state: "idle".to_owned(),
                name: "Яндекс Станция Миди Assist satellite".to_owned(),
                room: "Гостиная".to_owned(),
                device_id: "station-1".to_owned(),
                device: "Яндекс Станция Миди".to_owned(),
                ..Raw::default()
            },
        ]
    }

    #[test]
    fn speak_action_exists_only_with_the_home_assistant_script() {
        let entries = build(&speak_raws("off"), &rules());
        let speak = entries
            .iter()
            .find(|entry| entry.id == SPEAK_ENTRY_ID)
            .expect("speak action");
        assert_eq!(speak.kind, Kind::Action);
        assert_eq!(speak.kind.as_str(), "action");
        assert_eq!(speak.title, "Speak on the station");
        assert_eq!(speak.title_key, SPEAK_TITLE_KEY);
        assert_eq!(speak.actions(), vec![ACTION_SPEAK]);
        assert_eq!(speak.device_id, "station-1");
        assert_eq!(speak.device, "Яндекс Станция Миди");
        assert_eq!(speak.room, "Гостиная");
        assert!(speak.available);
        // The script itself is not a scenario (no label), so it adds no entry
        // of its own.
        assert!(!entries.iter().any(|entry| entry.id == SPEAK_SCRIPT));

        // Without the script the action is not advertised at all.
        let without: Vec<Raw> = speak_raws("off")
            .into_iter()
            .filter(|raw| raw.entity != SPEAK_SCRIPT)
            .collect();
        assert!(
            !build(&without, &rules())
                .iter()
                .any(|entry| entry.id == SPEAK_ENTRY_ID)
        );

        // A hidden script is disabled by the user, exactly like other entities.
        let mut hidden = speak_raws("off");
        if let Some(raw) = hidden.iter_mut().find(|raw| raw.entity == SPEAK_SCRIPT) {
            raw.hidden = true;
        }
        assert!(
            !build(&hidden, &rules())
                .iter()
                .any(|entry| entry.id == SPEAK_ENTRY_ID)
        );

        // Unavailable script: the action stays but is reported unavailable.
        let unavailable = build(&speak_raws("unavailable"), &rules());
        assert!(
            !unavailable
                .iter()
                .find(|entry| entry.id == SPEAK_ENTRY_ID)
                .expect("speak action")
                .available
        );
    }

    #[test]
    fn speak_without_a_satellite_stays_available_but_ungrouped() {
        let raws: Vec<Raw> = speak_raws("off")
            .into_iter()
            .filter(|raw| !raw.entity.starts_with(SPEAK_SATELLITE_DOMAIN))
            .collect();
        let speak = build(&raws, &rules())
            .into_iter()
            .find(|entry| entry.id == SPEAK_ENTRY_ID)
            .expect("speak action");
        assert!(speak.available);
        assert!(speak.device_id.is_empty());
        assert!(speak.device.is_empty());
        assert!(speak.room.is_empty());
    }

    #[test]
    fn speak_resolves_to_the_allowlisted_script_and_trims_the_text() {
        let entries = build(&speak_raws("off"), &rules());
        let speak = entries
            .iter()
            .find(|entry| entry.id == SPEAK_ENTRY_ID)
            .expect("speak action");
        let call = speak
            .resolve(
                ACTION_SPEAK,
                &Params {
                    text: Some("  Привет, дом.  ".to_owned()),
                    ..Params::default()
                },
            )
            .expect("speak");
        assert_eq!(
            (
                call.domain.as_str(),
                call.service.as_str(),
                call.entity.as_str()
            ),
            ("script", "turn_on", SPEAK_SCRIPT)
        );
        assert_eq!(
            call.data["variables"]["message"],
            Value::from("Привет, дом.")
        );
        assert_eq!(call.data.len(), 1);

        // The action accepts only `speak`.
        assert!(
            speak
                .resolve(ACTION_RUN, &Params::default())
                .expect_err("run is not an action")
                .to_string()
                .contains("not supported")
        );
    }

    #[test]
    fn speak_text_is_validated_without_echoing_it() {
        let entries = build(&speak_raws("off"), &rules());
        let speak = entries
            .iter()
            .find(|entry| entry.id == SPEAK_ENTRY_ID)
            .expect("speak action");
        let cases = [
            (None, "speak needs a text"),
            (Some(String::new()), "speak text is empty"),
            (Some("   \n\t ".to_owned()), "speak text is empty"),
            (
                Some("ж".repeat(SPEAK_MAX_TEXT + 1)),
                "speak text is longer than 500 characters",
            ),
            (
                Some("привет\u{0}мир".to_owned()),
                "speak text contains control characters",
            ),
        ];
        for (text, expected) in cases {
            let error = speak
                .resolve(
                    ACTION_SPEAK,
                    &Params {
                        text,
                        ..Params::default()
                    },
                )
                .expect_err("invalid text");
            assert_eq!(error.to_string(), expected);
            assert!(!error.to_string().contains("привет"));
        }
        // Exactly the limit is accepted.
        let limit = "ж".repeat(SPEAK_MAX_TEXT);
        assert!(
            speak
                .resolve(
                    ACTION_SPEAK,
                    &Params {
                        text: Some(limit),
                        ..Params::default()
                    }
                )
                .is_ok()
        );
    }

    #[test]
    fn rules_validate_like_the_go_loader() {
        assert!(rules().validate().is_ok());
        for invalid in [
            Rules::default(),
            Rules {
                scenario_label: "Phone App".to_owned(),
                profile_label: "voice_profile".to_owned(),
                ..Rules::default()
            },
            Rules {
                scenario_label: "ok".to_owned(),
                profile_label: "Voice Profile".to_owned(),
                ..Rules::default()
            },
            Rules {
                scenario_label: "ok".to_owned(),
                profile_label: "voice_profile".to_owned(),
                exclude: vec!["[".to_owned()],
            },
        ] {
            assert!(invalid.validate().is_err(), "accepted {invalid:?}");
        }
    }

    #[test]
    fn glob_patterns_match_path_match_semantics() {
        assert!(glob_match("switch.*_child_lock", "switch.router_child_lock").expect("match"));
        assert!(!glob_match("switch.*_child_lock", "switch.router").expect("match"));
        assert!(glob_match("light.ha?l", "light.hall").expect("match"));
        assert!(glob_match("light.[abc]all", "light.aall").expect("match"));
        assert!(!glob_match("light.[^abc]all", "light.aall").expect("match"));
        assert!(glob_match("switch.\\*", "switch.*").expect("match"));
        assert!(glob_match("[", "").is_err());
    }
}
