#![deny(unsafe_code)]
#![deny(unsafe_op_in_unsafe_fn)]
#![deny(clippy::undocumented_unsafe_blocks)]

pub mod clock;
pub mod clock_fx;
pub mod font;
pub mod led_fx;
pub mod leds;
mod music;
mod types;

pub use clock::{ClockLayers, Wall};
pub use clock_fx::Frame;
pub use led_fx::{Col, Voice, VoiceColor};
pub use leds::{Compositor, Layer, LedCommand, Matrix, Rgb};
pub use music::{MusicFrames, MusicSource};
pub use types::{CaseColor, ClockFace, ClockMusicStyle, Effect, MusicStyle};
