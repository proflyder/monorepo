//! Drives the real `agent/ha-config-agent.sh` against a temporary root.
//!
//! The script is embedded at compile time, so the test works from both the
//! Cargo tree and the Bazel runfiles tree. It needs only POSIX `sh`, `sha256sum`
//! or `shasum`, `base64` and the usual coreutils. `flock` is optional in the
//! agent, which is what lets the same test run on macOS.

use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

use base64::Engine as _;
use sha2::{Digest as _, Sha256};

const SCRIPT: &str = include_str!("../../../agent/ha-config-agent.sh");

struct Fixture {
    directory: tempfile::TempDir,
}

impl Fixture {
    fn new() -> Self {
        Self {
            directory: tempfile::tempdir().expect("tempdir"),
        }
    }

    fn script(&self) -> PathBuf {
        let path = self.directory.path().join("agent.sh");
        std::fs::write(&path, SCRIPT).expect("write agent");
        path
    }

    fn root(&self) -> PathBuf {
        self.directory.path().join("homeassistant")
    }

    fn state(&self) -> PathBuf {
        self.directory.path().join("state")
    }

    fn ensure_root(&self) {
        std::fs::create_dir_all(self.root()).expect("root");
    }

    fn write(&self, relative: &str, content: &[u8]) {
        let path = self.root().join(relative);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).expect("parent");
        }
        std::fs::write(path, content).expect("write");
    }

    fn read(&self, relative: &str) -> Option<Vec<u8>> {
        std::fs::read(self.root().join(relative)).ok()
    }

    fn run_with(
        &self,
        command: &str,
        stdin: &[u8],
        environment: &[(&str, &str)],
        directory: Option<&Path>,
    ) -> Output {
        let mut process = Command::new("sh");
        process
            .arg(self.script())
            .env("HA_CONFIG_AGENT_ROOT", self.root())
            .env("HA_CONFIG_AGENT_STATE", self.state())
            .env("SSH_ORIGINAL_COMMAND", command)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        for (name, value) in environment {
            process.env(name, value);
        }
        if let Some(directory) = directory {
            process.current_dir(directory);
        }
        let mut child = process.spawn().expect("spawn agent");
        child
            .stdin
            .as_mut()
            .expect("stdin")
            .write_all(stdin)
            .expect("write stdin");
        child.wait_with_output().expect("agent output")
    }

    fn run_ok(&self, command: &str, stdin: &[u8]) -> String {
        self.run_ok_with(command, stdin, &[])
    }

    fn run_ok_with(&self, command: &str, stdin: &[u8], environment: &[(&str, &str)]) -> String {
        let output = self.run_with(command, stdin, environment, None);
        assert!(
            output.status.success(),
            "{command} failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        String::from_utf8(output.stdout).expect("stdout")
    }

    fn run_err(&self, command: &str, stdin: &[u8]) -> String {
        self.run_err_with(command, stdin, &[], None)
    }

    fn run_err_with(
        &self,
        command: &str,
        stdin: &[u8],
        environment: &[(&str, &str)],
        directory: Option<&Path>,
    ) -> String {
        let output = self.run_with(command, stdin, environment, directory);
        assert!(
            !output.status.success(),
            "{command} unexpectedly succeeded: status {:?} stdout {:?} stderr {:?} stdin {:?}",
            output.status,
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr),
            String::from_utf8_lossy(stdin)
        );
        String::from_utf8(output.stderr).expect("stderr")
    }
}

fn sha(content: &[u8]) -> String {
    hex::encode(Sha256::digest(content))
}

fn apply_line(path: &str, content: &[u8]) -> String {
    format!(
        "{path} {} {}\n",
        sha(content),
        base64::engine::general_purpose::STANDARD.encode(content)
    )
}

#[test]
fn version_and_hashes_match_the_protocol() {
    let fixture = Fixture::new();
    fixture.ensure_root();
    fixture.write("scripts.yaml", b"a: 1\n");
    fixture.write("custom_sentences/ru/station_alarm.yaml", b"b: 2\n");

    let version = fixture.run_ok("version", b"");
    assert_eq!(version, "ha-config-agent 1\n");

    let hashes = fixture.run_ok(
        "hashes",
        b"scripts.yaml\nmissing.yaml\ncustom_sentences/ru/station_alarm.yaml\n",
    );
    assert_eq!(
        hashes,
        format!(
            "{} scripts.yaml\nMISSING missing.yaml\n{} custom_sentences/ru/station_alarm.yaml\n",
            sha(b"a: 1\n"),
            sha(b"b: 2\n")
        )
    );
}

#[test]
fn apply_creates_a_backup_and_restore_brings_the_old_state_back() {
    let fixture = Fixture::new();
    fixture.ensure_root();
    fixture.write("scripts.yaml", b"old: true\n");

    let request = format!(
        "{}{}",
        apply_line("scripts.yaml", b"new: true\n"),
        apply_line("shell_commands.yaml", b"restart: true\n"),
    );
    let backup = fixture.run_ok("apply", request.as_bytes());
    let backup = backup.trim();
    assert!(regex_backup_id(backup), "unexpected backup id: {backup:?}");
    assert_eq!(fixture.read("scripts.yaml"), Some(b"new: true\n".to_vec()));
    assert_eq!(
        fixture.read("shell_commands.yaml"),
        Some(b"restart: true\n".to_vec())
    );

    let backup_dir = fixture.state().join("backups").join(backup);
    assert_eq!(
        std::fs::read(backup_dir.join("files/scripts.yaml")).expect("backup copy"),
        b"old: true\n"
    );
    assert_eq!(
        std::fs::read_to_string(backup_dir.join("absent")).expect("absent list"),
        "shell_commands.yaml\n"
    );

    fixture.run_ok(&format!("restore {backup}"), b"");
    assert_eq!(fixture.read("scripts.yaml"), Some(b"old: true\n".to_vec()));
    assert_eq!(fixture.read("shell_commands.yaml"), None);
}

#[test]
fn a_sha256_mismatch_is_rejected_before_any_write() {
    let fixture = Fixture::new();
    fixture.ensure_root();
    fixture.write("scripts.yaml", b"old: true\n");
    let request = format!(
        "scripts.yaml {} {}\n",
        "0".repeat(64),
        base64::engine::general_purpose::STANDARD.encode(b"new: true\n")
    );
    let error = fixture.run_err("apply", request.as_bytes());
    assert!(error.contains("sha256 mismatch"), "{error}");
    assert_eq!(fixture.read("scripts.yaml"), Some(b"old: true\n".to_vec()));
}

#[cfg(unix)]
#[test]
fn escaping_paths_and_symlinks_are_rejected() {
    let fixture = Fixture::new();
    fixture.ensure_root();
    for command in [
        "../scripts.yaml",
        "/etc/passwd",
        "configuration.yaml",
        "custom_sentences/ru/sub/x.yaml",
    ] {
        let error = fixture.run_err("hashes", format!("{command}\n").as_bytes());
        assert!(error.contains("ERROR"), "{command}: {error}");
    }
    let absolute = format!(
        "/tmp/x.yaml {} {}\n",
        sha(b"x"),
        base64::engine::general_purpose::STANDARD.encode(b"x")
    );
    assert!(
        fixture
            .run_err("apply", absolute.as_bytes())
            .contains("ERROR")
    );

    std::fs::write(fixture.root().join("real.yaml"), b"x").expect("write");
    std::os::unix::fs::symlink(
        fixture.root().join("real.yaml"),
        fixture.root().join("scripts.yaml"),
    )
    .expect("symlink");
    assert!(
        fixture
            .run_err("hashes", b"scripts.yaml\n")
            .contains("ERROR")
    );
}

#[test]
fn the_configured_source_address_is_enforced() {
    let fixture = Fixture::new();
    fixture.ensure_root();

    // No HA_CONFIG_AGENT_ALLOWED_FROM: the SSH server's restrict/command is the
    // only restriction (development and tests).
    assert_eq!(fixture.run_ok("version", b"").trim(), "ha-config-agent 1");

    let allowed = [("HA_CONFIG_AGENT_ALLOWED_FROM", "192.0.2.10")];
    let with_client = [
        ("HA_CONFIG_AGENT_ALLOWED_FROM", "192.0.2.10"),
        ("SSH_CLIENT", "192.0.2.10 41234 22222"),
    ];
    assert_eq!(
        fixture.run_ok_with("version", b"", &with_client).trim(),
        "ha-config-agent 1"
    );

    // A comma or space separated list is accepted.
    for list in ["192.0.2.10,192.0.2.11", "192.0.2.10 192.0.2.11"] {
        let environment = [
            ("HA_CONFIG_AGENT_ALLOWED_FROM", list),
            ("SSH_CLIENT", "192.0.2.11 41234 22222"),
        ];
        assert_eq!(
            fixture.run_ok_with("version", b"", &environment).trim(),
            "ha-config-agent 1"
        );
    }

    let denied = [
        ("HA_CONFIG_AGENT_ALLOWED_FROM", "192.0.2.10"),
        ("SSH_CLIENT", "198.51.100.7 41234 22222"),
    ];
    assert!(
        fixture
            .run_err_with("version", b"", &denied, None)
            .contains("source not allowed")
    );

    // Without SSH_CLIENT/SSH_CONNECTION the source cannot be verified.
    assert!(
        fixture
            .run_err_with("version", b"", &allowed, None)
            .contains("source not allowed")
    );

    // The fallback variable is used when SSH_CLIENT is absent.
    let connection = [
        ("HA_CONFIG_AGENT_ALLOWED_FROM", "192.0.2.10"),
        ("SSH_CONNECTION", "192.0.2.10 41234 192.0.2.1 22222"),
    ];
    assert_eq!(
        fixture.run_ok_with("version", b"", &connection).trim(),
        "ha-config-agent 1"
    );
}

#[test]
fn glob_arguments_are_never_expanded() {
    let fixture = Fixture::new();
    fixture.ensure_root();
    // A file that `*` would expand to if `set -f` were missing.
    std::fs::write(fixture.directory.path().join("20260101T000000Z"), b"x").expect("write");
    let error = fixture.run_err_with("restore *", b"", &[], Some(fixture.directory.path()));
    assert!(error.contains("invalid backup id"), "{error}");
    let error = fixture.run_err_with("prune *", b"", &[], Some(fixture.directory.path()));
    assert!(error.contains("invalid keep count"), "{error}");
}

#[test]
fn unknown_and_missing_verbs_fail() {
    let fixture = Fixture::new();
    fixture.ensure_root();
    assert!(fixture.run_err("destroy", b"").contains("unknown verb"));
    assert!(fixture.run_err("", b"").contains("missing verb"));
    assert!(
        fixture
            .run_err("restore not/a/backup", b"")
            .contains("invalid backup id")
    );
    assert!(
        fixture
            .run_err("version extra", b"")
            .contains("takes no arguments")
    );
}

#[test]
fn prune_keeps_the_newest_backups() {
    let fixture = Fixture::new();
    fixture.ensure_root();
    let backups = fixture.state().join("backups");
    for stamp in [
        "20260101T000001Z",
        "20260101T000002Z",
        "20260101T000003Z",
        "20260101T000004Z",
    ] {
        std::fs::create_dir_all(backups.join(stamp)).expect("backup dir");
    }
    fixture.run_ok("prune 2", b"");
    let mut left: Vec<String> = std::fs::read_dir(&backups)
        .expect("backups")
        .map(|entry| {
            entry
                .expect("entry")
                .file_name()
                .to_string_lossy()
                .into_owned()
        })
        .collect();
    left.sort();
    assert_eq!(left, vec!["20260101T000003Z", "20260101T000004Z"]);
}

#[test]
fn an_empty_apply_changes_nothing() {
    let fixture = Fixture::new();
    fixture.ensure_root();
    assert!(fixture.run_err("apply", b"").contains("empty apply"));
    assert!(!fixture.state().join("backups").exists());
}

fn regex_backup_id(id: &str) -> bool {
    let bytes = id.as_bytes();
    bytes.len() == 16
        && bytes[8] == b'T'
        && bytes[15] == b'Z'
        && bytes
            .iter()
            .enumerate()
            .all(|(index, byte)| index == 8 || index == 15 || byte.is_ascii_digit())
}
