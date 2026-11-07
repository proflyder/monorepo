//! Renders the reviewed Hermes overlay into a live HERMES_HOME.
//!
//! Ported from `platform/deploy/internal/hermesconfig`: fragment, private
//! values and the voice prompt are merged in that order, TEST-NET/example
//! placeholders are refused, `.prev` backups are kept, and `--dry-run`/`--check`
//! behave like the Go implementation. The parsed tree is the contract; emitter
//! style differences are allowed (as in Go).

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::LazyLock;

use regex::Regex;

use crate::diff;
use crate::yaml::{self, Value};

static PLACEHOLDER_PATTERN: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)\b192\.0\.2\.\d{1,3}\b|\bexample\.(?:com|net|org)\b").expect("placeholder")
});
static SECRET_LINE_PATTERN: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r#"(?i)((?:^|\b)(?:api[_-]?key|token|secret|password|authorization|bearer)\b\s*[:=]\s*)(?:"[^"]*"|'[^']*'|\S+)"#,
    )
    .expect("secret line")
});

const PRIVATE_ONLY_KEYS: &[&str] = &["telegram_owner_chat_id"];

/// One render request.
pub struct Options {
    /// HERMES_HOME.
    pub home: String,
    /// Config fragment.
    pub fragment: String,
    /// Infra context installed as `.hermes.md`.
    pub context: String,
    /// Identity file installed as `SOUL.md`.
    pub soul: String,
    /// Source for `agent.personalities.voice`.
    pub voice_prompt: String,
    /// System prompt for the owner chat.
    pub telegram_prompt: String,
    /// Skill directories installed under `<home>/skills`.
    pub skills: String,
    /// Private values merged after the fragment (empty = none).
    pub private_values: String,
    /// Allow TEST-NET/example placeholders (tests only).
    pub allow_placeholders: bool,
}

/// One planned write.
pub struct File {
    /// Destination path.
    pub path: PathBuf,
    /// Complete file content.
    pub content: String,
}

/// Merges overlay into base: mappings merge, everything else replaces.
///
/// The owned signature mirrors the Go `interface{}` API and keeps callers from
/// juggling borrows for a tree that is rebuilt anyway.
#[must_use]
#[allow(clippy::needless_pass_by_value)]
pub fn deep_merge(base: Value, overlay: Value) -> Value {
    let (Value::Map(base_map), Value::Map(overlay_map)) = (&base, &overlay) else {
        return overlay;
    };
    let mut merged = base_map.clone();
    for (key, value) in overlay_map {
        if let Some((_, existing)) = merged.iter_mut().find(|(name, _)| name == key) {
            *existing = deep_merge(existing.clone(), value.clone());
        } else {
            merged.push((key.clone(), value.clone()));
        }
    }
    Value::Map(merged)
}

/// Yields `(dotted path, value)` for every non-mapping leaf.
#[must_use]
pub fn iter_leaves(data: &Value) -> Vec<(String, Value)> {
    let mut leaves = Vec::new();
    walk(data, &mut Vec::new(), &mut leaves);
    leaves
}

fn walk(node: &Value, path: &mut Vec<String>, leaves: &mut Vec<(String, Value)>) {
    if let Value::Map(entries) = node {
        for (key, value) in entries {
            path.push(key.clone());
            walk(value, path, leaves);
            path.pop();
        }
        return;
    }
    leaves.push((path.join("."), node.clone()));
}

/// Returns the keys whose placeholder value differs from (or is absent in) the
/// live config.
#[must_use]
pub fn placeholder_violations(merged: &Value, live: &Value) -> Vec<String> {
    let current: std::collections::BTreeMap<String, Value> =
        iter_leaves(live).into_iter().collect();
    let mut bad = std::collections::BTreeSet::new();
    for (path, value) in iter_leaves(merged) {
        let Value::Str(text) = &value else {
            continue;
        };
        if !PLACEHOLDER_PATTERN.is_match(text) {
            continue;
        }
        if current.get(&path) != Some(&value) {
            bad.insert(path);
        }
    }
    bad.into_iter().collect()
}

/// Serializes the merged document.
#[must_use]
pub fn dump_config(data: &Value) -> String {
    yaml::to_string(data)
}

/// Masks token-shaped values so a dry-run diff is safe to paste.
#[must_use]
pub fn redact(text: &str) -> String {
    SECRET_LINE_PATTERN
        .replace_all(text, "${1}<redacted>")
        .into_owned()
}

fn read_mapping(path: &str, what: &str) -> Result<Value, String> {
    let raw = fs::read_to_string(path).map_err(|error| format!("open {path}: {error}"))?;
    if raw.trim().is_empty() {
        return Ok(Value::Map(Vec::new()));
    }
    let value = yaml::parse(&raw).map_err(|error| format!("{path}: {error}"))?;
    if !value.is_mapping() {
        return Err(format!("{path}: {what} must be a YAML mapping"));
    }
    Ok(value)
}

/// Returns every file a render would change.
pub fn build_plan(options: &Options) -> Result<Vec<File>, String> {
    let config_path = Path::new(&options.home).join("config.yaml");
    let current = if config_path.exists() {
        read_mapping(&config_path.display().to_string(), "config.yaml")?
    } else {
        Value::Map(Vec::new())
    };

    let fragment = read_mapping(&options.fragment, "fragment")?;
    let mut merged = deep_merge(current.clone(), fragment);

    let mut owner_chat_id = String::new();
    if !options.private_values.is_empty() {
        if !Path::new(&options.private_values).exists() {
            return Err(format!(
                "{}: private values file not found",
                options.private_values
            ));
        }
        let mut private = read_mapping(&options.private_values, "private values")?;
        owner_chat_id = pop_string(&mut private, "telegram_owner_chat_id")
            .trim()
            .to_string();
        merged = deep_merge(merged, private);
    }
    for key in PRIVATE_ONLY_KEYS {
        merged.remove(key);
    }

    let voice = fs::read_to_string(&options.voice_prompt)
        .map_err(|_| format!("{}: voice prompt not found", options.voice_prompt))?;
    merged = set_voice_personality(merged, voice.trim())?;

    if !owner_chat_id.is_empty() {
        let prompt = fs::read_to_string(&options.telegram_prompt)
            .map_err(|_| format!("{}: Telegram prompt not found", options.telegram_prompt))?;
        merged = set_telegram_channel_prompt(merged, &owner_chat_id, prompt.trim())?;
        merged = set_telegram_home_channel(merged, &owner_chat_id)?;
    }

    if !options.allow_placeholders {
        let victims = placeholder_violations(&merged, &current);
        if !victims.is_empty() {
            let mut message =
                String::from("refusing to render: placeholder values would replace live ones:\n");
            for key in &victims {
                message.push_str("  ");
                message.push_str(key);
                message.push('\n');
            }
            message.push_str(
                "\nPass --private-values <file> with the real endpoints (copy \
                 hermes-private.example.yaml), or --allow-placeholders when rendering \
                 a throw-away test home.",
            );
            return Err(message);
        }
    }

    let mut plan = Vec::new();
    let rendered = dump_config(&merged);
    if fs::read_to_string(&config_path).map_or(true, |existing| existing != rendered) {
        plan.push(File {
            path: config_path,
            content: rendered,
        });
    }
    for (name, source) in [(".hermes.md", &options.context), ("SOUL.md", &options.soul)] {
        let Ok(raw) = fs::read(source) else {
            continue;
        };
        let destination = Path::new(&options.home).join(name);
        if fs::read_to_string(&destination)
            .is_ok_and(|existing| existing.as_bytes() == raw.as_slice())
        {
            continue;
        }
        plan.push(File {
            path: destination,
            content: String::from_utf8_lossy(&raw).into_owned(),
        });
    }
    plan.extend(skill_plan(&options.home, &options.skills)?);
    Ok(plan)
}

fn set_voice_personality(mut merged: Value, voice: &str) -> Result<Value, String> {
    let mut agent = child_mapping(&merged, "agent", "config.yaml: agent must be a mapping")?;
    let mut personalities = child_mapping(
        &agent,
        "personalities",
        "config.yaml: agent.personalities must be a mapping",
    )?;
    personalities.set("voice", Value::Str(voice.to_string()));
    agent.set("personalities", personalities);
    merged.set("agent", agent);
    Ok(merged)
}

fn set_telegram_channel_prompt(
    mut merged: Value,
    chat_id: &str,
    prompt: &str,
) -> Result<Value, String> {
    let mut platforms = child_mapping(
        &merged,
        "platforms",
        "config.yaml: platforms must be a mapping",
    )?;
    let mut telegram = child_mapping(
        &platforms,
        "telegram",
        "config.yaml: platforms.telegram must be a mapping",
    )?;
    let mut overrides = child_mapping(
        &telegram,
        "channel_overrides",
        "config.yaml: platforms.telegram.channel_overrides must be a mapping",
    )?;
    let mut channel = child_mapping(
        &overrides,
        chat_id,
        "config.yaml: platforms.telegram.channel_overrides entry must be a mapping",
    )?;
    channel.set("system_prompt", Value::Str(prompt.to_string()));
    overrides.set(chat_id, channel);
    telegram.set("channel_overrides", overrides);
    platforms.set("telegram", telegram);
    merged.set("platforms", platforms);
    Ok(merged)
}

fn set_telegram_home_channel(mut merged: Value, chat_id: &str) -> Result<Value, String> {
    let mut platforms = child_mapping(
        &merged,
        "platforms",
        "config.yaml: platforms must be a mapping",
    )?;
    let mut telegram = child_mapping(
        &platforms,
        "telegram",
        "config.yaml: platforms.telegram must be a mapping",
    )?;
    let name = if let Some(Value::Map(current)) = telegram.get("home_channel")
        && let Some(existing) = current
            .iter()
            .find(|(key, _)| key == "name")
            .map(|(_, value)| value.as_str().trim())
        && !existing.is_empty()
    {
        existing.to_string()
    } else {
        "Home".to_string()
    };
    telegram.set(
        "home_channel",
        Value::Map(vec![
            ("platform".to_string(), Value::Str("telegram".to_string())),
            ("chat_id".to_string(), Value::Str(chat_id.to_string())),
            ("name".to_string(), Value::Str(name)),
        ]),
    );
    platforms.set("telegram", telegram);
    merged.set("platforms", platforms);
    Ok(merged)
}

fn skill_plan(home: &str, skills_dir: &str) -> Result<Vec<File>, String> {
    let skills = Path::new(skills_dir);
    let Ok(info) = fs::metadata(skills) else {
        return Ok(Vec::new());
    };
    if !info.is_dir() {
        return Ok(Vec::new());
    }
    let mut entries: Vec<PathBuf> = fs::read_dir(skills)
        .map_err(|error| error.to_string())?
        .flatten()
        .map(|entry| entry.path())
        .collect();
    entries.sort();
    let mut plan = Vec::new();
    for skill_dir in entries {
        if !skill_dir.is_dir() {
            continue;
        }
        let skill_file = skill_dir.join("SKILL.md");
        if !skill_file.is_file() {
            continue;
        }
        let mut files = Vec::new();
        collect_files(&skill_dir, &mut files)?;
        files.sort();
        for path in files {
            let Ok(relative) = path.strip_prefix(skills) else {
                continue;
            };
            let destination = Path::new(home).join("skills").join(relative);
            let raw = fs::read(&path).map_err(|error| error.to_string())?;
            if fs::read_to_string(&destination).is_ok_and(|existing| existing.as_bytes() == raw) {
                continue;
            }
            plan.push(File {
                path: destination,
                content: String::from_utf8_lossy(&raw).into_owned(),
            });
        }
    }
    Ok(plan)
}

fn collect_files(path: &Path, files: &mut Vec<PathBuf>) -> Result<(), String> {
    if path.is_file() {
        files.push(path.to_path_buf());
        return Ok(());
    }
    let mut entries: Vec<PathBuf> = fs::read_dir(path)
        .map_err(|error| error.to_string())?
        .flatten()
        .map(|entry| entry.path())
        .collect();
    entries.sort();
    for entry in entries {
        collect_files(&entry, files)?;
    }
    Ok(())
}

fn child_mapping(parent: &Value, key: &str, message: &str) -> Result<Value, String> {
    match parent.get(key) {
        None | Some(Value::Null) => Ok(Value::Map(Vec::new())),
        Some(value @ Value::Map(_)) => Ok(value.clone()),
        Some(_) => Err(message.to_string()),
    }
}

fn pop_string(mapping: &mut Value, key: &str) -> String {
    let value = mapping.get(key).cloned();
    mapping.remove(key);
    match value {
        Some(Value::Str(text)) => text,
        _ => String::new(),
    }
}

/// Returns the redacted unified diff of one planned write.
#[must_use]
pub fn diff_for(path: &Path, new_text: &str) -> String {
    let old = fs::read_to_string(path).unwrap_or_default();
    let rendered = diff::unified_diff(&path.display().to_string(), &old, new_text);
    redact(&rendered)
}

/// Writes the plan, keeping at most one `.prev` copy (mode 600).
pub fn apply_plan(plan: &[File], stdout: &mut dyn Write) -> Result<Vec<String>, String> {
    let mut written = Vec::new();
    let stamp = time::OffsetDateTime::now_utc()
        .format(&time::macros::format_description!(
            "[year]-[month]-[day]T[hour]:[minute]:[second]Z"
        ))
        .map_err(|error| error.to_string())?;
    for file in plan {
        if let Ok(raw) = fs::read(&file.path) {
            let backup = PathBuf::from(format!("{}.prev", file.path.display()));
            fs::write(&backup, raw).map_err(|error| error.to_string())?;
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                let _ = fs::set_permissions(&backup, fs::Permissions::from_mode(0o600));
            }
        }
        if let Some(parent) = file.path.parent() {
            fs::create_dir_all(parent).map_err(|error| error.to_string())?;
        }
        fs::write(&file.path, file.content.as_bytes()).map_err(|error| error.to_string())?;
        written.push(file.path.display().to_string());
    }
    let _ = writeln!(stdout, "rendered {} file(s) at {stamp}", written.len());
    Ok(written)
}

/// Renders the overlay. Returns 1 for `--check` with pending changes.
pub fn run(
    options: &Options,
    dry_run: bool,
    check: bool,
    stdout: &mut dyn Write,
) -> Result<i32, String> {
    let home = Path::new(&options.home);
    if !home.is_dir() {
        return Err(format!(
            "{}: not a directory (pass the mounted HERMES_HOME)",
            options.home
        ));
    }
    if !Path::new(&options.fragment).exists() {
        return Err(format!("{}: fragment not found", options.fragment));
    }
    let plan = build_plan(options)?;
    if plan.is_empty() {
        let _ = writeln!(
            stdout,
            "up to date: config.yaml, .hermes.md, SOUL.md and skills already match the repository"
        );
        return Ok(0);
    }
    for file in &plan {
        let _ = writeln!(stdout, "--- would update {} ---", file.path.display());
        let _ = write!(stdout, "{}", diff_for(&file.path, &file.content));
    }
    if dry_run || check {
        return Ok(if check { 1 } else { 0 });
    }
    apply_plan(&plan, stdout)?;
    let _ = writeln!(
        stdout,
        "restart the gateway to load the new config and context"
    );
    Ok(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mapping(text: &str) -> Value {
        yaml::parse(text).expect("yaml")
    }

    #[test]
    fn nested_mappings_merge_and_scalars_win() {
        let base = mapping("a:\n  b: 1\n  c: 2\nlist:\n- one\n");
        let overlay = mapping("a:\n  c: 3\nlist:\n- two\n");
        let merged = deep_merge(base, overlay);
        assert_eq!(
            merged.get("a").and_then(|a| a.get("b")),
            Some(&Value::Int(1))
        );
        assert_eq!(
            merged.get("a").and_then(|a| a.get("c")),
            Some(&Value::Int(3))
        );
        let leaves = iter_leaves(&merged);
        assert!(
            leaves
                .iter()
                .any(|(path, value)| path == "list" && value.is_sequence())
        );
    }

    #[test]
    fn token_shaped_values_are_masked() {
        assert_eq!(redact("api_key: secret-value\n"), "api_key: <redacted>\n");
        assert_eq!(redact("ordinary line\n"), "ordinary line\n");
    }

    #[test]
    fn test_net_values_are_refused_over_live_endpoints() {
        let merged = mapping("ha:\n  url: http://192.0.2.10:8123\n");
        let live = mapping("ha:\n  url: http://real:8123\n");
        assert_eq!(placeholder_violations(&merged, &live), vec!["ha.url"]);
        assert!(placeholder_violations(&merged, &merged).is_empty());
    }

    #[test]
    fn voice_personality_comes_from_the_prompt() {
        let merged =
            set_voice_personality(mapping("agent:\n  name: hermes\n"), "speak").expect("voice");
        assert_eq!(
            merged
                .get("agent")
                .and_then(|agent| agent.get("personalities"))
                .and_then(|personalities| personalities.get("voice"))
                .map(Value::as_str),
            Some("speak")
        );
    }

    #[test]
    fn telegram_prompt_and_home_channel_are_installed() {
        let merged = set_telegram_channel_prompt(
            mapping("platforms:\n  telegram:\n    channel_overrides:\n      '1':\n        a: b\n"),
            "1",
            "prompt",
        )
        .expect("prompt");
        let merged = set_telegram_home_channel(merged, "1").expect("home");
        let telegram = merged
            .get("platforms")
            .and_then(|p| p.get("telegram"))
            .expect("telegram");
        assert_eq!(
            telegram
                .get("channel_overrides")
                .and_then(|o| o.get("1"))
                .and_then(|c| c.get("system_prompt"))
                .map(Value::as_str),
            Some("prompt")
        );
        assert_eq!(
            telegram
                .get("home_channel")
                .and_then(|home| home.get("chat_id"))
                .map(Value::as_str),
            Some("1")
        );
    }
}
