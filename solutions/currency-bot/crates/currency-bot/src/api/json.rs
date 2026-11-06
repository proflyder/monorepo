//! Go-compatible JSON number formatting.
//!
//! Go's `encoding/json` encodes `float64` 5.0 as `5` (no trailing `.0`), while
//! serde_json always keeps a fractional part. The golden fixtures therefore
//! contain integer tokens for integral rates; this serializer reproduces the
//! Go spelling field by field.

use serde::Serializer;

/// Serialize an `f64` the way Go's `encoding/json` would.
pub fn go_f64<S: Serializer>(value: &f64, serializer: S) -> Result<S::Ok, S::Error> {
    if value.is_finite() && value.fract() == 0.0 && value.abs() < 9_007_199_254_740_992.0 {
        return serializer.serialize_i64(*value as i64);
    }
    serializer.serialize_f64(*value)
}
