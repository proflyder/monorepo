//! Light capabilities derived from the color modes Home Assistant reports.
//!
//! A bulb with only color temperature has no `color`; a dimmable white bulb has
//! brightness alone. The active mode decides which live values are published:
//! the mode is resolved before any value, so a color-mode lamp never reports a
//! stale `color_temp_kelvin`.

use serde::Deserialize;
use serde_json::{Map, Value};

use super::{DEFAULT_MAX_KELVIN, DEFAULT_MIN_KELVIN};

/// Longest accepted `effect` value, in characters.
pub const MAX_EFFECT_CHARS: usize = 64;
/// Longest accepted `transition`, in seconds.
pub const MAX_TRANSITION_SECS: f64 = 300.0;

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
    /// Effects advertised by `effect_list`, empty when the bulb does not
    /// report any (or discovery could not read them).
    pub effects: Vec<String>,
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
    /// HA's `effect_list` attribute.
    pub effects: Vec<String>,
}

/// The adjustments of a `set`/`turn_on` command plus the text of a `speak`
/// action. `None` leaves a value untouched.
#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
pub struct Params {
    #[serde(default)]
    pub brightness: Option<i32>,
    #[serde(default)]
    pub kelvin: Option<i32>,
    #[serde(default)]
    pub hue: Option<i32>,
    #[serde(default)]
    pub saturation: Option<i32>,
    /// A bulb effect such as `"Day"`; must be listed in `effect_list` when the
    /// bulb advertises one.
    #[serde(default)]
    pub effect: Option<String>,
    /// Transition time in seconds, `0..=300`.
    #[serde(default)]
    pub transition: Option<f64>,
    /// Text of a `speak` action. It is validated and sent as a script
    /// variable, never stored, logged or echoed in an ack detail.
    #[serde(default)]
    pub text: Option<String>,
}

impl Params {
    /// Whether the payload carries no adjustment at all.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.brightness.is_none()
            && self.kelvin.is_none()
            && self.hue.is_none()
            && self.saturation.is_none()
            && self.effect.is_none()
            && self.transition.is_none()
            && self.text.is_none()
    }
}

/// A rejected light adjustment.
#[derive(Debug, Clone, PartialEq, thiserror::Error)]
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
    #[error("effect is empty or longer than {MAX_EFFECT_CHARS} characters")]
    EffectInvalid,
    #[error("unknown effect")]
    EffectUnknown,
    #[error("transition must be 0-300 seconds")]
    TransitionRange,
    #[error("turn_off accepts only a transition")]
    TurnOffAdjustment,
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
        light.effects = input
            .effects
            .into_iter()
            .map(|effect| effect.trim().to_owned())
            .filter(|effect| !effect.is_empty() && effect.chars().count() <= MAX_EFFECT_CHARS)
            .collect();
        Some(light)
    }

    /// Validate `params` against the capabilities and build `light.turn_on`
    /// data for the `set` action, which needs at least one adjustment.
    pub fn service_data(&self, params: &Params) -> Result<Map<String, Value>, LightError> {
        let data = self.turn_on_data(params)?;
        if data.is_empty() {
            return Err(LightError::Empty);
        }
        Ok(data)
    }

    /// Build `light.turn_on` data for a `turn_on` action. Every adjustment is
    /// optional, so an empty map simply turns the light on with its current
    /// settings.
    pub fn turn_on_data(&self, params: &Params) -> Result<Map<String, Value>, LightError> {
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
        if let Some(effect) = params.effect.as_deref() {
            let effect = effect.trim();
            if effect.is_empty() || effect.chars().count() > MAX_EFFECT_CHARS {
                return Err(LightError::EffectInvalid);
            }
            if !self.effects.is_empty() && !self.effects.iter().any(|known| known == effect) {
                return Err(LightError::EffectUnknown);
            }
            data.insert("effect".to_owned(), Value::String(effect.to_owned()));
        }
        insert_transition(&mut data, params)?;
        Ok(data)
    }

    /// Build `light.turn_off` data. Only a transition is meaningful there; an
    /// adjustment such as brightness would be silently dropped by HA, so it is
    /// rejected instead.
    pub fn turn_off_data(&self, params: &Params) -> Result<Map<String, Value>, LightError> {
        if params.brightness.is_some()
            || params.kelvin.is_some()
            || params.hue.is_some()
            || params.saturation.is_some()
            || params.effect.is_some()
        {
            return Err(LightError::TurnOffAdjustment);
        }
        let mut data = Map::new();
        insert_transition(&mut data, params)?;
        Ok(data)
    }
}

/// Validate and add the optional transition time.
fn insert_transition(data: &mut Map<String, Value>, params: &Params) -> Result<(), LightError> {
    let Some(transition) = params.transition else {
        return Ok(());
    };
    if !transition.is_finite() || !(0.0..=MAX_TRANSITION_SECS).contains(&transition) {
        return Err(LightError::TransitionRange);
    }
    data.insert("transition".to_owned(), Value::from(transition));
    Ok(())
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
            effects: Vec::new(),
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
    fn effect_and_transition_are_validated_and_forwarded() {
        let light = Light {
            brightness: true,
            color_temp: true,
            min_kelvin: 2700,
            max_kelvin: 6500,
            effects: vec!["Day".to_owned(), "Night".to_owned()],
            ..Light::default()
        };
        let data = light
            .service_data(&Params {
                brightness: Some(100),
                kelvin: Some(2800),
                effect: Some(" Day ".to_owned()),
                transition: Some(0.5),
                ..Params::default()
            })
            .expect("set with effect");
        assert_eq!(data["effect"], Value::from("Day"));
        assert_eq!(data["transition"], Value::from(0.5));

        // A known list rejects an effect it does not advertise.
        assert_eq!(
            light
                .service_data(&Params {
                    effect: Some("Party".to_owned()),
                    ..Params::default()
                })
                .expect_err("unknown effect"),
            LightError::EffectUnknown
        );
        // An unreadable list cannot validate, so a non-empty effect is passed
        // through instead of breaking a working bulb.
        let unlisted = Light {
            brightness: true,
            ..Light::default()
        };
        assert!(
            unlisted
                .service_data(&Params {
                    effect: Some("Party".to_owned()),
                    ..Params::default()
                })
                .is_ok()
        );
        // Empty or oversized effects are always rejected.
        for effect in [
            String::new(),
            "  ".to_owned(),
            "ж".repeat(MAX_EFFECT_CHARS + 1),
        ] {
            assert!(
                unlisted
                    .service_data(&Params {
                        effect: Some(effect),
                        ..Params::default()
                    })
                    .is_err()
            );
        }
        for transition in [-1.0, 300.5, f64::NAN] {
            assert!(
                light
                    .service_data(&Params {
                        transition: Some(transition),
                        ..Params::default()
                    })
                    .is_err(),
                "accepted transition {transition}"
            );
        }
    }

    #[test]
    fn turn_off_takes_only_a_transition() {
        let light = Light {
            brightness: true,
            ..Light::default()
        };
        let data = light
            .turn_off_data(&Params {
                transition: Some(1.5),
                ..Params::default()
            })
            .expect("turn_off");
        assert_eq!(data["transition"], Value::from(1.5));
        assert!(
            light
                .turn_off_data(&Params::default())
                .expect("empty")
                .is_empty()
        );
        assert_eq!(
            light
                .turn_off_data(&Params {
                    brightness: Some(10),
                    ..Params::default()
                })
                .expect_err("brightness on turn_off"),
            LightError::TurnOffAdjustment
        );
        assert_eq!(
            light
                .turn_off_data(&Params {
                    effect: Some("Day".to_owned()),
                    ..Params::default()
                })
                .expect_err("effect on turn_off"),
            LightError::TurnOffAdjustment
        );
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
