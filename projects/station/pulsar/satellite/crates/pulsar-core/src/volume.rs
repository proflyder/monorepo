use pulsar_model::VolumeCurve;

pub fn to_raw(curve: &VolumeCurve, v: f32) -> i64 {
    let m = curve.map;
    if v <= 0.0 || m.len() < 2 {
        return 0;
    }
    let p = f64::from(v.min(1.0) * m[m.len() - 1].level as f32);
    if curve.min_step && p <= f64::from(m[1].level) {
        return i64::from(m[1].raw);
    }
    for w in m.windows(2) {
        let (a, b) = (w[0], w[1]);
        if p <= f64::from(b.level) {
            let raw = f64::from(a.raw)
                + (p - f64::from(a.level)) * f64::from(b.raw - a.raw)
                    / f64::from(b.level - a.level);
            return raw.round() as i64;
        }
    }
    i64::from(m[m.len() - 1].raw)
}

pub fn from_raw(curve: &VolumeCurve, raw: i64) -> f32 {
    let m = curve.map;
    if m.len() < 2 || raw <= i64::from(m[0].raw) {
        return 0.0;
    }
    for w in m.windows(2) {
        let (a, b) = (w[0], w[1]);
        if raw <= i64::from(b.raw) {
            let num = (raw - i64::from(a.raw)) * i64::from(b.level - a.level);
            let level = f64::from(a.level) + num as f64 / f64::from(b.raw - a.raw);
            return (level / f64::from(m[m.len() - 1].level)) as f32;
        }
    }
    1.0
}

pub fn step_level(curve: &VolumeCurve, v: f32, up: bool) -> f32 {
    let m = curve.map;
    if m.len() < 2 {
        return clamp_unit(v);
    }
    grid_step(v, m[m.len() - 1].level, up)
}

pub fn step_alice(v: f32, up: bool) -> f32 {
    grid_step(v, 10, up)
}

fn grid_step(v: f32, grid: i32, up: bool) -> f32 {
    let p = clamp_unit(v) * grid as f32;
    let point = if up {
        (p + 1e-3).floor() as i32 + 1
    } else {
        (p - 1e-3).ceil() as i32 - 1
    };
    point.clamp(0, grid) as f32 / grid as f32
}

pub fn curve_gain(curve: &VolumeCurve, v: f32) -> f32 {
    let raw = to_raw(curve, v);
    if raw <= 0 {
        return 0.0;
    }
    let db = -60.0 + 60.0 * raw as f64 / 255.0;
    10f64.powf(db / 20.0) as f32
}

pub fn master_gain(curve: &VolumeCurve, v: f32) -> f32 {
    if !curve.software_volume {
        return 1.0;
    }
    curve_gain(curve, v)
}

fn legacy_raw(v: f32) -> i64 {
    if v <= 0.0 {
        return 0;
    }
    let db = 60.0 * 0.9f64.mul_add(f64::from(v.min(1.0)), 0.1).log10();
    let raw = ((db + 60.0) / 60.0 * 255.0).round() as i64;
    raw.clamp(0, 255)
}

fn legacy_volume(raw: i64) -> f32 {
    let db = -60.0 + 60.0 * raw as f64 / 255.0;
    ((10f64.powf(db / 60.0) - 0.1) / 0.9).max(0.0) as f32
}

pub fn from_legacy(curve: &VolumeCurve, v: f32) -> f32 {
    if curve.software_volume {
        return clamp_unit(v);
    }
    from_raw(curve, legacy_raw(v))
}

pub fn to_legacy(curve: &VolumeCurve, v: f32) -> f32 {
    if curve.software_volume {
        return clamp_unit(v);
    }
    legacy_volume(to_raw(curve, v))
}

pub fn clamp_unit(v: f32) -> f32 {
    let v = if 0.0 < v { v } else { 0.0 };
    if v < 1.0 { v } else { 1.0 }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pulsar_model::{profile_from_compatible, profile_from_dt_id};

    #[test]
    fn master_gain_follows_the_stock_softvol_curve() {
        let mini = profile_from_compatible(b"yandex,station-mini2\0").unwrap();
        assert_eq!(master_gain(&mini.volume, 0.2), 1.0);
        let mini = profile_from_dt_id(b"axg_s420_v03\0").unwrap();
        assert_eq!(master_gain(&mini.volume, 0.0), 0.0);
        assert_eq!(master_gain(&mini.volume, 1.0), 1.0);
        assert!((master_gain(&mini.volume, 0.05) - 0.0037711).abs() < 1e-7);
        let db = |a: f32, b: f32| 20.0 * (b / a).log10();
        let step_db = 60.0 * 206.0 / 19.0 / 255.0;
        for k in 1..20 {
            let a = master_gain(&mini.volume, k as f32 / 20.0);
            let b = master_gain(&mini.volume, (k + 1) as f32 / 20.0);
            assert!((db(a, b) - step_db).abs() < 0.2, "step {k}");
        }
        assert_eq!(master_gain(&mini.volume, 2.0), 1.0);
        assert_eq!(master_gain(&mini.volume, -1.0), 0.0);
    }

    #[test]
    fn curve_gain_exists_for_hardware_volume_models_too() {
        let midi = profile_from_compatible(b"yandex,cucumber\0").unwrap();
        assert_eq!(master_gain(&midi.volume, 0.5), 1.0);
        assert!(curve_gain(&midi.volume, 0.5) < 1.0);
        assert_eq!(curve_gain(&midi.volume, 0.0), 0.0);
        assert_eq!(curve_gain(&midi.volume, 1.0), 1.0);
        let mini = profile_from_dt_id(b"axg_s420_v03\0").unwrap();
        assert_eq!(
            curve_gain(&mini.volume, 0.15),
            master_gain(&mini.volume, 0.15)
        );
    }

    #[test]
    fn volume_is_clamped_like_cxx() {
        assert_eq!(clamp_unit(0.25), 0.25);
        assert_eq!(clamp_unit(-0.1), 0.0);
        assert_eq!(clamp_unit(1.5), 1.0);
        assert_eq!(clamp_unit(f32::NAN), 0.0);
        assert_eq!(clamp_unit(f32::NEG_INFINITY), 0.0);
    }

    #[test]
    fn legacy_curve_ends() {
        assert_eq!(legacy_raw(0.0), 0);
        assert_eq!(legacy_raw(-1.0), 0);
        assert_eq!(legacy_raw(1.0), 255);
        assert_eq!(legacy_raw(f32::NAN), 255);
        assert_eq!(legacy_volume(255), 1.0);
        assert_eq!(legacy_volume(0), 0.0);
    }
}
