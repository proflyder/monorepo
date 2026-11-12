//! The limited agent client: one forced command per call.
//!
//! The transport is a trait so the sync state machine can be tested without an
//! SSH server. Production uses [`SshAgent`] over [`forced_ssh`].

use std::collections::BTreeMap;
use std::future::Future;
use std::pin::Pin;
use std::time::Duration;

use base64::Engine as _;

use crate::config::SshSettings;
use crate::protocol::{self, AgentHash, DesiredFile, PROTOCOL_VERSION};

/// Bound for one agent call, including apply of a small YAML set.
pub const AGENT_TIMEOUT: Duration = Duration::from_secs(120);
/// Upper bound for an agent answer. The `hashes` answer is the largest one.
pub const AGENT_MAX_ANSWER_BYTES: usize = 1 << 20;

/// Boxed agent call.
pub type AgentFuture<'a, T> = Pin<Box<dyn Future<Output = Result<T, AgentError>> + Send + 'a>>;

/// The raw answer of one agent verb.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentAnswer {
    /// Stdout; the agent protocol lives here.
    pub stdout: Vec<u8>,
    /// SSH exit status, if the server reported one.
    pub exit_code: Option<u32>,
}

/// A failure while talking to the agent.
#[derive(Debug, thiserror::Error)]
pub enum AgentError {
    /// The SSH transport failed.
    #[error("agent transport: {0}")]
    Transport(#[from] forced_ssh::Error),
    /// The agent rejected the request and exited non-zero.
    #[error("agent exited with status {0:?}")]
    Exit(Option<u32>),
    /// Stdout was not the agreed protocol.
    #[error("agent protocol: {0}")]
    Protocol(#[from] protocol::ProtocolError),
    /// A backup id was empty.
    #[error("agent returned an empty backup id")]
    EmptyBackup,
}

/// Executes one agent verb.
pub trait Agent: Send + Sync {
    /// Run `verb` with `stdin` and return stdout. Errors must not contain file
    /// contents or tokens.
    fn call<'a>(&'a self, verb: &'a str, stdin: &'a [u8]) -> AgentFuture<'a, AgentAnswer>;
}

/// The production agent over pinned SSH.
#[derive(Debug, Clone)]
pub struct SshAgent {
    runner: forced_ssh::SshRunner,
}

impl SshAgent {
    /// Build an agent from the SSH settings.
    #[must_use]
    pub fn new(ssh: &SshSettings) -> Self {
        Self {
            runner: forced_ssh::SshRunner::new(forced_ssh::Options {
                address: ssh.address.clone(),
                user: ssh.user.clone(),
                key_file: ssh.key_file.clone(),
                known_hosts_file: ssh.known_hosts_file.clone(),
                command: "version".to_owned(),
                timeout: Some(AGENT_TIMEOUT),
                max_answer_bytes: Some(AGENT_MAX_ANSWER_BYTES),
                max_stderr_bytes: None,
            }),
        }
    }
}

impl Agent for SshAgent {
    fn call<'a>(&'a self, verb: &'a str, stdin: &'a [u8]) -> AgentFuture<'a, AgentAnswer> {
        Box::pin(async move {
            let answer = self.runner.run_command(verb, stdin).await?;
            Ok(AgentAnswer {
                stdout: answer.stdout,
                exit_code: answer.exit_code,
            })
        })
    }
}

fn success(answer: AgentAnswer) -> Result<Vec<u8>, AgentError> {
    if answer.exit_code == Some(0) {
        Ok(answer.stdout)
    } else {
        Err(AgentError::Exit(answer.exit_code))
    }
}

/// Ask the agent for its protocol version.
pub async fn version(agent: &dyn Agent) -> Result<u32, AgentError> {
    let stdout = success(agent.call("version", b"").await?)?;
    protocol::parse_version(&stdout).map_err(AgentError::from)
}

/// Read hashes for every managed path.
pub async fn hashes(
    agent: &dyn Agent,
    paths: &[String],
) -> Result<BTreeMap<String, AgentHash>, AgentError> {
    let mut stdin = String::new();
    for path in paths {
        stdin.push_str(path);
        stdin.push('\n');
    }
    let stdout = success(agent.call("hashes", stdin.as_bytes()).await?)?;
    protocol::parse_hashes(&stdout, paths).map_err(AgentError::from)
}

/// Deliver the exact bytes and return the backup id.
pub async fn apply(
    agent: &dyn Agent,
    files: &BTreeMap<String, DesiredFile>,
) -> Result<String, AgentError> {
    let mut stdin = String::new();
    for (path, file) in files {
        stdin.push_str(path);
        stdin.push(' ');
        stdin.push_str(&file.sha256);
        stdin.push(' ');
        stdin.push_str(&base64::engine::general_purpose::STANDARD.encode(&file.content));
        stdin.push('\n');
    }
    let stdout = success(agent.call("apply", stdin.as_bytes()).await?)?;
    let id = String::from_utf8(stdout)
        .map_err(|_| protocol::ProtocolError::Answer("backup id is not UTF-8".to_owned()))?
        .trim()
        .to_owned();
    if !protocol::is_backup_id(&id) {
        return Err(AgentError::EmptyBackup);
    }
    Ok(id)
}

/// Restore one backup.
pub async fn restore(agent: &dyn Agent, id: &str) -> Result<(), AgentError> {
    success(agent.call(&format!("restore {id}"), b"").await?)?;
    Ok(())
}

/// Keep only the newest `keep` backups.
pub async fn prune(agent: &dyn Agent, keep: u32) -> Result<(), AgentError> {
    success(agent.call(&format!("prune {keep}"), b"").await?)?;
    Ok(())
}

/// The protocol version this binary expects.
#[must_use]
pub const fn expected_version() -> u32 {
    PROTOCOL_VERSION
}
