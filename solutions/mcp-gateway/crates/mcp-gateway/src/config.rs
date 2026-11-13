//! Process configuration, ported one-to-one from
//! `solutions/mcp-gateway/internal/config/config.go`.
//!
//! Everything is read from the environment; secrets are only read from the
//! files named by `*_FILE` variables so the deployment never embeds a token in
//! a manifest and the process can run with a read-only projected volume.

use std::collections::BTreeSet;
use std::time::Duration;

use jiff::tz::TimeZone;

/// Default listen address.
pub const DEFAULT_HTTP_ADDR: &str = ":8080";
/// Default enabled tool groups.
pub const DEFAULT_GROUPS: &str = "infra.read";
/// Default pending-action TTL.
pub const DEFAULT_PENDING_TTL: Duration = Duration::from_secs(90);
/// Default restart cooldown.
pub const DEFAULT_COOLDOWN: Duration = Duration::from_secs(5 * 60);
/// Default delay before a self-disrupting restart.
pub const DEFAULT_SELF_RESTART_DELAY: Duration = Duration::from_secs(20);
/// Default per-backend query timeout.
pub const DEFAULT_QUERY_TIMEOUT: Duration = Duration::from_secs(4);
/// Default whole `infra_status` budget.
pub const DEFAULT_STATUS_TIMEOUT: Duration = Duration::from_secs(8);
/// Default whole `infra_report` budget.
pub const DEFAULT_REPORT_TIMEOUT: Duration = Duration::from_secs(9);
/// Default returned log payload cap.
pub const DEFAULT_LOG_BYTE_CAP: usize = 64 << 10;
/// Default Telegram Bot API root.
pub const DEFAULT_TELEGRAM_API_URL: &str = "https://api.telegram.org";
/// Default report timezone.
pub const DEFAULT_TIMEZONE: &str = "UTC";
/// Default bb bridge timeout.
pub const DEFAULT_BB_TIMEOUT: Duration = Duration::from_secs(15);
/// Default bb SSH user.
pub const DEFAULT_BB_USER: &str = "bb";

/// A validated process configuration.
#[derive(Debug, Clone)]
pub struct Config {
    /// Listen address, for example `:8080`.
    pub http_addr: String,
    /// MCP bearer token.
    pub auth_token: String,
    /// Enabled tool groups, in configuration order.
    pub groups: Vec<String>,

    /// Catalog YAML path.
    pub catalog_file: String,

    /// Prometheus base URL.
    pub prometheus_url: String,
    /// Alertmanager base URL.
    pub alertmanager_url: String,
    /// Loki base URL.
    pub loki_url: String,

    /// Home Assistant REST endpoint for `ha_*` checks.
    pub ha_url: String,
    /// Home Assistant token file.
    pub ha_token_file: String,
    /// Home Assistant token.
    pub ha_token: String,

    /// Kubernetes API root.
    pub kube_api_url: String,
    /// ServiceAccount token file.
    pub kube_token_file: String,
    /// CA bundle file.
    pub kube_ca_file: String,

    /// Telegram Bot API root.
    pub telegram_api_url: String,
    /// Telegram bot token file.
    pub telegram_token_file: String,
    /// Telegram chat id file.
    pub telegram_chat_file: String,
    /// Telegram bot token.
    pub telegram_token: String,
    /// Fixed Telegram recipient.
    pub telegram_chat_id: String,

    /// Pending restart TTL.
    pub pending_ttl: Duration,
    /// Restart cooldown.
    pub cooldown: Duration,
    /// Delay before a self-disrupting restart.
    pub self_restart_delay: Duration,
    /// Per-backend query timeout.
    pub query_timeout: Duration,
    /// Whole `infra_status` budget.
    pub status_timeout: Duration,
    /// Whole `infra_report` budget.
    pub report_timeout: Duration,
    /// Returned log payload cap.
    pub log_byte_cap: usize,

    /// bb SSH address.
    pub bb_address: String,
    /// bb SSH user.
    pub bb_user: String,
    /// bb SSH key file.
    pub bb_key_file: String,
    /// Pinned `known_hosts` file.
    pub bb_known_hosts_file: String,
    /// bb bridge timeout.
    pub bb_timeout: Duration,
    /// Personal allow-list of bb targets.
    pub bb_targets_file: String,
    /// Thread that receives work-profile voice commands.
    pub bb_dispatcher_thread_id: String,

    /// Voice token file; enables `/v1/voice/*` when set.
    pub voice_token_file: String,
    /// Voice bearer token.
    pub voice_token: String,

    /// Durable scheduler base URL.
    pub scheduler_url: String,
    /// Durable scheduler token file.
    pub scheduler_token_file: String,
    /// Durable scheduler bearer token.
    pub scheduler_token: String,

    /// Phone push service base URL.
    pub push_url: String,
    /// Phone push token file.
    pub push_token_file: String,
    /// Phone push bearer token.
    pub push_token: String,

    /// IANA timezone name.
    pub timezone: String,
    /// Loaded timezone.
    pub location: TimeZone,
}

type Getenv<'a> = dyn Fn(&str) -> Option<String> + 'a;
type ReadFile<'a> = dyn Fn(&str) -> std::io::Result<Vec<u8>> + 'a;

impl Config {
    /// Reads and validates the process environment.
    pub fn load() -> Result<Self, String> {
        Self::load_from(&|key| std::env::var(key).ok(), &|path| std::fs::read(path))
    }

    /// Reads and validates a supplied environment; the seam keeps the config
    /// tests free of process-global state.
    pub fn load_from(getenv: &Getenv<'_>, read_file: &ReadFile<'_>) -> Result<Self, String> {
        let mut cfg = Self {
            http_addr: value_or(getenv, "MCP_HTTP_ADDR", DEFAULT_HTTP_ADDR),
            auth_token: String::new(),
            groups: Vec::new(),
            catalog_file: trimmed(getenv, "MCP_INFRA_CATALOG_FILE"),
            prometheus_url: trimmed(getenv, "MCP_PROMETHEUS_URL"),
            alertmanager_url: trimmed(getenv, "MCP_ALERTMANAGER_URL"),
            loki_url: trimmed(getenv, "MCP_LOKI_URL"),
            ha_url: trimmed(getenv, "MCP_HA_URL"),
            ha_token_file: trimmed(getenv, "MCP_HA_TOKEN_FILE"),
            ha_token: String::new(),
            kube_api_url: trimmed(getenv, "KUBERNETES_API_URL"),
            kube_token_file: trimmed(getenv, "KUBERNETES_TOKEN_FILE"),
            kube_ca_file: trimmed(getenv, "KUBERNETES_CA_FILE"),
            telegram_api_url: value_or(getenv, "TELEGRAM_API_URL", DEFAULT_TELEGRAM_API_URL),
            telegram_token_file: trimmed(getenv, "TELEGRAM_BOT_TOKEN_FILE"),
            telegram_chat_file: trimmed(getenv, "TELEGRAM_CHAT_ID_FILE"),
            telegram_token: String::new(),
            telegram_chat_id: String::new(),
            pending_ttl: DEFAULT_PENDING_TTL,
            cooldown: DEFAULT_COOLDOWN,
            self_restart_delay: DEFAULT_SELF_RESTART_DELAY,
            query_timeout: DEFAULT_QUERY_TIMEOUT,
            status_timeout: DEFAULT_STATUS_TIMEOUT,
            report_timeout: DEFAULT_REPORT_TIMEOUT,
            log_byte_cap: DEFAULT_LOG_BYTE_CAP,
            bb_address: trimmed(getenv, "MCP_BB_SSH_ADDR"),
            bb_user: value_or(getenv, "MCP_BB_SSH_USER", DEFAULT_BB_USER),
            bb_key_file: trimmed(getenv, "MCP_BB_SSH_KEY_FILE"),
            bb_known_hosts_file: trimmed(getenv, "MCP_BB_KNOWN_HOSTS_FILE"),
            bb_timeout: DEFAULT_BB_TIMEOUT,
            bb_targets_file: trimmed(getenv, "MCP_BB_TARGETS_FILE"),
            bb_dispatcher_thread_id: trimmed(getenv, "MCP_BB_DISPATCHER_THREAD_ID"),
            voice_token_file: trimmed(getenv, "MCP_VOICE_TOKEN_FILE"),
            voice_token: String::new(),
            scheduler_url: trimmed(getenv, "MCP_SCHEDULER_URL"),
            scheduler_token_file: trimmed(getenv, "MCP_SCHEDULER_TOKEN_FILE"),
            scheduler_token: String::new(),
            push_url: trimmed(getenv, "MCP_PUSH_URL"),
            push_token_file: trimmed(getenv, "MCP_PUSH_TOKEN_FILE"),
            push_token: String::new(),
            timezone: value_or(getenv, "MCP_TIMEZONE", DEFAULT_TIMEZONE),
            location: TimeZone::UTC,
        };

        cfg.location = TimeZone::get(&cfg.timezone)
            .map_err(|_| format!("MCP_TIMEZONE: unknown IANA timezone {:?}", cfg.timezone))?;

        cfg.groups = split_list(&value_or(getenv, "MCP_GROUPS", DEFAULT_GROUPS));
        if cfg.groups.is_empty() {
            return Err("MCP_GROUPS must contain at least one group".to_owned());
        }

        let token_file = trimmed(getenv, "MCP_AUTH_TOKEN_FILE");
        if token_file.is_empty() {
            return Err("MCP_AUTH_TOKEN_FILE is required".to_owned());
        }
        cfg.auth_token = read_secret(read_file, &token_file, "MCP_AUTH_TOKEN_FILE")?;

        cfg.pending_ttl = duration_value(
            getenv,
            "MCP_PENDING_TTL",
            DEFAULT_PENDING_TTL,
            Duration::from_secs(1),
            Duration::from_secs(3600),
        )?;
        cfg.cooldown = duration_value(
            getenv,
            "MCP_RESTART_COOLDOWN",
            DEFAULT_COOLDOWN,
            Duration::from_secs(1),
            Duration::from_secs(24 * 3600),
        )?;
        cfg.self_restart_delay = duration_value(
            getenv,
            "MCP_SELF_RESTART_DELAY",
            DEFAULT_SELF_RESTART_DELAY,
            Duration::ZERO,
            Duration::from_secs(600),
        )?;
        cfg.query_timeout = duration_value(
            getenv,
            "MCP_QUERY_TIMEOUT",
            DEFAULT_QUERY_TIMEOUT,
            Duration::from_secs(1),
            Duration::from_secs(30),
        )?;
        cfg.status_timeout = duration_value(
            getenv,
            "MCP_STATUS_TIMEOUT",
            DEFAULT_STATUS_TIMEOUT,
            Duration::from_secs(1),
            Duration::from_secs(60),
        )?;
        cfg.report_timeout = duration_value(
            getenv,
            "MCP_REPORT_TIMEOUT",
            DEFAULT_REPORT_TIMEOUT,
            Duration::from_secs(1),
            Duration::from_secs(60),
        )?;
        cfg.log_byte_cap = int_value(
            getenv,
            "MCP_LOG_BYTE_CAP",
            DEFAULT_LOG_BYTE_CAP,
            1024,
            4 << 20,
        )?;
        cfg.bb_timeout = duration_value(
            getenv,
            "MCP_BB_TIMEOUT",
            DEFAULT_BB_TIMEOUT,
            Duration::from_secs(1),
            Duration::from_secs(60),
        )?;

        validate_groups(&cfg)?;
        load_telegram_files(&mut cfg, getenv, read_file)?;
        load_ha_files(&mut cfg, read_file)?;
        load_scheduler_files(&mut cfg, read_file)?;
        load_push_files(&mut cfg, read_file)?;
        load_bb_files(&mut cfg, read_file)?;
        Ok(cfg)
    }

    /// Loopback base URL for the `-healthcheck` probe.
    pub fn healthcheck_url(&self) -> Result<String, String> {
        let (host, port) = self
            .http_addr
            .rsplit_once(':')
            .ok_or_else(|| format!("invalid MCP_HTTP_ADDR: {:?}", self.http_addr))?;
        let host = match host {
            "" | "0.0.0.0" | "::" | "[::]" => "127.0.0.1",
            other => other,
        };
        if port.is_empty() {
            return Err(format!("invalid MCP_HTTP_ADDR: {:?}", self.http_addr));
        }
        Ok(format!("http://{host}:{port}"))
    }
}

fn load_bb_files(cfg: &mut Config, read_file: &ReadFile<'_>) -> Result<(), String> {
    let mut need_bb = false;
    let mut need_targets = false;
    for name in &cfg.groups {
        if name == "bb.read" || name == "bb.act" {
            need_bb = true;
        }
        if name == "bb.act" {
            need_targets = true;
        }
    }
    if !cfg.voice_token_file.is_empty() {
        need_bb = true;
    }
    if !need_bb {
        return Ok(());
    }
    if cfg.bb_address.is_empty() || cfg.bb_key_file.is_empty() || cfg.bb_known_hosts_file.is_empty()
    {
        return Err(
            "groups bb.read/bb.act and the voice route require MCP_BB_SSH_ADDR, MCP_BB_SSH_KEY_FILE and MCP_BB_KNOWN_HOSTS_FILE"
                .to_owned(),
        );
    }
    if need_targets && cfg.bb_targets_file.is_empty() {
        return Err("MCP_BB_TARGETS_FILE is required for group bb.act".to_owned());
    }
    read_secret(read_file, &cfg.bb_key_file, "MCP_BB_SSH_KEY_FILE")?;
    read_secret(
        read_file,
        &cfg.bb_known_hosts_file,
        "MCP_BB_KNOWN_HOSTS_FILE",
    )?;
    if !cfg.voice_token_file.is_empty() {
        if cfg.bb_timeout < Duration::from_secs(2) {
            return Err(
                "MCP_BB_TIMEOUT must be at least 2s when MCP_VOICE_TOKEN_FILE is set: the updates long-poll needs room below the bridge timeout"
                    .to_owned(),
            );
        }
        if cfg.bb_dispatcher_thread_id.is_empty() {
            return Err(
                "MCP_BB_DISPATCHER_THREAD_ID is required when MCP_VOICE_TOKEN_FILE is set"
                    .to_owned(),
            );
        }
        let token = read_secret(read_file, &cfg.voice_token_file, "MCP_VOICE_TOKEN_FILE")?;
        if token == cfg.auth_token {
            return Err("MCP_VOICE_TOKEN_FILE must differ from MCP_AUTH_TOKEN_FILE".to_owned());
        }
        cfg.voice_token = token;
    }
    Ok(())
}

fn load_push_files(cfg: &mut Config, read_file: &ReadFile<'_>) -> Result<(), String> {
    if !cfg.groups.iter().any(|name| name == "phone") {
        return Ok(());
    }
    if cfg.push_url.is_empty() || cfg.push_token_file.is_empty() {
        return Err("group phone requires MCP_PUSH_URL and MCP_PUSH_TOKEN_FILE".to_owned());
    }
    cfg.push_token = read_secret(read_file, &cfg.push_token_file, "MCP_PUSH_TOKEN_FILE")?;
    Ok(())
}

fn load_scheduler_files(cfg: &mut Config, read_file: &ReadFile<'_>) -> Result<(), String> {
    if !cfg.groups.iter().any(|name| name == "time") {
        return Ok(());
    }
    if cfg.scheduler_url.is_empty() || cfg.scheduler_token_file.is_empty() {
        return Err(
            "group time requires MCP_SCHEDULER_URL and MCP_SCHEDULER_TOKEN_FILE".to_owned(),
        );
    }
    cfg.scheduler_token = read_secret(
        read_file,
        &cfg.scheduler_token_file,
        "MCP_SCHEDULER_TOKEN_FILE",
    )?;
    Ok(())
}

fn load_ha_files(cfg: &mut Config, read_file: &ReadFile<'_>) -> Result<(), String> {
    if cfg.ha_url.is_empty() && cfg.ha_token_file.is_empty() {
        return Ok(());
    }
    if cfg.ha_url.is_empty() || cfg.ha_token_file.is_empty() {
        return Err("MCP_HA_URL and MCP_HA_TOKEN_FILE must be set together".to_owned());
    }
    cfg.ha_token = read_secret(read_file, &cfg.ha_token_file, "MCP_HA_TOKEN_FILE")?;
    Ok(())
}

fn validate_groups(cfg: &Config) -> Result<(), String> {
    let mut need_catalog = false;
    let mut need_report = false;
    for name in &cfg.groups {
        match name.as_str() {
            "infra.read" | "infra.fix" => need_catalog = true,
            "infra.report" => {
                need_report = true;
                if cfg.telegram_token_file.is_empty() || cfg.telegram_chat_file.is_empty() {
                    return Err(
                        "group infra.report requires TELEGRAM_BOT_TOKEN_FILE and TELEGRAM_CHAT_ID_FILE"
                            .to_owned(),
                    );
                }
            }
            "bb.read" | "bb.act" | "time" | "phone" => {}
            other => return Err(format!("unknown tool group {other:?}")),
        }
    }
    if need_catalog && cfg.catalog_file.is_empty() {
        return Err("MCP_INFRA_CATALOG_FILE is required for group infra.read/infra.fix".to_owned());
    }
    if need_report && cfg.telegram_token_file.is_empty() {
        return Err("TELEGRAM_BOT_TOKEN_FILE is required for group infra.report".to_owned());
    }
    Ok(())
}

fn load_telegram_files(
    cfg: &mut Config,
    getenv: &Getenv<'_>,
    read_file: &ReadFile<'_>,
) -> Result<(), String> {
    let _ = getenv;
    if !cfg.groups.iter().any(|name| name == "infra.report") {
        return Ok(());
    }
    let token = read_secret(
        read_file,
        &cfg.telegram_token_file,
        "TELEGRAM_BOT_TOKEN_FILE",
    )?;
    let mut chat = read_secret(read_file, &cfg.telegram_chat_file, "TELEGRAM_CHAT_ID_FILE")?;
    if let Some(index) = chat.find(',') {
        chat = String::from(chat[..index].trim());
    }
    if chat.is_empty() {
        return Err("TELEGRAM_CHAT_ID_FILE is empty".to_owned());
    }
    cfg.telegram_token = token;
    cfg.telegram_chat_id = chat;
    Ok(())
}

fn read_secret(read_file: &ReadFile<'_>, path: &str, variable: &str) -> Result<String, String> {
    let data = read_file(path).map_err(|error| format!("{variable}: {error}"))?;
    let value = String::from_utf8_lossy(&data).trim().to_owned();
    if value.is_empty() {
        return Err(format!("{variable} is empty"));
    }
    Ok(value)
}

fn value_or(getenv: &Getenv<'_>, key: &str, default: &str) -> String {
    let value = getenv(key).unwrap_or_default();
    let value = value.trim();
    if value.is_empty() {
        default.to_owned()
    } else {
        value.to_owned()
    }
}

fn trimmed(getenv: &Getenv<'_>, key: &str) -> String {
    getenv(key).unwrap_or_default().trim().to_owned()
}

fn split_list(raw: &str) -> Vec<String> {
    let mut seen = BTreeSet::new();
    raw.split(',')
        .map(str::trim)
        .filter(|part| !part.is_empty())
        .filter(|part| seen.insert((*part).to_owned()))
        .map(str::to_owned)
        .collect()
}

fn duration_value(
    getenv: &Getenv<'_>,
    key: &str,
    default: Duration,
    min: Duration,
    max: Duration,
) -> Result<Duration, String> {
    let raw = getenv(key).unwrap_or_default();
    let raw = raw.trim();
    if raw.is_empty() {
        return Ok(default);
    }
    let value = home_config::parse_go_duration(raw)
        .map_err(|_| format!("{key} must be a duration in {:?}..{:?}", min, max))?;
    if value < min || value > max {
        return Err(format!("{key} must be a duration in {min:?}..{max:?}"));
    }
    Ok(value)
}

fn int_value(
    getenv: &Getenv<'_>,
    key: &str,
    default: usize,
    min: usize,
    max: usize,
) -> Result<usize, String> {
    let raw = getenv(key).unwrap_or_default();
    let raw = raw.trim();
    if raw.is_empty() {
        return Ok(default);
    }
    let value = raw
        .parse::<usize>()
        .map_err(|_| format!("{key} must be an integer in {min}..{max}"))?;
    if value < min || value > max {
        return Err(format!("{key} must be an integer in {min}..{max}"));
    }
    Ok(value)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn env<'a>(pairs: &'a [(&'a str, &'a str)]) -> impl Fn(&str) -> Option<String> + 'a {
        move |key| {
            pairs
                .iter()
                .find(|(name, _)| *name == key)
                .map(|(_, value)| (*value).to_owned())
        }
    }

    #[test]
    fn defaults_match_the_go_config() {
        let pairs = [
            ("MCP_AUTH_TOKEN_FILE", "/token"),
            ("MCP_INFRA_CATALOG_FILE", "/catalog"),
        ];
        let read = |_path: &str| Ok(b"secret\n".to_vec());
        let config = Config::load_from(&env(&pairs), &read).expect("config");
        assert_eq!(config.http_addr, ":8080");
        assert_eq!(config.groups, vec!["infra.read"]);
        assert_eq!(config.pending_ttl, DEFAULT_PENDING_TTL);
        assert_eq!(config.log_byte_cap, 64 << 10);
        assert_eq!(config.auth_token, "secret");
        assert_eq!(config.timezone, "UTC");
    }

    #[test]
    fn time_group_requires_the_scheduler_url_and_token() {
        let pairs = [("MCP_AUTH_TOKEN_FILE", "/token"), ("MCP_GROUPS", "time")];
        let read = |_path: &str| Ok(b"secret\n".to_vec());
        let error = Config::load_from(&env(&pairs), &read).expect_err("missing scheduler");
        assert!(error.contains("MCP_SCHEDULER_URL"), "{error}");

        let pairs = [
            ("MCP_AUTH_TOKEN_FILE", "/token"),
            ("MCP_GROUPS", "time"),
            ("MCP_SCHEDULER_URL", "http://scheduler:8098"),
            ("MCP_SCHEDULER_TOKEN_FILE", "/scheduler-token"),
        ];
        let config = Config::load_from(&env(&pairs), &read).expect("config");
        assert_eq!(config.scheduler_token, "secret");
        assert_eq!(config.groups, vec!["time"]);
    }

    #[test]
    fn phone_group_requires_the_push_url_and_token() {
        let pairs = [("MCP_AUTH_TOKEN_FILE", "/token"), ("MCP_GROUPS", "phone")];
        let read = |_path: &str| Ok(b"secret\n".to_vec());
        let error = Config::load_from(&env(&pairs), &read).expect_err("missing push");
        assert!(error.contains("MCP_PUSH_URL"), "{error}");

        let pairs = [
            ("MCP_AUTH_TOKEN_FILE", "/token"),
            ("MCP_GROUPS", "phone"),
            ("MCP_PUSH_URL", "http://phone-push:8095"),
            ("MCP_PUSH_TOKEN_FILE", "/push-token"),
        ];
        let config = Config::load_from(&env(&pairs), &read).expect("config");
        assert_eq!(config.push_token, "secret");
        assert_eq!(config.groups, vec!["phone"]);
    }

    #[test]
    fn healthcheck_url_uses_loopback_for_wildcards() {
        let mut config = Config {
            http_addr: ":8080".to_owned(),
            ..Config::load_from(
                &env(&[
                    ("MCP_AUTH_TOKEN_FILE", "/t"),
                    ("MCP_INFRA_CATALOG_FILE", "/c"),
                ]),
                &|_| Ok(b"t".to_vec()),
            )
            .expect("config")
        };
        assert_eq!(
            config.healthcheck_url().expect("url"),
            "http://127.0.0.1:8080"
        );
        config.http_addr = "0.0.0.0:9000".to_owned();
        assert_eq!(
            config.healthcheck_url().expect("url"),
            "http://127.0.0.1:9000"
        );
    }
}
