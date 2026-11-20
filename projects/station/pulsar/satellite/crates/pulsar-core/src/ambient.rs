#[derive(Clone, Debug)]
pub struct AmbientDimmer {
    smooth: f32,
    out: f32,
    primed: bool,
    settling: bool,
}

const ALPHA: f32 = 0.3;
const HYSTERESIS: f32 = 0.03;
const DARK_LUX: f32 = 1.5;
const CONVERGE: f32 = 1e-3;

pub const DARK: f32 = 0.02;

pub fn perceptual(linear: f32) -> f32 {
    linear * linear
}

fn target(lux: f32) -> f32 {
    let lux = if lux < DARK_LUX { 0.0 } else { lux.max(0.0) };
    let a = (1.0 + lux).log10() / 501f32.log10();
    a.min(1.0).mul_add(1.0 - DARK, DARK)
}

impl Default for AmbientDimmer {
    fn default() -> Self {
        Self::new()
    }
}

impl AmbientDimmer {
    pub fn new() -> Self {
        Self {
            smooth: 1.0,
            out: 1.0,
            primed: false,
            settling: false,
        }
    }

    pub fn update(&mut self, lux: f32) -> bool {
        let target = target(lux);
        if !self.primed {
            self.smooth = target;
            self.out = target;
            self.primed = true;
            return true;
        }
        if !self.settling && (target - self.out).abs() <= HYSTERESIS {
            self.smooth = self.out;
            return false;
        }
        self.settling = true;
        self.smooth = (target - self.smooth).mul_add(ALPHA, self.smooth);
        if (target - self.smooth).abs() <= CONVERGE {
            self.smooth = target;
            self.settling = false;
        }
        if self.smooth == self.out {
            return false;
        }
        self.out = self.smooth;
        true
    }

    pub fn factor(&self) -> f32 {
        self.out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: f32, b: f32) -> bool {
        (a - b).abs() < 1e-4
    }

    fn settle(d: &mut AmbientDimmer, lux: f32) {
        for _ in 0..200 {
            if !d.update(lux) {
                return;
            }
        }
        panic!("dimmer did not settle");
    }

    #[test]
    fn first_reading_applies_at_once() {
        let mut d = AmbientDimmer::new();
        assert_eq!(d.factor(), 1.0);
        assert!(d.update(0.0));
        assert!(close(d.factor(), 0.02));

        let mut d = AmbientDimmer::new();
        assert!(d.update(1000.0));
        assert!(close(d.factor(), 1.0));
        let mut d = AmbientDimmer::new();
        assert!(d.update(-5.0));
        assert!(close(d.factor(), 0.02));
    }

    #[test]
    fn dark_dead_zone_covers_the_first_sensor_step() {
        let mut zero = AmbientDimmer::new();
        zero.update(0.0);
        for lux in [0.5, 1.14, 1.49] {
            let mut d = AmbientDimmer::new();
            d.update(lux);
            assert_eq!(d.factor(), zero.factor(), "{lux} lx");
        }
        let mut d = AmbientDimmer::new();
        d.update(2.28);
        assert!(d.factor() > zero.factor());
    }

    #[test]
    fn later_readings_are_smoothed() {
        let mut d = AmbientDimmer::new();
        d.update(500.0);
        assert!(d.update(0.0));
        assert!(close(d.factor(), 1.0 + 0.3 * (0.02 - 1.0)));
        assert!(d.update(0.0));
        assert!(close(d.factor(), 0.706 + 0.3 * (0.02 - 0.706)));
    }

    #[test]
    fn hysteresis_holds_changes_inside_the_dead_band() {
        let mut d = AmbientDimmer::new();
        d.update(25.0);
        let shown = d.factor();
        assert!((target(26.0) - shown).abs() <= HYSTERESIS);
        for _ in 0..100 {
            assert!(!d.update(26.0));
            assert_eq!(d.factor(), shown);
        }
        assert!(d.update(100.0));
        assert!(d.factor() > shown);
    }

    #[test]
    fn converges_exactly_to_the_target() {
        let mut d = AmbientDimmer::new();
        d.update(30.0);
        settle(&mut d, 0.0);
        assert_eq!(d.factor(), 0.02);
        assert_eq!(d.factor(), target(0.0));

        let mut d = AmbientDimmer::new();
        d.update(0.0);
        settle(&mut d, 58.0);
        assert_eq!(d.factor(), target(58.0));

        settle(&mut d, 0.0);
        assert_eq!(d.factor(), 0.02);
    }

    #[test]
    fn a_settled_target_leaves_nothing_to_change() {
        let mut d = AmbientDimmer::new();
        d.update(58.0);
        assert!(!d.update(58.0));
        settle(&mut d, 0.0);
        for _ in 0..5 {
            assert!(!d.update(0.0));
        }
    }

    #[test]
    fn rounds_like_the_fused_cxx_build() {
        let first = |lux: f32| {
            let mut d = AmbientDimmer::new();
            d.update(lux);
            d.factor().to_bits()
        };
        assert_eq!(first(500.0), 0x3f80_0000);
        assert_eq!(first(58.14), 0x3f29_c53f);
        assert_eq!(first(22.8), 0x3f05_0984);

        let mut d = AmbientDimmer::new();
        d.update(500.0);
        d.update(30.0);
        assert_eq!(d.factor().to_bits(), 0x3f5e_4fa0);
        d.update(100.0);
        assert_eq!(d.factor().to_bits(), 0x3f55_0756);
    }
}
