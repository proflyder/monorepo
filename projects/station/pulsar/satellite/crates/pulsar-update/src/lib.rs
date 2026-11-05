//! Обновления pulsar из OCI-артефакта в GHCR: сущность «Firmware» в Home Assistant находит
//! последний релиз, читает версию и digest слоёв из манифеста, скачивает
//! `pulsar-<arch>.bin`, проверяет digest слоя, `SHA256SUMS` и запуск (`--version`), затем
//! подменяет установленный бинарник (прежний остаётся как `.prev`). После установки демон
//! завершается: `supervise.sh` запускает новый бинарник и откатывается на `.prev`, если тот
//! уходит в crash-loop. HTTPS — reqwest на rustls (ring) в потоке рантайма; файловая работа
//! идёт на blocking-потоках.

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

/// sync(2): подменённый бинарник и `.prev` доходят до флеша до выхода демона.
pub(crate) fn sync() {
    sys::sync();
}
