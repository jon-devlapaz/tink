//! Fallible process output.
//!
//! Rust's `print!` macros panic when a downstream pipeline closes early. A closed
//! pipe is normal CLI control flow, so all application output passes through this
//! module and is returned to the command boundary as an ordinary I/O error.

use std::fmt;
use std::io::{self, Write};
use std::path::Path;

use crate::error::Error;

/// Render untrusted text without allowing it to create terminal control flow.
pub(crate) fn escape_untrusted(value: &str) -> String {
    let mut escaped = String::with_capacity(value.len());
    for character in value.chars() {
        match character {
            '\\' => escaped.push_str("\\\\"),
            '\n' => escaped.push_str("\\n"),
            '\r' => escaped.push_str("\\r"),
            '\t' => escaped.push_str("\\t"),
            '\0' => escaped.push_str("\\0"),
            '\u{1b}' => escaped.push_str("\\x1b"),
            character if character.is_control() => escaped.extend(character.escape_default()),
            character => escaped.push(character),
        }
    }
    escaped
}

/// Lossily render an operating-system path, then make control flow visible.
///
/// `Path::display` also substitutes invalid Unicode; this helper preserves that
/// user-facing behavior while ensuring a repository-controlled filename cannot
/// inject terminal control sequences into diagnostics.
pub(crate) fn display_path(path: &Path) -> String {
    escape_untrusted(path.to_string_lossy().as_ref())
}

fn write(
    mut sink: impl Write,
    stream: &str,
    args: fmt::Arguments<'_>,
    newline: bool,
) -> Result<(), Error> {
    sink.write_fmt(args)
        .map_err(|error| Error::output(stream, error))?;
    if newline {
        sink.write_all(b"\n")
            .map_err(|error| Error::output(stream, error))?;
    }
    Ok(())
}

pub(crate) fn stdout(args: fmt::Arguments<'_>) -> Result<(), Error> {
    write(io::stdout().lock(), "stdout", args, false)
}

pub(crate) fn stdout_line(args: fmt::Arguments<'_>) -> Result<(), Error> {
    write(io::stdout().lock(), "stdout", args, true)
}

pub(crate) fn stderr_line(args: fmt::Arguments<'_>) -> Result<(), Error> {
    write(io::stderr().lock(), "stderr", args, true)
}

/// Warnings are advisory and must never retroactively fail completed work.
pub(crate) fn warning_line(args: fmt::Arguments<'_>) {
    let _ = stderr_line(args);
}

pub(crate) fn flush_stdout() -> Result<(), Error> {
    io::stdout()
        .lock()
        .flush()
        .map_err(|error| Error::output("stdout", error))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn untrusted_terminal_text_uses_visible_reversible_escapes() {
        assert_eq!(
            escape_untrusted("a\\b\nrow\r\t\0\u{1b}[31m"),
            "a\\\\b\\nrow\\r\\t\\0\\x1b[31m"
        );
    }

    #[test]
    fn displayed_paths_escape_terminal_controls() {
        assert_eq!(
            display_path(Path::new("directory\u{1b}[31m/file\nname")),
            "directory\\x1b[31m/file\\nname"
        );
    }
}
