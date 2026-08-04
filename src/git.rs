//! Git checkout helpers for public GitHub sources.

use std::path::{Path, PathBuf};
use std::process::Command;

use tempfile::TempDir;

use crate::error::Error;
use crate::sources::RemoteSource;

/// Resolve the remote default branch tip without a full clone.
pub fn remote_head(remote: &RemoteSource) -> Result<String, Error> {
    let output = Command::new("git")
        .args(["ls-remote", "--quiet", &remote.url, "HEAD"])
        .env("GIT_TERMINAL_PROMPT", "0")
        .output()
        .map_err(|e| {
            if e.kind() == std::io::ErrorKind::NotFound {
                Error::msg("Git is required for remote skill sources")
            } else {
                Error::msg(format!("git ls-remote: {e}"))
            }
        })?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let detail = stderr
            .lines()
            .last()
            .unwrap_or("git ls-remote failed")
            .trim();
        return Err(Error::msg(format!(
            "Could not resolve {}: {detail}",
            remote.url
        )));
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    let revision = stdout
        .lines()
        .next()
        .and_then(|line| line.split_whitespace().next())
        .ok_or_else(|| Error::msg(format!("Could not resolve HEAD for {}", remote.url)))?
        .to_string();
    if revision.len() != 40 && revision.len() != 64
        || !revision.chars().all(|c| c.is_ascii_hexdigit())
    {
        return Err(Error::msg(format!(
            "Could not resolve HEAD for {}: got {revision}",
            remote.url
        )));
    }
    Ok(revision)
}

/// Clone `remote` into a temporary directory; return `(temp_keep_alive, repo_path, revision)`.
pub fn checkout(remote: &RemoteSource) -> Result<(TempDir, PathBuf, String), Error> {
    let temp = TempDir::new().map_err(|e| Error::msg(format!("temp dir: {e}")))?;
    let repository = temp.path().join("repository");
    let output = Command::new("git")
        .args([
            "clone",
            "--quiet",
            "--no-tags",
            &remote.url,
            repository.to_str().ok_or_else(|| Error::msg("non-utf8 path"))?,
        ])
        .env("GIT_TERMINAL_PROMPT", "0")
        .output()
        .map_err(|e| {
            if e.kind() == std::io::ErrorKind::NotFound {
                Error::msg("Git is required for remote skill sources")
            } else {
                Error::msg(format!("git clone: {e}"))
            }
        })?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let detail = stderr
            .lines()
            .last()
            .unwrap_or("git clone failed")
            .trim();
        return Err(Error::msg(format!(
            "Could not fetch {}: {detail}",
            remote.url
        )));
    }
    let revision = git_stdout(&repository, &["rev-parse", "HEAD"])?;
    let repository = repository
        .canonicalize()
        .map_err(|e| Error::msg(format!("repository: {e}")))?;
    Ok((temp, repository, revision))
}

/// Detached worktree at `revision`; returns `(temp_keep_alive, worktree_path)`.
pub fn checkout_revision(
    repository: &Path,
    revision: &str,
) -> Result<(TempDir, PathBuf), Error> {
    let temp = TempDir::new().map_err(|e| Error::msg(format!("temp dir: {e}")))?;
    let worktree = temp.path().join("worktree");
    let output = Command::new("git")
        .args([
            "-C",
            repository
                .to_str()
                .ok_or_else(|| Error::msg("non-utf8 path"))?,
            "worktree",
            "add",
            "--quiet",
            "--detach",
            worktree
                .to_str()
                .ok_or_else(|| Error::msg("non-utf8 path"))?,
            revision,
        ])
        .output()
        .map_err(|e| {
            if e.kind() == std::io::ErrorKind::NotFound {
                Error::msg("Git is required to verify recorded skill revisions")
            } else {
                Error::msg(format!("git worktree: {e}"))
            }
        })?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let detail = stderr
            .lines()
            .last()
            .unwrap_or("revision unavailable")
            .trim();
        return Err(Error::msg(format!(
            "Could not read recorded revision {revision}: {detail}"
        )));
    }
    let resolved = worktree
        .canonicalize()
        .map_err(|e| Error::msg(format!("worktree: {e}")))?;
    Ok((temp, resolved))
}

pub fn git_stdout(cwd: &Path, args: &[&str]) -> Result<String, Error> {
    let output = Command::new("git")
        .args(args)
        .current_dir(cwd)
        .output()
        .map_err(|e| Error::msg(format!("git: {e}")))?;
    if !output.status.success() {
        return Err(Error::msg(format!(
            "git {} failed",
            args.first().unwrap_or(&"")
        )));
    }
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}
