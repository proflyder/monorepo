//! Log-safe protocol error: a short static phrase, never a secret, key,
//! password or MAC, so it can be written to the logon log as is.

/// Protocol failure with a fixed, log-safe reason.
#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
#[error("{0}")]
pub struct Error(&'static str);

impl Error {
    pub const fn new(detail: &'static str) -> Self {
        Self(detail)
    }

    /// Short, log-safe reason.
    pub fn detail(self) -> &'static str {
        self.0
    }
}
