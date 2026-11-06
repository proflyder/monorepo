use thiserror::Error;

/// What a grant may do. A logon grant is only valid on a cold boot; an unlock
/// grant is for a running but locked workstation. The purpose is part of every
/// purpose-aware MAC message, so one can never be used for the other.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub enum Purpose {
    #[default]
    Logon,
    Unlock,
}

#[derive(Debug, Error, PartialEq, Eq)]
#[error("unknown purpose")]
pub struct PurposeError;

impl Purpose {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Logon => "logon",
            Self::Unlock => "unlock",
        }
    }

    pub fn parse(value: &str) -> Result<Self, PurposeError> {
        match value {
            "logon" => Ok(Self::Logon),
            "unlock" => Ok(Self::Unlock),
            _ => Err(PurposeError),
        }
    }
}

impl std::fmt::Display for Purpose {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(self.as_str())
    }
}
