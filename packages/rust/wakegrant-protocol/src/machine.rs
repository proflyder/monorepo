//! Machine identifier grammar from `PROTOCOL.md`.

/// Longest valid machine identifier.
pub const MACHINE_MAX_LEN: usize = 32;

/// `^[a-z0-9-]{1,32}$`.
#[must_use]
pub fn is_valid_machine(machine: &str) -> bool {
    !machine.is_empty()
        && machine.len() <= MACHINE_MAX_LEN
        && machine
            .bytes()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == b'-')
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn machine_grammar() {
        assert!(is_valid_machine("komp"));
        assert!(is_valid_machine("desk-2"));
        assert!(is_valid_machine(&"a".repeat(MACHINE_MAX_LEN)));
        assert!(!is_valid_machine(""));
        assert!(!is_valid_machine(&"a".repeat(MACHINE_MAX_LEN + 1)));
        assert!(!is_valid_machine("Komp"));
        assert!(!is_valid_machine("komp_1"));
        assert!(!is_valid_machine("komp 1"));
    }
}
