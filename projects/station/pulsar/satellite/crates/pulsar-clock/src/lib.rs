#![deny(unsafe_code)]
#![deny(unsafe_op_in_unsafe_fn)]
#![deny(clippy::undocumented_unsafe_blocks)]

mod clock;
mod light_sensor;

pub use clock::{
    ClockError, ClockHandle, ClockPanel, Lux, WallClock, face_names, level, music_styles,
    panel_present, spawn,
};
pub use light_sensor::{LightSensor, spawn_light_sensor};
pub use pulsar_ui::{ClockFace, ClockMusicStyle};
