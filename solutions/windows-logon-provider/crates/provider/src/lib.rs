//! Windows Credential Provider (in-proc COM server) for Ricardo Komp Logon.
//!
//! Behaviour, log lines, registry/secret formats, the CLSID and the two DLL
//! exports are the contract in `README.md`, `PROTOCOL.md` and the install
//! scripts. Portable modules run in host tests; Windows-only code is behind
//! `#[cfg(windows)]` and uses the generated [`bindings`]. Every
//! `extern "system"` boundary goes through [`ffi_guard::guard`].

// The Bazel target renames the crate to `RicardoKompLogon.dll`.
#![allow(non_snake_case)]

#[cfg(windows)]
pub mod bindings;
pub mod config;
pub mod credential;
pub mod ffi_guard;
pub mod kerb;
pub mod log;
pub mod provider;
pub mod worker;

#[cfg(windows)]
pub mod dll;
#[cfg(windows)]
pub mod http;
