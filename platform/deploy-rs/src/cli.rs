//! CLI parsing and dispatch.
//!
//! The Go binary uses `flag`, which accepts both `-flag` and `--flag`. Every
//! argument is normalised to the double-dash form before `clap` sees it, so the
//! existing deploy scripts and manual invocations keep working. Unimplemented
//! subcommands are declared here (so `--help` lists them) and exit with status
//! 2 until their module is ported.

use std::io::Write;

use clap::{Arg, ArgAction, Command, error::ErrorKind};

use crate::render::{self, Options};
use crate::{USAGE, buildid, envfile};

/// Commands that are declared but not ported yet.
const NOT_IMPLEMENTED: &[&str] = &[
    "oci",
    "converge",
    "state",
    "preflight",
    "runtime",
    "eso",
    "secret",
    "hermes",
];

/// Every top-level command, used to keep the Go binary's "unknown command"
/// response (full usage plus one line) instead of clap's shorter message.
const KNOWN_COMMANDS: &[&str] = &[
    "render",
    "env",
    "build-id",
    "oci",
    "converge",
    "state",
    "preflight",
    "runtime",
    "eso",
    "secret",
    "hermes",
];

/// Runs one CLI invocation and returns the process exit status.
pub fn run(args: &[String], stdout: &mut dyn Write, stderr: &mut dyn Write) -> i32 {
    if args.is_empty() {
        let _ = write!(stderr, "{USAGE}");
        let _ = writeln!(stderr, "missing command");
        return 1;
    }
    match args[0].as_str() {
        "-h" | "--help" | "help" => {
            let _ = write!(stdout, "{USAGE}");
            return 0;
        }
        command if !KNOWN_COMMANDS.contains(&command) => {
            let _ = write!(stderr, "{USAGE}");
            let _ = writeln!(stderr, "unknown command {:?}", args[0]);
            return 1;
        }
        _ => {}
    }

    let normalized = normalize_args(args);
    if NOT_IMPLEMENTED.contains(&args[0].as_str()) && !normalized.iter().any(|arg| arg == "--help")
    {
        let _ = writeln!(stderr, "home-deploy {}: not implemented yet", args[0]);
        return 2;
    }

    let mut argv = vec!["home-deploy".to_string()];
    argv.extend(normalized);
    let matches = match command().try_get_matches_from(argv) {
        Ok(matches) => matches,
        Err(error) => {
            if matches!(
                error.kind(),
                ErrorKind::DisplayHelp | ErrorKind::DisplayVersion
            ) {
                let _ = write!(stdout, "{error}");
                return 0;
            }
            let _ = write!(stderr, "{error}");
            return 1;
        }
    };

    match dispatch(&matches, stdout, stderr) {
        Ok(()) => 0,
        Err(Failure { code, message }) => {
            if let Some(message) = message {
                let _ = writeln!(stderr, "{message}");
            }
            code
        }
    }
}

/// Converts the Go `flag` single-dash form (`-unit`) to `--unit`.
fn normalize_args(args: &[String]) -> Vec<String> {
    args.iter()
        .map(|arg| {
            if arg == "-h" {
                return "--help".to_string();
            }
            if arg.len() > 1 && arg.starts_with('-') && !arg.starts_with("--") {
                return format!("-{arg}");
            }
            arg.clone()
        })
        .collect()
}

struct Failure {
    code: i32,
    message: Option<String>,
}

impl Failure {
    fn new(message: impl Into<String>) -> Self {
        Self {
            code: 1,
            message: Some(message.into()),
        }
    }

    fn code(code: i32) -> Self {
        Self {
            code,
            message: None,
        }
    }
}

fn dispatch(
    matches: &clap::ArgMatches,
    stdout: &mut dyn Write,
    stderr: &mut dyn Write,
) -> Result<(), Failure> {
    match matches.subcommand() {
        Some(("render", sub)) => run_render(sub, stdout),
        Some(("env", sub)) => run_env(sub, stdout),
        Some(("build-id", sub)) => run_build_id(sub, stdout),
        Some((name, _)) => {
            let _ = writeln!(stderr, "home-deploy {name}: not implemented yet");
            Err(Failure::code(2))
        }
        None => {
            let _ = write!(stderr, "{USAGE}");
            Err(Failure::new("missing command"))
        }
    }
}

fn run_render(matches: &clap::ArgMatches, stdout: &mut dyn Write) -> Result<(), Failure> {
    let unit = matches
        .get_one::<String>("unit")
        .map_or("scheduler", String::as_str);
    let env_file = matches.get_one::<String>("env").map_or("", String::as_str);
    let out_dir = matches.get_one::<String>("out").map_or("", String::as_str);
    let rollout_key = matches
        .get_one::<String>("rollout-key")
        .map_or("", String::as_str);
    let sets: Vec<String> = matches
        .get_many::<String>("set")
        .map(|values| values.cloned().collect())
        .unwrap_or_default();
    let files: Vec<String> = matches
        .get_many::<String>("files")
        .map(|values| values.cloned().collect())
        .unwrap_or_default();
    if out_dir.is_empty() {
        return Err(Failure::new("render: --out is required"));
    }
    if files.is_empty() {
        return Err(Failure::new("render: at least one template is required"));
    }
    let options = Options {
        env_file,
        sets: &sets,
        out_dir,
        files: &files,
        unit,
        rollout_key,
    };
    render::run(&options, stdout).map_err(Failure::new)
}

fn run_env(matches: &clap::ArgMatches, stdout: &mut dyn Write) -> Result<(), Failure> {
    let Some(("get", sub)) = matches.subcommand() else {
        return Err(Failure::new("usage: home-deploy env get FILE KEY"));
    };
    let file = sub
        .get_one::<String>("file")
        .ok_or_else(|| Failure::new("usage: home-deploy env get FILE KEY"))?;
    let key = sub
        .get_one::<String>("key")
        .ok_or_else(|| Failure::new("usage: home-deploy env get FILE KEY"))?;
    let value = envfile::lookup(std::path::Path::new(file), key).map_err(Failure::new)?;
    let _ = writeln!(stdout, "{value}");
    Ok(())
}

fn run_build_id(matches: &clap::ArgMatches, stdout: &mut dyn Write) -> Result<(), Failure> {
    let fingerprint = matches
        .get_one::<String>("fingerprint")
        .map_or("", String::as_str);
    let revision = matches
        .get_one::<String>("revision")
        .map_or("", String::as_str);
    if fingerprint.is_empty() {
        return Err(Failure::new("build-id: --fingerprint is required"));
    }
    let value = if revision.is_empty() {
        buildid::fingerprint(fingerprint)
    } else {
        buildid::build_id(revision, fingerprint)
    };
    let _ = writeln!(stdout, "{value}");
    Ok(())
}

/// A one-value string flag.
fn string(name: &'static str) -> Arg {
    Arg::new(name).long(name).num_args(1).action(ArgAction::Set)
}

/// A repeatable one-value string flag.
fn repeated(name: &'static str) -> Arg {
    Arg::new(name)
        .long(name)
        .num_args(1)
        .action(ArgAction::Append)
}

/// A boolean flag.
fn boolean(name: &'static str) -> Arg {
    Arg::new(name).long(name).action(ArgAction::SetTrue)
}

/// A positional list of files.
fn files() -> Arg {
    Arg::new("files").num_args(0..).action(ArgAction::Append)
}

fn command() -> Command {
    Command::new("home-deploy")
        .disable_help_flag(false)
        .disable_version_flag(true)
        .subcommand_required(false)
        .subcommand(
            Command::new("render")
                .arg(string("unit"))
                .arg(string("env"))
                .arg(string("out"))
                .arg(string("rollout-key"))
                .arg(repeated("set"))
                .arg(files()),
        )
        .subcommand(
            Command::new("env").subcommand(
                Command::new("get")
                    .arg(Arg::new("file").num_args(1))
                    .arg(Arg::new("key").num_args(1)),
            ),
        )
        .subcommand(
            Command::new("build-id")
                .arg(string("fingerprint"))
                .arg(string("revision")),
        )
        .subcommand(oci_command())
        .subcommand(
            Command::new("converge")
                .arg(string("kubectl"))
                .arg(string("namespace"))
                .arg(string("name"))
                .arg(string("file"))
                .arg(boolean("preserve-replicas"))
                .arg(boolean("preserve-env"))
                .arg(string("summary-file")),
        )
        .subcommand(state_command())
        .subcommand(
            Command::new("preflight")
                .arg(string("ha-url"))
                .arg(string("ha-token-file"))
                .arg(string("scheduler-url"))
                .arg(string("scheduler-token-file"))
                .arg(string("expected-endpoint")),
        )
        .subcommand(
            Command::new("runtime").subcommand(
                Command::new("split")
                    .arg(string("render"))
                    .arg(string("private"))
                    .arg(string("storage-out"))
                    .arg(string("values-out"))
                    .arg(string("health-exporter-image"))
                    .arg(string("health-exporter-workload")),
            ),
        )
        .subcommand(
            Command::new("eso").subcommand(Command::new("names").arg(string("out")).arg(files())),
        )
        .subcommand(
            Command::new("secret").subcommand(
                Command::new("opaque")
                    .arg(string("name"))
                    .arg(string("namespace"))
                    .arg(string("key"))
                    .arg(string("from-env-file"))
                    .arg(string("env")),
            ),
        )
        .subcommand(hermes_command())
}

fn oci_command() -> Command {
    let archive = |name: &'static str| {
        Command::new(name)
            .arg(Arg::new("layout").num_args(1))
            .arg(string("ref"))
            .arg(string("out"))
    };
    Command::new("oci")
        .subcommand(Command::new("digest").arg(Arg::new("layout").num_args(1)))
        .subcommand(archive("pack"))
        .subcommand(archive("docker"))
}

fn state_command() -> Command {
    Command::new("state")
        .subcommand(
            Command::new("record")
                .arg(string("state-dir"))
                .arg(string("namespace"))
                .arg(string("name"))
                .arg(string("kubectl")),
        )
        .subcommand(
            Command::new("mark")
                .arg(string("state-dir"))
                .arg(string("name")),
        )
        .subcommand(
            Command::new("rollback")
                .arg(string("state-dir"))
                .arg(string("namespace"))
                .arg(string("kubectl")),
        )
}

fn hermes_command() -> Command {
    Command::new("hermes")
        .subcommand(
            Command::new("rollout")
                .arg(string("values-file"))
                .arg(string("hermes-digest"))
                .arg(string("voice-proxy-digest"))
                .arg(string("exporter-digest"))
                .arg(string("smoke-image"))
                .arg(string("release"))
                .arg(string("deployment"))
                .arg(string("exporter-deployment"))
                .arg(string("namespace"))
                .arg(string("chart"))
                .arg(string("kubectl"))
                .arg(string("helm"))
                .arg(string("state-dir"))
                .arg(string("timeout"))
                .arg(boolean("force"))
                .arg(boolean("simulate-smoke-failure"))
                .arg(boolean("rollback-only")),
        )
        .subcommand(
            Command::new("render-config")
                .arg(string("home"))
                .arg(string("private-values"))
                .arg(string("fragment"))
                .arg(string("voice-prompt"))
                .arg(string("telegram-prompt"))
                .arg(string("context"))
                .arg(string("soul"))
                .arg(string("skills"))
                .arg(boolean("allow-placeholders"))
                .arg(boolean("dry-run"))
                .arg(boolean("check")),
        )
        .subcommand(
            Command::new("assist-smoke")
                .arg(string("ha-url"))
                .arg(string("agent"))
                .arg(string("text"))
                .arg(string("token-file"))
                .arg(string("timeout")),
        )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn execute(args: &[&str]) -> (i32, String, String) {
        let args: Vec<String> = args.iter().map(|arg| (*arg).to_string()).collect();
        let mut stdout = Vec::new();
        let mut stderr = Vec::new();
        let code = run(&args, &mut stdout, &mut stderr);
        (
            code,
            String::from_utf8(stdout).expect("stdout"),
            String::from_utf8(stderr).expect("stderr"),
        )
    }

    #[test]
    fn no_arguments_prints_usage_and_fails() {
        let (code, stdout, stderr) = execute(&[]);
        assert_eq!(code, 1);
        assert!(stdout.is_empty());
        assert!(stderr.contains("home-deploy <command>"));
        assert!(stderr.contains("missing command"));
    }

    #[test]
    fn help_goes_to_stdout() {
        let (code, stdout, stderr) = execute(&["--help"]);
        assert_eq!(code, 0);
        assert!(stdout.contains("home-deploy <command>"));
        assert!(stderr.is_empty());
    }

    #[test]
    fn unknown_command_prints_usage_and_fails() {
        let (code, _, stderr) = execute(&["bogus"]);
        assert_eq!(code, 1);
        assert!(stderr.contains("home-deploy <command>"));
        assert!(stderr.contains("unknown command \"bogus\""));
    }

    #[test]
    fn unimplemented_commands_exit_2() {
        for name in NOT_IMPLEMENTED {
            let (code, _, stderr) = execute(&[name]);
            assert_eq!(code, 2, "{name}: {stderr}");
            assert!(stderr.contains("not implemented"));
        }
    }

    #[test]
    fn single_dash_flags_match_the_go_binary() {
        let (code, stdout, _) = execute(&["build-id", "-fingerprint", "hello"]);
        assert_eq!(code, 0);
        assert_eq!(stdout, "2cf24dba5fb0\n");
    }
}
