use pulsar_alsa::{Ctl, Pcm, Stream};
use pulsar_core::volume::{clamp_unit, from_raw, to_raw};
use pulsar_model::{Playback, VolumeCurve};
use thiserror::Error;
use tracing::{debug, error, info};

const MASTER: &str = "Master Playback Volume";

#[derive(Clone, Debug, Error)]
pub enum MixerError {
    #[error("snd_ctl_open hw:0: {0}")]
    Open(String),
    #[error("control '{0}' not found")]
    NoControl(&'static str),
}

pub struct Mixer {
    curve: &'static VolumeCurve,
    ctl: Option<Ctl>,
    volume: f32,
    muted: bool,
}

impl Mixer {
    pub fn open(
        playback: &'static Playback,
        curve: &'static VolumeCurve,
    ) -> Result<Self, MixerError> {
        let mut mixer = Self::fallback(curve);
        if curve.software_volume {
            info!("mixer: software volume");
            return Ok(mixer);
        }
        let ctl = Ctl::open("hw:0").map_err(|e| MixerError::Open(e.to_string()))?;
        if !ctl.mixer_exists(MASTER) {
            drop(Pcm::open(playback.pcm, Stream::Playback, true));
            if !ctl.mixer_exists(MASTER) {
                return Err(MixerError::NoControl(MASTER));
            }
        }
        if let Ok(raw) = ctl.read_integer(MASTER, 0) {
            if raw > 0 {
                mixer.volume = from_raw(curve, raw);
            }
            info!("mixer: master raw={raw} -> volume {:.2}", mixer.volume);
        }
        mixer.ctl = Some(ctl);
        Ok(mixer)
    }

    pub fn fallback(curve: &'static VolumeCurve) -> Self {
        Self {
            curve,
            ctl: None,
            volume: if curve.software_volume { 0.1 } else { 0.5 },
            muted: false,
        }
    }

    pub fn available(&self) -> bool {
        self.curve.software_volume || self.ctl.is_some()
    }

    pub fn volume(&self) -> f32 {
        self.volume
    }

    pub fn set_volume(&mut self, v: f32) {
        self.volume = clamp_unit(v);
        self.apply();
    }

    pub fn muted(&self) -> bool {
        self.muted
    }

    pub fn set_muted(&mut self, m: bool) {
        self.muted = m;
        self.apply();
    }

    fn apply(&self) {
        let Some(ctl) = &self.ctl else { return };
        let raw = if self.muted {
            0
        } else {
            to_raw(self.curve, self.volume)
        };
        if let Err(e) = ctl.write_integers(MASTER, &[raw, raw]) {
            error!("mixer: write {raw}: {e}");
        } else {
            debug!(
                "mixer: volume {:.2} muted={} -> raw {raw}",
                self.volume,
                i32::from(self.muted)
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pulsar_core::volume::{step_alice, step_level};
    use pulsar_model::profile_from_compatible;

    fn midi() -> &'static VolumeCurve {
        &profile_from_compatible(b"yandex,cucumber\0")
            .unwrap()
            .volume
    }

    #[test]
    fn steps_from_a_level_move_exactly_one() {
        for c in [
            &b"yandex,cucumber\0"[..],
            b"yandex,plum\0",
            b"yandex,station-mini2\0",
            b"yandex,bergamot\0",
        ] {
            let m = &profile_from_compatible(c).unwrap().volume;
            for k in 0..=20 {
                let v = k as f32 / 20.0;
                assert_eq!(step_level(m, v, true), (k + 1).min(20) as f32 / 20.0);
                assert_eq!(step_level(m, v, false), (k - 1).max(0) as f32 / 20.0);
            }
        }
        for k in 0..=10 {
            let v = k as f32 / 10.0;
            assert_eq!(step_alice(v, true), (k + 1).min(10) as f32 / 10.0);
            assert_eq!(step_alice(v, false), (k - 1).max(0) as f32 / 10.0);
        }
    }

    #[test]
    fn steps_from_between_levels_go_to_the_next_level() {
        let m = midi();
        assert_eq!(step_level(m, 0.07, false), 0.05);
        assert_eq!(step_level(m, 0.07, true), 0.1);
        assert_eq!(step_level(m, 0.33, true), 0.35);
        assert_eq!(step_level(m, 0.33, false), 0.3);
        assert_eq!(step_alice(0.05, true), 0.1);
        assert_eq!(step_alice(0.15, false), 0.1);
        assert_eq!(step_alice(0.33, false), 0.3);
        assert_eq!(step_level(m, 0.150_000_1, false), 0.1);
        assert_eq!(step_level(m, 0.149_999_9, true), 0.2);
        assert_eq!(step_alice(0.199_999_9, true), 0.3);
    }

    #[test]
    fn steps_down_reach_exact_silence() {
        let m = midi();
        let mut v = 0.15;
        for _ in 0..3 {
            v = step_level(m, v, false);
        }
        assert_eq!(v, 0.0);
        let mut v = 0.3;
        for _ in 0..3 {
            v = step_alice(v, false);
        }
        assert_eq!(v, 0.0);
        assert_eq!(step_level(m, 1.0, true), 1.0);
        assert_eq!(step_alice(0.0, false), 0.0);
    }
}
