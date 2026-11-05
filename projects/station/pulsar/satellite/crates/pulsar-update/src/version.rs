//! Версия релиза: тег `vMAJOR.MINOR.PATCH` и `git describe`-строка сборки
//! (`v1.2.3-4-gabc1234`, `v1.2.3-dirty`) или голый хеш без тегов.

use std::fmt;

/// Разобранная версия: исходный текст тега и три числа для сравнения.
///
/// Числа читаются как C `sscanf("%d.%d.%d")` после необязательного `v` — этой же
/// семантикой пользовался удалённый C++ пробник (`tests/golden/versions.txt`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Version {
    text: String,
    numbers: [i32; 3],
}

impl Version {
    /// Тег или `describe`-строка; `None`, если первые три числа не читаются.
    #[must_use]
    pub fn parse(text: &str) -> Option<Self> {
        Some(Self {
            text: text.to_owned(),
            numbers: parse_numbers(text)?,
        })
    }

    /// Тег OCI-релиза: ровно `vX.Y.Z` из цифр, без суффиксов и ведущих пробелов.
    pub(crate) fn parse_tag(text: &str) -> Option<Self> {
        let rest = text.strip_prefix('v')?;
        let mut parts = rest.split('.');
        let (Some(a), Some(b), Some(c), None) =
            (parts.next(), parts.next(), parts.next(), parts.next())
        else {
            return None;
        };
        let digit = |p: &str| !p.is_empty() && p.bytes().all(|b| b.is_ascii_digit());
        if !(digit(a) && digit(b) && digit(c)) {
            return None;
        }
        Self::parse(text)
    }

    /// Тег или `describe`-строка как она пришла (для HA, `update_attempt` и `--version`).
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.text
    }

    /// Это более новый релиз, чем `current`: `(newer, current_known)`. Неразобранная
    /// текущая версия (голый хеш сборки) сравнивается как строка, и автообновление
    /// такой сборке не предлагается.
    #[must_use]
    pub fn is_newer_than(&self, current: &str) -> (bool, bool) {
        match Self::parse(current) {
            Some(cur) => (self.numbers > cur.numbers, true),
            None => (self.text != current, false),
        }
    }

    #[cfg(test)]
    fn numbers(&self) -> [i32; 3] {
        self.numbers
    }
}

impl fmt::Display for Version {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.text)
    }
}

/// `latest` новее `current`: `(newer, current_known)`. Граница с Home Assistant: значение
/// сенсора обновления приходит строкой и может быть мусором.
#[must_use]
pub fn release_newer(latest: &str, current: &str) -> (bool, bool) {
    Version::parse(latest).map_or_else(
        || (false, Version::parse(current).is_some()),
        |latest| latest.is_newer_than(current),
    )
}

/// MAJOR.MINOR.PATCH из `s`, у которой может быть `-`-суффикс.
fn parse_numbers(s: &str) -> Option<[i32; 3]> {
    let s = s.as_bytes();
    let mut pos = usize::from(s.first() == Some(&b'v'));
    let mut v = [0; 3];
    for (i, n) in v.iter_mut().enumerate() {
        if i > 0 {
            if s.get(pos) != Some(&b'.') {
                return None;
            }
            pos += 1;
        }
        *n = scan_int(s, &mut pos)?;
    }
    match s.get(pos) {
        None | Some(b'-') => Some(v),
        Some(_) => None,
    }
}

/// `%d`: ведущие пробелы C, знак, хотя бы одна цифра; насыщение на границах `i32`.
fn scan_int(s: &[u8], pos: &mut usize) -> Option<i32> {
    let mut p = *pos;
    while s
        .get(p)
        .is_some_and(|&c| c == b' ' || (b'\t'..=b'\r').contains(&c))
    {
        p += 1;
    }
    let neg = s.get(p) == Some(&b'-');
    if matches!(s.get(p), Some(b'-' | b'+')) {
        p += 1;
    }
    let start = p;
    let mut n: i64 = 0;
    while let Some(d) = s.get(p).filter(|c| c.is_ascii_digit()) {
        n = (n * 10 + i64::from(d - b'0')).min(i64::from(i32::MAX) + 1);
        p += 1;
    }
    if p == start {
        return None;
    }
    *pos = p;
    let n = if neg { -n } else { n };
    Some(n.clamp(i64::from(i32::MIN), i64::from(i32::MAX)) as i32)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn versions_table_of_the_cxx_probe() {
        // Строки C++ `update_probe versions` (tests/golden/versions.txt).
        let cases = [
            ("v1.2.4", "v1.2.3", (true, true)),
            ("v1.2.3", "v1.2.3", (false, true)),
            ("v1.2.3", "v1.2.3-4-gabc1234", (false, true)),
            ("v1.3.0", "v1.2.3-4-gabc", (true, true)),
            ("v1.2.3", "v1.10.0", (false, true)),
            ("v1.2.3", "6ac30be", (true, false)),
            ("v2.0.0", "v1.9.9-dirty", (true, true)),
            ("junk", "v1.0.0", (false, true)),
        ];
        for (latest, current, want) in cases {
            assert_eq!(release_newer(latest, current), want, "{latest} {current}");
        }
    }

    #[test]
    fn parse_like_sscanf() {
        let numbers = |s| Version::parse(s).map(|v| v.numbers());
        assert_eq!(numbers("v1.2.3"), Some([1, 2, 3]));
        assert_eq!(numbers("1.2.3"), Some([1, 2, 3]));
        assert_eq!(numbers("v1.2.3-4-gabc"), Some([1, 2, 3]));
        assert_eq!(numbers("v1.2.3-dirty"), Some([1, 2, 3]));
        // `%d` пропускает пробелы и принимает знак.
        assert_eq!(numbers("v 1.+2.\t-3"), Some([1, 2, -3]));
        assert_eq!(numbers("v1.2.3x"), None);
        assert_eq!(numbers("v1.2.3.4"), None);
        assert_eq!(numbers("v1.2"), None);
        assert_eq!(numbers("vv1.2.3"), None);
        assert_eq!(numbers("v1..3"), None);
        assert_eq!(numbers(""), None);
        assert_eq!(numbers("v99999999999.0.0"), Some([i32::MAX, 0, 0]));
        // Считается только тройка: сборка после тега не старше тега.
        assert_eq!(release_newer("v1.2.3", "v1.2.2-7-gdeadbee"), (true, true));
        assert_eq!(release_newer("v1.2.3", "6ac30be-dirty"), (true, false));
        assert_eq!(release_newer("6ac30be", "6ac30be"), (false, false));
    }

    #[test]
    fn oci_tags_are_strict() {
        assert_eq!(
            Version::parse_tag("v1.2.3").map(|v| v.as_str().to_owned()),
            Some("v1.2.3".into())
        );
        for tag in ["1.2.3", "v1.2", "v1.2.3-rc1", "latest", "v1.2.x", ""] {
            assert_eq!(Version::parse_tag(tag), None, "{tag}");
        }
    }
}
