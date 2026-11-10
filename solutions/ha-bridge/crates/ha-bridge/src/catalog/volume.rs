//! Station volume controls.
//!
//! The pulsar media player and the announcement number are hard-coded: the
//! phone may choose only a catalog entry and a `set`/`mute`/`unmute` action,
//! never an entity id or a service. The level is a 0–100 percentage in steps of
//! five, matching both the phone slider and the Home Assistant number entity.

use serde_json::{Map, Value};

use super::Params;

/// The pulsar media player exposed as `media_player.<station>` by the
/// integration. It is the only mutable media player of the catalog.
pub const VOLUME_MEDIA_PLAYER: &str = "media_player.iandeks_stantsiia_midi_media_player";
/// The announcement volume number of the station.
pub const VOLUME_ANNOUNCE_NUMBER: &str =
    "number.spalnia_iandeks_stantsiia_midi_gromkost_obiavlenii";

/// Live volume state and what the entry can do.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Volume {
    /// 0–100, rounded from Home Assistant.
    pub level: i32,
    pub muted: bool,
    /// Whether `mute`/`unmute` are available (the media player, not the number).
    pub mute: bool,
    /// The entry drives the announcement number instead of the media player.
    pub announce: bool,
}

impl Volume {
    /// Validate the level of a `set` action and return it.
    pub fn set_level(&self, params: &Params) -> Result<i32, VolumeError> {
        let level = params.level.ok_or(VolumeError::Missing)?;
        if !(0..=100).contains(&level) {
            return Err(VolumeError::Range(level));
        }
        if level % 5 != 0 {
            return Err(VolumeError::Step(level));
        }
        Ok(level)
    }

    /// Build the `media_player.volume_set` data.
    #[must_use]
    pub fn set_data(level: i32) -> Map<String, Value> {
        let mut data = Map::new();
        data.insert(
            "volume_level".to_owned(),
            Value::from(f64::from(level) / 100.0),
        );
        data
    }

    /// Build the `media_player.volume_mute` data.
    #[must_use]
    pub fn mute_data(muted: bool) -> Map<String, Value> {
        let mut data = Map::new();
        data.insert("is_volume_muted".to_owned(), Value::Bool(muted));
        data
    }
}

/// Build the `number.set_value` data.
#[must_use]
pub fn value_data(level: i32) -> Map<String, Value> {
    let mut data = Map::new();
    data.insert("value".to_owned(), Value::from(level));
    data
}

/// A rejected volume adjustment.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum VolumeError {
    #[error("volume needs a level")]
    Missing,
    #[error("volume level {0} out of range 0-100")]
    Range(i32),
    #[error("volume level {0} must be a multiple of 5")]
    Step(i32),
    #[error("mute accepts no parameters")]
    UnexpectedParams,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn volume(announce: bool, mute: bool) -> Volume {
        Volume {
            level: 45,
            muted: false,
            mute,
            announce,
        }
    }

    #[test]
    fn level_must_be_a_step_of_five_in_range() {
        let station = volume(false, true);
        for (level, expected) in [
            (None, "volume needs a level"),
            (Some(-5), "volume level -5 out of range 0-100"),
            (Some(105), "volume level 105 out of range 0-100"),
            (Some(42), "volume level 42 must be a multiple of 5"),
        ] {
            let error = station
                .set_level(&Params {
                    level,
                    ..Params::default()
                })
                .expect_err("invalid level");
            assert_eq!(error.to_string(), expected);
        }
        assert_eq!(
            station
                .set_level(&Params {
                    level: Some(45),
                    ..Params::default()
                })
                .expect("level"),
            45
        );
        assert_eq!(
            station
                .set_level(&Params {
                    level: Some(0),
                    ..Params::default()
                })
                .expect("zero"),
            0
        );
    }

    #[test]
    fn service_data_uses_fractions_and_boolean_mute() {
        assert_eq!(Volume::set_data(45)["volume_level"], Value::from(0.45_f64));
        assert_eq!(
            Volume::mute_data(true)["is_volume_muted"],
            Value::Bool(true)
        );
    }
}
