//! Literal `${KEY}` substitution for the private deploy templates.
//!
//! This is not `text/template`: every placeholder is a literal value from the
//! unit allowlist, every declared placeholder needs a non-empty value, and a
//! leftover placeholder is a hard error. The port keeps the Go error text and
//! the content fingerprint (`--rollout-key`) byte-identical.

use std::collections::BTreeMap;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use regex::Regex;

use crate::{buildid, envfile};

/// Placeholder filled with the content fingerprint of the rendered unit.
pub const ROLLOUT_KEY: &str = "ROLLOUT_ID";

/// Value masked while hashing: the fingerprint itself.
const ROLLOUT_MASK: &str = "rollout-fingerprint";
/// Value masked while hashing: the git revision.
const BUILD_MASK: &str = "build-traceability";

const SCHEDULER_PLACEHOLDERS: &[&str] = &[
    "BUILD_ID",
    ROLLOUT_KEY,
    "HA_URL",
    "LAN_CIDR",
    "NODE_LAN_IP",
    "SCHEDULER_NODE_PORT",
    "INTERCOM_NODE_PORT",
    "SCHEDULER_PUBLIC_URL",
    "CONTROL_NODE_PORT",
    "SCHEDULER_IMAGE",
    "HA_BRIDGE_IMAGE",
    "HA_CONTROL_IMAGE",
    "HA_SCENARIO_IMAGE",
    "PREFLIGHT_IMAGE",
    "CURRENCY_BOT_IMAGE",
    "CLOUDFLARED_IMAGE",
    "CURRENCY_WEBHOOK_URL",
    "CURRENCY_BOT_NODE_PORT",
];

const INGRESS_PLACEHOLDERS: &[&str] = &[
    "HOME_DOMAIN",
    "LAN_CIDR",
    "HAOS_ADDRESS",
    "HA_HOST",
    "GRAFANA_HOST",
    "ALARMS_HOST",
    "CONTROL_HOST",
    "CURRENCY_HOST",
    "INTERCOM_HOST",
];

const MCP_GATEWAY_EXTRA: &[&str] = &[
    "MCP_GATEWAY_IMAGE",
    "HAOS_IP",
    "STATION_IP",
    "BB_CT_IP",
    "BB_PROJECT_STATION_LAB",
    "BB_PROJECT_ENGLISH_NOTES",
    "BB_DISPATCHER_THREAD_ID",
];

const WAKEGRANT_EXTRA: &[&str] = &["WAKE_GRANT_IMAGE", "WAKE_GRANT_NODE_PORT"];

const INTERCOM_EXTRA: &[&str] = &["INTERCOM_IMAGE", "INTERCOM_NODE_PORT"];

/// One unit's placeholder policy.
#[derive(Debug)]
pub struct Preset {
    /// `--unit` value.
    pub name: &'static str,
    /// Exact, case-sensitive allowlist.
    pub allowed: Vec<&'static str>,
    /// Switches the leftover check from `${KEY}` to any `${`.
    pub contains_leftover: bool,
}

impl Preset {
    fn allows(&self, key: &str) -> bool {
        self.allowed.contains(&key)
    }
}

/// Returns the placeholder policy for `unit`.
pub fn preset_for(unit: &str) -> Result<Preset, String> {
    let base = || Preset {
        name: "scheduler",
        allowed: SCHEDULER_PLACEHOLDERS.to_vec(),
        contains_leftover: false,
    };
    match unit {
        "" | "scheduler" => Ok(base()),
        "currency-bot" => Ok(Preset {
            name: "currency-bot",
            ..base()
        }),
        "mcp-gateway" => {
            let mut allowed = SCHEDULER_PLACEHOLDERS.to_vec();
            allowed.extend_from_slice(MCP_GATEWAY_EXTRA);
            Ok(Preset {
                name: "mcp-gateway",
                allowed,
                contains_leftover: false,
            })
        }
        "wakegrant" => {
            let mut allowed = SCHEDULER_PLACEHOLDERS.to_vec();
            allowed.extend_from_slice(WAKEGRANT_EXTRA);
            Ok(Preset {
                name: "wakegrant",
                allowed,
                contains_leftover: false,
            })
        }
        "intercom" => {
            let mut allowed = SCHEDULER_PLACEHOLDERS.to_vec();
            allowed.extend_from_slice(INTERCOM_EXTRA);
            Ok(Preset {
                name: "intercom",
                allowed,
                contains_leftover: false,
            })
        }
        "ingress" => Ok(Preset {
            name: "ingress",
            allowed: INGRESS_PLACEHOLDERS.to_vec(),
            contains_leftover: true,
        }),
        other => Err(format!(
            "unknown unit {other:?} (expected one of: {})",
            unit_names().join(", ")
        )),
    }
}

/// Supported `--unit` values in stable order.
pub fn unit_names() -> Vec<&'static str> {
    let mut names = vec![
        "scheduler",
        "currency-bot",
        "mcp-gateway",
        "wakegrant",
        "intercom",
        "ingress",
    ];
    names.sort_unstable();
    names
}

fn placeholder_regex() -> &'static Regex {
    static PLACEHOLDER: std::sync::OnceLock<Regex> = std::sync::OnceLock::new();
    PLACEHOLDER.get_or_init(|| Regex::new(r"\$\{([A-Z0-9_]+)\}").expect("valid regex"))
}

/// Substitutes placeholders in `text`; `origin` is used in error messages.
pub fn render(
    text: &str,
    values: &BTreeMap<String, String>,
    origin: &str,
    preset: &Preset,
) -> Result<String, String> {
    let pattern = placeholder_regex();
    let mut output = String::with_capacity(text.len());
    let mut last = 0;
    let mut failure = None;
    for capture in pattern.captures_iter(text) {
        let Some(whole) = capture.get(0) else {
            continue;
        };
        let key = &text[capture.get(1).expect("group").range()];
        if failure.is_none() {
            if !preset.allows(key) {
                failure = Some(format!("{origin}: placeholder ${{{key}}} is not allowed"));
            } else if let Some(value) = values.get(key).filter(|value| !value.is_empty()) {
                output.push_str(&text[last..whole.start()]);
                output.push_str(value);
                last = whole.end();
            } else {
                failure = Some(format!("{origin}: missing value for ${{{key}}}"));
            }
        }
        if failure.is_some() {
            break;
        }
    }
    if let Some(error) = failure {
        return Err(error);
    }
    output.push_str(&text[last..]);
    let rendered = output;
    if preset.contains_leftover {
        if rendered.contains("${") {
            return Err(format!("unrendered placeholder in {origin}"));
        }
        return Ok(rendered);
    }
    if let Some(found) = pattern.find(&rendered) {
        return Err(format!(
            "{origin}: unrendered placeholder {}",
            found.as_str()
        ));
    }
    Ok(rendered)
}

/// One `render` invocation.
pub struct Options<'a> {
    /// Private `KEY=VALUE` file; empty means `sets` only.
    pub env_file: &'a str,
    /// `KEY=VALUE` overrides applied after the env file, in order.
    pub sets: &'a [String],
    /// Output directory.
    pub out_dir: &'a str,
    /// Templates to render.
    pub files: &'a [String],
    /// Unit selecting the preset.
    pub unit: &'a str,
    /// Placeholder replaced by the content fingerprint, when non-empty.
    pub rollout_key: &'a str,
}

/// Renders every file in `options`, writing progress to `stdout`.
pub fn run(options: &Options<'_>, stdout: &mut dyn Write) -> Result<(), String> {
    let preset = preset_for(options.unit)?;

    let mut values = if options.env_file.is_empty() {
        BTreeMap::new()
    } else {
        envfile::parse(Path::new(options.env_file))?
    };
    for item in options.sets {
        let (key, value) = item.split_once('=').unwrap_or((item.as_str(), ""));
        if !preset.allows(key) {
            return Err(format!("--set {key}: not an allowed placeholder"));
        }
        values.insert(key.to_string(), value.to_string());
    }
    if !options.rollout_key.is_empty() && !preset.allows(options.rollout_key) {
        return Err(format!(
            "rollout key {} is not an allowed placeholder",
            options.rollout_key
        ));
    }

    fs::create_dir_all(options.out_dir).map_err(|error| error.to_string())?;

    let mut sources = Vec::with_capacity(options.files.len());
    for src in options.files {
        let raw = fs::read(src).map_err(|error| error.to_string())?;
        sources.push(Source {
            path: PathBuf::from(src),
            raw: String::from_utf8_lossy(&raw).into_owned(),
        });
    }

    if !options.rollout_key.is_empty() {
        let fingerprint = content_fingerprint(&sources, &values, &preset, options.rollout_key)?;
        values.insert(options.rollout_key.to_string(), fingerprint.clone());
        let _ = writeln!(stdout, "rollout id: {fingerprint}");
    }

    for source in &sources {
        let rendered = render(
            &source.raw,
            &values,
            &source.path.to_string_lossy(),
            &preset,
        )?;
        let name = source
            .path
            .file_name()
            .map_or_else(String::new, |name| name.to_string_lossy().into_owned());
        let dest = Path::new(options.out_dir).join(name);
        fs::write(&dest, rendered).map_err(|error| error.to_string())?;
        let _ = writeln!(
            stdout,
            "rendered {} -> {}",
            source.path.display(),
            dest.display()
        );
    }
    Ok(())
}

struct Source {
    path: PathBuf,
    raw: String,
}

/// Hashes the rendered unit with `ROLLOUT_ID` and `BUILD_ID` masked out.
fn content_fingerprint(
    sources: &[Source],
    values: &BTreeMap<String, String>,
    preset: &Preset,
    rollout_key: &str,
) -> Result<String, String> {
    let mut masked = values.clone();
    masked.insert(rollout_key.to_string(), ROLLOUT_MASK.to_string());
    if masked.contains_key("BUILD_ID") {
        masked.insert("BUILD_ID".to_string(), BUILD_MASK.to_string());
    }
    let mut canonical = String::new();
    for source in sources {
        let rendered = render(&source.raw, &masked, &source.path.to_string_lossy(), preset)?;
        let name = source
            .path
            .file_name()
            .map_or_else(String::new, |name| name.to_string_lossy().into_owned());
        canonical.push_str(&name);
        canonical.push('\0');
        canonical.push_str(&rendered);
        canonical.push('\0');
    }
    Ok(buildid::fingerprint(&canonical))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn values(pairs: &[(&str, &str)]) -> BTreeMap<String, String> {
        pairs
            .iter()
            .map(|(key, value)| ((*key).to_string(), (*value).to_string()))
            .collect()
    }

    #[test]
    fn substitutes_and_does_not_rescan_values() {
        let preset = preset_for("scheduler").expect("preset");
        let rendered = render(
            "cidr=${LAN_CIDR} ip=${NODE_LAN_IP}\n",
            &values(&[("LAN_CIDR", "192.0.2.0/24"), ("NODE_LAN_IP", "192.0.2.27")]),
            "t.yaml",
            &preset,
        )
        .expect("render");
        assert_eq!(rendered, "cidr=192.0.2.0/24 ip=192.0.2.27\n");

        let literal = render(
            "${LAN_CIDR}",
            &values(&[("LAN_CIDR", "${lower_value}")]),
            "t.yaml",
            &preset,
        )
        .expect("literal value");
        assert_eq!(literal, "${lower_value}");
    }

    #[test]
    fn refuses_unknown_missing_and_leftover_placeholders() {
        let preset = preset_for("scheduler").expect("preset");
        assert_eq!(
            render("${NOPE}", &values(&[]), "t.yaml", &preset).unwrap_err(),
            "t.yaml: placeholder ${NOPE} is not allowed"
        );
        assert_eq!(
            render("${LAN_CIDR}", &values(&[]), "t.yaml", &preset).unwrap_err(),
            "t.yaml: missing value for ${LAN_CIDR}"
        );
        assert_eq!(
            render("${lower}", &values(&[]), "t.yaml", &preset).unwrap(),
            "${lower}"
        );
        let ingress = preset_for("ingress").expect("ingress");
        assert_eq!(
            render(
                "${HOME_DOMAIN}${lower}",
                &values(&[("HOME_DOMAIN", "x")]),
                "t.yaml",
                &ingress
            )
            .unwrap_err(),
            "unrendered placeholder in t.yaml"
        );
    }

    #[test]
    fn unknown_unit_lists_the_supported_names() {
        assert_eq!(
            preset_for("nope").unwrap_err(),
            "unknown unit \"nope\" (expected one of: currency-bot, ingress, intercom, mcp-gateway, scheduler, wakegrant)"
        );
    }
}
