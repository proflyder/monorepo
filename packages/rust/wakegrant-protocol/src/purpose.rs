//! The two wakegrant purposes: `logon` after a cold boot and `unlock` for a
//! running but locked workstation.
//!
//! A purpose is part of every purpose-aware MAC message and of the claim body,
//! so a grant of one purpose can never be used for the other.

/// Grant purpose, exactly as it travels on the wire.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Purpose {
    Logon,
    Unlock,
}

impl Purpose {
    /// Wire value: `logon` or `unlock`.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Logon => "logon",
            Self::Unlock => "unlock",
        }
    }

    /// Parses the wire value; anything else is unknown.
    pub fn parse(text: &str) -> Option<Self> {
        match text {
            "logon" => Some(Self::Logon),
            "unlock" => Some(Self::Unlock),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::Purpose;

    #[test]
    fn purposes_round_trip_through_their_wire_values() {
        assert_eq!(Purpose::Logon.as_str(), "logon");
        assert_eq!(Purpose::Unlock.as_str(), "unlock");
        assert_eq!(Purpose::parse("logon"), Some(Purpose::Logon));
        assert_eq!(Purpose::parse("unlock"), Some(Purpose::Unlock));
        assert_eq!(Purpose::parse("LOGON"), None);
        assert_eq!(Purpose::parse(""), None);
    }
}
