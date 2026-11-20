#![deny(unsafe_code)]
#![deny(unsafe_op_in_unsafe_fn)]
#![deny(clippy::undocumented_unsafe_blocks)]

pub mod engine;
mod frontend;
#[allow(unsafe_code)]
mod ricardo;
pub mod wake_events;

pub use frontend::{CaptureError, FrontendEvent, FrontendHandle, FrontendStats, spawn};
pub use pulsar_dsp::{LevelDb, SpectrumAnalyzer, endpoint};
pub use ricardo::RicardoWake;
pub use wake_events::{Outcome, WakeEventOptions, WakeEventsHandle};
