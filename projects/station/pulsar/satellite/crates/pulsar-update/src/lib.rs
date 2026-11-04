//! pulsar updates from an OCI artifact in GHCR (Home Assistant "Firmware" update entity): finds the
//! latest image, reads its version and layer digests from the manifest, downloads
//! `pulsar-<arch>.bin`, checks the layer digest, the image's `SHA256SUMS` and that it runs here
//! (`--version`), then replaces the installed binary (kept as `.prev`). A plain `owner/repo` still
//! uses GitHub Releases. The caller exits the daemon afterwards; `supervise.sh` starts the new
//! binary and rolls back to `.prev` if it crash-loops. HTTPS is rustls on the runtime thread; file
//! work runs on blocking threads.
#![deny(unsafe_code)]
#![deny(unsafe_op_in_unsafe_fn)]
#![deny(clippy::undocumented_unsafe_blocks)]

/// For update_probe only, not part of the crate's public API.
#[doc(hidden)]
pub mod https;
mod json;
#[allow(unsafe_code)]
mod sys;
#[cfg(test)]
mod test_server;
mod updater;
mod version;

/// Источник обновлений демона — публичный OCI-пакет в GHCR, который публикует монорепозиторий
/// (`proflyder/monorepo`); upstream им никогда не бывает.
pub const UPDATE_REPO: &str = "ghcr.io/proflyder/pulsar-firmware";

pub use updater::{Layer, Release, UpdateEvent, UpdaterHandle, arch, spawn};
pub use version::release_newer;
