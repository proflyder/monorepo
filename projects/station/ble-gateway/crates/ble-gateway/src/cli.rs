//! Command-line interface (clap derive).

use std::path::PathBuf;

use clap::Parser;
use clap::builder::BoolishValueParser;

/// Station BLE provisioning gateway.
#[derive(Debug, Clone, Parser)]
#[rustfmt::skip]
#[command(name = "ble-gateway", version, about = "Local BLE and HTTP control gateway for the station Wi-Fi setup")]
pub struct Config {
    /// `--auto`, `--auto=true|false`, `--auto=1|0`.
    #[arg(long = "auto", value_name = "BOOL", num_args = 0..=1, default_missing_value = "true", default_value = "true", value_parser = BoolishValueParser::new(), action = clap::ArgAction::Set)]
    pub auto_wifi: bool,
    #[arg(long, value_name = "PATH")]
    pub log: Option<PathBuf>,
    #[arg(long, value_name = "PATH", default_value = "/data/quasar/ble-gateway/token")]
    pub token_file: PathBuf,
    #[arg(long, value_name = "PATH", default_value = "/dev/input/event2")]
    pub button: PathBuf,
    #[arg(long)]
    pub dry_run: bool,
    #[arg(long)]
    pub no_button: bool,
    #[arg(long)]
    pub force: bool,
    #[arg(long)]
    pub test_wpa: bool,
    #[arg(long, value_name = "ADDR", default_value = "0.0.0.0:8099")]
    pub control_addr: String,
    #[arg(long, value_name = "PATH", default_value = "/data/quasar/ble-gateway/control-token")]
    pub control_token_file: PathBuf,
    #[arg(long, value_name = "PATH", default_value = "/var/run/wpa_supplicant/wlan0")]
    pub wpa_ctrl: PathBuf,
    #[arg(long, value_name = "FILTER", default_value = "info")]
    pub log_level: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(arguments: &[&str]) -> Config {
        Config::try_parse_from(arguments).expect("valid arguments")
    }

    #[test]
    fn defaults_match_the_deployed_paths() {
        let cli = parse(&["ble-gateway"]);
        assert!(cli.auto_wifi);
        assert_eq!(
            cli.token_file,
            PathBuf::from("/data/quasar/ble-gateway/token")
        );
        assert_eq!(cli.button, PathBuf::from("/dev/input/event2"));
        assert_eq!(cli.control_addr, "0.0.0.0:8099");
        assert_eq!(cli.wpa_ctrl, PathBuf::from("/var/run/wpa_supplicant/wlan0"));
        assert_eq!(cli.log, None);
    }

    #[test]
    fn station_start_script_and_boolean_forms() {
        let cli = parse(&["ble-gateway", "--auto=true", "--log", "/tmp/gw.log"]);
        assert!(cli.auto_wifi);
        assert_eq!(cli.log, Some(PathBuf::from("/tmp/gw.log")));
        assert!(!parse(&["ble-gateway", "--auto=false"]).auto_wifi);
        assert!(!parse(&["ble-gateway", "--auto=0"]).auto_wifi);
        assert!(parse(&["ble-gateway", "--auto"]).auto_wifi);
        assert!(Config::try_parse_from(["ble-gateway", "--auto=maybe"]).is_err());
    }

    #[test]
    fn value_flags_and_switches() {
        let cli = parse(&[
            "ble-gateway",
            "--dry-run",
            "--no-button",
            "--force",
            "--test-wpa",
            "--token-file=/tmp/token",
            "--button",
            "/tmp/event",
            "--control-addr",
            "",
            "--control-token-file",
            "/tmp/control",
            "--wpa-ctrl",
            "/tmp/wpa",
            "--log-level",
            "debug",
        ]);
        assert!(cli.dry_run && cli.no_button && cli.force && cli.test_wpa);
        assert_eq!(cli.token_file, PathBuf::from("/tmp/token"));
        assert_eq!(cli.control_addr, "");
        assert_eq!(cli.wpa_ctrl, PathBuf::from("/tmp/wpa"));
        assert_eq!(cli.log_level, "debug");
    }
}
