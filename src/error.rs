//! User-facing refusals and failures.

use std::fmt;
use std::io;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ErrorKind {
    Message,
    Conflict,
    /// Already reported on stdout (e.g. a `--json` error document); exit with this code.
    Reported(u8),
    StdoutBrokenPipe,
    StderrBrokenPipe,
}

#[derive(Debug)]
pub struct Error {
    message: String,
    kind: ErrorKind,
}

impl Error {
    pub fn msg(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            kind: ErrorKind::Message,
        }
    }

    /// A safe refusal caused by an existing conflicting resource.
    pub fn conflict(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            kind: ErrorKind::Conflict,
        }
    }

    /// A failure whose report was already written; the CLI only sets the exit code.
    pub fn reported(message: impl Into<String>, exit_code: u8) -> Self {
        Self {
            message: message.into(),
            kind: ErrorKind::Reported(exit_code),
        }
    }

    pub(crate) fn reported_exit_code(&self) -> Option<u8> {
        match self.kind {
            ErrorKind::Reported(code) => Some(code),
            _ => None,
        }
    }

    pub(crate) fn output(stream: &str, error: io::Error) -> Self {
        let kind = if error.kind() == io::ErrorKind::BrokenPipe {
            if stream == "stdout" {
                ErrorKind::StdoutBrokenPipe
            } else {
                ErrorKind::StderrBrokenPipe
            }
        } else {
            ErrorKind::Message
        };
        Self {
            message: format!("{stream}: {error}"),
            kind,
        }
    }

    pub(crate) fn is_stdout_broken_pipe(&self) -> bool {
        self.kind == ErrorKind::StdoutBrokenPipe
    }

    pub(crate) fn is_conflict(&self) -> bool {
        self.kind == ErrorKind::Conflict
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for Error {}
