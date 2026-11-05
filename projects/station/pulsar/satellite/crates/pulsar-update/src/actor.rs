//! Актор обновлений: команды из handle, одна проверка или установка за раз, события в
//! контроллер. Задание — отдельная задача супервизора с собственным `CancellationToken`.

use std::cell::Cell;
use std::path::PathBuf;
use std::rc::Rc;

use ricardo_actor::Handle;
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender, unbounded_channel};
use tokio_util::sync::CancellationToken;

use crate::error::UpdateError;
use crate::installer::{Extra, Installer};
use crate::release::Release;
use crate::source::{OciConfig, ReleaseSource, Source};

/// Репозиторий обновлений: публичный OCI-пакет в GHCR, который публикует монорепозиторий.
pub const UPDATE_REPO: &str = "ghcr.io/proflyder/pulsar-firmware";

/// Идентификатор запроса `check()` / `install()` в событиях.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct RequestId(u64);

impl RequestId {
    #[must_use]
    pub fn get(self) -> u64 {
        self.0
    }
}

/// События актора.
#[derive(Debug)]
pub enum UpdateEvent {
    Checked {
        id: RequestId,
        result: Result<Release, UpdateError>,
    },
    /// Прогресс загрузки бинарника, 0..95 (остальное — подмена).
    Progress { id: RequestId, value: f32 },
    /// Итог установки; `fatal` смотрится через `UpdateError::fatal`.
    Installed {
        id: RequestId,
        result: Result<(), UpdateError>,
    },
}

/// Неблокирующая сторона актора (все методы `&self`).
#[derive(Clone)]
pub struct UpdaterHandle {
    commands: Handle<Cmd>,
    /// Единственный экземпляр: ставится здесь при приёме запроса, снимается заданием
    /// непосредственно перед итоговым событием.
    busy: Rc<Cell<bool>>,
    next_id: Rc<Cell<u64>>,
}

enum Cmd {
    Check {
        id: RequestId,
    },
    Install {
        id: RequestId,
        release: Release,
        binary: PathBuf,
    },
    AddExtra(Extra),
}

impl UpdaterHandle {
    /// Синхронно: `check()` / `install()` ставят флаг сразу.
    #[must_use]
    pub fn busy(&self) -> bool {
        self.busy.get()
    }

    /// Последний стабильный релиз. `None` — занят, запрос не принят, события не будет.
    pub fn check(&self) -> Option<RequestId> {
        self.start(|id| Cmd::Check { id })
    }

    /// Заменяет `binary` релизом `release`. `None` — занят.
    pub fn install(&self, release: Release, binary: PathBuf) -> Option<RequestId> {
        self.start(|id| Cmd::Install {
            id,
            release,
            binary,
        })
    }

    /// Другой ассет релиза, устанавливаемый вместе с бинарником (образы радио): `name` из
    /// того же релиза, проверенный по `SHA256SUMS` и записанный в `path`. Действует со
    /// следующего `install()`.
    pub fn add_extra_asset(&self, name: &str, path: PathBuf) {
        let _ = self.commands.send(Cmd::AddExtra(Extra {
            name: name.into(),
            path,
        }));
    }

    fn start(&self, command: impl FnOnce(RequestId) -> Cmd) -> Option<RequestId> {
        if self.busy.replace(true) {
            return None;
        }
        let id = RequestId(self.next_id.get());
        if !self.commands.send(command(id)) {
            self.busy.set(false);
            return None;
        }
        self.next_id.set(id.0 + 1);
        Some(id)
    }
}

/// Суффикс ассета этой сборки.
#[must_use]
pub fn arch() -> &'static str {
    if cfg!(target_arch = "aarch64") {
        "aarch64"
    } else if cfg!(target_arch = "arm") {
        "armhf"
    } else {
        "host"
    }
}

/// `repo`: `ghcr.io/owner/name`. Вызывать внутри LocalSet контроллера.
pub fn spawn(
    repo: String,
    current_version: &str,
) -> (UpdaterHandle, UnboundedReceiver<UpdateEvent>) {
    spawn_with(&OciConfig::ghcr(repo, current_version, arch()))
}

fn spawn_with(config: &OciConfig) -> (UpdaterHandle, UnboundedReceiver<UpdateEvent>) {
    let source = Source::new(config);
    spawn_source(source, config.arch)
}

/// `arch` — суффикс ассета сборки; в тестах `"host"` с фейковым источником.
fn spawn_source<S: ReleaseSource + 'static>(
    source: S,
    arch: &'static str,
) -> (UpdaterHandle, UnboundedReceiver<UpdateEvent>) {
    let installer = Installer::new(arch);
    let (commands, commands_rx) = ricardo_actor::channel();
    let (events, events_rx) = unbounded_channel();
    let busy = Rc::new(Cell::new(false));
    pulsar_core::supervisor().spawn_local(
        "updater",
        run(source, installer, commands_rx, events, busy.clone()),
    );
    (
        UpdaterHandle {
            commands,
            busy,
            next_id: Rc::new(Cell::new(1)),
        },
        events_rx,
    )
}

async fn run<S: ReleaseSource + 'static>(
    source: S,
    installer: Installer,
    mut commands: UnboundedReceiver<Cmd>,
    events: UnboundedSender<UpdateEvent>,
    busy: Rc<Cell<bool>>,
) {
    let source = Rc::new(source);
    let installer = Rc::new(installer);
    // Копия в каждую установку: поздний add_extra_asset() не меняет идущую.
    let mut extras: Vec<Extra> = Vec::new();
    // Не больше одного задания: handle не принимает запрос, пока busy.
    let mut active: Option<CancellationToken> = None;
    while let Some(command) = commands.recv().await {
        match command {
            Cmd::AddExtra(extra) => extras.push(extra),
            Cmd::Check { id } => {
                active = Some(spawn_check(
                    source.clone(),
                    events.clone(),
                    busy.clone(),
                    id,
                ));
            }
            Cmd::Install {
                id,
                release,
                binary,
            } => {
                active = Some(spawn_install(
                    source.clone(),
                    installer.clone(),
                    events.clone(),
                    busy.clone(),
                    id,
                    release,
                    binary,
                    extras.clone(),
                ));
            }
        }
    }
    // Все handle закрыты (выход демона): задание останавливается, событие не отправляется.
    if let Some(token) = active {
        token.cancel();
    }
}

fn job_token() -> CancellationToken {
    pulsar_core::supervisor().token().child_token()
}

fn spawn_check<S: ReleaseSource + 'static>(
    source: Rc<S>,
    events: UnboundedSender<UpdateEvent>,
    busy: Rc<Cell<bool>>,
    id: RequestId,
) -> CancellationToken {
    let token = job_token();
    let cancel = token.clone();
    pulsar_core::supervisor().spawn_local("updater-check", async move {
        let result = source.latest(&cancel).await;
        finish(
            &cancel,
            || busy.set(false),
            || {
                let _ = events.send(UpdateEvent::Checked { id, result });
            },
        );
    });
    token
}

#[allow(clippy::too_many_arguments)]
fn spawn_install<S: ReleaseSource + 'static>(
    source: Rc<S>,
    installer: Rc<Installer>,
    events: UnboundedSender<UpdateEvent>,
    busy: Rc<Cell<bool>>,
    id: RequestId,
    release: Release,
    binary: PathBuf,
    extras: Vec<Extra>,
) -> CancellationToken {
    let token = job_token();
    let cancel = token.clone();
    pulsar_core::supervisor().spawn_local("updater-install", async move {
        let mut last = None;
        let mut progress = |got: usize, total: usize| {
            let percent = if total > 0 {
                got as u64 * 95 / total as u64
            } else {
                0
            };
            if last != Some(percent) {
                last = Some(percent);
                let _ = events.send(UpdateEvent::Progress {
                    id,
                    value: percent as f32,
                });
            }
        };
        let result = installer
            .install(&*source, &release, &binary, &extras, &mut progress, &cancel)
            .await;
        drop(progress);
        finish(
            &cancel,
            || busy.set(false),
            || {
                let _ = events.send(UpdateEvent::Installed { id, result });
            },
        );
    });
    token
}

/// Снимает busy и шлёт итоговое событие, если задание не отменено выходом демона.
fn finish(cancel: &CancellationToken, reset_busy: impl FnOnce(), send: impl FnOnce()) {
    if cancel.is_cancelled() {
        return;
    }
    reset_busy();
    send();
}

#[cfg(test)]
mod tests;
