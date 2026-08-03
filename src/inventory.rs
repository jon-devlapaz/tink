//! Offline home inventory root (`~/.tink` or `TINK_HOME`).
//!
//! Not an agent discovery root. Skills live only under the project's
//! `.agents/skills/`.

use std::env;
use std::fs;
use std::path::{Path, PathBuf};

use crate::error::Error;
use crate::paths::{map_io, mkdir_p, refuse_symlink, require_file};

pub const TINK_HOME_ENV: &str = "TINK_HOME";
pub const TINK_HOME_NAME: &str = ".tink";
pub const LAYOUT_FILENAME: &str = "layout.json";
pub const LAYOUT_KIND: &str = "tink-skill-inventory";

const INVENTORY_README: &str = "\
# Tink home (`~/.tink`)

Tink home directory. This is **not** an agent skill discovery root. Agents load
skills only from a project's `.agents/skills/`.

Default location: `~/.tink` (override with `TINK_HOME`).
";

/// Resolve the offline inventory root.
pub fn resolve_home() -> Result<PathBuf, Error> {
    if let Ok(custom) = env::var(TINK_HOME_ENV) {
        if !custom.is_empty() {
            return Ok(PathBuf::from(custom));
        }
    }
    let home = env::var_os("HOME").ok_or_else(|| Error::msg("HOME is not set"))?;
    Ok(PathBuf::from(home).join(TINK_HOME_NAME))
}

/// Ensure inventory root + layout marker.
///
/// Returns `(path, created)` where `created` is true only when the root
/// directory did not exist before this call.
pub fn ensure_inventory_root(root: Option<&Path>) -> Result<(PathBuf, bool), Error> {
    let root = match root {
        Some(path) => path.to_path_buf(),
        None => resolve_home()?,
    };
    refuse_symlink(&root)?;
    let created = !root.exists();
    if root.exists() && !root.is_dir() {
        return Err(Error::msg(format!(
            "Refusing to replace non-directory inventory root: {}",
            root.display()
        )));
    }
    mkdir_p(&root)?;
    write_layout_marker(&root)?;
    Ok((root, created))
}

fn write_layout_marker(root: &Path) -> Result<(), Error> {
    let layout_path = root.join(LAYOUT_FILENAME);
    require_file(&layout_path)?;
    if layout_path.is_file() {
        let existing = fs::read_to_string(&layout_path).map_err(|e| map_io(&layout_path, e))?;
        if !existing.contains(LAYOUT_KIND) {
            return Err(Error::msg(format!(
                "Not a Tink home inventory: {}",
                root.display()
            )));
        }
    } else {
        let body = format!("{{\n  \"kind\": \"{LAYOUT_KIND}\"\n}}\n");
        fs::write(&layout_path, body).map_err(|e| map_io(&layout_path, e))?;
    }

    let readme = root.join("README.md");
    require_file(&readme)?;
    if !readme.exists() {
        fs::write(&readme, INVENTORY_README).map_err(|e| map_io(&readme, e))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn ensure_creates_layout() {
        let temp = TempDir::new().unwrap();
        let root = temp.path().join("inv");
        let (path, created) = ensure_inventory_root(Some(&root)).unwrap();
        assert!(created);
        assert_eq!(path, root);
        assert!(root.join("layout.json").is_file());
        let (_, created_again) = ensure_inventory_root(Some(&root)).unwrap();
        assert!(!created_again);
    }

    #[test]
    fn refuse_symlink_root() {
        let temp = TempDir::new().unwrap();
        let real = temp.path().join("real");
        fs::create_dir(&real).unwrap();
        let link = temp.path().join("link");
        std::os::unix::fs::symlink(&real, &link).unwrap();
        let err = ensure_inventory_root(Some(&link)).unwrap_err();
        assert!(err.to_string().contains("symlink"));
    }
}
