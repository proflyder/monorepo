//! Hardware UI of the supported Yandex Stations (layout per `Model`): the LED ring (+ Midi logo)
//! and the touch / mute buttons. All via framebuffers, sysfs and evdev; no Yandex code.
#![deny(unsafe_code)]
#![deny(unsafe_op_in_unsafe_fn)]
#![deny(clippy::undocumented_unsafe_blocks)]

mod input;
mod leds;
#[allow(unsafe_code)]
mod sys;

pub use input::{Button, InputEvent, InputHandle, Press, spawn_input};
pub use leds::{LedsHandle, case_colors, effect_names, music_styles, spawn_leds};
pub use pulsar_ui::{CaseColor, Effect, MusicStyle, Rgb, VoiceColor};

use home_linux::io_error_text as errstr;
