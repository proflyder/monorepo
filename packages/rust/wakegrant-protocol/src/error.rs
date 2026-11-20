#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
#[error("{0}")]
pub struct Error(&'static str);

impl Error {
    pub const fn new(detail: &'static str) -> Self {
        Self(detail)
    }

    pub fn detail(self) -> &'static str {
        self.0
    }
}
