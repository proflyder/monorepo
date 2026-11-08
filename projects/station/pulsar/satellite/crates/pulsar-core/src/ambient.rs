/// Ambient-light dimming factor shared by the clock and the LED ring: log-shaped (dark room
/// ~0.02, 10 lx ~0.4, >= 500 lx full), low-pass filtered, with hysteresis so that sensor noise
/// around a step boundary does not make the LEDs flicker between two levels.
///
/// Гистерезис сравнивается с целью, а не с одним шагом сглаживания: изменение меньше мёртвой
/// зоны отбрасывается целиком, а настоящее доводится до цели полностью. Отсчёты ниже
/// `DARK_LUX` считаются тьмой, чтобы первый квант датчика не поднимал часы и кольцо с пола.
#[derive(Clone, Debug)]
pub struct AmbientDimmer {
    smooth: f32,
    out: f32,
    primed: bool,
    /// Идёт изменение больше мёртвой зоны: довести его до точной цели, даже когда остаток
    /// уже попал в мёртвую зону.
    settling: bool,
}

/// ~6 s time constant at one reading per 2 s.
const ALPHA: f32 = 0.3;
/// Мёртвая зона показанного значения вокруг цели: цель ближе этого к `out` его не двигает.
const HYSTERESIS: f32 = 0.03;
/// Отсчёты ниже — тьма. Датчик квантован шагом 1.14 лк, и первый ненулевой отсчёт не должен
/// поднимать выход с тёмного пола.
const DARK_LUX: f32 = 1.5;
/// Остаток до цели, меньше которого показанное значение сразу становится целью: иначе
/// экспоненциальный фильтр замирает рядом с ней, и часы не доходят ни до пола, ни до полного
/// уровня. Меньше одного уровня выхода 0..255.
const CONVERGE: f32 = 1e-3;

/// Доля полного уровня в полной темноте: нижняя граница фактора и часов, и кольца.
pub const DARK: f32 = 0.02;

/// Гамма 2 линейной величины 0..1: воспринимаемая яркость примерно линейна по ней. По этой
/// кривой приглушаются и кольцо (`pulsar_ui::leds::to_px`), и часы (`pulsar_clock::level`).
pub fn perceptual(linear: f32) -> f32 {
    linear * linear
}

/// Фактор приглушения `DARK`..1 по отсчёту люксов: логарифмическая шкала, растянутая на
/// `1 - DARK` поверх тёмного пола.
fn target(lux: f32) -> f32 {
    let lux = if lux < DARK_LUX { 0.0 } else { lux.max(0.0) };
    let a = (1.0 + lux).log10() / 501f32.log10();
    // C++-сборки станции (clang -ffp-contract=on, aarch64 / cortex-a53) сливали такие
    // `a + b * c` — здесь и в `update` — в один FMA; mul_add сохраняет их округление.
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

    /// Feeds a lux reading (polled every ~2 s); true if `factor()` changed.
    pub fn update(&mut self, lux: f32) -> bool {
        let target = target(lux);
        if !self.primed {
            self.smooth = target;
            self.out = target;
            self.primed = true;
            return true;
        }
        if !self.settling && (target - self.out).abs() <= HYSTERESIS {
            // В мёртвой зоне: забыть накопленный дрейф, чтобы настоящее изменение начиналось
            // от показанного значения.
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

    /// 1 until the first reading.
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

    /// Кормит отсчёты, пока показанное значение не успокоится.
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
        // Negative readings count as darkness.
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
        // Следующий шаг (2.28 лк) уже уходит с тёмного пола.
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
        // 26 лк — один шаг датчика: это внутри мёртвой зоны, поэтому показанное значение не
        // двигается, сколько бы отсчёт ни держался.
        assert!((target(26.0) - shown).abs() <= HYSTERESIS);
        for _ in 0..100 {
            assert!(!d.update(26.0));
            assert_eq!(d.factor(), shown);
        }
        // Отсчёт вне зоны отслеживается.
        assert!(d.update(100.0));
        assert!(d.factor() > shown);
    }

    #[test]
    fn converges_exactly_to_the_target() {
        // Темнота: пол достигается точно, а не на два-три уровня выше него.
        let mut d = AmbientDimmer::new();
        d.update(30.0);
        settle(&mut d, 0.0);
        assert_eq!(d.factor(), 0.02);
        assert_eq!(d.factor(), target(0.0));

        // Свет: то же самое на пути вверх.
        let mut d = AmbientDimmer::new();
        d.update(0.0);
        settle(&mut d, 58.0);
        assert_eq!(d.factor(), target(58.0));

        // И обратно: сглаженное значение не замирает внутри мёртвой зоны.
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

    /// Ожидаемые биты тех же FMA-выражений, что и в C++ `AmbientDimmer` (`target` и шаг
    /// сглаживания); без FMA часть из них округляется иначе.
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
