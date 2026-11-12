//! SSH bridge to the bb home-dispatcher.
//!
//! Ports `solutions/mcp-gateway/internal/bb/{ssh,hostkey}.go`. Every call is one
//! SSH session running the forced command `bb-dispatch`: one JSON object on
//! stdin, one JSON answer on stdout (bounded at 1 MiB). The gateway owns only an
//! ed25519 key restricted to that bridge and a pinned `known_hosts` file.
//!
//! The transport itself lives in the shared [`forced_ssh`] crate; this module
//! keeps the gateway's fixed command, limits and [`crate::ErrorKind`] mapping so
//! its callers and logs are unchanged.

use std::fmt;
use std::path::PathBuf;
use std::time::Duration;

pub use forced_ssh::{Answer, ErrorClass, check_host_key, host_key_algorithms};

/// Default bound for one bridge call, including the SSH handshake.
pub const DEFAULT_TIMEOUT: Duration = Duration::from_secs(15);
/// Upper bound for the bridge answer. The bridge itself limits responses to
/// 1 MiB; a longer answer is truncated at the same boundary.
pub const MAX_ANSWER_BYTES: usize = 1 << 20;
const COMMAND: &str = "bb-dispatch";

/// One bridge call failure. It never contains key material or a payload.
#[derive(Debug)]
pub struct Error(forced_ssh::Error);

impl Error {
    /// The failed phase.
    #[must_use]
    pub fn class(&self) -> ErrorClass {
        self.0.class()
    }

    /// Whether a retry may succeed. A timeout, dial, handshake and session
    /// failure is transient; a rejected host key, key or file is not.
    #[must_use]
    pub fn kind(&self) -> crate::ErrorKind {
        match self.0.kind() {
            forced_ssh::ErrorKind::Transient => crate::ErrorKind::Transient,
            forced_ssh::ErrorKind::Permanent => crate::ErrorKind::Permanent,
        }
    }
}

impl fmt::Display for Error {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "bb ssh {}: {}",
            self.0.class().as_str(),
            self.0.detail()
        )
    }
}

impl std::error::Error for Error {}

/// Options for one bridge runner.
#[derive(Debug, Clone)]
pub struct Options {
    /// `host:port` of the bb host.
    pub address: String,
    /// The user that owns the restricted key (normally `bb`).
    pub user: String,
    /// An unencrypted ed25519 private key.
    pub key_file: PathBuf,
    /// Pins the bb host key.
    pub known_hosts_file: PathBuf,
    /// Per-call timeout; [`DEFAULT_TIMEOUT`] when `None`.
    pub timeout: Option<Duration>,
}

/// Executes one forced command per call.
#[derive(Debug, Clone)]
pub struct SshRunner {
    inner: forced_ssh::SshRunner,
}

impl SshRunner {
    /// Build a runner. Files are read on every call, like the Go client.
    #[must_use]
    pub fn new(options: Options) -> Self {
        Self {
            inner: forced_ssh::SshRunner::new(forced_ssh::Options {
                address: options.address,
                user: options.user,
                key_file: options.key_file,
                known_hosts_file: options.known_hosts_file,
                command: COMMAND.to_owned(),
                timeout: options.timeout,
                max_answer_bytes: Some(MAX_ANSWER_BYTES),
                max_stderr_bytes: None,
            }),
        }
    }

    /// Run one bridge call.
    pub async fn run(&self, payload: &[u8]) -> Result<Answer, Error> {
        self.inner.run(payload).await.map_err(Error)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_bridge_command_and_limits_are_fixed() {
        assert_eq!(COMMAND, "bb-dispatch");
        assert_eq!(DEFAULT_TIMEOUT, Duration::from_secs(15));
        assert_eq!(MAX_ANSWER_BYTES, 1 << 20);
    }

    #[test]
    fn pinned_host_keys_still_verify_through_the_shared_transport() {
        let ed = russh::keys::PrivateKey::random(&mut rand::rng(), russh::keys::Algorithm::Ed25519)
            .expect("generate key");
        let line = format!(
            "example.test:22 {}\n",
            ed.public_key().to_openssh().expect("public key")
        );
        assert!(check_host_key(&line, "example.test:22", ed.public_key()));
    }
}
