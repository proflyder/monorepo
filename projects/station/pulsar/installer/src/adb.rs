//! The device side of the installer: an `adb` process, or a fake in tests.
//!
//! `adb shell` on station firmwares is a pty: output has `\r`, the channel is not
//! binary-safe (binary data goes through `adb push`/`pull` only), and the exit status is
//! always 0 — the real one is carried by the `__rc=` marker.

use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

/// One device command must fit these firmwares' shell; docs/development.md, «Правила
/// installer/».
pub const MAX_COMMAND: usize = 4096;

#[derive(Debug, thiserror::Error)]
pub enum ShError {
    #[error("no exit status")]
    NoStatus,
    /// The remote status and the output it produced: a failed `mv`/`ctl.sh` prints its
    /// message, and Go's `shRaw` returned both.
    #[error("exit {code}")]
    Exit { code: String, out: String },
    #[error("command is {0} bytes, over the {MAX_COMMAND}-byte device limit")]
    TooLong(usize),
    #[error(transparent)]
    Io(#[from] io::Error),
}

impl ShError {
    /// The station output collected before the failure ("" when there was none).
    pub fn out(&self) -> &str {
        match self {
            Self::Exit { out, .. } => out,
            _ => "",
        }
    }
}

pub trait Adb {
    fn push(&mut self, serial: &str, local: &Path, remote: &str) -> io::Result<()>;
    fn pull(&mut self, serial: &str, remote: &str, local: &Path) -> io::Result<()>;
    fn sh_raw(&mut self, serial: &str, cmd: &str) -> Result<String, ShError>;

    /// `sh_raw` with trailing newlines removed (for captured output). Like Go's `sh`, a
    /// failed command still returns its output through the error.
    fn sh(&mut self, serial: &str, cmd: &str) -> Result<String, ShError> {
        match self.sh_raw(serial, cmd) {
            Ok(out) => Ok(out.trim_end_matches('\n').to_string()),
            Err(ShError::Exit { code, out }) => Err(ShError::Exit {
                code,
                out: out.trim_end_matches('\n').to_string(),
            }),
            Err(e) => Err(e),
        }
    }

    /// Runs `cmd` and copies its output to `w` even when the remote status is non-zero.
    fn sh_to(&mut self, w: &mut dyn Write, serial: &str, cmd: &str) -> Result<(), ShError> {
        match self.sh_raw(serial, cmd) {
            Ok(out) => {
                w.write_all(out.as_bytes())?;
                Ok(())
            }
            Err(e) => {
                if !e.out().is_empty() {
                    w.write_all(e.out().as_bytes())?;
                }
                Err(e)
            }
        }
    }
}

/// Parse a captured `adb shell` stdout: strip `\r`, split off the last `__rc=` marker and
/// return the output before it. A non-zero remote status carries that output in the error.
pub fn parse_shell_output(raw: &[u8]) -> Result<String, ShError> {
    let replaced = String::from_utf8_lossy(raw).replace('\r', "");
    let o = replaced.trim_end_matches('\n');
    let Some(i) = o.rfind("__rc=") else {
        return Err(ShError::NoStatus);
    };
    let out = &o[..i];
    let rc = &o[i + "__rc=".len()..];
    if rc != "0" {
        return Err(ShError::Exit {
            code: rc.to_string(),
            out: out.to_string(),
        });
    }
    Ok(out.to_string())
}

/// A duplicate of our stderr for a child's stdout (Go sets `Stdout = os.Stderr`).
fn stderr_stdio() -> Stdio {
    #[cfg(unix)]
    {
        use std::os::fd::AsFd;
        if let Ok(fd) = io::stderr().as_fd().try_clone_to_owned() {
            return Stdio::from(fd);
        }
    }
    #[cfg(windows)]
    {
        use std::os::windows::io::AsHandle;
        if let Ok(handle) = io::stderr().as_handle().try_clone_to_owned() {
            return Stdio::from(handle);
        }
    }
    Stdio::inherit()
}

/// The real `adb` executable.
pub struct ProcessAdb {
    path: PathBuf,
}

impl ProcessAdb {
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self { path: path.into() }
    }

    /// Starts the adb server with our stderr, not a pipe: a server spawned by a captured adb
    /// call would inherit the pipe and keep it open (blocking that call on Windows). Both
    /// streams go to stderr, like the Go installer.
    pub fn start_server(&self) -> io::Result<()> {
        let status = Command::new(&self.path)
            .arg("start-server")
            .stdout(stderr_stdio())
            .stderr(Stdio::inherit())
            .status()?;
        if status.success() {
            Ok(())
        } else {
            Err(io::Error::other("adb start-server exited non-zero"))
        }
    }

    pub fn connect(&self, serial: &str) {
        let _ = Command::new(&self.path)
            .args(["connect", serial])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
    }

    pub fn disconnect(&self, serial: &str) {
        let _ = Command::new(&self.path)
            .args(["disconnect", serial])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
    }

    /// Serials adb lists in state `device`.
    pub fn devices(&self) -> Vec<String> {
        let Ok(out) = Command::new(&self.path).arg("devices").output() else {
            return Vec::new();
        };
        let text = String::from_utf8_lossy(&out.stdout).replace('\r', "");
        text.split('\n')
            .skip(1)
            .filter_map(|l| {
                let f: Vec<&str> = l.split_whitespace().collect();
                (f.len() >= 2 && f[1] == "device").then(|| f[0].to_string())
            })
            .collect()
    }

    /// Serials of network devices (`ip:port`).
    pub fn network_devices(&self) -> Vec<String> {
        self.devices()
            .into_iter()
            .filter(|s| {
                s.rsplit_once(':')
                    .is_some_and(|(_, p)| !p.is_empty() && p.bytes().all(|b| b.is_ascii_digit()))
            })
            .collect()
    }
}

impl Adb for ProcessAdb {
    fn push(&mut self, serial: &str, local: &Path, remote: &str) -> io::Result<()> {
        let status = Command::new(&self.path)
            .args(["-s", serial, "push"])
            .arg(local)
            .arg(remote)
            .stdout(Stdio::null())
            .stderr(Stdio::inherit())
            .status()?;
        if status.success() {
            Ok(())
        } else {
            Err(io::Error::other("adb push failed"))
        }
    }

    fn pull(&mut self, serial: &str, remote: &str, local: &Path) -> io::Result<()> {
        let status = Command::new(&self.path)
            .args(["-s", serial, "pull", remote])
            .arg(local)
            .stdout(Stdio::null())
            .stderr(Stdio::inherit())
            .status()?;
        if status.success() {
            Ok(())
        } else {
            Err(io::Error::other("adb pull failed"))
        }
    }

    fn sh_raw(&mut self, serial: &str, cmd: &str) -> Result<String, ShError> {
        let full = format!("{cmd}\necho __rc=$?");
        if full.len() > MAX_COMMAND {
            return Err(ShError::TooLong(full.len()));
        }
        // Go uses Output(): stdout is parsed, stderr is captured and dropped.
        let out = Command::new(&self.path)
            .args(["-s", serial, "shell", &full])
            .stderr(Stdio::piped())
            .output()?;
        if !out.status.success() {
            return Err(ShError::Io(io::Error::other("adb shell failed")));
        }
        parse_shell_output(&out.stdout)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Value;

    #[test]
    fn shell_output_matches_go_golden() {
        let cases: Value = serde_json::from_str(include_str!("../testdata/sh_raw.json")).unwrap();
        for c in cases.as_array().unwrap() {
            let got = parse_shell_output(c["raw"].as_str().unwrap().as_bytes());
            match &c["err"] {
                Value::Null => assert_eq!(got.unwrap(), c["out"].as_str().unwrap()),
                Value::String(err) => {
                    let e = got.unwrap_err();
                    assert_eq!(&e.to_string(), err);
                    // Go returns the output together with a non-zero status.
                    assert_eq!(e.out(), c["out"].as_str().unwrap());
                }
                _ => panic!("unexpected fixture"),
            }
        }
    }

    #[test]
    fn sh_to_prints_output_of_failed_command() {
        struct F;
        impl Adb for F {
            fn push(&mut self, _: &str, _: &Path, _: &str) -> io::Result<()> {
                Ok(())
            }
            fn pull(&mut self, _: &str, _: &str, _: &Path) -> io::Result<()> {
                Ok(())
            }
            fn sh_raw(&mut self, _: &str, _: &str) -> Result<String, ShError> {
                Err(ShError::Exit {
                    code: "1".into(),
                    out: "ctl.sh: failed\n".into(),
                })
            }
        }
        let mut buf = Vec::new();
        assert!(F.sh_to(&mut buf, "s", "ctl.sh restart").is_err());
        assert_eq!(buf, b"ctl.sh: failed\n");
    }

    #[test]
    fn command_limit() {
        let mut adb = ProcessAdb::new("/nonexistent");
        let err = adb.sh_raw("s", &"x".repeat(MAX_COMMAND)).unwrap_err();
        assert!(matches!(err, ShError::TooLong(_)));
    }
}
