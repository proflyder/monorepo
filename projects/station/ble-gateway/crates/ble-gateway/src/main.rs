use std::process::ExitCode;

use ble_gateway::cli::{self, Parsed};
use ble_gateway::logf;
use ble_gateway::{Config, Gateway, Logger, Timings, Wpa};
use time::UtcOffset;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let config = match cli::parse(args) {
        Ok(Parsed::Config(config)) => config,
        Ok(Parsed::Help) => {
            print!("{}", cli::USAGE);
            return ExitCode::SUCCESS;
        }
        Err(err) => {
            eprintln!("{err}");
            eprint!("{}", cli::USAGE);
            return ExitCode::from(2);
        }
    };

    // `time` only permits `now_local()` while the process is single-threaded.
    // Resolve the offset before opening files or building the Tokio runtime.
    let offset = UtcOffset::current_local_offset().unwrap_or(UtcOffset::UTC);
    let logger = match &config.log {
        Some(path) => match Logger::file(path, offset) {
            Ok(logger) => logger,
            Err(err) => {
                eprintln!("log: {err}");
                return ExitCode::FAILURE;
            }
        },
        None => Logger::stderr(offset),
    };

    let runtime = match tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
    {
        Ok(runtime) => runtime,
        Err(err) => {
            logf!(logger, "runtime: {err}");
            return ExitCode::FAILURE;
        }
    };
    runtime.block_on(run(config, logger))
}

async fn run(config: Config, logger: Logger) -> ExitCode {
    let gateway = Gateway::new(config.clone(), logger, Wpa::system(), Timings::default());
    if let Err(err) = gateway.load_token() {
        logf!(gateway.logger, "token: {err}");
        return ExitCode::FAILURE;
    }
    if config.test_wpa {
        if let Err(message) = gateway.wpa.test_wpa_set().await {
            logf!(gateway.logger, "test-wpa: {message}");
            return ExitCode::FAILURE;
        }
        return ExitCode::SUCCESS;
    }

    logf!(
        gateway.logger,
        "station BLE gateway starting; token file {}; dry-run={} auto={}",
        config.token_file.display(),
        config.dry_run,
        config.auto_wifi
    );

    let connection = match zbus::Connection::system().await {
        Ok(connection) => connection,
        Err(err) => {
            logf!(gateway.logger, "system bus: {err}");
            return ExitCode::FAILURE;
        }
    };
    if let Err(err) = connection
        .request_name_with_flags(
            "com.station.gateway",
            zbus::fdo::RequestNameFlags::DoNotQueue.into(),
        )
        .await
    {
        logf!(gateway.logger, "RequestName: {err}");
    }
    match zbus::fdo::DBusProxy::new(&connection).await {
        Ok(dbus) => match dbus.list_names().await {
            Ok(names) => logf!(gateway.logger, "dbus names: {names:?}"),
            Err(err) => logf!(gateway.logger, "dbus names: {err}"),
        },
        Err(err) => logf!(gateway.logger, "dbus names: {err}"),
    }
    gateway.set_connection(connection);

    if !config.control_addr.is_empty()
        && let Err(err) = gateway
            .start_control(&config.control_addr, &config.control_token_file)
            .await
    {
        logf!(gateway.logger, "control HTTP disabled: {err}");
    }

    if !config.no_button {
        let button_gateway = gateway.clone();
        let button_path = config.button.clone();
        tokio::spawn(async move {
            button_gateway.run_button(&button_path).await;
        });
    }
    tokio::spawn(gateway.clone().auto_loop());

    // Idle until a phone is needed. 3 short presses or auto-no-Wi-Fi enables BLE.
    gateway.set_status("IDLE").await;
    if config.force
        && let Err(err) = gateway.enable_ble().await
    {
        logf!(gateway.logger, "force enable BLE: {err}");
    }

    let mut sigterm = match tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
    {
        Ok(signal) => signal,
        Err(err) => {
            logf!(gateway.logger, "signal: {err}");
            return ExitCode::FAILURE;
        }
    };
    let mut sigint = match tokio::signal::unix::signal(tokio::signal::unix::SignalKind::interrupt())
    {
        Ok(signal) => signal,
        Err(err) => {
            logf!(gateway.logger, "signal: {err}");
            return ExitCode::FAILURE;
        }
    };
    tokio::select! {
        _ = sigterm.recv() => {}
        _ = sigint.recv() => {}
    }

    logf!(gateway.logger, "shutdown signal; unregistering BLE");
    gateway.disable_ble().await;
    ExitCode::SUCCESS
}
