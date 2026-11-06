//! Typed environment configuration.
//!
//! The contract:
//!
//! * a `*_FILE` variable wins over the inline value and is re-read from disk on
//!   every use; a configured but unreadable file yields an **empty** secret and
//!   never falls back;
//! * trailing slashes are stripped from every base URL;
//! * a default applies only when the variable is unset, not when it is set to
//!   an empty string.

use std::net::SocketAddr;
use std::path::PathBuf;
use std::time::Duration;

use clap::Parser;
pub use home_config::LogLevel;
use serde::Serialize;
use thiserror::Error;

use crate::services::TokenProvider;

/// Default OpenAI-compatible upstream.
pub const DEFAULT_UPSTREAM: &str = "http://127.0.0.1:8642";
/// Default listen address.
pub const DEFAULT_BIND: &str = "127.0.0.1";
/// Default listen port.
pub const DEFAULT_PORT: u16 = 8643;
/// Default reasoning effort injected into every chat request.
pub const DEFAULT_REASONING_EFFORT: &str = "low";
/// Default profile `input_select` entity.
pub const DEFAULT_PROFILE_ENTITY: &str = "input_select.ricardo_voice_profile";
/// Default successful profile-read cache.
pub const DEFAULT_PROFILE_CACHE_MS: u64 = 2_000;
/// Default Home Assistant timeout.
pub const DEFAULT_HA_TIMEOUT: f64 = 3.0;
/// Default gateway timeout.
pub const DEFAULT_GATEWAY_TIMEOUT: f64 = 5.0;
/// Default late-answer budget.
pub const DEFAULT_RESULT_TIMEOUT: f64 = 900.0;
/// Default late-answer retry pause.
pub const DEFAULT_RESULT_RETRY: f64 = 2.0;
/// Default long-poll wait.
pub const DEFAULT_RESULT_WAIT: u64 = 25;
/// Default updates retry minimum.
pub const DEFAULT_UPDATES_RETRY: f64 = 2.0;
/// Default updates rescan window.
pub const DEFAULT_UPDATES_RESCAN: i64 = 300;
/// Default announcement truncation limit.
pub const DEFAULT_ANNOUNCE_MAX_CHARS: usize = 200;
/// Default statement announcement script.
pub const DEFAULT_ANNOUNCE_SCRIPT: &str = "script.schedule_announce";
/// Default satellite entity.
pub const DEFAULT_SATELLITE: &str = "assist_satellite.iandeks_stantsiia_midi_assist_satellite";
/// Default media-player idle gate.
pub const DEFAULT_MEDIA_PLAYER: &str = "media_player.iandeks_stantsiia_midi_media_player";
/// Default scheduler consumer id.
pub const DEFAULT_QUEUE_CONSUMER: &str = "voice-proxy-home";
/// Default announcement TTL.
pub const DEFAULT_ANNOUNCE_TTL: i64 = 10_800;

/// Non-secret settings parsed by clap with `env`.
#[derive(Debug, Clone, Parser)]
#[command(name = "voice-proxy", version, disable_help_subcommand = true)]
pub struct Settings {
    #[arg(long, env = "HERMES_UPSTREAM", default_value = DEFAULT_UPSTREAM, hide = true)]
    pub upstream: String,
    #[arg(long, env = "HERMES_PROXY_BIND", default_value = DEFAULT_BIND, hide = true)]
    pub bind: String,
    #[arg(long, env = "HERMES_PROXY_PORT", default_value_t = DEFAULT_PORT, hide = true)]
    pub port: u16,
    #[arg(
        long,
        env = "HERMES_VOICE_REASONING_EFFORT",
        default_value = DEFAULT_REASONING_EFFORT,
        hide = true
    )]
    pub reasoning_effort: String,
    #[arg(long, env = "HERMES_HA_URL", default_value = "", hide = true)]
    pub ha_url: String,
    #[arg(long, env = "HERMES_HA_TOKEN_FILE", default_value = "", hide = true)]
    pub ha_token_file: String,
    #[arg(long, env = "HERMES_HA_TOKEN", default_value = "", hide = true)]
    pub ha_token: String,
    #[arg(
        long,
        env = "HERMES_VOICE_GATEWAY_URL",
        default_value = "",
        hide = true
    )]
    pub gateway_url: String,
    #[arg(
        long,
        env = "HERMES_VOICE_GATEWAY_TOKEN_FILE",
        default_value = "",
        hide = true
    )]
    pub gateway_token_file: String,
    #[arg(
        long,
        env = "HERMES_VOICE_GATEWAY_TOKEN",
        default_value = "",
        hide = true
    )]
    pub gateway_token: String,
    #[arg(
        long,
        env = "HERMES_VOICE_PROFILE_ENTITY",
        default_value = DEFAULT_PROFILE_ENTITY,
        hide = true
    )]
    pub profile_entity: String,
    #[arg(
        long,
        env = "HERMES_VOICE_PROFILE_CACHE_MS",
        default_value_t = DEFAULT_PROFILE_CACHE_MS,
        hide = true
    )]
    pub profile_cache_ms: u64,
    #[arg(
        long,
        env = "HERMES_HA_TIMEOUT",
        default_value_t = DEFAULT_HA_TIMEOUT,
        hide = true
    )]
    pub ha_timeout: f64,
    #[arg(
        long,
        env = "HERMES_VOICE_GATEWAY_TIMEOUT",
        default_value_t = DEFAULT_GATEWAY_TIMEOUT,
        hide = true
    )]
    pub gateway_timeout: f64,
    #[arg(
        long,
        env = "HERMES_VOICE_RESULT_TIMEOUT",
        default_value_t = DEFAULT_RESULT_TIMEOUT,
        hide = true
    )]
    pub result_timeout: f64,
    #[arg(
        long,
        env = "HERMES_VOICE_RESULT_RETRY",
        default_value_t = DEFAULT_RESULT_RETRY,
        hide = true
    )]
    pub result_retry: f64,
    #[arg(
        long,
        env = "HERMES_VOICE_RESULT_WAIT",
        default_value_t = DEFAULT_RESULT_WAIT,
        hide = true
    )]
    pub result_wait: u64,
    #[arg(long, env = "HERMES_VOICE_UPDATES", default_value = "1", hide = true)]
    pub updates: String,
    #[arg(
        long,
        env = "HERMES_VOICE_UPDATES_RETRY",
        default_value_t = DEFAULT_UPDATES_RETRY,
        hide = true
    )]
    pub updates_retry: f64,
    #[arg(
        long,
        env = "HERMES_VOICE_UPDATES_RESCAN",
        default_value_t = DEFAULT_UPDATES_RESCAN,
        hide = true
    )]
    pub updates_rescan: i64,
    #[arg(
        long,
        env = "HERMES_VOICE_ANNOUNCE_MAX_CHARS",
        default_value_t = DEFAULT_ANNOUNCE_MAX_CHARS,
        hide = true
    )]
    pub announce_max_chars: usize,
    #[arg(
        long,
        env = "HERMES_VOICE_ANNOUNCE_SCRIPT",
        default_value = DEFAULT_ANNOUNCE_SCRIPT,
        hide = true
    )]
    pub announce_script: String,
    #[arg(
        long,
        env = "HERMES_VOICE_SATELLITE",
        default_value = DEFAULT_SATELLITE,
        hide = true
    )]
    pub satellite: String,
    #[arg(
        long,
        env = "HERMES_VOICE_MEDIA_PLAYER",
        default_value = DEFAULT_MEDIA_PLAYER,
        hide = true
    )]
    pub media_player: String,
    #[arg(long, env = "HERMES_SCHEDULER_URL", default_value = "", hide = true)]
    pub scheduler_url: String,
    #[arg(
        long,
        env = "HERMES_SCHEDULER_TOKEN_FILE",
        default_value = "",
        hide = true
    )]
    pub scheduler_token_file: String,
    #[arg(long, env = "HERMES_SCHEDULER_TOKEN", default_value = "", hide = true)]
    pub scheduler_token: String,
    #[arg(long, env = "HERMES_VOICE_QUEUE", default_value = "1", hide = true)]
    pub queue: String,
    #[arg(
        long,
        env = "HERMES_VOICE_CONSUMER",
        default_value = DEFAULT_QUEUE_CONSUMER,
        hide = true
    )]
    pub queue_consumer: String,
    #[arg(
        long,
        env = "HERMES_VOICE_ANNOUNCE_TTL",
        default_value_t = DEFAULT_ANNOUNCE_TTL,
        hide = true
    )]
    pub announce_ttl: i64,
}

/// A secret that may come from a mounted file (re-read on every use) or from
/// the environment.
#[derive(Debug, Clone, Default)]
pub struct SecretSource {
    file: Option<PathBuf>,
    inline: String,
}

impl SecretSource {
    /// Builds the source; a non-empty file path wins over the inline value.
    #[must_use]
    pub fn new(file: &str, inline: &str) -> Self {
        Self {
            file: (!file.is_empty()).then(|| PathBuf::from(file)),
            inline: inline.to_owned(),
        }
    }

    /// The current plaintext. A configured but unreadable file is empty and
    /// does not fall back.
    #[must_use]
    pub fn value(&self) -> String {
        self.file.as_ref().map_or_else(
            || self.inline.trim().to_owned(),
            |path| {
                std::fs::read_to_string(path)
                    .map(|text| text.trim().to_owned())
                    .unwrap_or_default()
            },
        )
    }

    /// Whether the secret currently resolves to a non-empty value. A
    /// configured but unreadable file counts as unset, like the Go checks.
    #[must_use]
    pub fn is_configured(&self) -> bool {
        !self.value().is_empty()
    }

    /// A provider that re-reads the file on every call.
    #[must_use]
    pub fn provider(self: &std::sync::Arc<Self>) -> TokenProvider {
        let source = std::sync::Arc::clone(self);
        std::sync::Arc::new(move || source.value())
    }
}

/// A validated process configuration.
#[derive(Debug, Clone)]
pub struct Config {
    pub upstream: String,
    pub bind: String,
    pub port: u16,
    pub reasoning_effort: String,
    pub ha_url: String,
    pub gateway_url: String,
    pub scheduler_url: String,
    pub profile_entity: String,
    pub profile_cache: Duration,
    pub ha_timeout: Duration,
    pub gateway_timeout: Duration,
    pub result_timeout: Duration,
    pub result_retry: Duration,
    pub result_wait: u64,
    pub updates_disabled: bool,
    pub updates_retry: Duration,
    pub updates_rescan: i64,
    pub announce_max_chars: usize,
    pub announce_script: String,
    pub satellite: String,
    pub media_player: String,
    pub queue_disabled: bool,
    pub queue_consumer: String,
    pub announce_ttl: i64,
    pub ha_token: std::sync::Arc<SecretSource>,
    pub gateway_token: std::sync::Arc<SecretSource>,
    pub scheduler_token: std::sync::Arc<SecretSource>,
    pub log_level: LogLevel,
}

#[derive(Debug, Error)]
pub enum ConfigError {
    #[error(transparent)]
    Cli(#[from] clap::Error),
    #[error("listen address {0:?} is not valid")]
    Listen(String),
}

impl Config {
    /// Reads the process environment. Extra CLI arguments (`-healthcheck`) are
    /// handled by `main` before this is called.
    ///
    /// # Errors
    ///
    /// Returns a clap error for an invalid value or a listen-address error.
    pub fn load() -> Result<Self, ConfigError> {
        let program = std::env::args_os()
            .next()
            .unwrap_or_else(|| "voice-proxy".into());
        let settings = Settings::try_parse_from([program])?;
        Self::from_settings(
            settings,
            LogLevel::parse(&std::env::var("LOG_LEVEL").unwrap_or_default()),
        )
    }

    /// Builds the validated configuration from parsed settings.
    ///
    /// # Errors
    ///
    /// Returns a listen-address error.
    pub fn from_settings(settings: Settings, log_level: LogLevel) -> Result<Self, ConfigError> {
        let config = Self {
            upstream: base_url(&settings.upstream),
            bind: settings.bind,
            port: settings.port,
            reasoning_effort: settings.reasoning_effort,
            ha_url: base_url(&settings.ha_url),
            gateway_url: base_url(&settings.gateway_url),
            scheduler_url: base_url(&settings.scheduler_url),
            profile_entity: settings.profile_entity,
            profile_cache: Duration::from_millis(settings.profile_cache_ms),
            ha_timeout: seconds(settings.ha_timeout),
            gateway_timeout: seconds(settings.gateway_timeout),
            result_timeout: seconds(settings.result_timeout),
            result_retry: seconds(settings.result_retry),
            result_wait: settings.result_wait,
            updates_disabled: switched_off(&settings.updates),
            updates_retry: seconds(settings.updates_retry),
            updates_rescan: settings.updates_rescan,
            announce_max_chars: settings.announce_max_chars,
            announce_script: settings.announce_script,
            satellite: settings.satellite,
            media_player: settings.media_player,
            queue_disabled: switched_off(&settings.queue),
            queue_consumer: settings.queue_consumer,
            announce_ttl: settings.announce_ttl,
            ha_token: std::sync::Arc::new(SecretSource::new(
                &settings.ha_token_file,
                &settings.ha_token,
            )),
            gateway_token: std::sync::Arc::new(SecretSource::new(
                &settings.gateway_token_file,
                &settings.gateway_token,
            )),
            scheduler_token: std::sync::Arc::new(SecretSource::new(
                &settings.scheduler_token_file,
                &settings.scheduler_token,
            )),
            log_level,
        };
        // Fail before the service starts rather than on the first request.
        let _ = config.listen()?;
        Ok(config)
    }

    /// The TCP address the HTTP server binds.
    ///
    /// # Errors
    ///
    /// Returns an error when the bind/port pair is not an address.
    pub fn listen(&self) -> Result<SocketAddr, ConfigError> {
        let host = if self.bind.contains(':') && !self.bind.starts_with('[') {
            format!("[{}]", self.bind)
        } else {
            self.bind.clone()
        };
        format!("{host}:{}", self.port)
            .parse()
            .map_err(|_| ConfigError::Listen(self.bind.clone()))
    }

    /// Routing is enabled only when the full profile → gateway path is
    /// configured; a partially configured deployment keeps the plain sanitiser
    /// behaviour.
    #[must_use]
    pub fn routing_enabled(&self) -> bool {
        !self.gateway_url.is_empty()
            && self.gateway_token.is_configured()
            && !self.ha_url.is_empty()
            && self.ha_token.is_configured()
    }

    /// Whether the dispatcher updates poller should run.
    #[must_use]
    pub fn updates_enabled(&self) -> bool {
        self.routing_enabled() && !self.updates_disabled
    }

    /// Whether the durable announcement queue is fully configured.
    #[must_use]
    pub fn queue_enabled(&self) -> bool {
        !self.scheduler_url.is_empty()
            && self.scheduler_token.is_configured()
            && !self.queue_disabled
    }

    /// The consumer needs both switches.
    #[must_use]
    pub fn queue_enabled_and_routed(&self) -> bool {
        self.queue_enabled() && self.routing_enabled()
    }

    /// The JSON-safe summary printed by `-status`; never a token value.
    #[must_use]
    pub fn redacted(&self) -> Redacted {
        Redacted {
            listen: self.listen().map_or_else(
                |_| format!("{}:{}", self.bind, self.port),
                |address| address.to_string(),
            ),
            upstream: self.upstream.clone(),
            reasoning_effort: self.reasoning_effort.clone(),
            routing_enabled: self.routing_enabled(),
            updates_enabled: self.updates_enabled(),
            queue_enabled: self.queue_enabled(),
            queue_consumer_enabled: self.queue_enabled_and_routed(),
            profile_entity: self.profile_entity.clone(),
            announce_script: self.announce_script.clone(),
            satellite: self.satellite.clone(),
            media_player: self.media_player.clone(),
            announce_max_chars: self.announce_max_chars,
            queue_consumer: self.queue_consumer.clone(),
            scheduler_url: self.scheduler_url.clone(),
            ha_configured: !self.ha_url.is_empty() && self.ha_token.is_configured(),
            gateway_configured: !self.gateway_url.is_empty() && self.gateway_token.is_configured(),
            scheduler_configured: !self.scheduler_url.is_empty()
                && self.scheduler_token.is_configured(),
            announce_ttl: self.announce_ttl,
            updates_rescan: self.updates_rescan,
            updates_retry_ms: duration_millis(self.updates_retry),
            result_timeout_ms: duration_millis(self.result_timeout),
            result_retry_ms: duration_millis(self.result_retry),
            result_wait: self.result_wait,
        }
    }
}

/// The redacted `-status` summary.
#[derive(Debug, Clone, Serialize)]
pub struct Redacted {
    pub listen: String,
    pub upstream: String,
    pub reasoning_effort: String,
    pub routing_enabled: bool,
    pub updates_enabled: bool,
    pub queue_enabled: bool,
    pub queue_consumer_enabled: bool,
    pub profile_entity: String,
    pub announce_script: String,
    pub satellite: String,
    pub media_player: String,
    pub announce_max_chars: usize,
    pub queue_consumer: String,
    pub scheduler_url: String,
    pub ha_configured: bool,
    pub gateway_configured: bool,
    pub scheduler_configured: bool,
    pub announce_ttl: i64,
    pub updates_rescan: i64,
    pub updates_retry_ms: i64,
    pub result_timeout_ms: i64,
    pub result_retry_ms: i64,
    pub result_wait: u64,
}

fn duration_millis(duration: Duration) -> i64 {
    i64::try_from(duration.as_millis()).unwrap_or(i64::MAX)
}

fn seconds(value: f64) -> Duration {
    Duration::from_secs_f64(value.max(0.0))
}

/// Strips trailing slashes from a base URL.
fn base_url(value: &str) -> String {
    value.trim().trim_end_matches('/').to_owned()
}

/// The accepted off spellings, case-insensitively.
#[must_use]
pub fn switched_off(value: &str) -> bool {
    matches!(
        value.trim().to_lowercase().as_str(),
        "0" | "false" | "no" | "off"
    )
}

#[cfg(test)]
mod tests {
    use super::{
        Config, DEFAULT_PORT, DEFAULT_SATELLITE, SecretSource, Settings, base_url, switched_off,
    };
    use clap::Parser as _;

    fn settings() -> Settings {
        Settings::try_parse_from(["voice-proxy"]).expect("settings")
    }

    #[test]
    fn defaults_apply_to_an_empty_environment() {
        let settings = settings();
        assert_eq!(settings.port, DEFAULT_PORT);
        assert_eq!(settings.satellite, DEFAULT_SATELLITE);
        assert_eq!(settings.updates, "1");
        assert_eq!(settings.queue, "1");
    }

    #[test]
    fn off_spellings_are_case_insensitive() {
        for value in ["0", "false", "NO", "Off", " off "] {
            assert!(switched_off(value), "{value}");
        }
        for value in ["1", "true", "", "yes"] {
            assert!(!switched_off(value), "{value}");
        }
    }

    #[test]
    fn base_urls_lose_trailing_slashes_only() {
        assert_eq!(base_url(" http://ha.local/ "), "http://ha.local");
        assert_eq!(base_url("http://ha.local/api/"), "http://ha.local/api");
        assert_eq!(base_url(""), "");
    }

    #[test]
    fn secret_file_wins_and_is_reread_on_every_use() {
        let path = std::env::temp_dir().join(format!(
            "voice-proxy-secret-{}-{}.txt",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("clock")
                .as_nanos()
        ));
        std::fs::write(&path, " from-file\n").expect("write");
        let source = SecretSource::new(path.to_str().expect("path"), "inline");
        assert_eq!(source.value(), "from-file");
        std::fs::write(&path, "rotated\n").expect("rewrite");
        assert_eq!(source.value(), "rotated");
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn an_unreadable_file_is_empty_and_never_falls_back() {
        let source = SecretSource::new("/definitely/not/a/secret", "inline");
        assert_eq!(source.value(), "");
        assert!(!source.is_configured());
    }

    #[test]
    fn routing_and_queue_gates_require_both_halves() {
        let mut settings = settings();
        let config =
            Config::from_settings(settings.clone(), home_config::LogLevel::Info).expect("config");
        assert!(!config.routing_enabled());
        assert!(!config.updates_enabled());
        assert!(!config.queue_enabled());

        settings.ha_url = "http://ha.local".to_owned();
        settings.ha_token = "ha".to_owned();
        settings.gateway_url = "http://gateway.local".to_owned();
        settings.gateway_token = "gateway".to_owned();
        settings.scheduler_url = "http://scheduler.local".to_owned();
        settings.scheduler_token = "scheduler".to_owned();
        let config =
            Config::from_settings(settings.clone(), home_config::LogLevel::Info).expect("config");
        assert!(config.routing_enabled());
        assert!(config.updates_enabled());
        assert!(config.queue_enabled());
        assert!(config.queue_enabled_and_routed());

        settings.queue = "0".to_owned();
        let config = Config::from_settings(settings, home_config::LogLevel::Info).expect("config");
        assert!(!config.queue_enabled());
    }

    #[test]
    fn redacted_never_contains_a_token() {
        let mut settings = settings();
        settings.ha_url = "http://ha.local".to_owned();
        settings.ha_token = "super-secret".to_owned();
        let config = Config::from_settings(settings, home_config::LogLevel::Info).expect("config");
        let json = serde_json::to_string(&config.redacted()).expect("json");
        assert!(!json.contains("super-secret"));
        assert!(json.contains("\"ha_configured\":true"));
    }
}
