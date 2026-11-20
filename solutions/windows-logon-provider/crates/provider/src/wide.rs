pub fn widen(text: &str) -> Vec<u16> {
    text.encode_utf16().collect()
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WideNul(Vec<u16>);

impl WideNul {
    pub fn new(text: &str) -> Self {
        Self::from_wide(&widen(text))
    }

    pub fn from_wide(wide: &[u16]) -> Self {
        let mut units: Vec<u16> = wide.iter().copied().take_while(|&unit| unit != 0).collect();
        units.push(0);
        Self(units)
    }

    pub fn text(&self) -> &[u16] {
        let end = self.0.len().saturating_sub(1);
        &self.0[..end]
    }

    pub fn into_vec(self) -> Vec<u16> {
        self.0
    }

    #[cfg(windows)]
    pub fn as_pcwstr(&self) -> windows_core::PCWSTR {
        windows_core::PCWSTR(self.0.as_ptr())
    }
}

#[cfg(test)]
impl WideNul {
    pub fn as_slice(&self) -> &[u16] {
        &self.0
    }
}

#[cfg(test)]
mod tests {
    use super::{WideNul, widen};

    #[test]
    fn wide_nul_has_exactly_one_terminator_at_the_end() {
        let value = WideNul::new("hello");
        assert_eq!(value.as_slice(), &[0x68, 0x65, 0x6c, 0x6c, 0x6f, 0]);
        assert_eq!(value.text().len(), 5);
        assert!(!value.text().is_empty());
        assert_eq!(value.as_slice()[..value.text().len()], *value.text());
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
        assert_eq!(value.text().len(), 0);
        assert!(value.text().is_empty());
    }

    #[test]
    fn non_ascii_text_is_converted_by_utf16_units() {
        let value = WideNul::new("Автовход");
        assert_eq!(value.text().len(), 8);
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
