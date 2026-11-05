//! Go-compatible flag parsing (`-flag`, `--flag`, `-flag=value`).

use std::path::PathBuf;

use crate::gateway::Config;

pub const USAGE: &str = "\
usage: ble-gateway [flags]
  -auto=true: advertise automatically when Wi-Fi is down
  -button string: input event device (default \"/dev/input/event2\")
  -control-addr string: HTTP control listen address (empty disables)
  -control-token-file string: HTTP control token file
  -dry-run: do not apply Wi-Fi changes
  -force: enable BLE immediately for testing
  -log string: log file
  -no-button: disable button monitor
  -test-wpa: add/set/remove a temporary wpa network and exit
  -token-file string: token file
";

pub enum Parsed {
    Config(Config),
    Help,
}

fn parse_bool(value: &str) -> Result<bool, String> {
    match value {
        "1" | "t" | "T" | "true" | "TRUE" | "True" => Ok(true),
        "0" | "f" | "F" | "false" | "FALSE" | "False" => Ok(false),
        other => Err(format!("invalid boolean value {other:?}")),
    }
}

/// Parse the arguments after argv[0]. Non-flag arguments end flag parsing, as
/// in Go's `flag` package.
pub fn parse<I: IntoIterator<Item = String>>(args: I) -> Result<Parsed, String> {
    let mut config = Config::default();
    let mut args = args.into_iter();
    while let Some(argument) = args.next() {
        if argument == "--" {
            break;
        }
        if !argument.starts_with('-') || argument == "-" {
            break;
        }
        let trimmed = argument.trim_start_matches('-');
        let (name, inline) = match trimmed.split_once('=') {
            Some((name, value)) => (name, Some(value.to_string())),
            None => (trimmed, None),
        };
        match name {
            "h" | "help" => return Ok(Parsed::Help),
            "dry-run" | "auto" | "no-button" | "force" | "test-wpa" => {
                let value = match inline {
                    Some(value) => parse_bool(&value)?,
                    None => true,
                };
                match name {
                    "dry-run" => config.dry_run = value,
                    "auto" => config.auto_wifi = value,
                    "no-button" => config.no_button = value,
                    "force" => config.force = value,
                    "test-wpa" => config.test_wpa = value,
                    _ => unreachable!(),
                }
            }
            "log" | "token-file" | "button" | "control-addr" | "control-token-file" => {
                let value = match inline {
                    Some(value) => value,
                    None => args
                        .next()
                        .ok_or_else(|| format!("flag needs an argument: -{name}"))?,
                };
                match name {
                    "log" => config.log = Some(PathBuf::from(value)),
                    "token-file" => config.token_file = PathBuf::from(value),
                    "button" => config.button = PathBuf::from(value),
                    "control-addr" => config.control_addr = value,
                    "control-token-file" => config.control_token_file = PathBuf::from(value),
                    _ => unreachable!(),
                }
            }
            other => return Err(format!("flag provided but not defined: -{other}")),
        }
    }
    Ok(Parsed::Config(config))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_match_go() {
        let Parsed::Config(config) = parse(Vec::new()).unwrap() else {
            panic!("expected config");
        };
        assert!(!config.dry_run);
        assert!(config.auto_wifi);
        assert!(!config.no_button);
        assert!(!config.force);
        assert!(!config.test_wpa);
        assert_eq!(
            config.token_file,
            PathBuf::from("/data/quasar/ble-gateway/token")
        );
        assert_eq!(config.button, PathBuf::from("/dev/input/event2"));
        assert_eq!(config.control_addr, "0.0.0.0:8099");
        assert_eq!(
            config.control_token_file,
            PathBuf::from("/data/quasar/ble-gateway/control-token")
        );
    }

    #[test]
    fn start_script_form() {
        let Parsed::Config(config) = parse([
            "--auto=true".to_string(),
            "--log".to_string(),
            "/tmp/x.log".to_string(),
        ])
        .unwrap() else {
            panic!("expected config");
        };
        assert!(config.auto_wifi);
        assert_eq!(config.log, Some(PathBuf::from("/tmp/x.log")));
    }

    #[test]
    fn bool_and_value_flags() {
        let Parsed::Config(config) = parse(
            [
                "--dry-run",
                "-auto=false",
                "--no-button",
                "--force=1",
                "--token-file=/tmp/token",
            ]
            .map(str::to_string),
        )
        .unwrap() else {
            panic!("expected config");
        };
        assert!(config.dry_run);
        assert!(!config.auto_wifi);
        assert!(config.no_button);
        assert!(config.force);
        assert_eq!(config.token_file, PathBuf::from("/tmp/token"));
    }

    #[test]
    fn rejects_unknown_and_bad_bool() {
        assert!(parse(["--nope".to_string()]).is_err());
        assert!(parse(["--auto=maybe".to_string()]).is_err());
    }
}
