#![deny(unsafe_code)]
#![deny(unsafe_op_in_unsafe_fn)]
#![deny(clippy::undocumented_unsafe_blocks)]

mod input;
mod leds;

pub use input::{Button, InputError, InputEvent, InputHandle, Press, spawn_input};
pub use leds::{
    LedError, LedSink, LedsHandle, case_colors, effect_names, music_styles, spawn_leds,
};
pub use pulsar_ui::{CaseColor, Effect, MusicStyle, Rgb, VoiceColor};

use home_linux::io_error_text as errstr;
