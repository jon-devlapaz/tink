//! Shared refusals for unsafe paths.

use std::io;
use std::path::Path;

use crate::error::Error;

pub fn refuse_symlink(path: &Path) -> Result<(), Error> {
    if path.is_symlink() {
        return Err(Error::msg(format!(
            "Refusing to follow symlink: {}",
            path.display()
        )));
    }
    Ok(())
}

pub fn require_directory(path: &Path) -> Result<(), Error> {
    refuse_symlink(path)?;
    if path.exists() && !path.is_dir() {
        return Err(Error::msg(format!(
            "Refusing to replace non-directory: {}",
            path.display()
        )));
    }
    Ok(())
}

pub fn require_file(path: &Path) -> Result<(), Error> {
    refuse_symlink(path)?;
    if path.exists() && !path.is_file() {
        return Err(Error::msg(format!(
            "Refusing to replace non-file: {}",
            path.display()
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

pub fn map_io(path: &Path, err: io::Error) -> Error {
    Error::msg(format!("{}: {err}", path.display()))
}
