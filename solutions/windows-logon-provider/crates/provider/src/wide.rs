//! UTF-16 helpers for the Win32 `...W` entry points.
//!
//! Most Win32 wide-char APIs take a `PCWSTR`, i.e. a pointer to a
//! NUL-terminated UTF-16 string. A pointer into a `Vec<u16>` that was built
//! without a terminator makes them read past the allocation, so [`WideNul`]
//! makes the terminator part of the type: every `WideNul` ends with exactly one
//! NUL and cannot be built from anything else. Code that needs the length
//! without a terminator (KERB packing, comparisons) uses [`widen`] directly.
//!
//! The module is host-portable: it uses `str::encode_utf16`, not Win32.

/// UTF-16 without a terminating NUL, for explicit-length uses.
pub fn widen(text: &str) -> Vec<u16> {
    text.encode_utf16().collect()
}

/// UTF-16 with exactly one terminating NUL, ready for `PCWSTR`.
///
/// Input NUL units are dropped while converting, so the result always has a
/// single terminator at the very end (and no interior one).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WideNul(Vec<u16>);

impl WideNul {
    /// Converts `text`; the result always ends with one NUL.
    pub fn new(text: &str) -> Self {
        Self::from_wide(&widen(text))
    }

    /// Wraps already-widened text, adding the terminator and dropping any NUL
    /// units at or after the first one.
    pub fn from_wide(wide: &[u16]) -> Self {
        let mut units: Vec<u16> = wide.iter().copied().take_while(|&unit| unit != 0).collect();
        units.push(0);
        Self(units)
    }

    /// The buffer including its single trailing NUL.
    pub fn as_slice(&self) -> &[u16] {
        &self.0
    }

    /// The text without the terminator.
    pub fn text(&self) -> &[u16] {
        let end = self.0.len().saturating_sub(1);
        &self.0[..end]
    }

    /// Units of text, excluding the terminator.
    pub fn len(&self) -> usize {
        self.text().len()
    }

    /// No text at all: the buffer is just the terminator (`[0]`).
    pub fn is_empty(&self) -> bool {
        self.text().is_empty()
    }

    /// Bypasses the `PCWSTR` and gives up the buffer, e.g. to a wiper.
    pub fn into_vec(self) -> Vec<u16> {
        self.0
    }

    /// Mutable access for wiping secrets; the terminator is still present.
    pub fn as_mut_slice(&mut self) -> &mut [u16] {
        &mut self.0
    }

    /// Pointer to the NUL-terminated string.
    pub fn as_ptr(&self) -> *const u16 {
        self.0.as_ptr()
    }

    /// A `PCWSTR` over the NUL-terminated buffer.
    #[cfg(windows)]
    pub fn as_pcwstr(&self) -> windows_core::PCWSTR {
        windows_core::PCWSTR(self.0.as_ptr())
    }
}

impl From<&str> for WideNul {
    fn from(text: &str) -> Self {
        Self::new(text)
    }
}

#[cfg(test)]
mod tests {
    use super::{WideNul, widen};

    #[test]
    fn wide_nul_has_exactly_one_terminator_at_the_end() {
        let value = WideNul::new("hello");
        assert_eq!(value.as_slice(), &[0x68, 0x65, 0x6c, 0x6c, 0x6f, 0]);
        assert_eq!(value.len(), 5);
        assert!(!value.is_empty());
        assert_eq!(value.as_slice()[..value.len()], *value.text());
        assert_eq!(
            value.as_slice().iter().filter(|&&unit| unit == 0).count(),
            1
        );
        assert_eq!(value.as_slice().last(), Some(&0));
    }

    #[test]
    fn the_empty_string_is_just_the_terminator() {
        let value = WideNul::new("");
        assert_eq!(value.as_slice(), &[0]);
        assert_eq!(value.len(), 0);
        assert!(value.is_empty());
    }

    #[test]
    fn non_ascii_text_is_converted_by_utf16_units() {
        let value = WideNul::new("Автовход");
        assert_eq!(value.len(), 8);
        assert_eq!(value.text()[0], 0x0410);
        assert_eq!(value.as_slice().last(), Some(&0));
        assert_eq!(String::from_utf16(value.text()).unwrap(), "Автовход");
    }

    #[test]
    fn already_terminated_input_is_not_double_terminated() {
        let value = WideNul::from_wide(&[0x41, 0x42, 0]);
        assert_eq!(value.as_slice(), &[0x41, 0x42, 0]);
    }

    #[test]
    fn interface_nul_units_are_dropped() {
        let value = WideNul::from_wide(&[0x41, 0, 0x42, 0]);
        assert_eq!(value.as_slice(), &[0x41, 0]);
    }

    #[test]
    fn widen_has_no_terminator() {
        assert_eq!(widen("ab"), vec![0x61, 0x62]);
        assert!(widen("").is_empty());
    }
}
