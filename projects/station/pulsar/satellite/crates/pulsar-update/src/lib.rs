#![deny(unsafe_code)]
#![deny(unsafe_op_in_unsafe_fn)]
#![deny(clippy::undocumented_unsafe_blocks)]

mod actor;
mod error;
mod http;
mod installer;
mod release;
mod source;
#[allow(unsafe_code)]
mod sys;
#[cfg(test)]
mod test_server;
#[cfg(test)]
mod test_source;
mod version;

pub use actor::{RequestId, UPDATE_REPO, UpdateEvent, UpdaterHandle, arch, spawn};
pub use error::UpdateError;
pub use http::Progress;
pub use release::{Digest, Layer, Release};
pub use source::{Oci, OciConfig, ReleaseSource};
pub use version::{Version, release_newer};

pub(crate) fn sync() {
    sys::sync();
}
