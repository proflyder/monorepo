//! `cp-selftest.exe`: loads the credential provider outside LogonUI.
//!
//! It changes nothing on the machine:
//! but `SetUsageScenario(CPUS_LOGON)` starts the single poller for a moment:
//! run it only when no wake grant is pending.
//!
//! The tool is Windows-only (see `main_windows.rs`); the stub below keeps host
//! `cargo clippy --all-targets`/`cargo test` builds working.

#[cfg(windows)]
mod main_windows;

#[cfg(windows)]
fn main() {
    std::process::exit(main_windows::run());
}

#[cfg(not(windows))]
fn main() {
    eprintln!("cp-selftest is a Windows-only tool");
    std::process::exit(2);
}
