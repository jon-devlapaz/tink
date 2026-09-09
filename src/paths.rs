//! Shared refusals for unsafe paths.

use std::io;
use std::path::{Component, Path, PathBuf};

use crate::error::Error;
use crate::output;

pub fn refuse_symlink(path: &Path) -> Result<(), Error> {
    if path.is_symlink() {
        return Err(Error::msg(format!(
            "Refusing to follow symlink: {}",
            output::display_path(path)
        )));
    }
    Ok(())
}

pub fn require_directory(path: &Path) -> Result<(), Error> {
    refuse_symlink(path)?;
    if path.exists() && !path.is_dir() {
        return Err(Error::msg(format!(
            "Refusing to replace non-directory: {}",
            output::display_path(path)
        )));
    }
    Ok(())
}

pub fn require_file(path: &Path) -> Result<(), Error> {
    refuse_symlink(path)?;
    if path.exists() && !path.is_file() {
        return Err(Error::msg(format!(
            "Refusing to replace non-file: {}",
            output::display_path(path)
        )));
    }
    Ok(())
}

pub fn mkdir_p(path: &Path) -> Result<(), Error> {
    require_directory(path)?;
    if !path.exists() {
        std::fs::create_dir_all(path).map_err(|e| map_io(path, e))?;
    }
    Ok(())
}

/// Resolve a caller-validated relative path beneath a trusted base without
/// following symlinks in any relative component.
pub fn canonicalize_beneath(base: &Path, relative: &Path) -> Result<PathBuf, Error> {
    if relative.as_os_str().is_empty() || relative.is_absolute() {
        return Err(Error::msg(format!(
            "Path must be non-empty and relative: {}",
            output::display_path(relative)
        )));
    }

    let mut candidate = base.to_path_buf();
    for component in relative.components() {
        match component {
            Component::Normal(part) => {
                candidate.push(part);
                refuse_symlink(&candidate)?;
            }
            Component::CurDir => {}
            Component::ParentDir | Component::RootDir | Component::Prefix(_) => {
                return Err(Error::msg(format!(
                    "Refusing path outside trusted root: {}",
                    output::display_path(&base.join(relative))
                )));
            }
        }
    }

    let canonical_base = base.canonicalize().map_err(|error| map_io(base, error))?;
    let canonical_candidate = candidate.canonicalize().map_err(|error| {
        if error.kind() == io::ErrorKind::NotFound {
            Error::msg(format!(
                "Path does not exist: {}",
                output::display_path(&candidate)
            ))
        } else {
            map_io(&candidate, error)
        }
    })?;
    if !canonical_candidate.starts_with(&canonical_base) {
        return Err(Error::msg(format!(
            "Refusing path outside trusted root: {}",
            output::display_path(&candidate)
        )));
    }
    Ok(canonical_candidate)
}

pub fn map_io(path: &Path, err: io::Error) -> Error {
    Error::msg(format!("{}: {err}", output::display_path(path)))
}

/// Durable recovery path beside `destination_root` for a displaced file or tree
/// named like `displaced`.
pub fn orphan_recovery_path(destination_root: &Path, displaced: &Path) -> PathBuf {
    let name = displaced
        .file_name()
        .unwrap_or_else(|| std::ffi::OsStr::new("artifact"));
    let suffix = format!(
        "{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|duration| duration.as_nanos())
            .unwrap_or(0)
    );
    destination_root.join(format!(".tink-orphan-{}-{suffix}", name.to_string_lossy()))
}

/// Rename a recovery backup to a durable orphan path beside `destination_root`.
pub fn move_file_to_orphan(
    backup: &Path,
    destination_root: &Path,
    displaced: &Path,
) -> Result<PathBuf, std::io::Error> {
    let orphan = orphan_recovery_path(destination_root, displaced);
    std::fs::rename(backup, &orphan).map(|()| orphan)
}

/// Outcome of attempting rollback, then orphaning the backup on rollback failure.
#[derive(Debug)]
pub enum RestoreOrOrphan {
    /// Rollback restored the live path; caller should surface the publish failure.
    Restored,
    /// Rollback failed; backup moved to a durable `.tink-orphan-*` path.
    Orphaned {
        recovery: PathBuf,
        rollback_error: io::Error,
    },
    /// Rollback and orphan move both failed; backup retained at `recovery`.
    Retained {
        recovery: PathBuf,
        orphan_path: PathBuf,
        rollback_error: io::Error,
        orphan_error: io::Error,
    },
}

fn orphan_or_retain(
    backup: &Path,
    destination_root: &Path,
    displaced: &Path,
    retain_backup: impl FnOnce() -> PathBuf,
) -> Result<PathBuf, (PathBuf, io::Error)> {
    match move_file_to_orphan(backup, destination_root, displaced) {
        Ok(recovery) => Ok(recovery),
        Err(orphan_error) => Err((retain_backup(), orphan_error)),
    }
}

/// Try restoring `backup` over `displaced`; on failure move the backup to a durable
/// orphan path beside `destination_root`, or retain it via `retain_backup` when the
/// orphan rename fails.
pub fn restore_or_orphan(
    restore: impl FnOnce() -> Result<(), io::Error>,
    backup: &Path,
    destination_root: &Path,
    displaced: &Path,
    retain_backup: impl FnOnce() -> PathBuf,
) -> RestoreOrOrphan {
    match restore() {
        Ok(()) => RestoreOrOrphan::Restored,
        Err(rollback_error) => {
            match orphan_or_retain(backup, destination_root, displaced, retain_backup) {
                Ok(recovery) => RestoreOrOrphan::Orphaned {
                    recovery,
                    rollback_error,
                },
                Err((recovery, orphan_error)) => {
                    let orphan_path = orphan_recovery_path(destination_root, displaced);
                    RestoreOrOrphan::Retained {
                        recovery,
                        orphan_path,
                        rollback_error,
                        orphan_error,
                    }
                }
            }
        }
    }
}

/// Like [`restore_or_orphan`] when rollback already failed and only orphan-or-retain
/// remains (for example after a consuming restore attempt).
pub fn orphan_or_retain_after_restore_failure(
    rollback_error: io::Error,
    backup: &Path,
    destination_root: &Path,
    displaced: &Path,
    retain_backup: impl FnOnce() -> PathBuf,
) -> RestoreOrOrphan {
    match orphan_or_retain(backup, destination_root, displaced, retain_backup) {
        Ok(recovery) => RestoreOrOrphan::Orphaned {
            recovery,
            rollback_error,
        },
        Err((recovery, orphan_error)) => RestoreOrOrphan::Retained {
            recovery,
            orphan_path: orphan_recovery_path(destination_root, displaced),
            rollback_error,
            orphan_error,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    #[test]
    fn restore_or_orphan_restores_on_successful_rollback() {
        let temp = TempDir::new().unwrap();
        let backup = temp.path().join("backup.txt");
        let target = temp.path().join("target.txt");
        fs::write(&backup, "backup-bytes").unwrap();
        fs::write(&target, "live-bytes").unwrap();

        let outcome = restore_or_orphan(
            || fs::rename(&backup, &target),
            &backup,
            temp.path(),
            &target,
            || temp.path().join("unused"),
        );

        assert!(matches!(outcome, RestoreOrOrphan::Restored));
        assert_eq!(fs::read(&target).unwrap(), b"backup-bytes");
        assert!(!backup.exists());
    }

    #[test]
    fn restore_or_orphan_moves_backup_to_durable_orphan_on_double_failure() {
        let temp = TempDir::new().unwrap();
        let backup = temp.path().join("old");
        fs::create_dir_all(&backup).unwrap();
        fs::write(backup.join("payload.txt"), "preserve me").unwrap();
        let target = temp.path().join("target");
        fs::write(&target, "rollback blocker").unwrap();

        let outcome = restore_or_orphan(
            || fs::rename(&backup, &target),
            &backup,
            temp.path(),
            &target,
            || temp.path().join("fallback"),
        );

        let RestoreOrOrphan::Orphaned { recovery, .. } = outcome else {
            panic!("expected orphaned backup, got {outcome:?}");
        };
        assert!(
            recovery
                .file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.starts_with(".tink-orphan-target-")),
            "unexpected orphan name: {}",
            recovery.display()
        );
        assert_eq!(
            fs::read(recovery.join("payload.txt")).unwrap(),
            b"preserve me"
        );
        assert!(!backup.exists());
    }
}
