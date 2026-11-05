//! Пробник обновлений: проверяет OCI-источник и апдейтер без демона.
//! usage: update_probe versions
//!          таблица release_newer() для нескольких describe-строк (без сети)
//!        update_probe check [owner/repo | ghcr.io/owner/name]
//!          последний релиз и его слои так, как их видит демон
//!        update_probe download <title> [repo]
//!          скачивает слой последнего релиза и проверяет его digest
//!        update_probe install --binary <path> [repo]
//!          проверяет последний релиз и ставит его в <path> (как демон)

#![deny(unsafe_code)]
#![deny(unsafe_op_in_unsafe_fn)]

use std::io::Write as _;
use std::path::PathBuf;
use std::process::ExitCode;

use clap::{Parser, Subcommand};
use pulsar_update::{Oci, OciConfig, Release, ReleaseSource, UpdateError, UpdateEvent, arch};
use tokio_util::sync::CancellationToken;

#[derive(Parser)]
#[command(about = "pulsar updater probe")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Таблица сравнения версий (без сети).
    Versions,
    /// Последний релиз и его слои.
    Check {
        #[arg(default_value = pulsar_update::UPDATE_REPO)]
        repo: String,
    },
    /// Скачать слой последнего релиза и проверить digest.
    Download {
        title: String,
        #[arg(default_value = pulsar_update::UPDATE_REPO)]
        repo: String,
    },
    /// Проверить и установить последний релиз в бинарь.
    Install {
        #[arg(long)]
        binary: PathBuf,
        #[arg(default_value = pulsar_update::UPDATE_REPO)]
        repo: String,
    },
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    match cli.command {
        Command::Versions => versions(),
        Command::Check { repo } => run(check(&repo)),
        Command::Download { title, repo } => run(download(&repo, &title)),
        Command::Install { binary, repo } => run(install(&repo, binary)),
    }
}

/// Строки в stdout; закрытый pipe не повод для паники.
fn out(bytes: &[u8]) {
    let _ = std::io::stdout().write_all(bytes);
}

fn versions() -> ExitCode {
    const CASES: [(&str, &str); 8] = [
        ("v1.2.4", "v1.2.3"),
        ("v1.2.3", "v1.2.3"),
        ("v1.2.3", "v1.2.3-4-gabc1234"),
        ("v1.3.0", "v1.2.3-4-gabc"),
        ("v1.2.3", "v1.10.0"),
        ("v1.2.3", "6ac30be"),
        ("v2.0.0", "v1.9.9-dirty"),
        ("junk", "v1.0.0"),
    ];
    for (latest, current) in CASES {
        let (newer, known) = pulsar_update::release_newer(latest, current);
        out(format!(
            "latest {latest:<8} current {current:<20} newer={} current_known={}\n",
            u8::from(newer),
            u8::from(known)
        )
        .as_bytes());
    }
    ExitCode::SUCCESS
}

/// Апдейтер — актор current-thread рантайма в LocalSet, как в демоне.
fn run(future: impl Future<Output = ExitCode>) -> ExitCode {
    let runtime = match tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
    {
        Ok(runtime) => runtime,
        Err(e) => {
            eprintln!("tokio runtime: {e}");
            return ExitCode::FAILURE;
        }
    };
    tokio::task::LocalSet::new().block_on(&runtime, future)
}

fn sha256_hex(data: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let digest = ring::digest::digest(&ring::digest::SHA256, data);
    let mut out = String::with_capacity(64);
    for byte in digest.as_ref() {
        out.push(char::from(HEX[usize::from(byte >> 4)]));
        out.push(char::from(HEX[usize::from(byte & 15)]));
    }
    out
}

fn source(repo: &str) -> Result<Oci, UpdateError> {
    Oci::new(&OciConfig::ghcr(repo.into(), "probe", arch()))
}

async fn latest(repo: &str) -> Result<Release, UpdateError> {
    source(repo)?.latest(&CancellationToken::new()).await
}

async fn check(repo: &str) -> ExitCode {
    match latest(repo).await {
        Ok(release) => {
            out(format!(
                "version {}\nurl {}\n",
                release.version(),
                release.page_url()
            )
            .as_bytes());
            for layer in release.layers() {
                out(format!("layer {} {}\n", layer.title, layer.digest()).as_bytes());
            }
            ExitCode::SUCCESS
        }
        Err(error) => {
            out(format!("error: {error}\n").as_bytes());
            ExitCode::FAILURE
        }
    }
}

async fn download(repo: &str, title: &str) -> ExitCode {
    let release = match latest(repo).await {
        Ok(release) => release,
        Err(error) => {
            out(format!("error: {error}\n").as_bytes());
            return ExitCode::FAILURE;
        }
    };
    let Some(layer) = release.layers().iter().find(|layer| layer.title == title) else {
        out(format!("error: release has no {title} layer\n").as_bytes());
        return ExitCode::FAILURE;
    };
    let source = match source(repo) {
        Ok(source) => source,
        Err(error) => {
            out(format!("error: {error}\n").as_bytes());
            return ExitCode::FAILURE;
        }
    };
    let body = match source
        .download(layer, 64 << 20, None, &CancellationToken::new())
        .await
    {
        Ok(Some(body)) => body,
        Ok(None) => {
            out(format!("error: {title}: HTTP 404\n").as_bytes());
            return ExitCode::FAILURE;
        }
        Err(error) => {
            out(format!("error: {error}\n").as_bytes());
            return ExitCode::FAILURE;
        }
    };
    let hex = sha256_hex(&body);
    let expected = layer.digest().to_string();
    out(format!(
        "layer {title}\nbytes {}\nsha256 {expected}\nmatches {}\n",
        body.len(),
        u8::from(expected == format!("sha256:{hex}"))
    )
    .as_bytes());
    if expected == format!("sha256:{hex}") {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    }
}

async fn install(repo: &str, binary: PathBuf) -> ExitCode {
    let (updater, mut events) = pulsar_update::spawn(repo.into(), "probe");
    let Some(check_id) = updater.check() else {
        out(b"error: updater is busy\n");
        return ExitCode::FAILURE;
    };
    let mut release = None;
    while let Some(event) = events.recv().await {
        match event {
            UpdateEvent::Checked { id, result } if id == check_id => match result {
                Ok(found) => {
                    release = Some(found);
                    break;
                }
                Err(error) => {
                    out(format!("error: {error}\n").as_bytes());
                    return ExitCode::FAILURE;
                }
            },
            _ => {}
        }
    }
    let Some(release) = release else {
        return ExitCode::FAILURE;
    };
    out(format!("version {}\n", release.version()).as_bytes());
    let Some(_install_id) = updater.install(release, binary) else {
        out(b"error: updater is busy\n");
        return ExitCode::FAILURE;
    };
    while let Some(event) = events.recv().await {
        match event {
            UpdateEvent::Progress { value, .. } => {
                out(format!("progress {value:.0}\n").as_bytes());
            }
            UpdateEvent::Installed { result, .. } => {
                return match result {
                    Ok(()) => {
                        out(b"installed\n");
                        ExitCode::SUCCESS
                    }
                    Err(error) => {
                        print_error(&error);
                        ExitCode::FAILURE
                    }
                };
            }
            UpdateEvent::Checked { .. } => {}
        }
    }
    ExitCode::FAILURE
}

fn print_error(error: &UpdateError) {
    out(format!("error (fatal={}): {error}\n", u8::from(error.fatal())).as_bytes());
}
