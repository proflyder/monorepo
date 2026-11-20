use std::fmt::Write as _;

use prost::Message;

use crate::pb::ParsedTimezone;

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
