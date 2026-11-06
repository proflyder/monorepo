//! Установщик: скачивает и проверяет ассеты, пробует новый бинарь и атомарно подменяет
//! файлы. Файловая система и запуск `--version` — здесь, сеть — за `ReleaseSource`.

use std::fs::{self, File, OpenOptions, Permissions};
use std::io::{self, Write as _};
use std::os::unix::fs::{OpenOptionsExt as _, PermissionsExt as _};
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::Duration;

use home_linux::io_error_text as os_text;
use tokio::io::AsyncReadExt as _;
use tokio::process::Command;
use tokio::time::Instant;
use tokio_util::sync::CancellationToken;

use crate::error::UpdateError;
use crate::http::Progress;
use crate::release::{Digest, Release};
use crate::source::ReleaseSource;

const MAX_BINARY: usize = 64 << 20;
const MAX_SMALL: usize = 1 << 20;
const VERSION_TIMEOUT: Duration = Duration::from_secs(10);

/// Другой ассет релиза, устанавливаемый вместе с бинарником (образы радио): `name` в
/// релизе, `path` на станции.
#[derive(Clone, Debug)]
pub struct Extra {
    pub name: String,
    pub path: PathBuf,
}

/// Установщик одной сборки: суффикс ассета задан при сборке, не `cfg!(test)`.
pub struct Installer {
    arch: &'static str,
}

impl Installer {
    pub(crate) const fn new(arch: &'static str) -> Self {
        Self { arch }
    }

    /// Скачивает, проверяет и подменяет `binary`, сохраняя текущий как `.prev`.
    ///
    /// `progress` — по байтам бинарника (0..95 отдаёт актор).
    pub async fn install<S: ReleaseSource>(
        &self,
        source: &S,
        release: &Release,
        binary: &Path,
        extras: &[Extra],
        progress: Progress<'_>,
        cancel: &CancellationToken,
    ) -> Result<(), UpdateError> {
        let asset = format!("pulsar-{}.bin", self.arch);
        let dir = binary_dir(binary);
        let staged = dir.join("pulsar.new");
        let bin_layer = release
            .layer(&asset)
            .ok_or_else(|| UpdateError::fatal_text(format!("manifest has no {asset} layer")))?;
        let sums_layer = release
            .layer("SHA256SUMS")
            .ok_or_else(|| UpdateError::fatal_text("manifest has no SHA256SUMS layer"))?;

        let Some(bin) = source
            .download(bin_layer, MAX_BINARY, Some(progress), cancel)
            .await?
        else {
            return Err(UpdateError::fatal_text(format!("{asset}: HTTP 404")));
        };
        let Some(sums) = source.download(sums_layer, MAX_SMALL, None, cancel).await? else {
            return Err(UpdateError::fatal_text("SHA256SUMS: HTTP 404"));
        };
        if Digest::of(&sums) != sums_layer.digest {
            return Err(UpdateError::fatal_text("SHA256SUMS: blob digest mismatch"));
        }
        let (body, digest) = hash(bin)
            .await
            .map_err(|e| UpdateError::retry(format!("{asset}: {}", os_text(&e))))?;
        if digest != bin_layer.digest {
            return Err(UpdateError::fatal_text(format!(
                "{asset}: blob digest mismatch"
            )));
        }
        match expected_sha(&sums, &asset) {
            None => {
                return Err(UpdateError::fatal_text(format!(
                    "SHA256SUMS lists no {asset}"
                )));
            }
            Some(want) if !digest.matches_hex(want) => {
                return Err(UpdateError::fatal_text(format!(
                    "{asset}: checksum mismatch"
                )));
            }
            Some(_) => {}
        }
        // Выход демона не начинает новых шагов; уже идущая подмена завершается.
        if cancel.is_cancelled() {
            return Err(UpdateError::Cancelled);
        }
        if let Err(e) = write_file(staged.clone(), body, 0o755).await {
            return Err(UpdateError::retry(format!(
                "cannot write {}: {}",
                staged.display(),
                os_text(&e)
            )));
        }
        // Отмена во время пробы убивает `--version` (kill_on_drop) и убирает pulsar.new.
        let says = tokio::select! {
            biased;
            () = cancel.cancelled() => None,
            says = run_version(&staged) => Some(says),
        };
        let Some(says) = says else {
            let _ = tokio::fs::remove_file(&staged).await;
            return Err(UpdateError::Cancelled);
        };
        if says != format!("pulsar {}", release.version()) {
            let _ = tokio::fs::remove_file(&staged).await;
            let reports = if says.is_empty() {
                String::new()
            } else {
                format!(" (reports '{says}')")
            };
            return Err(UpdateError::fatal_text(format!(
                "{asset} does not run here as {}{reports}",
                release.version()
            )));
        }

        // Дополнительные ассеты скачиваются и проверяются первыми и подменяются только
        // вместе с бинарником: новый образ радио рядом со старым демоном был бы записан
        // по старому протоколу.
        let mut extras_staged: Vec<Option<PathBuf>> = Vec::with_capacity(extras.len());
        for extra in extras {
            match self
                .stage_extra(source, release, extra, &sums, cancel)
                .await
            {
                Ok(new) => extras_staged.push(new),
                Err(failed) => {
                    let staged = staged.clone();
                    let _ = blocking(move || {
                        drop_staged(&staged, &extras_staged);
                        Ok(())
                    })
                    .await;
                    return Err(failed);
                }
            }
        }
        let cancel_set = cancel.is_cancelled();
        let binary = binary.to_path_buf();
        let targets: Vec<PathBuf> = extras.iter().map(|extra| extra.path.clone()).collect();
        blocking(move || {
            if cancel_set {
                drop_staged(&staged, &extras_staged);
                return Ok(Err(UpdateError::Cancelled));
            }
            Ok(swap(&binary, &dir, &staged, &targets, extras_staged))
        })
        .await
        .unwrap_or_else(|e| Err(UpdateError::retry(os_text(&e))))
    }

    /// Скачивает, проверяет и пишет `<path>.new` одного ассета; `None`, если в релизе его нет.
    async fn stage_extra<S: ReleaseSource>(
        &self,
        source: &S,
        release: &Release,
        extra: &Extra,
        sums: &[u8],
        cancel: &CancellationToken,
    ) -> Result<Option<PathBuf>, UpdateError> {
        let Some(layer) = release.layer(&extra.name) else {
            return Ok(None);
        };
        let Some(fetched) = source.download(layer, MAX_BINARY, None, cancel).await? else {
            return Ok(None); // релиз без этого ассета
        };
        let body = hash(fetched)
            .await
            .map_err(|e| UpdateError::retry(format!("{}: {}", extra.name, os_text(&e))))?;
        let digest = body.1;
        let body = body.0;
        if digest != layer.digest {
            return Err(UpdateError::fatal_text(format!(
                "{}: blob digest mismatch",
                extra.name
            )));
        }
        match expected_sha(sums, &extra.name) {
            None => {
                return Err(UpdateError::fatal_text(format!(
                    "SHA256SUMS lists no {}",
                    extra.name
                )));
            }
            Some(want) if !digest.matches_hex(want) => {
                return Err(UpdateError::fatal_text(format!(
                    "{}: checksum mismatch",
                    extra.name
                )));
            }
            Some(_) => {}
        }
        let new = with_suffix(&extra.path, ".new");
        match write_file(new.clone(), body, 0o644).await {
            Ok(()) => Ok(Some(new)),
            Err(e) => Err(UpdateError::retry(format!(
                "cannot write {}: {}",
                new.display(),
                os_text(&e)
            ))),
        }
    }
}

/// Каталог бинарника, куда пишутся pulsar.new и pulsar.prev. Голое имя файла даёт "."
/// (иначе `pulsar.new` искался бы в PATH запуском `--version`).
fn binary_dir(binary: &Path) -> PathBuf {
    match binary.parent() {
        Some(parent) if !parent.as_os_str().is_empty() => parent.to_path_buf(),
        _ => PathBuf::from("."),
    }
}

/// Hex digest для `name` в списке sha256sum (`<hex>  <name>` или `<hex> *<name>`).
fn expected_sha<'a>(sums: &'a [u8], name: &str) -> Option<&'a str> {
    for line in sums.split(|&b| b == b'\n') {
        let line = line.strip_suffix(b"\r").unwrap_or(line);
        if line.iter().position(|&b| b == b' ') != Some(64) {
            continue;
        }
        let Some(name_at) = line[64..].iter().position(|&b| b != b' ') else {
            continue;
        };
        let file = &line[64 + name_at..];
        let file = file.strip_prefix(b"*").unwrap_or(file);
        if file == name.as_bytes() {
            return std::str::from_utf8(&line[..64]).ok();
        }
    }
    None
}

/// Сохраняет текущий бинарь как .prev, затем переименовывает дополнительные ассеты и бинарь.
fn swap(
    binary: &Path,
    dir: &Path,
    staged: &Path,
    targets: &[PathBuf],
    mut extras_staged: Vec<Option<PathBuf>>,
) -> Result<(), UpdateError> {
    // supervise.sh вернёт .prev, если новый бинарь уйдёт в crash-loop.
    let tmp = dir.join("pulsar.prev.tmp");
    if let Err(e) =
        copy_file(binary, &tmp, 0o755).and_then(|()| fs::rename(&tmp, dir.join("pulsar.prev")))
    {
        // /data на Лайт 2 мал: недописанная копия не остаётся.
        let _ = fs::remove_file(&tmp);
        drop_staged(staged, &extras_staged);
        return Err(UpdateError::retry(format!(
            "cannot keep the current binary as .prev: {}",
            os_text(&e)
        )));
    }
    for (i, target) in targets.iter().enumerate() {
        let Some(new) = &extras_staged[i] else {
            continue;
        };
        if let Err(e) = fs::rename(new, target) {
            drop_staged(staged, &extras_staged);
            return Err(UpdateError::retry(format!(
                "cannot replace {}: {}",
                target.display(),
                os_text(&e)
            )));
        }
        extras_staged[i] = None;
    }
    if let Err(e) = fs::rename(staged, binary) {
        let _ = fs::remove_file(staged);
        return Err(UpdateError::retry(format!(
            "cannot replace {}: {}",
            binary.display(),
            os_text(&e)
        )));
    }
    crate::sync();
    Ok(())
}

fn drop_staged(staged: &Path, extras_staged: &[Option<PathBuf>]) {
    let _ = fs::remove_file(staged);
    for staged in extras_staged.iter().flatten() {
        let _ = fs::remove_file(staged);
    }
}

fn with_suffix(path: &Path, suffix: &str) -> PathBuf {
    let mut name = path.as_os_str().to_owned();
    name.push(suffix);
    name.into()
}

/// Файловая работа вне потока рантайма.
async fn blocking<T: Send + 'static>(
    f: impl FnOnce() -> io::Result<T> + Send + 'static,
) -> io::Result<T> {
    tokio::task::spawn_blocking(f)
        .await
        .unwrap_or_else(|e| Err(io::Error::other(e)))
}

/// SHA-256 скачанного вне потока рантайма (десятки мс для бинаря на Лайт 2).
async fn hash(body: Vec<u8>) -> io::Result<(Vec<u8>, Digest)> {
    blocking(move || {
        let digest = Digest::of(&body);
        Ok((body, digest))
    })
    .await
}

/// Записывает `data` с правами `mode` (в том числе под umask), fsync; неудачная запись
/// не оставляет файла.
async fn write_file(path: PathBuf, data: Vec<u8>, mode: u32) -> io::Result<()> {
    blocking(move || {
        let written = open_for_write(&path, mode).and_then(|mut f| {
            f.write_all(&data)?;
            f.sync_all()?;
            f.set_permissions(Permissions::from_mode(mode))
        });
        if written.is_err() {
            let _ = fs::remove_file(&path);
        }
        written
    })
    .await
}

fn open_for_write(path: &Path, mode: u32) -> io::Result<File> {
    OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .mode(mode)
        .open(path)
}

/// Копирует `src` в `dst` с правами `mode`, fsync (в ядре: копии бинаря в памяти нет).
fn copy_file(src: &Path, dst: &Path, mode: u32) -> io::Result<()> {
    let mut from = File::open(src)?;
    let mut to = open_for_write(dst, mode)?;
    io::copy(&mut from, &mut to)?;
    to.sync_all()?;
    to.set_permissions(Permissions::from_mode(mode))
}

/// Первая строка `<path> --version`; "" — не завершился с 0 за таймаут.
async fn run_version(path: &Path) -> String {
    let deadline = Instant::now() + VERSION_TIMEOUT;
    let mut child = loop {
        let spawned = Command::new(path)
            .arg("--version")
            .stdout(Stdio::piped())
            .kill_on_drop(true)
            .spawn();
        match spawned {
            Ok(child) => break child,
            // Только что записанный файл может быть ещё открыт на запись в дочернем
            // процессе другого потока (до его exec): короткая повторная попытка.
            Err(e) if e.raw_os_error() == Some(libc::ETXTBSY) && Instant::now() < deadline => {
                tokio::time::sleep(Duration::from_millis(20)).await;
            }
            Err(_) => return String::new(),
        }
    };
    let Some(mut stdout) = child.stdout.take() else {
        return String::new();
    };
    let mut out = Vec::new();
    let read = async {
        let mut buf = [0u8; 256];
        while out.len() < 1024 {
            match stdout.read(&mut buf).await {
                Ok(0) | Err(_) => break,
                Ok(n) => out.extend_from_slice(&buf[..n]),
            }
        }
    };
    let _ = tokio::time::timeout_at(deadline, read).await;
    let ok = if let Ok(Ok(status)) = tokio::time::timeout_at(deadline, child.wait()).await {
        status.success()
    } else {
        let _ = child.kill().await;
        false
    };
    drop(stdout);
    if !ok {
        return String::new();
    }
    let line = out.split(|&b| b == b'\n').next().unwrap_or_default();
    String::from_utf8_lossy(line).into_owned()
}

#[cfg(test)]
mod tests;
