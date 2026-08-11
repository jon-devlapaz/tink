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
