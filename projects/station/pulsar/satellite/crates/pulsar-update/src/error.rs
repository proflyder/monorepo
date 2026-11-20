#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum UpdateError {
    #[error("{0}")]
    Fatal(String),
    #[error("{0}")]
    Retry(String),
    #[error("cancelled")]
    Cancelled,
}

impl UpdateError {
    #[must_use]
    pub fn fatal(&self) -> bool {
        matches!(self, Self::Fatal(_))
    }

    pub(crate) fn fatal_text(text: impl Into<String>) -> Self {
        Self::Fatal(text.into())
    }

    pub(crate) fn retry(text: impl Into<String>) -> Self {
        Self::Retry(text.into())
    }
}
