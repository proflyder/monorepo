//! Преобразование выборок VQE в s16: одна реализация на весь аудиотракт.

/// Выборка VQE в [-1, 1] → s16 так, как это делает C++ демон (насыщение, `lrint`: ties to even).
pub fn to_s16(v: f32) -> i16 {
    let v = v * 32768.0;
    if v > 32767.0 {
        32767
    } else if v < -32768.0 {
        -32768
    } else {
        v.round_ties_even() as i16
    }
}

/// `src` в `dst` по [`to_s16`]; длина — по короткому срезу.
pub fn to_s16_slice(src: &[f32], dst: &mut [i16]) {
    for (o, &v) in dst.iter_mut().zip(src) {
        *o = to_s16(v);
    }
}

/// `src * gain` в `dst` по [`to_s16`] (усиление до перевода в s16, с насыщением на его границах);
/// длина — по короткому срезу.
pub fn to_s16_scaled(src: &[f32], gain: f32, dst: &mut [i16]) {
    for (o, &v) in dst.iter_mut().zip(src) {
        *o = to_s16(v * gain);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn float_to_s16_clamps_and_rounds_to_even() {
        assert_eq!(to_s16(0.0), 0);
        assert_eq!(to_s16(0.5), 16384);
        assert_eq!(to_s16(-0.5), -16384);
        assert_eq!(to_s16(1.0), 32767);
        assert_eq!(to_s16(-1.0), -32768);
        assert_eq!(to_s16(2.0), 32767);
        assert_eq!(to_s16(-2.0), -32768);
        // lrint: ties to even.
        assert_eq!(to_s16(1.5 / 32768.0), 2);
        assert_eq!(to_s16(2.5 / 32768.0), 2);
        assert_eq!(to_s16(-2.5 / 32768.0), -2);
        assert_eq!(to_s16(0.4 / 32768.0), 0);
        assert_eq!(to_s16(0.6 / 32768.0), 1);
        assert_eq!(to_s16(32766.5 / 32768.0), 32766);
        assert_eq!(to_s16(f32::NAN), 0);
    }

    #[test]
    fn scaled_conversion_applies_gain_before_clamping() {
        let mut dst = [0i16; 6];
        to_s16_scaled(&[0.01, -0.01, 0.5, -0.5, 2.0, -2.0], 10.0, &mut dst);
        assert_eq!(dst, [3277, -3277, 32767, -32768, 32767, -32768]);
        // 0 дБ — то же, что to_s16.
        let src = [0.25, -0.75, 0.0];
        let mut dst = [0i16; 3];
        to_s16_scaled(&src, 1.0, &mut dst);
        let mut want = [0i16; 3];
        to_s16_slice(&src, &mut want);
        assert_eq!(dst, want);
    }
}
