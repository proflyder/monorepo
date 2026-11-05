//! Status values shared by the GATT status characteristic, the status file and
//! the HTTP control API.

use std::fmt;

/// Explicit daemon status. The wire form is the `Display` implementation.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Status {
    Idle,
    Ready,
    Connecting,
    Connected,
    DryRunOk,
    Error(String),
}

impl Status {
    /// `ERROR:<CODE>` with a short machine-readable code.
    pub fn error(code: impl Into<String>) -> Self {
        Self::Error(format!("ERROR:{}", code.into()))
    }

    pub fn as_str(&self) -> &str {
        match self {
            Self::Idle => "IDLE",
            Self::Ready => "READY",
            Self::Connecting => "CONNECTING",
            Self::Connected => "CONNECTED",
            Self::DryRunOk => "DRY_RUN_OK",
            Self::Error(code) => code,
        }
    }
}

impl fmt::Display for Status {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wire_values_are_stable() {
        assert_eq!(Status::Idle.to_string(), "IDLE");
        assert_eq!(Status::Ready.to_string(), "READY");
        assert_eq!(Status::Connecting.to_string(), "CONNECTING");
        assert_eq!(Status::Connected.to_string(), "CONNECTED");
        assert_eq!(Status::DryRunOk.to_string(), "DRY_RUN_OK");
        assert_eq!(Status::error("BAD_TOKEN").to_string(), "ERROR:BAD_TOKEN");
    }
}
