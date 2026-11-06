//! Описание железа поддерживаемых Яндекс Станций: набор capability-структур, где отсутствие
//! устройства выражено через `Option` или enum, а не через пустые строки и нули.
//!
//! Решения «есть ли железо» принимаются при сборке сервисов (см. `pulsar`): каждый крейт
//! получает только ту часть профиля, которая ему нужна.
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
