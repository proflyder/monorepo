//! SSH bridge to the bb home-dispatcher.
//!
//! Ports `solutions/mcp-gateway/internal/bb/{ssh,hostkey}.go`. Every call is one
//! SSH session running the forced command `bb-dispatch`: one JSON object on
//! stdin, one JSON answer on stdout (bounded at 1 MiB). The gateway owns only an
//! ed25519 key restricted to that bridge and a pinned `known_hosts` file.
//!
//! `known_hosts` pinning is the security boundary. The client offers only the
//! host-key algorithms of the key types pinned for the address, so a server
//! cannot negotiate a type that could never pass verification. Host patterns
//! follow OpenSSH semantics: comma-separated lists, `!` negation, `*`/`?`
//! wildcards and `|1|` hashed names.

use std::borrow::Cow;
use std::fmt;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use hmac::{Hmac, KeyInit, Mac};
use russh::client;
use russh::keys::ssh_key::known_hosts::{Entry, HostPatterns, KnownHosts, Marker};
use russh::keys::{
    Algorithm, HashAlg, PrivateKeyWithHashAlg, PublicKey, PublicKeyOrCertificate, load_secret_key,
};
use russh::{ChannelMsg, Disconnect, Preferred};
use sha1::Sha1;

use crate::ErrorKind;

/// Default bound for one bridge call, including the SSH handshake.
pub const DEFAULT_TIMEOUT: Duration = Duration::from_secs(15);
/// Upper bound for the bridge answer. The bridge itself limits responses to
/// 1 MiB; a longer answer is truncated at the same boundary.
pub const MAX_ANSWER_BYTES: usize = 1 << 20;
const MAX_STDERR_BYTES: usize = 4 << 10;
const COMMAND: &str = "bb-dispatch";

/// The phase that failed, for logs and callers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorClass {
    /// The private key could not be read or parsed.
    Key,
    /// The `known_hosts` file could not be read or parsed.
    KnownHosts,
    /// The TCP connection could not be established.
    Dial,
    /// The host key is unpinned, mismatched or revoked.
    HostKey,
    /// The server rejected the public key.
    Auth,
    /// The SSH handshake failed after the host key was accepted.
    Handshake,
    /// A channel or exec failed.
    Session,
    /// The whole call exceeded its timeout.
    Timeout,
}

impl ErrorClass {
    /// Stable class name for a structured log line.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Key => "key",
            Self::KnownHosts => "known_hosts",
            Self::Dial => "dial",
            Self::HostKey => "hostkey",
            Self::Auth => "auth",
            Self::Handshake => "handshake",
            Self::Session => "session",
            Self::Timeout => "timeout",
        }
    }
}

/// One bridge call failure. It never contains key material or a payload.
#[derive(Debug)]
pub struct Error {
    class: ErrorClass,
    detail: String,
}

impl Error {
    fn new(class: ErrorClass, detail: impl Into<String>) -> Self {
        Self {
            class,
            detail: detail.into(),
        }
    }

    /// The failed phase.
    #[must_use]
    pub fn class(&self) -> ErrorClass {
        self.class
    }

    /// Whether a retry may succeed. A timeout, dial, handshake and session
    /// failure is transient; a rejected host key, key or file is not.
    #[must_use]
    pub fn kind(&self) -> ErrorKind {
        match self.class {
            ErrorClass::Dial
            | ErrorClass::Handshake
            | ErrorClass::Session
            | ErrorClass::Timeout => ErrorKind::Transient,
            ErrorClass::Key | ErrorClass::KnownHosts | ErrorClass::HostKey | ErrorClass::Auth => {
                ErrorKind::Permanent
            }
        }
    }
}

impl fmt::Display for Error {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "bb ssh {}: {}", self.class.as_str(), self.detail)
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

/// The answer of one bridge call.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Answer {
    /// Raw stdout, truncated at [`MAX_ANSWER_BYTES`].
    pub stdout: Vec<u8>,
    /// Non-zero exit codes still carry a JSON answer from the bridge.
    pub exit_code: Option<u32>,
}

/// Executes one forced command per call.
#[derive(Debug, Clone)]
pub struct SshRunner {
    options: Options,
}

impl SshRunner {
    /// Build a runner. Files are read on every call, like the Go client.
    #[must_use]
    pub fn new(options: Options) -> Self {
        Self { options }
    }

    /// Run one bridge call.
    pub async fn run(&self, payload: &[u8]) -> Result<Answer, Error> {
        let timeout = self.options.timeout.unwrap_or(DEFAULT_TIMEOUT);
        tokio::time::timeout(timeout, self.call(payload))
            .await
            .map_err(|_| Error::new(ErrorClass::Timeout, format!("call exceeded {timeout:?}")))?
    }

    async fn call(&self, payload: &[u8]) -> Result<Answer, Error> {
        let key = load_secret_key(&self.options.key_file, None)
            .map_err(|error| Error::new(ErrorClass::Key, format!("read or parse key: {error}")))?;
        let known_hosts =
            std::fs::read_to_string(&self.options.known_hosts_file).map_err(|error| {
                Error::new(ErrorClass::KnownHosts, format!("read known_hosts: {error}"))
            })?;
        let entries = parse_known_hosts(&known_hosts)?;
        let (host, port) = split_address(&self.options.address);
        // Offer only host key types this host actually pins, so the server
        // cannot pick a key we would always reject.
        let algorithms = host_key_algorithms(&known_hosts, &self.options.address)?;

        let mut config = client::Config {
            inactivity_timeout: Some(timeout_of(&self.options)),
            ..client::Config::default()
        };
        if !algorithms.is_empty() {
            config.preferred = Preferred {
                key: Cow::Owned(algorithms),
                ..Preferred::default()
            };
        }
        let handler = Handler {
            entries,
            host,
            port,
        };
        let mut session = client::connect(Arc::new(config), &self.options.address, handler)
            .await
            .map_err(|error| match error {
                ConnectError::HostKey => Error::new(
                    ErrorClass::HostKey,
                    "host key is not pinned or does not match",
                ),
                ConnectError::Ssh(russh::Error::IO(_)) => {
                    Error::new(ErrorClass::Dial, "connect failed")
                }
                ConnectError::Ssh(error) => Error::new(ErrorClass::Handshake, format!("{error:?}")),
            })?;

        let auth = session
            .authenticate_publickey(
                self.options.user.clone(),
                PrivateKeyWithHashAlg::new(Arc::new(key), None),
            )
            .await
            .map_err(|error| Error::new(ErrorClass::Auth, format!("{error:?}")))?;
        if !auth.success() {
            let _ = session
                .disconnect(Disconnect::ByApplication, "", "English")
                .await;
            return Err(Error::new(ErrorClass::Auth, "public key rejected"));
        }

        let mut channel = session
            .channel_open_session()
            .await
            .map_err(|error| Error::new(ErrorClass::Session, format!("{error:?}")))?;
        channel
            .exec(true, COMMAND)
            .await
            .map_err(|error| Error::new(ErrorClass::Session, format!("exec: {error:?}")))?;
        channel
            .data(payload)
            .await
            .map_err(|error| Error::new(ErrorClass::Session, format!("stdin: {error:?}")))?;
        channel
            .eof()
            .await
            .map_err(|error| Error::new(ErrorClass::Session, format!("stdin eof: {error:?}")))?;

        let mut stdout = Vec::new();
        let mut stderr = Vec::new();
        let mut exit_code = None;
        while let Some(message) = channel.wait().await {
            match message {
                ChannelMsg::Data { data } => extend_limited(&mut stdout, &data, MAX_ANSWER_BYTES),
                ChannelMsg::ExtendedData { data, ext: 1 } => {
                    extend_limited(&mut stderr, &data, MAX_STDERR_BYTES);
                }
                ChannelMsg::ExitStatus { exit_status } => exit_code = Some(exit_status),
                ChannelMsg::Close => break,
                _ => {}
            }
        }
        let _ = session
            .disconnect(Disconnect::ByApplication, "", "English")
            .await;
        // stderr is diagnostics only and may contain no secrets; it is dropped
        // so a bridge warning can never be mistaken for the answer.
        drop(stderr);
        Ok(Answer { stdout, exit_code })
    }
}

fn timeout_of(options: &Options) -> Duration {
    options.timeout.unwrap_or(DEFAULT_TIMEOUT)
}

fn extend_limited(buffer: &mut Vec<u8>, data: &[u8], limit: usize) {
    let remaining = limit.saturating_sub(buffer.len());
    if remaining > 0 {
        buffer.extend_from_slice(&data[..data.len().min(remaining)]);
    }
}

#[derive(Debug)]
enum ConnectError {
    HostKey,
    Ssh(russh::Error),
}

impl From<russh::Error> for ConnectError {
    fn from(error: russh::Error) -> Self {
        Self::Ssh(error)
    }
}

impl fmt::Display for ConnectError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::HostKey => formatter.write_str("host key mismatch"),
            Self::Ssh(error) => write!(formatter, "{error}"),
        }
    }
}

impl std::error::Error for ConnectError {}

struct Handler {
    entries: Vec<Entry>,
    host: String,
    port: u16,
}

impl client::Handler for Handler {
    type Error = ConnectError;

    async fn check_server_key(
        &mut self,
        server_public_key: &PublicKeyOrCertificate,
    ) -> Result<bool, Self::Error> {
        let PublicKeyOrCertificate::PublicKey { key, .. } = server_public_key else {
            // Certificates need an authority we deliberately do not trust.
            return Err(ConnectError::HostKey);
        };
        if check_host_key_entries(&self.entries, &self.host, self.port, key) {
            Ok(true)
        } else {
            Err(ConnectError::HostKey)
        }
    }
}

/// Parse a `known_hosts` file into entries.
fn parse_known_hosts(content: &str) -> Result<Vec<Entry>, Error> {
    KnownHosts::new(content)
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| {
            Error::new(
                ErrorClass::KnownHosts,
                format!("parse known_hosts: {error}"),
            )
        })
}

/// Host key algorithms the client may negotiate for `address`.
///
/// Only algorithms that can verify a pinned key are returned: RSA expands to
/// `rsa-sha2-256`, `rsa-sha2-512` and `ssh-rsa`, every other type maps to
/// itself. Marker lines (`@revoked`, `@cert-authority`) are skipped: neither
/// says which host key type the host presents. An empty result means
/// `known_hosts` has no entry for the address; the caller then keeps the
/// library defaults and the host key check still refuses the unknown host.
pub fn host_key_algorithms(content: &str, address: &str) -> Result<Vec<Algorithm>, Error> {
    // The Go scan skips malformed lines; only the later `knownhosts.New` in
    // `run` is strict, and `run` re-parses the same file strictly.
    let entries: Vec<Entry> = KnownHosts::new(content).filter_map(Result::ok).collect();
    let (host, port) = split_address(address);
    let mut algorithms = Vec::new();
    for entry in &entries {
        if entry.marker().is_some() {
            continue;
        }
        if !host_patterns_match(entry.host_patterns(), &host, port) {
            continue;
        }
        for algorithm in algorithms_for(entry.public_key().algorithm()) {
            if !algorithms.contains(&algorithm) {
                algorithms.push(algorithm);
            }
        }
    }
    Ok(algorithms)
}

/// Algorithms that can verify a key of `key_type`.
fn algorithms_for(key_type: Algorithm) -> Vec<Algorithm> {
    match key_type {
        Algorithm::Rsa { .. } => vec![
            Algorithm::Rsa {
                hash: Some(HashAlg::Sha256),
            },
            Algorithm::Rsa {
                hash: Some(HashAlg::Sha512),
            },
            Algorithm::Rsa { hash: None },
        ],
        other => vec![other],
    }
}

/// Whether the pinned entries accept `key` for `host:port`.
///
/// The semantics match Go's `knownhosts` callback: a revoked matching key is
/// rejected even if another line pins it, a byte-identical key is accepted, and
/// a key of a pinned type with different bytes is a mismatch.
#[must_use]
pub fn check_host_key(content: &str, address: &str, key: &PublicKey) -> bool {
    let Ok(entries) = parse_known_hosts(content) else {
        return false;
    };
    let (host, port) = split_address(address);
    check_host_key_entries(&entries, &host, port, key)
}

fn check_host_key_entries(entries: &[Entry], host: &str, port: u16, key: &PublicKey) -> bool {
    let matching: Vec<&Entry> = entries
        .iter()
        .filter(|entry| host_patterns_match(entry.host_patterns(), host, port))
        .collect();
    if matching.is_empty() {
        return false;
    }
    for entry in &matching {
        if entry.marker() == Some(&Marker::Revoked) && same_key(entry.public_key(), key) {
            return false;
        }
    }
    for entry in &matching {
        if entry.marker().is_some() {
            continue;
        }
        if same_key(entry.public_key(), key) {
            return true;
        }
    }
    false
}

/// Compare key material, not the `known_hosts` comment (Go compares
/// `key.Marshal()` bytes only).
fn same_key(left: &PublicKey, right: &PublicKey) -> bool {
    left.key_data() == right.key_data()
}

fn host_patterns_match(patterns: &HostPatterns, host: &str, port: u16) -> bool {
    match patterns {
        HostPatterns::Patterns(list) => {
            let mut matched = false;
            for pattern in list {
                let (negate, pattern) = pattern
                    .strip_prefix('!')
                    .map_or((false, pattern.as_str()), |rest| (true, rest));
                if pattern.is_empty() {
                    continue;
                }
                let (pattern_host, pattern_port) = split_pattern(pattern);
                if pattern_port != port || !wildcard_match(pattern_host, host) {
                    continue;
                }
                if negate {
                    return false;
                }
                matched = true;
            }
            matched
        }
        HostPatterns::HashedName { salt, hash } => {
            let normalized = normalized_host(host, port);
            let Ok(mut mac) = Hmac::<Sha1>::new_from_slice(salt) else {
                return false;
            };
            mac.update(normalized.as_bytes());
            mac.verify_slice(hash).is_ok()
        }
    }
}

/// Split `host:port` into the parts `known_hosts` matches against, with `22`
/// implicit.
fn split_address(address: &str) -> (String, u16) {
    let address = address.trim();
    if let Some(rest) = address.strip_prefix('[')
        && let Some((host, port)) = rest.split_once("]:")
    {
        return (host.to_owned(), port.parse().unwrap_or(22));
    }
    if let Some((host, port)) = address.rsplit_once(':')
        && address.matches(':').count() == 1
    {
        return (host.to_owned(), port.parse().unwrap_or(22));
    }
    (address.to_owned(), 22)
}

fn split_pattern(pattern: &str) -> (&str, u16) {
    if let Some(rest) = pattern.strip_prefix('[')
        && let Some((host, port)) = rest.split_once("]:")
    {
        return (host, port.parse().unwrap_or(22));
    }
    if let Some((host, port)) = pattern.rsplit_once(':')
        && pattern.matches(':').count() == 1
    {
        return (host, port.parse().unwrap_or(22));
    }
    (pattern, 22)
}

/// Render host and port the way `known_hosts` stores them.
fn normalized_host(host: &str, port: u16) -> String {
    if port == 22 {
        host.to_owned()
    } else {
        format!("[{host}]:{port}")
    }
}

/// OpenSSH matching: `*` spans any characters (including separators) and `?`
/// matches exactly one.
fn wildcard_match(pattern: &str, text: &str) -> bool {
    let pattern: Vec<char> = pattern.chars().collect();
    let text: Vec<char> = text.chars().collect();
    wildcard_match_chars(&pattern, &text)
}

fn wildcard_match_chars(pattern: &[char], text: &[char]) -> bool {
    if pattern.is_empty() {
        return text.is_empty();
    }
    if text.is_empty() {
        return false;
    }
    match pattern[0] {
        '*' => {
            if pattern.len() == 1 {
                return true;
            }
            (0..text.len()).any(|index| wildcard_match_chars(&pattern[1..], &text[index..]))
        }
        '?' => wildcard_match_chars(&pattern[1..], &text[1..]),
        character => character == text[0] && wildcard_match_chars(&pattern[1..], &text[1..]),
    }
}

#[cfg(test)]
mod tests {
    use std::net::SocketAddr;
    use std::path::{Path, PathBuf};
    use std::sync::{Arc, Mutex};

    use base64::Engine as _;
    use rand::Rng as _;
    use russh::keys::PrivateKey;
    use russh::keys::ssh_key::LineEnding;
    use russh::server::{
        self, Auth, ChannelOpenHandle, Server as _, Session as ServerSession, VerifiedPublicKey,
    };
    use russh::{Channel, ChannelId};
    use tokio::net::TcpListener;
    use tokio::task::JoinHandle;

    use super::*;

    fn key(algorithm: Algorithm) -> PrivateKey {
        PrivateKey::random(&mut rand::rng(), algorithm).expect("generate key")
    }

    fn ssh_line(hosts: &str, key: &PrivateKey) -> String {
        format!(
            "{hosts} {} comment\n",
            key.public_key().to_openssh().expect("public key")
        )
    }

    fn hashed_line(host: &str, key: &PrivateKey) -> String {
        let mut salt = [0u8; 20];
        rand::rng().fill_bytes(&mut salt);
        let mut mac = Hmac::<Sha1>::new_from_slice(&salt).expect("mac");
        mac.update(host.as_bytes());
        let hash = mac.finalize().into_bytes();
        format!(
            "|1|{}|{} {}",
            base64::engine::general_purpose::STANDARD.encode(salt),
            base64::engine::general_purpose::STANDARD.encode(hash),
            key.public_key().to_openssh().expect("public key")
        )
    }

    #[test]
    fn algorithms_follow_pinned_key_types() {
        let ed = key(Algorithm::Ed25519);
        let rsa = key(Algorithm::Rsa { hash: None });
        let ed_line = ssh_line("example.test:22", &ed);
        let rsa_line = ssh_line("[example.test]:2222", &rsa);

        assert_eq!(
            host_key_algorithms(&ed_line, "example.test:22").expect("algorithms"),
            vec![Algorithm::Ed25519]
        );
        assert_eq!(
            host_key_algorithms(&rsa_line, "example.test:2222").expect("algorithms"),
            vec![
                Algorithm::Rsa {
                    hash: Some(HashAlg::Sha256)
                },
                Algorithm::Rsa {
                    hash: Some(HashAlg::Sha512)
                },
                Algorithm::Rsa { hash: None }
            ]
        );
        assert!(
            host_key_algorithms(&rsa_line, "example.test:22")
                .expect("algorithms")
                .is_empty(),
            "a different port must not match"
        );
        assert_eq!(
            host_key_algorithms(&ssh_line("*.test", &ed), "example.test:22").expect("algorithms"),
            vec![Algorithm::Ed25519]
        );
        assert_eq!(
            host_key_algorithms(&hashed_line("example.test", &ed), "example.test:22")
                .expect("algorithms"),
            vec![Algorithm::Ed25519]
        );
        assert!(
            host_key_algorithms(&ssh_line("other.test", &ed), "example.test:22")
                .expect("algorithms")
                .is_empty()
        );
        assert_eq!(
            host_key_algorithms(&format!("# pin\n\n{ed_line}"), "example.test:22")
                .expect("algorithms"),
            vec![Algorithm::Ed25519]
        );
        assert!(
            host_key_algorithms(&format!("@revoked {ed_line}"), "example.test:22")
                .expect("algorithms")
                .is_empty()
        );
    }

    #[test]
    fn host_key_verification_matches_bytes_and_honours_revocation() {
        let ed = key(Algorithm::Ed25519);
        let other = key(Algorithm::Ed25519);
        let line = ssh_line("example.test:22", &ed);
        assert!(check_host_key(&line, "example.test:22", ed.public_key()));
        assert!(!check_host_key(
            &line,
            "example.test:22",
            other.public_key()
        ));
        assert!(!check_host_key(
            &format!("@revoked {line}"),
            "example.test:22",
            ed.public_key()
        ));
        assert!(!check_host_key(&line, "other.test:22", ed.public_key()));
    }

    #[test]
    fn split_address_follows_known_hosts_rules() {
        assert_eq!(
            split_address("example.test:2222"),
            ("example.test".to_owned(), 2222)
        );
        assert_eq!(
            split_address("[example.test]:2222"),
            ("example.test".to_owned(), 2222)
        );
        assert_eq!(
            split_address("example.test"),
            ("example.test".to_owned(), 22)
        );
        assert_eq!(split_address("[::1]:2222"), ("::1".to_owned(), 2222));
    }

    struct TestServer {
        response: Arc<Vec<u8>>,
        stdin: Arc<Mutex<Vec<u8>>>,
        connections: Arc<Mutex<usize>>,
    }

    impl server::Server for TestServer {
        type Handler = TestHandler;

        fn new_client(&mut self, _: Option<SocketAddr>) -> TestHandler {
            *self.connections.lock().expect("lock") += 1;
            TestHandler {
                response: self.response.clone(),
                stdin: self.stdin.clone(),
            }
        }
    }

    struct TestHandler {
        response: Arc<Vec<u8>>,
        stdin: Arc<Mutex<Vec<u8>>>,
    }

    impl server::Handler for TestHandler {
        type Error = russh::Error;

        async fn auth_publickey(
            &mut self,
            _user: &str,
            _key: VerifiedPublicKey<'_>,
        ) -> Result<Auth, Self::Error> {
            Ok(Auth::Accept)
        }

        async fn channel_open_session(
            &mut self,
            _channel: Channel<server::Msg>,
            reply: ChannelOpenHandle,
            _session: &mut ServerSession,
        ) -> Result<(), Self::Error> {
            reply.accept().await;
            Ok(())
        }

        async fn exec_request(
            &mut self,
            channel: ChannelId,
            _data: &[u8],
            session: &mut ServerSession,
        ) -> Result<(), Self::Error> {
            session.channel_success(channel)?;
            Ok(())
        }

        async fn data(
            &mut self,
            _channel: ChannelId,
            data: &[u8],
            _session: &mut ServerSession,
        ) -> Result<(), Self::Error> {
            self.stdin.lock().expect("lock").extend_from_slice(data);
            Ok(())
        }

        async fn channel_eof(
            &mut self,
            channel: ChannelId,
            session: &mut ServerSession,
        ) -> Result<(), Self::Error> {
            for chunk in self.response.chunks(16 * 1024) {
                session.data(channel, chunk.to_vec())?;
            }
            session.exit_status_request(channel, 0)?;
            session.eof(channel)?;
            session.close(channel)?;
            Ok(())
        }
    }

    struct ServerHandle {
        address: String,
        host_key: PrivateKey,
        stdin: Arc<Mutex<Vec<u8>>>,
        _connections: Arc<Mutex<usize>>,
        _task: JoinHandle<()>,
    }

    async fn start_server(response: Vec<u8>) -> ServerHandle {
        let host_key = key(Algorithm::Ed25519);
        let server_key = host_key.clone();
        let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
        let address = listener.local_addr().expect("addr").to_string();
        let stdin = Arc::new(Mutex::new(Vec::new()));
        let connections = Arc::new(Mutex::new(0));
        let config = Arc::new(server::Config {
            keys: vec![server_key],
            auth_rejection_time: Duration::ZERO,
            auth_rejection_time_initial: Some(Duration::ZERO),
            ..server::Config::default()
        });
        let server_response = Arc::new(response);
        let server_stdin = stdin.clone();
        let server_connections = connections.clone();
        let task = tokio::spawn(async move {
            let mut server = TestServer {
                response: server_response,
                stdin: server_stdin,
                connections: server_connections,
            };
            let _ = server.run_on_socket(config, &listener).await;
        });
        ServerHandle {
            address,
            host_key,
            stdin,
            _connections: connections,
            _task: task,
        }
    }

    fn write_key(dir: &Path, name: &str, key: &PrivateKey) -> PathBuf {
        let path = dir.join(name);
        std::fs::write(
            &path,
            key.to_openssh(LineEnding::LF).expect("openssh").as_bytes(),
        )
        .expect("write key");
        path
    }

    fn write_known_hosts(dir: &Path, address: &str, key: &PrivateKey) -> PathBuf {
        let path = dir.join("known_hosts");
        std::fs::write(&path, ssh_line(address, key)).expect("write known_hosts");
        path
    }

    #[tokio::test]
    async fn run_executes_the_forced_command_and_returns_stdout() {
        let server = start_server(br#"{"ok":true,"result":{"threads":[]}}"#.to_vec()).await;
        let dir = tempfile::tempdir().expect("tempdir");
        let client_key = key(Algorithm::Ed25519);
        let known_hosts = write_known_hosts(dir.path(), &server.address, &server.host_key);
        let runner = SshRunner::new(Options {
            address: server.address.clone(),
            user: "bb".to_owned(),
            key_file: write_key(dir.path(), "id_ed25519", &client_key),
            known_hosts_file: known_hosts,
            timeout: Some(Duration::from_secs(5)),
        });
        let answer = runner.run(b"{\"method\":\"status\"}").await.expect("run");
        assert_eq!(answer.stdout, br#"{"ok":true,"result":{"threads":[]}}"#);
        assert_eq!(answer.exit_code, Some(0));
        assert_eq!(
            server.stdin.lock().expect("lock").as_slice(),
            b"{\"method\":\"status\"}"
        );
    }

    #[tokio::test]
    async fn a_mismatched_host_key_is_reported_as_hostkey() {
        let server = start_server(b"{}".to_vec()).await;
        let dir = tempfile::tempdir().expect("tempdir");
        let client_key = key(Algorithm::Ed25519);
        let other_host = key(Algorithm::Ed25519);
        let known_hosts = write_known_hosts(dir.path(), &server.address, &other_host);
        let runner = SshRunner::new(Options {
            address: server.address.clone(),
            user: "bb".to_owned(),
            key_file: write_key(dir.path(), "id_ed25519", &client_key),
            known_hosts_file: known_hosts,
            timeout: Some(Duration::from_secs(5)),
        });
        let error = runner.run(b"{}").await.expect_err("host key mismatch");
        assert_eq!(error.class(), ErrorClass::HostKey);
        assert_eq!(error.kind(), ErrorKind::Permanent);
    }

    #[tokio::test]
    async fn the_answer_is_truncated_at_one_mib() {
        let server = start_server(vec![b'x'; 2 * MAX_ANSWER_BYTES]).await;
        let dir = tempfile::tempdir().expect("tempdir");
        let client_key = key(Algorithm::Ed25519);
        let known_hosts = write_known_hosts(dir.path(), &server.address, &server.host_key);
        let runner = SshRunner::new(Options {
            address: server.address.clone(),
            user: "bb".to_owned(),
            key_file: write_key(dir.path(), "id_ed25519", &client_key),
            known_hosts_file: known_hosts,
            timeout: Some(Duration::from_secs(10)),
        });
        let answer = runner.run(b"{}").await.expect("run");
        assert_eq!(answer.stdout.len(), MAX_ANSWER_BYTES);
    }
}
