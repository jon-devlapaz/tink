//! Offline home root (`~/.tink` or `TINK_HOME`): layout, migration, paths.
//!
//! Not an agent discovery root. Live skills stay under the project's
//! `.agents/skills/`.

use std::env;
use std::fs;
use std::path::{Component, Path, PathBuf};

use crate::error::Error;
use crate::paths::{map_io, mkdir_p, refuse_symlink, require_file};

pub const TINK_HOME_ENV: &str = "TINK_HOME";
pub const TINK_HOME_NAME: &str = ".tink";
pub const LAYOUT_FILENAME: &str = "layout.json";
pub const LAYOUT_KIND: &str = "tink-skill-inventory";
pub const BY_PROJECT: &str = "by-project";
pub const BY_SKILLSET: &str = "by-skillset";

/// Project agent directory (`.agents`) — the single owner of this layout decision.
pub const PROJECT_AGENTS_DIR: &str = ".agents";
/// Project installed-skill root (`.agents/skills`).
pub const PROJECT_SKILLS_DIR: &str = "skills";

/// Path to a project's agent directory (`.agents`).
pub fn project_agents_path(project_root: &Path) -> PathBuf {
    project_root.join(PROJECT_AGENTS_DIR)
}

/// Path to a project's installed skill root (`.agents/skills`).
pub fn project_skills_path(project_root: &Path) -> PathBuf {
    project_root
        .join(PROJECT_AGENTS_DIR)
        .join(PROJECT_SKILLS_DIR)
}

const HOME_README: &str = "\
# Tink home (`~/.tink`)

Tink home directory. This is **not** an agent skill discovery root. Agents load
skills only from a project's `.agents/skills/`.

Successful installs:
- copy skill and validated project skillset trees into the library under `skills/<name>/`
- skillsets use canonical `<name>-skillset` roots; their project tree is primary
- record skill **names** under `catalog/by-project/<project>/meta.json`
- read pinned skillset definitions from `catalog/by-skillset/<name>/meta.json`

`skill remove` and `destroy` update that name catalog; they do not delete
library trees.

Default location: `~/.tink` (override with `TINK_HOME`; relative values
resolve against the process working directory to an absolute path).
";

/// Make `path` absolute without requiring it to exist (no symlink follow).
/// Collapses `.` / `..` lexically so display and layout stay stable across cwd.
fn absolutize(path: PathBuf) -> Result<PathBuf, Error> {
    let absolute = if path.is_absolute() {
        path
    } else {
        let cwd = env::current_dir().map_err(|e| Error::msg(format!("current_dir: {e}")))?;
        cwd.join(path)
    };
    Ok(normalize_lexically(&absolute))
}

fn normalize_lexically(path: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for component in path.components() {
        match component {
            Component::Prefix(_) | Component::RootDir => out.push(component.as_os_str()),
            Component::CurDir => {}
            Component::ParentDir => {
                out.pop();
            }
            Component::Normal(part) => out.push(part),
        }
    }
    out
}

/// Resolve the offline inventory root.
pub fn resolve_home() -> Result<PathBuf, Error> {
    if let Ok(custom) = env::var(TINK_HOME_ENV) {
        if !custom.is_empty() {
            return absolutize(PathBuf::from(custom));
        }
    }
    let home = env::var_os("HOME").ok_or_else(|| Error::msg("HOME is not set"))?;
    absolutize(PathBuf::from(home).join(TINK_HOME_NAME))
}

/// Resolve an existing, owned inventory root without creating anything.
///
/// Missing homes are treated as absent. Existing homes must have the Tink
/// layout marker and safe direct owner directories before callers inspect them.
pub fn existing_inventory_root(root: Option<&Path>) -> Result<Option<PathBuf>, Error> {
    let root = match root {
        Some(path) => absolutize(path.to_path_buf())?,
        None => resolve_home()?,
    };
    refuse_symlink(&root)?;
    if !root.exists() {
        return Ok(None);
    }
    if !root.is_dir() {
        return Err(Error::msg(format!(
            "Refusing to read non-directory inventory root: {}",
            root.display()
        )));
    }
    validate_layout_marker(&root, &root.join(LAYOUT_FILENAME))?;
    validate_direct_owners(&root)?;
    Ok(Some(root))
}

/// Path to `catalog/by-project` under a home root.
pub fn by_project_path(home: &Path) -> PathBuf {
    home.join("catalog").join(BY_PROJECT)
}

/// Path to the catalog of authored skillset definitions.
pub fn by_skillset_path(home: &Path) -> PathBuf {
    home.join("catalog").join(BY_SKILLSET)
}

/// Path to the skill-tree library root (`skills/`).
pub fn skills_library_path(home: &Path) -> PathBuf {
    home.join("skills")
}

/// Ensure inventory root + library dir + catalog + layout marker.
///
/// Returns `(path, created)` where `created` is true only when the root
/// directory did not exist before this call. Relative roots are absolutized
/// against the process cwd before create/refuse checks.
pub fn ensure_inventory_root(root: Option<&Path>) -> Result<(PathBuf, bool), Error> {
    let root = match root {
        Some(path) => absolutize(path.to_path_buf())?,
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
    preflight_inventory_root(&root)?;
    mkdir_p(&root)?;
    validate_direct_owners(&root)?;
    mkdir_p(&skills_library_path(&root))?;
    migrate_catalog_if_needed(&root)?;
    mkdir_p(&by_project_path(&root))?;
    mkdir_p(&by_skillset_path(&root))?;
    write_layout_marker(&root)?;
    Ok((root, created))
}

/// Refuse to claim an unrelated existing directory as Tink home.
///
/// A valid layout marker establishes ownership. An existing empty directory is
/// also safe to initialize; every other non-empty unmarked directory is left
/// byte-for-byte untouched.
fn preflight_inventory_root(root: &Path) -> Result<(), Error> {
    if !root.exists() {
        return Ok(());
    }
    let layout = root.join(LAYOUT_FILENAME);
    require_file(&layout)?;
    if layout.is_file() {
        return validate_layout_marker(root, &layout);
    }
    let is_empty = fs::read_dir(root)
        .map_err(|e| map_io(root, e))?
        .next()
        .is_none();
    if is_empty {
        return Ok(());
    }
    Err(Error::msg(format!(
        "Refusing to initialize non-empty directory as Tink home: {}",
        root.display()
    )))
}

/// Refuse direct Tink-owned paths that would make creation or inspection
/// traverse a symlink or replace a non-directory.
fn validate_direct_owners(root: &Path) -> Result<(), Error> {
    for name in ["catalog", "skills"] {
        let owner = root.join(name);
        refuse_symlink(&owner)?;
        if owner.exists() && !owner.is_dir() {
            return Err(Error::msg(format!(
                "Refusing non-directory Tink home owner: {}",
                owner.display()
            )));
        }
    }
    Ok(())
}

/// True when `skills/by-project` looks like the old name catalog (not a skill tree).
pub(crate) fn looks_like_legacy_catalog(path: &Path) -> bool {
    path.is_dir() && !path.join("SKILL.md").is_file()
}

/// Older marked homes kept the name catalog at `skills/by-project/`; move it
/// out so `skills/<name>/` can hold library trees.
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
    // A skill tree mistakenly placed as by-project has SKILL.md — leave it.
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
        validate_layout_marker(root, &layout_path)?;
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

fn validate_layout_marker(root: &Path, layout: &Path) -> Result<(), Error> {
    refuse_symlink(layout)?;
    if !layout.is_file() {
        return Err(Error::msg(format!(
            "Not a Tink home inventory: {}",
            root.display()
        )));
    }
    let raw = fs::read_to_string(layout).map_err(|e| map_io(layout, e))?;
    let value: serde_json::Value = serde_json::from_str(&raw)
        .map_err(|_| Error::msg(format!("Not a Tink home inventory: {}", root.display())))?;
    if value.get("kind").and_then(serde_json::Value::as_str) != Some(LAYOUT_KIND) {
        return Err(Error::msg(format!(
            "Not a Tink home inventory: {}",
            root.display()
        )));
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
        assert!(skills_library_path(&root).is_dir());
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
            root.join(LAYOUT_FILENAME),
            format!("{{\n  \"kind\": \"{LAYOUT_KIND}\"\n}}\n"),
        )
        .unwrap();
        fs::write(
            legacy.join("meta.json"),
            "{\"name\":\"app\",\"root\":\"/tmp/app\",\"skills\":[\"x\"]}\n",
        )
        .unwrap();
        ensure_inventory_root(Some(&root)).unwrap();
        assert!(
            by_project_path(&root)
                .join("app")
                .join("meta.json")
                .is_file()
        );
        assert!(!root.join("skills").join(BY_PROJECT).exists());
    }

    #[test]
    fn migrate_refuses_when_both_catalog_paths_exist() {
        let temp = TempDir::new().unwrap();
        let root = temp.path().join("inv");
        fs::create_dir_all(root.join("skills").join(BY_PROJECT).join("old")).unwrap();
        fs::create_dir_all(by_project_path(&root).join("new")).unwrap();
        fs::write(
            root.join(LAYOUT_FILENAME),
            format!("{{\n  \"kind\": \"{LAYOUT_KIND}\"\n}}\n"),
        )
        .unwrap();
        let err = ensure_inventory_root(Some(&root)).unwrap_err();
        assert!(err.to_string().contains("Catalog split"), "{err}");
    }

    #[test]
    fn migrate_refuses_legacy_file() {
        let temp = TempDir::new().unwrap();
        let root = temp.path().join("inv");
        fs::create_dir_all(root.join("skills")).unwrap();
        fs::write(root.join("skills").join(BY_PROJECT), "not a dir\n").unwrap();
        fs::write(
            root.join(LAYOUT_FILENAME),
            format!("{{\n  \"kind\": \"{LAYOUT_KIND}\"\n}}\n"),
        )
        .unwrap();
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
    fn ensure_initializes_existing_empty_root() {
        let temp = TempDir::new().unwrap();
        let root = temp.path().join("inv");
        fs::create_dir(&root).unwrap();

        let (_, created) = ensure_inventory_root(Some(&root)).unwrap();

        assert!(!created);
        assert!(root.join(LAYOUT_FILENAME).is_file());
        assert!(skills_library_path(&root).is_dir());
        assert!(by_project_path(&root).is_dir());
    }

    #[test]
    fn ensure_refuses_nonempty_unmarked_root_without_writes() {
        let temp = TempDir::new().unwrap();
        let root = temp.path().join("project");
        fs::create_dir(&root).unwrap();
        let readme = root.join("README.md");
        fs::write(&readme, "# Important project\n").unwrap();
        let before = fs::read(&readme).unwrap();

        let err = ensure_inventory_root(Some(&root)).unwrap_err();

        assert!(err.to_string().contains("non-empty"), "{err}");
        assert_eq!(fs::read(&readme).unwrap(), before);
        assert!(!root.join(LAYOUT_FILENAME).exists());
        assert!(!root.join("skills").exists());
        assert!(!root.join("catalog").exists());
    }

    #[test]
    fn ensure_refuses_malformed_marker_without_writes() {
        let temp = TempDir::new().unwrap();
        let root = temp.path().join("inv");
        fs::create_dir(&root).unwrap();
        let layout = root.join(LAYOUT_FILENAME);
        fs::write(&layout, "{not-json}\n").unwrap();
        let before = fs::read(&layout).unwrap();

        let err = ensure_inventory_root(Some(&root)).unwrap_err();

        assert!(err.to_string().contains("Not a Tink home"), "{err}");
        assert_eq!(fs::read(&layout).unwrap(), before);
        assert!(!root.join("skills").exists());
        assert!(!root.join("catalog").exists());
        assert!(!root.join("README.md").exists());
    }

    #[test]
    fn existing_home_requires_marker_but_keeps_missing_home_empty() {
        let temp = TempDir::new().unwrap();
        let missing = temp.path().join("missing");
        assert!(existing_inventory_root(Some(&missing)).unwrap().is_none());

        let unmarked = temp.path().join("unmarked");
        fs::create_dir(&unmarked).unwrap();
        let err = existing_inventory_root(Some(&unmarked)).unwrap_err();
        assert!(err.to_string().contains("Not a Tink home"), "{err}");

        fs::write(
            unmarked.join(LAYOUT_FILENAME),
            format!("{{\"kind\":\"{LAYOUT_KIND}\"}}"),
        )
        .unwrap();
        assert_eq!(
            existing_inventory_root(Some(&unmarked)).unwrap(),
            Some(unmarked)
        );
    }

    #[test]
    fn existing_home_keeps_marked_legacy_layout_compatible() {
        let temp = TempDir::new().unwrap();
        let root = temp.path().join("legacy");
        fs::create_dir_all(root.join("skills").join(BY_PROJECT)).unwrap();
        fs::write(
            root.join(LAYOUT_FILENAME),
            format!("{{\"kind\":\"{LAYOUT_KIND}\"}}"),
        )
        .unwrap();

        assert_eq!(existing_inventory_root(Some(&root)).unwrap(), Some(root));
    }

    #[test]
    fn existing_home_refuses_non_directory_direct_owners() {
        let temp = TempDir::new().unwrap();
        for owner in ["catalog", "skills"] {
            let root = temp.path().join(format!("{owner}-home"));
            fs::create_dir(&root).unwrap();
            fs::write(
                root.join(LAYOUT_FILENAME),
                format!("{{\"kind\":\"{LAYOUT_KIND}\"}}"),
            )
            .unwrap();
            fs::write(root.join(owner), "not a directory\n").unwrap();

            let err = existing_inventory_root(Some(&root)).unwrap_err();

            assert!(err.to_string().contains("non-directory"), "{err}");
        }
    }

    #[test]
    fn ensure_refuses_direct_owner_symlinks_before_traversal() {
        let temp = TempDir::new().unwrap();
        for owner in ["catalog", "skills"] {
            let root = temp.path().join(owner);
            let target = temp.path().join(format!("{owner}-target"));
            fs::create_dir_all(&root).unwrap();
            fs::create_dir(&target).unwrap();
            fs::write(
                root.join(LAYOUT_FILENAME),
                format!("{{\"kind\":\"{LAYOUT_KIND}\"}}"),
            )
            .unwrap();
            std::os::unix::fs::symlink(&target, root.join(owner)).unwrap();

            let err = ensure_inventory_root(Some(&root)).unwrap_err();

            assert!(err.to_string().contains("symlink"), "{err}");
            assert!(fs::read_dir(&target).unwrap().next().is_none());
        }
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
