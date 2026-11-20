use std::collections::BTreeMap;
use std::future::Future;
use std::pin::Pin;
use std::time::Duration;

use base64::Engine as _;

use crate::config::SshSettings;
use crate::protocol::{self, AgentHash, DesiredFile, PROTOCOL_VERSION};

pub const AGENT_TIMEOUT: Duration = Duration::from_secs(120);
pub const AGENT_MAX_ANSWER_BYTES: usize = 1 << 20;

pub type AgentFuture<'a, T> = Pin<Box<dyn Future<Output = Result<T, AgentError>> + Send + 'a>>;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentAnswer {
    pub stdout: Vec<u8>,
    pub exit_code: Option<u32>,
}

#[derive(Debug, thiserror::Error)]
pub enum AgentError {
    #[error("agent transport: {0}")]
    Transport(#[from] forced_ssh::Error),
    #[error("agent exited with status {0:?}")]
    Exit(Option<u32>),
    #[error("agent protocol: {0}")]
    Protocol(#[from] protocol::ProtocolError),
    #[error("agent returned an empty backup id")]
    EmptyBackup,
}

pub trait Agent: Send + Sync {
    fn call<'a>(&'a self, verb: &'a str, stdin: &'a [u8]) -> AgentFuture<'a, AgentAnswer>;
}

#[derive(Debug, Clone)]
pub struct SshAgent {
    runner: forced_ssh::SshRunner,
}

impl SshAgent {
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

pub async fn version(agent: &dyn Agent) -> Result<u32, AgentError> {
    let stdout = success(agent.call("version", b"").await?)?;
    protocol::parse_version(&stdout).map_err(AgentError::from)
}

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

pub async fn restore(agent: &dyn Agent, id: &str) -> Result<(), AgentError> {
    success(agent.call(&format!("restore {id}"), b"").await?)?;
    Ok(())
}

pub async fn prune(agent: &dyn Agent, keep: u32) -> Result<(), AgentError> {
    success(agent.call(&format!("prune {keep}"), b"").await?)?;
    Ok(())
}

#[must_use]
pub const fn expected_version() -> u32 {
    PROTOCOL_VERSION
}
