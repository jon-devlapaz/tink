//! Offline home root (`~/.tink` or `TINK_HOME`): layout, migration, paths.
//!
//! Not an agent discovery root. Live skills stay under the project's
//! `.agents/skills/`.

use std::env;
use std::fs;
use std::path::{Component, Path, PathBuf};

use crate::error::Error;
use crate::output;
use crate::paths::{map_io, mkdir_p, refuse_symlink, require_file};

pub const TINK_HOME_ENV: &str = "TINK_HOME";
pub const TINK_HOME_NAME: &str = ".tink";
pub const LAYOUT_FILENAME: &str = "layout.json";
pub const LAYOUT_KIND: &str = "tink-skill-inventory";

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
- copy standalone skill trees into the library under `skills/<name>/`
- mirror validated project skillset trees under `skillsets/<name>-skillset/`
- skillsets use canonical `<name>-skillset` roots; their project tree is primary
- read pinned skillset definitions from `skillsets/<name>.json` (sibling of the
  mirrored tree directory `skillsets/<name>/`)

`skill remove` and `destroy` delete project trees only; they do not prune
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
    if let Ok(custom) = env::var(TINK_HOME_ENV)
        && !custom.is_empty()
    {
        return absolutize(PathBuf::from(custom));
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

/// Path to a skillset desired-pin file (`skillsets/<name>.json`).
///
/// Sibling of the mirrored tree at `skillsets/<name>/`; never nested inside it.
pub fn skillset_pin_path(home: &Path, name: &str) -> PathBuf {
    skillsets_library_path(home).join(format!("{name}.json"))
}

/// Path to the standalone skill-tree library root (`skills/`).
pub fn skills_library_path(home: &Path) -> PathBuf {
    home.join("skills")
}

/// Path to the derived skillset library root (`skillsets/`).
pub fn skillsets_library_path(home: &Path) -> PathBuf {
    home.join("skillsets")
}

/// Ensure inventory root + library dirs + layout marker.
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
    publish_layout_marker(&root)?;
    mkdir_p(&skills_library_path(&root))?;
    mkdir_p(&skillsets_library_path(&root))?;
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
    for name in ["skills", "skillsets"] {
        let owner = root.join(name);
        refuse_symlink(&owner)?;
        if owner.exists() && !owner.is_dir() {
            return Err(Error::msg(format!(
                "Refusing non-directory Tink home owner: {}",
                output::display_path(&owner)
            )));
        }
    }
    Ok(())
}

fn write_layout_marker(root: &Path) -> Result<(), Error> {
    publish_layout_marker(root)?;

    let readme = root.join("README.md");
    require_file(&readme)?;
    let refresh_readme = if readme.is_file() {
        let existing = fs::read_to_string(&readme).map_err(|e| map_io(&readme, e))?;
        existing.contains("skills/by-project")
            || existing.contains("catalog/by-project")
            || existing.contains("catalog/by-skillset")
            || !existing.contains("skillsets/<name>.json")
    } else {
        true
    };
    if refresh_readme {
        fs::write(&readme, HOME_README).map_err(|e| map_io(&readme, e))?;
    }
    Ok(())
}

/// Publish the ownership marker before creating owned child directories.
fn publish_layout_marker(root: &Path) -> Result<(), Error> {
    let layout_path = root.join(LAYOUT_FILENAME);
    require_file(&layout_path)?;
    if layout_path.is_file() {
        validate_layout_marker(root, &layout_path)?;
    } else {
        let body = format!("{{\n  \"kind\": \"{LAYOUT_KIND}\"\n}}\n");
        fs::write(&layout_path, body).map_err(|e| map_io(&layout_path, e))?;
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
    fn ensure_creates_layout_and_libraries() {
        let temp = TempDir::new().unwrap();
        let root = temp.path().join("inv");
        let (path, created) = ensure_inventory_root(Some(&root)).unwrap();
        assert!(created);
        assert_eq!(path, root);
        assert!(root.join("layout.json").is_file());
        assert!(skills_library_path(&root).is_dir());
        assert!(skillsets_library_path(&root).is_dir());
        assert!(!root.join("catalog").exists());
        let (_, created_again) = ensure_inventory_root(Some(&root)).unwrap();
        assert!(!created_again);
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
            "old text mentioning catalog/by-project only\n",
        )
        .unwrap();
        ensure_inventory_root(Some(&root)).unwrap();
        let readme = fs::read_to_string(root.join("README.md")).unwrap();
        assert!(readme.contains("skillsets/<name>.json"));
        assert!(!readme.contains("catalog/by-project"));
        assert!(!readme.contains("catalog/by-skillset"));
    }

    #[test]
    fn skillset_pin_is_sibling_json_not_inside_tree() {
        let home = Path::new("/tmp/tink-home");
        let pin = skillset_pin_path(home, "common-skillset");
        assert_eq!(
            pin,
            PathBuf::from("/tmp/tink-home/skillsets/common-skillset.json")
        );
        assert_ne!(
            pin,
            skillsets_library_path(home)
                .join("common-skillset")
                .join("meta.json")
        );
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
        assert!(skillsets_library_path(&root).is_dir());
        assert!(!root.join("catalog").exists());
    }

    #[test]
    fn ensure_resumes_after_marker_only_interruption() {
        let temp = TempDir::new().unwrap();
        let root = temp.path().join("inv");
        fs::create_dir(&root).unwrap();

        publish_layout_marker(&root).unwrap();
        assert!(root.join(LAYOUT_FILENAME).is_file());
        assert!(!root.join("README.md").exists());
        assert!(!root.join("skills").exists());
        assert!(!root.join("skillsets").exists());
        assert!(!root.join("catalog").exists());

        ensure_inventory_root(Some(&root)).unwrap();

        assert!(root.join("README.md").is_file());
        assert!(skills_library_path(&root).is_dir());
        assert!(skillsets_library_path(&root).is_dir());
        assert!(!root.join("catalog").exists());
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
    fn existing_home_tolerates_leftover_by_project_dirs() {
        let temp = TempDir::new().unwrap();
        let root = temp.path().join("legacy");
        fs::create_dir_all(root.join("skills").join("by-project")).unwrap();
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
        for owner in ["skills", "skillsets"] {
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
        for owner in ["skills", "skillsets"] {
            let root = temp.path().join(owner);
            let target = temp.path().join(format!("{owner}-target"));
            fs::create_dir_all(&root).unwrap();
            fs::create_dir(&target).unwrap();
            fs::write(
                root.join(LAYOUT_FILENAME),
                format!("{{\"kind\":\"{LAYOUT_KIND}\"}}"),
            )
            .unwrap();
            let readme = root.join("README.md");
            fs::write(&readme, "existing home text\n").unwrap();
            let before = fs::read(&readme).unwrap();
            std::os::unix::fs::symlink(&target, root.join(owner)).unwrap();

            let err = ensure_inventory_root(Some(&root)).unwrap_err();

            assert!(err.to_string().contains("symlink"), "{err}");
            assert!(fs::read_dir(&target).unwrap().next().is_none());
            assert_eq!(fs::read(&readme).unwrap(), before);
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
