//! Git checkout helpers for public GitHub sources.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

use tempfile::TempDir;

use crate::error::Error;
use crate::sources::RemoteSource;

const GIT_LOW_SPEED_LIMIT_SETTING: &str = "http.lowSpeedLimit=1024";
const GIT_LOW_SPEED_TIME_SETTING: &str = "http.lowSpeedTime=30";
const GIT_TIMEOUT: Duration = Duration::from_secs(300);

fn git_transport_args() -> [&'static str; 4] {
    [
        "-c",
        GIT_LOW_SPEED_LIMIT_SETTING,
        "-c",
        GIT_LOW_SPEED_TIME_SETTING,
    ]
}

/// Full argv prefix + subcommand args for every git spawn (proves transport flags).
fn git_command_args<'a>(args: &[&'a str]) -> Vec<&'a str> {
    let mut full = Vec::with_capacity(git_transport_args().len() + args.len());
    full.extend_from_slice(&git_transport_args());
    full.extend_from_slice(args);
    full
}

fn git_subcommand<'a>(args: &[&'a str]) -> Option<&'a str> {
    let mut index = 0;
    while let Some(argument) = args.get(index) {
        match *argument {
            "-C" | "-c" | "--git-dir" | "--work-tree" | "--namespace" => index += 2,
            value if value.starts_with('-') => index += 1,
            value => return Some(value),
        }
    }
    None
}

fn guard_git_command(args: &[&str]) -> Result<(), Error> {
    if matches!(
        git_subcommand(args),
        Some("init" | "add" | "commit" | "push")
    ) {
        return Err(Error::msg("Tink refuses project-mutating Git commands"));
    }
    Ok(())
}

fn run_git(
    args: &[&str],
    cwd: Option<&Path>,
    non_interactive: bool,
    io_context: &str,
    missing_message: Option<&str>,
) -> Result<std::process::Output, Error> {
    guard_git_command(args)?;
    let mut command = Command::new("git");
    command.args(git_command_args(args));
    if non_interactive {
        command.env("GIT_TERMINAL_PROMPT", "0");
    }
    if let Some(cwd) = cwd {
        command.current_dir(cwd);
    }

    crate::process::run_bounded(&mut command, GIT_TIMEOUT, io_context, missing_message)
}

fn git_detail(stderr: &[u8], fallback: &str) -> String {
    let stderr = String::from_utf8_lossy(stderr);
    stderr.lines().last().unwrap_or(fallback).trim().to_string()
}

/// Resolve the remote default branch tip without a full clone.
pub fn remote_head(remote: &RemoteSource) -> Result<String, Error> {
    let output = run_git(
        &["ls-remote", "--quiet", &remote.url, "HEAD"],
        None,
        true,
        "git ls-remote",
        Some("Git is required for remote skill sources"),
    )?;
    if !output.status.success() {
        let detail = git_detail(&output.stderr, "git ls-remote failed");
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

/// List branch and tag names advertised by a remote.
pub fn remote_ref_names(remote: &RemoteSource) -> Result<BTreeSet<String>, Error> {
    let output = run_git(
        &["ls-remote", "--quiet", "--heads", "--tags", &remote.url],
        None,
        true,
        "git ls-remote",
        Some("Git is required for remote skill sources"),
    )?;
    if !output.status.success() {
        let detail = git_detail(&output.stderr, "git ls-remote failed");
        return Err(Error::msg(format!(
            "Could not resolve refs for {}: {detail}",
            remote.url
        )));
    }

    let mut names = BTreeSet::new();
    for line in String::from_utf8_lossy(&output.stdout).lines() {
        let Some(reference) = line.split_whitespace().nth(1) else {
            continue;
        };
        let reference = reference.trim_end_matches("^{}");
        if let Some(name) = reference
            .strip_prefix("refs/heads/")
            .or_else(|| reference.strip_prefix("refs/tags/"))
        {
            names.insert(name.to_string());
        }
    }
    Ok(names)
}

/// Clone `remote` into a temporary directory; return `(temp_keep_alive, repo_path, revision)`.
pub fn checkout(remote: &RemoteSource) -> Result<(TempDir, PathBuf, String), Error> {
    checkout_ref(remote, None)
}

/// Clone `remote`, optionally checking out the requested branch or tag.
pub fn checkout_ref(
    remote: &RemoteSource,
    requested_ref: Option<&str>,
) -> Result<(TempDir, PathBuf, String), Error> {
    let temp = TempDir::new().map_err(|e| Error::msg(format!("temp dir: {e}")))?;
    let repository = temp.path().join("repository");
    let repository_string = repository
        .to_str()
        .ok_or_else(|| Error::msg("non-utf8 path"))?
        .to_string();
    let mut args = vec!["clone", "--quiet", "--no-tags"];
    if let Some(requested_ref) = requested_ref {
        args.push("--branch");
        args.push(requested_ref);
    }
    args.push(&remote.url);
    args.push(&repository_string);
    let output = run_git(
        &args,
        None,
        true,
        "git clone",
        Some("Git is required for remote skill sources"),
    )?;
    if !output.status.success() {
        let detail = git_detail(&output.stderr, "git clone failed");
        let subject = requested_ref
            .map(|requested_ref| format!(" ref {requested_ref}"))
            .unwrap_or_default();
        return Err(Error::msg(format!(
            "Could not fetch {}{}: {detail}",
            remote.url, subject
        )));
    }
    let revision = git_stdout(&repository, &["rev-parse", "HEAD"])?;
    let repository = repository
        .canonicalize()
        .map_err(|e| Error::msg(format!("repository: {e}")))?;
    Ok((temp, repository, revision))
}

/// Detached worktree at `revision`; returns `(temp_keep_alive, worktree_path)`.
pub fn checkout_revision(repository: &Path, revision: &str) -> Result<(TempDir, PathBuf), Error> {
    let temp = TempDir::new().map_err(|e| Error::msg(format!("temp dir: {e}")))?;
    let worktree = temp.path().join("worktree");
    let output = run_git(
        &[
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
        ],
        None,
        false,
        "git worktree",
        Some("Git is required to verify recorded skill revisions"),
    )?;
    if !output.status.success() {
        let detail = git_detail(&output.stderr, "revision unavailable");
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
    let output = run_git(args, Some(cwd), false, "git", None)?;
    if !output.status.success() {
        return Err(Error::msg(format!(
            "git {} failed",
            args.first().unwrap_or(&"")
        )));
    }
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn git_command_args_prepend_transport_timeouts() {
        assert_eq!(
            git_command_args(&["ls-remote", "--quiet", "https://example.test", "HEAD"]),
            [
                "-c",
                GIT_LOW_SPEED_LIMIT_SETTING,
                "-c",
                GIT_LOW_SPEED_TIME_SETTING,
                "ls-remote",
                "--quiet",
                "https://example.test",
                "HEAD",
            ]
        );
    }

    #[test]
    fn git_guard_rejects_project_mutations_and_allows_owned_reads() {
        for args in [
            &["init"][..],
            &["add", "."][..],
            &["commit", "-m", "message"][..],
            &["push", "origin", "main"][..],
            &["-C", "/tmp/repo", "commit"][..],
        ] {
            assert!(guard_git_command(args).is_err(), "must refuse {args:?}");
        }

        for args in [
            &["ls-remote", "https://example.test", "HEAD"][..],
            &["clone", "https://example.test/repo.git", "/tmp/repo"][..],
            &["rev-parse", "HEAD"][..],
            &["-C", "/tmp/repo", "worktree", "add", "/tmp/tree"][..],
        ] {
            assert!(guard_git_command(args).is_ok(), "must allow {args:?}");
        }
    }
}
