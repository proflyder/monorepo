//! Subprocess runner for `kubectl` (and later `helm`).
//!
//! The Go helpers keep the exact argv the Python scripts used, so a fake
//! kubectl can record a trace and compare it against the fixture. The runner is
//! injectable so unit tests never need a cluster. A failed start is reported as
//! exit status 1 with the start error on stderr, like the shell would.

use std::io::Write;
use std::process::{Command as Process, Stdio};

/// What one kubectl invocation produced.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Output {
    /// Captured stdout.
    pub stdout: String,
    /// Captured stderr.
    pub stderr: String,
    /// Exit status, or 1 when the process could not start.
    pub code: i32,
}

/// Executes a command with arguments (without the binary name).
pub trait Runner {
    /// Runs one command.
    fn run(&self, args: &[String]) -> Output;
}

/// Runs the real binary named by the deploy script.
pub struct SystemRunner {
    program: String,
}

impl SystemRunner {
    /// Creates a runner for `program`.
    #[must_use]
    pub fn new(program: &str) -> Self {
        Self {
            program: program.to_string(),
        }
    }
}

impl Runner for SystemRunner {
    fn run(&self, args: &[String]) -> Output {
        match Process::new(&self.program).args(args).output() {
            Ok(output) => Output {
                stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
                stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
                code: output.status.code().unwrap_or(1),
            },
            Err(error) => Output {
                stdout: String::new(),
                stderr: error.to_string(),
                code: 1,
            },
        }
    }
}

/// One subprocess invocation for the Hermes rollout (kubectl or helm).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Invocation {
    /// Binary name.
    pub program: String,
    /// Arguments.
    pub args: Vec<String>,
    /// Optional stdin.
    pub stdin: Option<String>,
}

/// Executes a full [`Invocation`].
pub trait CommandRunner {
    /// Runs one command.
    fn run(&self, command: &Invocation) -> Output;
}

/// Runs the real binary named by the command.
pub struct SystemCommandRunner;

impl CommandRunner for SystemCommandRunner {
    fn run(&self, command: &Invocation) -> Output {
        let mut process = Process::new(&command.program);
        process.args(&command.args);
        if command.stdin.is_some() {
            process.stdin(Stdio::piped());
        }
        process.stdout(Stdio::piped()).stderr(Stdio::piped());
        let mut child = match process.spawn() {
            Ok(child) => child,
            Err(error) => {
                return Output {
                    stdout: String::new(),
                    stderr: error.to_string(),
                    code: 1,
                };
            }
        };
        if let Some(stdin) = &command.stdin
            && let Some(mut handle) = child.stdin.take()
        {
            let _ = handle.write_all(stdin.as_bytes());
        }
        match child.wait_with_output() {
            Ok(output) => Output {
                stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
                stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
                code: output.status.code().unwrap_or(1),
            },
            Err(error) => Output {
                stdout: String::new(),
                stderr: error.to_string(),
                code: 1,
            },
        }
    }
}

#[cfg(test)]
pub mod testing {
    use std::cell::RefCell;

    use super::{Output, Runner};

    /// Records every argv and answers from an injected handler.
    pub struct FakeRunner<F> {
        handler: F,
        calls: RefCell<Vec<Vec<String>>>,
    }

    impl<F> FakeRunner<F>
    where
        F: Fn(&[String]) -> Output,
    {
        /// Creates a fake runner.
        pub fn new(handler: F) -> Self {
            Self {
                handler,
                calls: RefCell::new(Vec::new()),
            }
        }

        /// Returns the recorded calls.
        pub fn calls(&self) -> Vec<Vec<String>> {
            self.calls.borrow().clone()
        }

        /// Returns the verb (third argv element) of every call.
        pub fn verbs(&self) -> Vec<String> {
            self.calls
                .borrow()
                .iter()
                .map(|args| args.get(2).cloned().unwrap_or_default())
                .collect()
        }
    }

    impl<F> Runner for FakeRunner<F>
    where
        F: Fn(&[String]) -> Output,
    {
        fn run(&self, args: &[String]) -> Output {
            self.calls.borrow_mut().push(args.to_vec());
            (self.handler)(args)
        }
    }

    /// Records every [`super::Invocation`] and answers from an injected handler.
    pub struct FakeCommandRunner<F> {
        handler: F,
        calls: RefCell<Vec<super::Invocation>>,
    }

    impl<F> FakeCommandRunner<F>
    where
        F: Fn(&super::Invocation) -> Output,
    {
        /// Creates a fake command runner.
        pub fn new(handler: F) -> Self {
            Self {
                handler,
                calls: RefCell::new(Vec::new()),
            }
        }

        /// Returns the recorded invocations.
        pub fn calls(&self) -> Vec<super::Invocation> {
            self.calls.borrow().clone()
        }
    }

    impl<F> super::CommandRunner for FakeCommandRunner<F>
    where
        F: Fn(&super::Invocation) -> Output,
    {
        fn run(&self, command: &super::Invocation) -> Output {
            self.calls.borrow_mut().push(command.clone());
            (self.handler)(command)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_missing_binary_is_exit_1_with_the_start_error() {
        let runner = SystemRunner::new("/nonexistent/home-deploy-kubectl");
        let result = runner.run(&["get".to_string()]);
        assert_eq!(result.code, 1);
        assert!(!result.stderr.is_empty());
    }
}
