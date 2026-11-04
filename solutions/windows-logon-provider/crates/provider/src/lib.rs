//! Windows Credential Provider (in-proc COM server) for Ricardo Komp Logon.
//!
//! This is the Rust port of the C++ provider in `solutions/windows-logon-provider/src`.
//! The C++ tree stays the reference implementation; behaviour, log lines,
//! registry/secret formats, CLSID and the two DLL exports are the same.
//!
//! # Layout
//!
//! - [`dll`] — `DllGetClassObject`/`DllCanUnloadNow`, the class factory and the
//!   DLL/worker reference counters.
//! - [`provider`] — `ICredentialProvider`, `ICredentialProviderSetUserArray` and
//!   the shared [`provider::Session`].
//! - [`credential`] — the single auto-logon tile and `GetSerialization`.
//! - [`worker`] — the one background "/v1/challenge -> /v1/claim" poller.
//! - [`config`] — registry configuration, the DPAPI secret and `FailCount`.
//! - [`http`] — the minimal WinHTTP JSON POST client.
//! - [`log`] — the append-only, size-rotated logon log.
//! - [`kerb`] — portable packing of `KERB_INTERACTIVE_UNLOCK_LOGON`.
//! - [`ffi_guard`] — `catch_unwind` at every `extern "system"` boundary.
//!
//! # Portability
//!
//! The crate is a `cdylib` for `x86_64-pc-windows-gnu`. Everything that does not
//! need Windows (log sanitising/rotation, KERB packing, retry/budget decisions,
//! `FailCount` arithmetic) is plain safe Rust and is covered by host tests.
//! Windows-only code is behind `#[cfg(windows)]`; `cargo test`/`cargo clippy` on
//! the build host therefore only exercises that portable core, while the Bazel
//! Windows build compiles and clippy-checks the real DLL.
//!
//! # Panic policy
//!
//! The release profile keeps `panic = "unwind"`. Every COM method body and both
//! DLL exports are wrapped in [`ffi_guard::guard`], so a panic is turned into a
//! safe return value and never unwinds across the `extern "system"` boundary
//! into LogonUI.

// The Bazel target names the crate `RicardoKompLogon` so rustc emits
// `RicardoKompLogon.dll`; allow the crate-name style lint that this implies.
#![allow(non_snake_case)]

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
