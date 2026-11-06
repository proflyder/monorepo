//! Light capabilities derived from the color modes Home Assistant reports.
//!
//! A bulb with only color temperature has no `color`; a dimmable white bulb has
//! brightness alone. The active mode decides which live values are published:
//! the mode is resolved before any value, so a color-mode lamp never reports a
//! stale `color_temp_kelvin`.

use serde::Deserialize;
use serde_json::{Map, Value};

use super::{DEFAULT_MAX_KELVIN, DEFAULT_MIN_KELVIN};

/// What a light can be adjusted to, plus its live values.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Light {
    pub brightness: bool,
    pub color_temp: bool,
    pub color: bool,
    pub min_kelvin: i32,
    pub max_kelvin: i32,
    pub brightness_pct: i32,
    pub kelvin: i32,
    pub hue: i32,
    pub saturation: i32,
    /// `"color_temp"`, `"color"` or empty (unknown / white only).
    pub mode: String,
}

/// The raw Home Assistant data [`Light::new`] derives from.
#[derive(Debug, Clone, Default)]
pub struct LightInput {
    pub color_modes: Vec<String>,
    pub min_kelvin: i32,
    pub max_kelvin: i32,
    /// HA's 0-255 value, -1 when absent.
    pub brightness: i32,
    pub kelvin: i32,
    pub hue: f64,
    pub saturation: f64,
    pub has_hs: bool,
    pub color_mode: String,
}

/// The adjustments of a `set` command. `None` leaves a value untouched.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize)]
pub struct Params {
    #[serde(default)]
    pub brightness: Option<i32>,
    #[serde(default)]
    pub kelvin: Option<i32>,
    #[serde(default)]
    pub hue: Option<i32>,
    #[serde(default)]
    pub saturation: Option<i32>,
}

/// A rejected light adjustment.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum LightError {
    #[error("light does not support brightness")]
    Brightness,
    #[error("brightness {0} out of range 1-100")]
    BrightnessRange(i32),
    #[error("color temperature and color cannot be set together")]
    MixedModes,
    #[error("light does not support color temperature")]
    ColorTemp,
    #[error("color temperature {value} out of range {min}-{max}")]
    KelvinRange { value: i32, min: i32, max: i32 },
    #[error("light does not support color")]
    Color,
    #[error("color needs both hue and saturation")]
    ColorIncomplete,
    #[error("hue must be 0-360 and saturation must be 0-100")]
    ColorRange,
    #[error("set needs at least one adjustment")]
    Empty,
}

impl Light {
    /// Derive the capabilities of a light, or `None` when it is on/off only.
    #[must_use]
    #[allow(clippy::needless_pass_by_value)]
    pub fn new(input: LightInput) -> Option<Self> {
        let mut light = Self::default();
        for mode in &input.color_modes {
            match mode.to_lowercase().as_str() {
                "brightness" | "white" => light.brightness = true,
                "color_temp" => {
                    light.brightness = true;
                    light.color_temp = true;
                }
                "hs" | "xy" | "rgb" | "rgbw" | "rgbww" => {
                    light.brightness = true;
                    light.color = true;
                }
                _ => {}
            }
        }
        if !light.brightness {
            return None;
        }
        // The active mode decides which live values are meaningful.
        match input.color_mode.to_lowercase().as_str() {
            "color_temp" => "color_temp".clone_into(&mut light.mode),
            "hs" | "xy" | "rgb" | "rgbw" | "rgbww" => "color".clone_into(&mut light.mode),
            _ => {}
        }
        if light.color_temp {
            light.min_kelvin = input.min_kelvin;
            light.max_kelvin = input.max_kelvin;
            if light.min_kelvin <= 0 || light.max_kelvin <= light.min_kelvin {
                light.min_kelvin = DEFAULT_MIN_KELVIN;
                light.max_kelvin = DEFAULT_MAX_KELVIN;
            }
            if light.mode != "color"
                && input.kelvin >= light.min_kelvin
                && input.kelvin <= light.max_kelvin
            {
                light.kelvin = input.kelvin;
            }
        }
        if input.brightness >= 0 {
            light.brightness_pct = round(f64::from(input.brightness) * 100.0 / 255.0);
        }
        if light.color && input.has_hs && light.mode != "color_temp" {
            light.hue = round(input.hue).rem_euclid(360);
            light.saturation = round(input.saturation);
        }
        Some(light)
    }

    /// Validate `params` against the capabilities and build `light.turn_on`
    /// data.
    pub fn service_data(&self, params: &Params) -> Result<Map<String, Value>, LightError> {
        let mut data = Map::new();
        if let Some(brightness) = params.brightness {
            if !self.brightness {
                return Err(LightError::Brightness);
            }
            if !(1..=100).contains(&brightness) {
                return Err(LightError::BrightnessRange(brightness));
            }
            data.insert("brightness_pct".to_owned(), Value::from(brightness));
        }
        let has_color = params.hue.is_some() || params.saturation.is_some();
        if params.kelvin.is_some() && has_color {
            return Err(LightError::MixedModes);
        }
        if let Some(kelvin) = params.kelvin {
            if !self.color_temp {
                return Err(LightError::ColorTemp);
            }
            if kelvin < self.min_kelvin || kelvin > self.max_kelvin {
                return Err(LightError::KelvinRange {
                    value: kelvin,
                    min: self.min_kelvin,
                    max: self.max_kelvin,
                });
            }
            data.insert("color_temp_kelvin".to_owned(), Value::from(kelvin));
        }
        if has_color {
            if !self.color {
                return Err(LightError::Color);
            }
            let (Some(hue), Some(saturation)) = (params.hue, params.saturation) else {
                return Err(LightError::ColorIncomplete);
            };
            if !(0..=360).contains(&hue) || !(0..=100).contains(&saturation) {
                return Err(LightError::ColorRange);
            }
            data.insert(
                "hs_color".to_owned(),
                Value::Array(vec![Value::from(hue), Value::from(saturation)]),
            );
        }
        if data.is_empty() {
            return Err(LightError::Empty);
        }
        Ok(data)
    }
}

/// `math.Round`: halves away from zero.
fn round(value: f64) -> i32 {
    #[allow(clippy::cast_possible_truncation)]
    let rounded = value.round() as i32;
    rounded
}

#[cfg(test)]
mod tests {
    use super::*;

    fn light(modes: &[&str], brightness: i32) -> Option<Light> {
        Light::new(LightInput {
            color_modes: modes.iter().map(|mode| (*mode).to_owned()).collect(),
            brightness,
            ..LightInput::default()
        })
    }

    #[test]
    fn capabilities_match_the_go_derivation() {
        assert!(light(&["onoff"], -1).is_none());
        let white = Light::new(LightInput {
            color_modes: vec!["color_temp".to_owned()],
            min_kelvin: 2700,
            max_kelvin: 6500,
            brightness: 255,
            kelvin: 4000,
            ..LightInput::default()
        })
        .expect("color temp");
        assert!(white.brightness && white.color_temp && !white.color);
        assert_eq!(white.brightness_pct, 100);
        assert_eq!(white.kelvin, 4000);

        let color = Light::new(LightInput {
            color_modes: vec!["color_temp".to_owned(), "xy".to_owned()],
            brightness: 128,
            hue: 359.6,
            saturation: 50.0,
            has_hs: true,
            ..LightInput::default()
        })
        .expect("color");
        assert!(color.color);
        assert_eq!(color.min_kelvin, 2000);
        assert_eq!(color.brightness_pct, 50);
        assert_eq!(color.hue, 0);
    }

    #[test]
    fn active_mode_drops_inactive_values() {
        let mut input = LightInput {
            color_modes: vec!["color_temp".to_owned(), "hs".to_owned()],
            min_kelvin: 1700,
            max_kelvin: 6500,
            brightness: 128,
            kelvin: 3000,
            hue: 30.0,
            saturation: 60.0,
            has_hs: true,
            color_mode: "color_temp".to_owned(),
        };
        let color_temp = Light::new(input.clone()).expect("color temp");
        assert_eq!(color_temp.mode, "color_temp");
        assert_eq!(color_temp.kelvin, 3000);
        assert_eq!(color_temp.hue, 0);

        input.color_mode = "hs".to_owned();
        let color = Light::new(input).expect("color");
        assert_eq!(color.mode, "color");
        assert_eq!(color.kelvin, 0, "a color-mode lamp has no live kelvin");
        assert_eq!(color.hue, 30);
        assert_eq!(color.saturation, 60);
    }

    #[test]
    fn set_validation_matches_the_go_errors() {
        let color_temp = Light {
            brightness: true,
            color_temp: true,
            min_kelvin: 2700,
            max_kelvin: 6500,
            ..Light::default()
        };
        for params in [
            Params::default(),
            Params {
                brightness: Some(0),
                ..Params::default()
            },
            Params {
                brightness: Some(101),
                ..Params::default()
            },
            Params {
                kelvin: Some(2000),
                ..Params::default()
            },
            Params {
                kelvin: Some(3000),
                hue: Some(1),
                saturation: Some(1),
                ..Params::default()
            },
        ] {
            assert!(color_temp.service_data(&params).is_err(), "{params:?}");
        }
        let color = Light {
            brightness: true,
            color: true,
            ..Light::default()
        };
        assert_eq!(
            color
                .service_data(&Params {
                    hue: Some(10),
                    ..Params::default()
                })
                .expect_err("incomplete color"),
            LightError::ColorIncomplete
        );
        let data = color
            .service_data(&Params {
                brightness: Some(5),
                hue: Some(10),
                saturation: Some(80),
                ..Params::default()
            })
            .expect("set");
        assert_eq!(data["brightness_pct"], Value::from(5));
        assert_eq!(data["hs_color"], Value::Array(vec![10.into(), 80.into()]));
    }
}
