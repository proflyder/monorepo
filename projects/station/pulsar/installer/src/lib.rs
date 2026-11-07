//! `pulsar-install`: finds rooted Yandex Stations on the local network and installs or
//! updates pulsar.
//!
//! Rust port of the former Go installer; behaviour, messages and exit codes are 1:1.
//! Pure text/decision modules are separated from the `adb` process port so they are covered
//! by the golden fixtures in `testdata/`.

pub mod adb;
pub mod cli;
pub mod console;
pub mod deploy;
pub mod discover;
pub mod platformtools;
pub mod probe;
pub mod scripts;

pub use deploy::Station;
