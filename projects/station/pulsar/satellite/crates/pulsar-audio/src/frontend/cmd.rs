use std::path::PathBuf;

use tokio::sync::mpsc::UnboundedSender;

#[derive(Debug)]
pub(super) enum FrontendCmd {
    SetModel(Option<PathBuf>),
    SetDetection(bool),
    SetCustomWake(bool),
    SetCommands(bool),
    SetStreaming(bool),
    SetPreroll(bool),
    SetMusic(bool),
    SetRecording(bool),
    SetDryRun(bool),
    SetMicGain(u32),
    WakeHandled,
    Missed,
}

pub(super) enum CaptureCmd {
    Tap(Option<UnboundedSender<Vec<i16>>>),
}
