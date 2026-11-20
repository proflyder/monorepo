use std::fmt;
use std::path::PathBuf;
use std::time::Duration;

pub use forced_ssh::{Answer, ErrorClass, check_host_key, host_key_algorithms};

pub const DEFAULT_TIMEOUT: Duration = Duration::from_secs(15);
pub const MAX_ANSWER_BYTES: usize = 1 << 20;
const COMMAND: &str = "bb-dispatch";

#[derive(Debug)]
pub struct Error(forced_ssh::Error);

impl Error {
    #[must_use]
    pub fn class(&self) -> ErrorClass {
        self.0.class()
    }

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

#[derive(Debug, Clone)]
pub struct Options {
    pub address: String,
    pub user: String,
    pub key_file: PathBuf,
    pub known_hosts_file: PathBuf,
    pub timeout: Option<Duration>,
}

#[derive(Debug, Clone)]
pub struct SshRunner {
    inner: forced_ssh::SshRunner,
}

impl SshRunner {
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
