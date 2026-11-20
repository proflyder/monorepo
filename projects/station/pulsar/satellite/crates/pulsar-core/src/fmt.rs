use crate::sys;

pub fn g6(v: f64) -> String {
    sys::format_g(v)
}

pub fn parse_f32_prefix(s: &str) -> f32 {
    sys::strtof(s)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn g6_matches_printf() {
        assert_eq!(g6(0.5), "0.5");
        assert_eq!(g6(1.0), "1");
        assert_eq!(g6(1.0 / 3.0), "0.333333");
        assert_eq!(g6(0.15), "0.15");
        assert_eq!(g6(1e-7), "1e-07");
        assert_eq!(g6(123456789.0), "1.23457e+08");
        assert_eq!(g6(0.0), "0");
        assert_eq!(g6(-2.5), "-2.5");
        assert_eq!(g6(-1.0 / 3.0), "-0.333333");
        assert_eq!(g6(999999.0), "999999");
        assert_eq!(g6(999999.5), "1e+06");
        assert_eq!(g6(0.0001), "0.0001");
        assert_eq!(g6(0.00001), "1e-05");
        assert_eq!(g6(f64::from(0.3f32)), "0.3");
    }

    #[test]
    fn parse_f32_prefix_matches_strtof() {
        assert_eq!(parse_f32_prefix("0.3abc"), 0.3);
        assert_eq!(parse_f32_prefix(""), 0.0);
        assert_eq!(parse_f32_prefix("x"), 0.0);
        assert_eq!(parse_f32_prefix(" 1e2\n"), 100.0);
        assert_eq!(parse_f32_prefix("-0.25"), -0.25);
        assert_eq!(parse_f32_prefix("1\0junk"), 1.0);
    }
}
