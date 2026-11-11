//! Команды потокам фронтенда.

use std::path::PathBuf;

use tokio::sync::mpsc::UnboundedSender;

/// Команды потоку обработки: один канал, порядок FIFO. Writer wake-событий по-прежнему получает
/// `outcome` и метку ложного срабатывания своим каналом (см. [`FrontendHandle`]).
#[derive(Debug)]
pub(super) enum FrontendCmd {
    /// Пустой путь — не переключать.
    SetModel(Option<PathBuf>),
    SetDetection(bool),
    SetCustomWake(bool),
    SetCommands(bool),
    SetStreaming(bool),
    SetPreroll(bool),
    SetMusic(bool),
    SetRecording(bool),
    SetDryRun(bool),
    /// «Microphone gain», дБ: усиление звука запроса перед отправкой в HA.
    SetMicGain(u32),
    WakeHandled,
    Missed,
}

/// Команды потоку захвата: raw tap меняется на своём потоке, как и раньше.
pub(super) enum CaptureCmd {
    Tap(Option<UnboundedSender<Vec<i16>>>),
}
