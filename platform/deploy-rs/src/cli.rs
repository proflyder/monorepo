//! CLI parsing and dispatch.
//!
//! The Go binary uses `flag`, which accepts both `-flag` and `--flag`. Every
//! argument is normalised to the double-dash form before `clap` sees it, so the
//! existing deploy scripts and manual invocations keep working. Unimplemented
//! subcommands are declared here (so `--help` lists them) and exit with status
//! 2 until their module is ported.

use std::io::Write;

use clap::{Arg, ArgAction, Command, error::ErrorKind};

use std::path::{Path, PathBuf};

use crate::process::{SystemCommandRunner, SystemRunner};
use crate::render::{self, Options};
use crate::{
    USAGE, buildid, converge, envfile, hermes, hermesconfig, manifests, oci, preflight,
    runtimevalues, state,
};

/// Commands that are declared but not ported yet.
const NOT_IMPLEMENTED: &[&str] = &[];

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
        Some(("oci", sub)) => run_oci(sub, stdout),
        Some(("converge", sub)) => run_converge(sub, stdout),
        Some(("state", sub)) => run_state(sub, stdout),
        Some(("preflight", sub)) => run_preflight(sub, stdout),
        Some(("hermes", sub)) => run_hermes(sub, stdout, stderr),
        Some(("runtime", sub)) => run_runtime(sub, stdout),
        Some(("eso", sub)) => run_eso(sub, stdout),
        Some(("secret", sub)) => run_secret(sub, stdout),
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
    let value = envfile::lookup(Path::new(file), key).map_err(Failure::new)?;
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

fn run_preflight(matches: &clap::ArgMatches, stdout: &mut dyn Write) -> Result<(), Failure> {
    let ha_url = matches
        .get_one::<String>("ha-url")
        .map_or("", String::as_str);
    let ha_token_file = matches
        .get_one::<String>("ha-token-file")
        .map_or("", String::as_str);
    let scheduler_url = matches
        .get_one::<String>("scheduler-url")
        .map_or("", String::as_str);
    let scheduler_token_file = matches
        .get_one::<String>("scheduler-token-file")
        .map_or("", String::as_str);
    let expected_endpoint = matches
        .get_one::<String>("expected-endpoint")
        .map_or("", String::as_str);
    if ha_url.is_empty()
        || ha_token_file.is_empty()
        || scheduler_url.is_empty()
        || scheduler_token_file.is_empty()
    {
        return Err(Failure::new(
            "preflight: --ha-url, --ha-token-file, --scheduler-url and --scheduler-token-file are required",
        ));
    }
    if !Path::new(ha_token_file).exists() {
        return Err(Failure::new(format!(
            "preflight: read HA token: open {ha_token_file}: no such file or directory"
        )));
    }
    if !Path::new(scheduler_token_file).exists() {
        return Err(Failure::new(format!(
            "preflight: read scheduler token: open {scheduler_token_file}: no such file or directory"
        )));
    }
    let status = preflight::run(
        &preflight::Options {
            ha_url,
            ha_token_file,
            scheduler_url,
            scheduler_token_file,
            expected_endpoint,
        },
        stdout,
    );
    if status != 0 {
        return Err(Failure::code(status));
    }
    Ok(())
}

fn run_hermes(
    matches: &clap::ArgMatches,
    stdout: &mut dyn Write,
    stderr: &mut dyn Write,
) -> Result<(), Failure> {
    let Some((command, sub)) = matches.subcommand() else {
        return Err(Failure::new(
            "usage: home-deploy hermes rollout|render-config|assist-smoke ...",
        ));
    };
    match command {
        "rollout" => run_hermes_rollout(sub, stdout, stderr),
        "render-config" => run_hermes_render_config(sub, stdout),
        "assist-smoke" => run_hermes_assist_smoke(sub, stdout),
        other => Err(Failure::new(format!(
            "unknown hermes command {other:?} (expected rollout, render-config or assist-smoke)"
        ))),
    }
}

fn run_hermes_rollout(
    sub: &clap::ArgMatches,
    stdout: &mut dyn Write,
    stderr: &mut dyn Write,
) -> Result<(), Failure> {
    let default = hermes::Options::default();
    let chart = sub.get_one::<String>("chart").map_or("", String::as_str);
    let chart = if chart.is_empty() {
        repo_root()
            .map(|root| {
                root.join("projects/home-assistant/deploy/chart")
                    .display()
                    .to_string()
            })
            .unwrap_or_default()
    } else {
        chart.to_string()
    };
    let state_dir = sub
        .get_one::<String>("state-dir")
        .map_or("", String::as_str);
    let state_dir = if state_dir.is_empty() {
        temporary_state_dir()?
    } else {
        state_dir.to_string()
    };
    let options = hermes::Options {
        values_file: sub
            .get_one::<String>("values-file")
            .map_or("", String::as_str)
            .to_string(),
        hermes_digest: sub
            .get_one::<String>("hermes-digest")
            .map_or("", String::as_str)
            .to_string(),
        voice_proxy_digest: sub
            .get_one::<String>("voice-proxy-digest")
            .map_or("", String::as_str)
            .to_string(),
        exporter_digest: sub
            .get_one::<String>("exporter-digest")
            .map_or("", String::as_str)
            .to_string(),
        smoke_image: sub
            .get_one::<String>("smoke-image")
            .map_or("", String::as_str)
            .to_string(),
        release: sub
            .get_one::<String>("release")
            .map_or(default.release.as_str(), String::as_str)
            .to_string(),
        deployment: sub
            .get_one::<String>("deployment")
            .map_or(default.deployment.as_str(), String::as_str)
            .to_string(),
        exporter_deployment: sub
            .get_one::<String>("exporter-deployment")
            .map_or(default.exporter_deployment.as_str(), String::as_str)
            .to_string(),
        namespace: sub
            .get_one::<String>("namespace")
            .map_or(default.namespace.as_str(), String::as_str)
            .to_string(),
        chart,
        state_dir,
        timeout: sub
            .get_one::<String>("timeout")
            .map_or(default.timeout.as_str(), String::as_str)
            .to_string(),
        force: sub.get_flag("force"),
        simulate_failure: sub.get_flag("simulate-smoke-failure"),
        rollback_only: sub.get_flag("rollback-only"),
        kubectl: sub
            .get_one::<String>("kubectl")
            .map_or("kubectl", String::as_str)
            .to_string(),
        helm: sub
            .get_one::<String>("helm")
            .map_or("helm", String::as_str)
            .to_string(),
        retry_window_secs: default.retry_window_secs,
        retry_delay_secs: default.retry_delay_secs,
    };
    let code = hermes::run(&SystemCommandRunner, &options, stdout, stderr).map_err(Failure::new)?;
    if code == 0 {
        Ok(())
    } else {
        Err(Failure::code(code))
    }
}

fn run_hermes_render_config(sub: &clap::ArgMatches, stdout: &mut dyn Write) -> Result<(), Failure> {
    let home = sub.get_one::<String>("home").map_or("", String::as_str);
    if home.is_empty() {
        return Err(Failure::new("hermes render-config: --home is required"));
    }
    let dry_run = sub.get_flag("dry-run");
    let check = sub.get_flag("check");
    if dry_run && check {
        return Err(Failure::new(
            "hermes render-config: --dry-run and --check are mutually exclusive",
        ));
    }
    let source_dir = repo_root().map(|root| root.join("projects/home-assistant/hermes"));
    let default_path = |value: &str, name: &str| {
        if value.is_empty() {
            source_dir
                .as_ref()
                .map(|dir| dir.join(name).display().to_string())
                .unwrap_or_default()
        } else {
            value.to_string()
        }
    };
    let options = hermesconfig::Options {
        home: home.to_string(),
        fragment: default_path(
            sub.get_one::<String>("fragment").map_or("", String::as_str),
            "config.fragment.yaml",
        ),
        context: default_path(
            sub.get_one::<String>("context").map_or("", String::as_str),
            "HERMES.md",
        ),
        soul: default_path(
            sub.get_one::<String>("soul").map_or("", String::as_str),
            "SOUL.md",
        ),
        voice_prompt: default_path(
            sub.get_one::<String>("voice-prompt")
                .map_or("", String::as_str),
            "voice_prompt.txt",
        ),
        telegram_prompt: default_path(
            sub.get_one::<String>("telegram-prompt")
                .map_or("", String::as_str),
            "telegram_prompt.txt",
        ),
        skills: default_path(
            sub.get_one::<String>("skills").map_or("", String::as_str),
            "skills",
        ),
        private_values: sub
            .get_one::<String>("private-values")
            .map_or("", String::as_str)
            .to_string(),
        allow_placeholders: sub.get_flag("allow-placeholders"),
    };
    let mut buffer = Vec::new();
    let code = hermesconfig::run(&options, dry_run, check, &mut buffer).map_err(Failure::new)?;
    stdout
        .write_all(&buffer)
        .map_err(|error| Failure::new(error.to_string()))?;
    if code == 0 {
        Ok(())
    } else {
        Err(Failure::code(code))
    }
}

fn run_hermes_assist_smoke(sub: &clap::ArgMatches, stdout: &mut dyn Write) -> Result<(), Failure> {
    let ha_url = sub
        .get_one::<String>("ha-url")
        .map_or(hermes::HA_URL, String::as_str);
    let agent = sub
        .get_one::<String>("agent")
        .map_or(hermes::HA_AGENT, String::as_str);
    let text = sub
        .get_one::<String>("text")
        .map_or(hermes::SMOKE_TEXT, String::as_str);
    let token_file = sub
        .get_one::<String>("token-file")
        .map_or("/run/secrets/ha/token", String::as_str);
    let timeout = sub
        .get_one::<String>("timeout")
        .map_or(Ok(std::time::Duration::from_secs(150)), |value| {
            parse_go_duration(value)
        })
        .map_err(|error| Failure::new(format!("hermes assist-smoke: {error}")))?;
    let code = hermes::assist_smoke_client(ha_url, agent, text, token_file, timeout, stdout);
    if code == 0 {
        Ok(())
    } else {
        Err(Failure::code(code))
    }
}

/// Parses a Go duration (`150s`, `1m30s`, `500ms`).
fn parse_go_duration(value: &str) -> Result<std::time::Duration, String> {
    let mut total = 0.0f64;
    let mut rest = value.trim();
    if rest.is_empty() {
        return Err("time: invalid duration".to_string());
    }
    if let Some(stripped) = rest.strip_prefix('-') {
        rest = stripped;
    }
    while !rest.is_empty() {
        let number_end = rest
            .find(|character: char| !character.is_ascii_digit() && character != '.')
            .unwrap_or(rest.len());
        if number_end == 0 {
            return Err(format!("time: invalid duration {value:?}"));
        }
        let number = rest[..number_end]
            .parse::<f64>()
            .map_err(|_| format!("time: invalid duration {value:?}"))?;
        rest = &rest[number_end..];
        let (unit, scale) = if let Some(rest) = rest.strip_prefix("ms") {
            (rest, 1e-3)
        } else if let Some(rest) = rest.strip_prefix("ns") {
            (rest, 1e-9)
        } else if let Some(rest) = rest.strip_prefix("us") {
            (rest, 1e-6)
        } else if let Some(rest) = rest.strip_prefix("µs") {
            (rest, 1e-6)
        } else if let Some(rest) = rest.strip_prefix('s') {
            (rest, 1.0)
        } else if let Some(rest) = rest.strip_prefix('m') {
            (rest, 60.0)
        } else if let Some(rest) = rest.strip_prefix('h') {
            (rest, 3600.0)
        } else {
            return Err(format!("time: invalid duration {value:?}"));
        };
        total += number * scale;
        rest = unit;
    }
    Ok(std::time::Duration::from_secs_f64(total))
}

/// Walks up from the working directory to the repository root.
fn repo_root() -> Option<PathBuf> {
    let mut directory = std::env::current_dir().ok()?;
    loop {
        if directory.join("MODULE.bazel").is_file() {
            return Some(directory);
        }
        if !directory.pop() {
            return None;
        }
    }
}

fn temporary_state_dir() -> Result<String, Failure> {
    let index = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |duration| duration.as_nanos());
    let path = std::env::temp_dir().join(format!("hermes-rollout-{}-{index}", std::process::id()));
    std::fs::create_dir_all(&path).map_err(|error| Failure::new(error.to_string()))?;
    Ok(path.display().to_string())
}

fn run_oci(matches: &clap::ArgMatches, stdout: &mut dyn Write) -> Result<(), Failure> {
    let Some((name, sub)) = matches.subcommand() else {
        return Err(Failure::new(
            "usage: home-deploy oci digest|pack|docker LAYOUT --ref REF --out FILE",
        ));
    };
    if name == "digest" {
        let layout = sub.get_one::<String>("layout").map_or("", String::as_str);
        if layout.is_empty() {
            return Err(Failure::new("usage: home-deploy oci digest LAYOUT"));
        }
        let digest = oci::digest(layout).map_err(Failure::new)?;
        let _ = writeln!(stdout, "{digest}");
        return Ok(());
    }
    let layout = sub.get_one::<String>("layout").map_or("", String::as_str);
    let reference = sub.get_one::<String>("ref").map_or("", String::as_str);
    let output = sub.get_one::<String>("out").map_or("", String::as_str);
    if layout.is_empty() {
        return Err(Failure::new(format!(
            "oci {name}: layout argument is required"
        )));
    }
    if reference.is_empty() {
        return Err(Failure::new(format!("oci {name}: --ref is required")));
    }
    if output.is_empty() {
        return Err(Failure::new(format!("oci {name}: --out is required")));
    }
    let build = if name == "docker" {
        oci::docker_archive
    } else {
        oci::pack
    };
    let digest = build(layout, reference, output).map_err(Failure::new)?;
    let _ = writeln!(stdout, "{digest}");
    Ok(())
}

fn run_converge(matches: &clap::ArgMatches, stdout: &mut dyn Write) -> Result<(), Failure> {
    let kubectl = matches
        .get_one::<String>("kubectl")
        .map_or("kubectl", String::as_str);
    let namespace = matches
        .get_one::<String>("namespace")
        .map_or("home-assistant", String::as_str);
    let name = matches.get_one::<String>("name").map_or("", String::as_str);
    let file = matches.get_one::<String>("file").map_or("", String::as_str);
    let summary_file = matches
        .get_one::<String>("summary-file")
        .map_or("", String::as_str);
    if name.is_empty() {
        return Err(Failure::new("converge: --name is required"));
    }
    if file.is_empty() {
        return Err(Failure::new("converge: --file is required"));
    }
    let runner = SystemRunner::new(kubectl);
    let message = converge::converge(
        &converge::Options {
            namespace,
            name,
            file,
            preserve_replicas: matches.get_flag("preserve-replicas"),
            preserve_env: matches.get_flag("preserve-env"),
            summary_file,
        },
        &runner,
    )
    .map_err(Failure::new)?;
    let _ = writeln!(stdout, "{message}");
    Ok(())
}

fn run_state(matches: &clap::ArgMatches, stdout: &mut dyn Write) -> Result<(), Failure> {
    let Some((command, sub)) = matches.subcommand() else {
        return Err(Failure::new(
            "usage: home-deploy state record|mark|rollback ...",
        ));
    };
    let state_dir = sub
        .get_one::<String>("state-dir")
        .map_or("", String::as_str);
    match command {
        "record" => {
            let name = sub.get_one::<String>("name").map_or("", String::as_str);
            if state_dir.is_empty() || name.is_empty() {
                return Err(Failure::new(
                    "state record: --state-dir and --name are required",
                ));
            }
            let namespace = sub
                .get_one::<String>("namespace")
                .map_or("home-assistant", String::as_str);
            let kubectl = sub
                .get_one::<String>("kubectl")
                .map_or("kubectl", String::as_str);
            state::record(state_dir, namespace, name, &SystemRunner::new(kubectl))
                .map_err(Failure::new)?;
            Ok(())
        }
        "mark" => {
            let name = sub.get_one::<String>("name").map_or("", String::as_str);
            if state_dir.is_empty() || name.is_empty() {
                return Err(Failure::new(
                    "state mark: --state-dir and --name are required",
                ));
            }
            state::mark(state_dir, name).map_err(Failure::new)?;
            Ok(())
        }
        "rollback" => {
            if state_dir.is_empty() {
                return Err(Failure::new("state rollback: --state-dir is required"));
            }
            let namespace = sub
                .get_one::<String>("namespace")
                .map_or("home-assistant", String::as_str);
            let kubectl = sub
                .get_one::<String>("kubectl")
                .map_or("kubectl", String::as_str);
            for message in state::rollback(state_dir, namespace, &SystemRunner::new(kubectl)) {
                let _ = writeln!(stdout, "{message}");
            }
            Ok(())
        }
        other => Err(Failure::new(format!(
            "unknown state command {other:?} (expected record, mark or rollback)"
        ))),
    }
}

fn run_runtime(matches: &clap::ArgMatches, stdout: &mut dyn Write) -> Result<(), Failure> {
    let Some(("split", sub)) = matches.subcommand() else {
        return Err(Failure::new(
            "usage: home-deploy runtime split --render F --private F --storage-out F --values-out F",
        ));
    };
    let render_path = sub.get_one::<String>("render").map_or("", String::as_str);
    let private_path = sub.get_one::<String>("private").map_or("", String::as_str);
    let storage_out = sub
        .get_one::<String>("storage-out")
        .map_or("", String::as_str);
    let values_out = sub
        .get_one::<String>("values-out")
        .map_or("", String::as_str);
    let exporter_image = sub
        .get_one::<String>("health-exporter-image")
        .map_or("", String::as_str);
    let exporter_workload = sub
        .get_one::<String>("health-exporter-workload")
        .map_or("runtime-health-exporter", String::as_str);
    if render_path.is_empty()
        || private_path.is_empty()
        || storage_out.is_empty()
        || values_out.is_empty()
    {
        return Err(Failure::new(
            "runtime split: --render, --private, --storage-out and --values-out are required",
        ));
    }
    let storage = runtimevalues::split_render(render_path).map_err(Failure::new)?;
    runtimevalues::write(storage_out, &storage).map_err(Failure::new)?;
    let values = runtimevalues::workload_values(private_path).map_err(Failure::new)?;
    let values = runtimevalues::pin_health_exporter(values, exporter_image, exporter_workload)
        .map_err(Failure::new)?;
    runtimevalues::write(values_out, &runtimevalues::marshal(&values)).map_err(Failure::new)?;
    let _ = writeln!(stdout, "wrote {storage_out} and {values_out}");
    Ok(())
}

fn run_eso(matches: &clap::ArgMatches, stdout: &mut dyn Write) -> Result<(), Failure> {
    let Some(("names", sub)) = matches.subcommand() else {
        return Err(Failure::new(
            "usage: home-deploy eso names --out F MANIFEST...",
        ));
    };
    let out = sub.get_one::<String>("out").map_or("", String::as_str);
    let files: Vec<String> = sub
        .get_many::<String>("files")
        .map(|values| values.cloned().collect())
        .unwrap_or_default();
    if out.is_empty() {
        return Err(Failure::new("eso names: --out is required"));
    }
    if files.is_empty() {
        return Err(Failure::new("eso names: at least one manifest is required"));
    }
    let names = manifests::external_secret_names(&files).map_err(Failure::new)?;
    manifests::write_names(out, &names).map_err(Failure::new)?;
    let _ = writeln!(stdout, "{} ExternalSecret(s)", names.len());
    Ok(())
}

fn run_secret(matches: &clap::ArgMatches, stdout: &mut dyn Write) -> Result<(), Failure> {
    let Some(("opaque", sub)) = matches.subcommand() else {
        return Err(Failure::new(
            "usage: home-deploy secret opaque --name N --namespace NS --key K --from-env-file F --env KEY",
        ));
    };
    let name = sub.get_one::<String>("name").map_or("", String::as_str);
    let namespace = sub
        .get_one::<String>("namespace")
        .map_or("", String::as_str);
    let key = sub.get_one::<String>("key").map_or("", String::as_str);
    let from_env_file = sub
        .get_one::<String>("from-env-file")
        .map_or("", String::as_str);
    let env_key = sub.get_one::<String>("env").map_or("", String::as_str);
    if name.is_empty()
        || namespace.is_empty()
        || key.is_empty()
        || from_env_file.is_empty()
        || env_key.is_empty()
    {
        return Err(Failure::new(
            "secret opaque: --name, --namespace, --key, --from-env-file and --env are required",
        ));
    }
    let document = manifests::opaque_secret(name, namespace, key, from_env_file, env_key)
        .map_err(Failure::new)?;
    let text = String::from_utf8(document)
        .map_err(|_| Failure::new("secret opaque: generated document is not UTF-8"))?;
    let _ = writeln!(stdout, "{text}");
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
                    .arg(
                        string("health-exporter-workload").default_value("runtime-health-exporter"),
                    ),
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
    fn single_dash_flags_match_the_go_binary() {
        let (code, stdout, _) = execute(&["build-id", "-fingerprint", "hello"]);
        assert_eq!(code, 0);
        assert_eq!(stdout, "2cf24dba5fb0\n");
    }
}
