#![deny(unsafe_code)]
#![deny(unsafe_op_in_unsafe_fn)]
#![deny(clippy::undocumented_unsafe_blocks)]

mod controller;
#[allow(unsafe_code)]
mod crash;
mod settings;
#[allow(unsafe_code)]
mod sys;
mod version;

use std::ffi::OsStr;
use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::time::Duration;

use tracing::{error, info};

use controller::{Controller, prepare_process};
use version::VERSION;

const DEFAULT_STATE: &str = "/data/pulsar/state.conf";

const TEARDOWN_SECS: u64 = 5;

fn arm_teardown_watchdog(status: i32) {
    let spawned = std::thread::Builder::new()
        .name("teardown-wd".into())
        .spawn(move || {
            std::thread::sleep(Duration::from_secs(TEARDOWN_SECS));
            sys::write_stderr(b"teardown did not finish in 5 s, _exit\n");
            sys::exit_now(status);
        });
    if let Err(e) = spawned {
        error!("teardown watchdog: {e}");
    }
}

fn main() -> ExitCode {
    let arg = std::env::args_os().nth(1);
    if arg.as_deref() == Some(OsStr::new("--version")) {
        let _ = writeln!(std::io::stdout(), "pulsar {VERSION}");
        return ExitCode::SUCCESS;
    }
    let state = arg.map_or_else(|| PathBuf::from(DEFAULT_STATE), PathBuf::from);

    pulsar_core::init_logger();
    crash::install();
    let Some(model) = pulsar_model::detect_profile() else {
        return ExitCode::from(1);
    };
    prepare_process(model);

    let rt = match tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .max_blocking_threads(4)
        .build()
    {
        Ok(rt) => rt,
        Err(e) => {
            error!("tokio runtime: {e}");
            return ExitCode::from(1);
        }
    };
    let local = tokio::task::LocalSet::new();
    let status = local.block_on(&rt, async {
        let restart_marker = state
            .parent()
            .unwrap_or_else(|| Path::new("."))
            .join("enabled");
        let mut ctl = match Controller::start(model, state) {
            Ok(c) => c,
            Err(e) => {
                error!("{e}");
                arm_teardown_watchdog(1);
                return 1;
            }
        };
        info!(
            "pulsar {VERSION} running on {} ({})",
            model.identity.model_name, model.identity.codename
        );
        ctl.run().await;
        if supervisor_restarts(&restart_marker) {
            ctl.promise_sendspin_restart();
        }
        arm_teardown_watchdog(0);
        ctl.flush_settings();
        drop(ctl);
        0
    });

    let stuck = local.block_on(
        &rt,
        pulsar_core::supervisor().shutdown(Duration::from_secs(3)),
    );
    for name in stuck {
        tracing::warn!("actor {name} did not stop");
    }
    {
        let _rt = rt.enter();
        drop(local);
    }
    rt.shutdown_timeout(Duration::from_secs(1));
    ExitCode::from(status)
}

fn supervisor_restarts(enabled: &Path) -> bool {
    enabled.is_file()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn supervisor_restart_follows_the_enabled_file_next_to_the_state() {
        let dir = std::env::temp_dir().join(format!("pulsar-restart-test-{}", std::process::id()));
        let state = dir.join("state.conf");
        let marker = dir.join("enabled");
        std::fs::create_dir_all(&dir).unwrap();
        assert!(!supervisor_restarts(&marker));
        std::fs::write(&marker, b"").unwrap();
        assert!(supervisor_restarts(&marker));
        std::fs::remove_file(&marker).unwrap();
        assert!(!supervisor_restarts(&marker));
        assert_eq!(
            state.parent().unwrap().join("enabled"),
            marker,
            "marker next to state.conf"
        );
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
