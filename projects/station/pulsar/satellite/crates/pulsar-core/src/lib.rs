#![deny(unsafe_code)]
#![deny(unsafe_op_in_unsafe_fn)]
#![deny(clippy::undocumented_unsafe_blocks)]

mod ambient;
#[allow(unsafe_code)]
mod compat;
pub mod fmt;
mod logger;
pub mod net;
pub mod runtime;
#[allow(unsafe_code)]
mod sys;
mod timers;
pub mod tz;
pub mod volume;

pub use ambient::{AmbientDimmer, DARK, perceptual};
pub use logger::{
    LogLine, add_own_target, init_logger, set_log_sink, stderr_level, take_log_lines,
};
pub use runtime::supervisor;
pub use timers::Timers;

pub fn now_ms() -> i64 {
    sys::monotonic_ms()
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct MusicFrame {
    pub bands: [f32; 8],
    pub loudness: f32,
    pub beat: bool,
}

pub const SENDSPIN_FRESH_MS: i64 = 500;
pub const MUSIC_STALE_MS: i64 = 600;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UiState {
    Idle,
    Listening,
    Thinking,
    Speaking,
    Error,
    TimerRing,
    Call,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ClockShot {
    Boot,
    Pause,
    Next,
    Previous,
}
