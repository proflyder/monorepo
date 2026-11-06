//! ParsedTimezone из GetTimeResponse в виде POSIX TZ-строки, которую хранит демон.

use std::fmt::Write as _;

use prost::Message;

use crate::pb::ParsedTimezone;

/// POSIX TZ-строка для ParsedTimezone (поле 3 GetTimeResponse) без правил DST, например
/// "<+0300>-3". std_offset_seconds использует знак POSIX (секунды к западу от UTC: MSK-3 =
/// -10800). "" — если у зоны есть DST (здесь не представимо) или сообщение битое.
pub fn posix_tz_from_parsed(p: &[u8]) -> String {
    let Ok(tz) = ParsedTimezone::decode(p) else {
        return String::new();
    };
    let std_off = tz.std_offset_seconds.unwrap_or(0);
    let has_dst = [tz.dst_start.as_ref(), tz.dst_end.as_ref()]
        .into_iter()
        .flatten()
        .any(|rule| rule.r#type.unwrap_or(0) != 0);
    if has_dst {
        return String::new();
    }
    let east = std_off.wrapping_neg();
    let a = east.unsigned_abs();
    let sign = if east < 0 { '-' } else { '+' };
    let mut out = format!("<{sign}{:02}{:02}>", a / 3600, a / 60 % 60);
    // Смещение POSIX: часы на запад [:mm[:ss]].
    let w = std_off.unsigned_abs();
    let west = if std_off < 0 { "-" } else { "" };
    let _ = if !w.is_multiple_of(60) {
        write!(out, "{west}{}:{:02}:{:02}", w / 3600, w / 60 % 60, w % 60)
    } else if !w.is_multiple_of(3600) {
        write!(out, "{west}{}:{:02}", w / 3600, w / 60 % 60)
    } else {
        write!(out, "{west}{}", w / 3600)
    };
    out
}
