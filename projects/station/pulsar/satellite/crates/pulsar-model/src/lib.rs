#![deny(unsafe_code)]
#![deny(unsafe_op_in_unsafe_fn)]
#![deny(clippy::undocumented_unsafe_blocks)]

mod detect;
mod profile;

pub use detect::{
    detect_profile, mini_from_board_and_cfg, profile_from_compatible, profile_from_dt_id,
    quasar_device_type,
};
pub use profile::{
    ALL, Alsa, Audio, Capture, CaptureFormat, ClockPanel, DeviceProfile, Earcons, Identity, Input,
    LedBackend, Leds, LightSensor, MuteSwitch, Playback, Preroll, PrerollRule, StripLayout,
    VolumeCurve, VolumePoint, WakePreroll, ZigbeeRadio,
};
