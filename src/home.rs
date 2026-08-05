//! Offline home root (`~/.tink` or `TINK_HOME`): layout, migration, paths.
//!
//! Not an agent discovery root. Live skills stay under the project's
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
pub const BY_PROJECT: &str = "by-project";

const HOME_README: &str = "\
# Tink home (`~/.tink`)

Tink home directory. This is **not** an agent skill discovery root. Agents load
skills only from a project's `.agents/skills/`.

Successful installs:
- stash skill trees under `skills/<name>/` (divergent entries are repaired)
- record skill **names** under `catalog/by-project/<project>/meta.json`

`skill remove` and `destroy` update that name catalog; they do not delete
stash trees.

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

/// Path to `catalog/by-project` under a home root.
pub fn by_project_path(home: &Path) -> PathBuf {
    home.join("catalog").join(BY_PROJECT)
}

/// Path to the skill-tree stash root (`skills/`).
pub fn skills_stash_path(home: &Path) -> PathBuf {
    home.join("skills")
}

/// Ensure inventory root + stash dir + catalog + layout marker.
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
    mkdir_p(&skills_stash_path(&root))?;
    migrate_catalog_if_needed(&root)?;
    mkdir_p(&by_project_path(&root))?;
    write_layout_marker(&root)?;
    Ok((root, created))
}

/// True when `skills/by-project` looks like the old name catalog (not a skill tree).
pub(crate) fn looks_like_legacy_catalog(path: &Path) -> bool {
    path.is_dir() && !path.join("SKILL.md").is_file()
}

/// Older homes kept the name catalog at `skills/by-project/`; move it out so
/// `skills/<name>/` can hold stashed trees.
fn migrate_catalog_if_needed(home: &Path) -> Result<(), Error> {
    let old = home.join("skills").join(BY_PROJECT);
    let new = by_project_path(home);
    if !old.exists() {
        return Ok(());
    }
    if old.is_symlink() {
        return Err(Error::msg(format!(
            "Refusing legacy catalog symlink {}: replace it with a real directory, then re-run",
            old.display()
        )));
    }
    if !old.is_dir() {
        return Err(Error::msg(format!(
            "Refusing non-directory legacy catalog {}: move or delete it, then re-run",
            old.display()
        )));
    }
    // A skill tree mistakenly stashed as by-project has SKILL.md — leave it.
    if !looks_like_legacy_catalog(&old) {
        return Ok(());
    }
    if new.exists() {
        return Err(Error::msg(format!(
            "Catalog split across {} and {}: keep catalog/by-project, remove skills/by-project, then re-run",
            old.display(),
            new.display()
        )));
    }
    mkdir_p(&home.join("catalog"))?;
    fs::rename(&old, &new).map_err(|e| map_io(&old, e))?;
    Ok(())
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
    let refresh_readme = if readme.is_file() {
        let existing = fs::read_to_string(&readme).map_err(|e| map_io(&readme, e))?;
        existing.contains("skills/by-project") || !existing.contains("catalog/by-project")
    } else {
        true
    };
    if refresh_readme {
        fs::write(&readme, HOME_README).map_err(|e| map_io(&readme, e))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn ensure_creates_layout_and_catalog() {
        let temp = TempDir::new().unwrap();
        let root = temp.path().join("inv");
        let (path, created) = ensure_inventory_root(Some(&root)).unwrap();
        assert!(created);
        assert_eq!(path, root);
        assert!(root.join("layout.json").is_file());
        assert!(by_project_path(&root).is_dir());
        assert!(skills_stash_path(&root).is_dir());
        assert!(!root.join("skills").join(BY_PROJECT).exists());
        let (_, created_again) = ensure_inventory_root(Some(&root)).unwrap();
        assert!(!created_again);
    }

    #[test]
    fn migrate_moves_legacy_by_project() {
        let temp = TempDir::new().unwrap();
        let root = temp.path().join("inv");
        let legacy = root.join("skills").join(BY_PROJECT).join("app");
        fs::create_dir_all(&legacy).unwrap();
        fs::write(
            legacy.join("meta.json"),
            "{\"name\":\"app\",\"root\":\"/tmp/app\",\"skills\":[\"x\"]}\n",
        )
        .unwrap();
        ensure_inventory_root(Some(&root)).unwrap();
        assert!(by_project_path(&root).join("app").join("meta.json").is_file());
        assert!(!root.join("skills").join(BY_PROJECT).exists());
    }

    #[test]
    fn migrate_refuses_when_both_catalog_paths_exist() {
        let temp = TempDir::new().unwrap();
        let root = temp.path().join("inv");
        fs::create_dir_all(root.join("skills").join(BY_PROJECT).join("old")).unwrap();
        fs::create_dir_all(by_project_path(&root).join("new")).unwrap();
        let err = ensure_inventory_root(Some(&root)).unwrap_err();
        assert!(err.to_string().contains("Catalog split"), "{err}");
    }

    #[test]
    fn migrate_refuses_legacy_file() {
        let temp = TempDir::new().unwrap();
        let root = temp.path().join("inv");
        fs::create_dir_all(root.join("skills")).unwrap();
        fs::write(root.join("skills").join(BY_PROJECT), "not a dir\n").unwrap();
        let err = ensure_inventory_root(Some(&root)).unwrap_err();
        assert!(
            err.to_string().contains("non-directory legacy catalog"),
            "{err}"
        );
    }

    #[test]
    fn ensure_refreshes_stale_home_readme() {
        let temp = TempDir::new().unwrap();
        let root = temp.path().join("inv");
        fs::create_dir_all(&root).unwrap();
        fs::write(
            root.join("layout.json"),
            format!("{{\n  \"kind\": \"{LAYOUT_KIND}\"\n}}\n"),
        )
        .unwrap();
        fs::write(
            root.join("README.md"),
            "old text mentioning skills/by-project only\n",
        )
        .unwrap();
        ensure_inventory_root(Some(&root)).unwrap();
        let readme = fs::read_to_string(root.join("README.md")).unwrap();
        assert!(readme.contains("catalog/by-project"));
        assert!(!readme.contains("skills/by-project/<project>"));
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
