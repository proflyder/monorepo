//! Log-safe protocol error.
//!
//! Every message is a short static phrase. Secrets, keys, passwords, URLs with
//! paths and MAC values are never part of it, so the value can be written to
//! the logon log as is.

use core::fmt;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Error(&'static str);

impl Error {
    pub(crate) const fn new(detail: &'static str) -> Self {
        Self(detail)
    }

    /// Short, log-safe reason.
    pub fn detail(self) -> &'static str {
        self.0
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.0)
    }
}

impl std::error::Error for Error {}
