//! By-project name catalog under `$TINK_HOME/catalog/by-project/`.
//!
//! Names are recorded on install and dropped on `skill remove`; a project's
//! catalog entry is removed on `destroy` (or when the last name is withdrawn).

use std::collections::BTreeSet;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use serde_json::{Value, json};
use sha2::{Digest, Sha256};

use crate::error::Error;
use crate::home::{
    BY_PROJECT, by_project_path, ensure_inventory_root, existing_inventory_root,
    looks_like_legacy_catalog, resolve_home,
};
use crate::output;
use crate::paths::{map_io, mkdir_p, refuse_symlink, require_directory, require_file};

fn safe_project_dirname(name: &str) -> String {
    let cleaned = name.trim().replace(['/', '\\'], "-").replace('\0', "");
    let cleaned = cleaned.trim_end_matches(['.', ' ']);
    if cleaned.is_empty() || cleaned == "." || cleaned == ".." {
        "project".into()
    } else {
        cleaned.to_string()
    }
}

fn project_catalog_dir(home: &Path, project_root: &Path) -> Result<PathBuf, Error> {
    // Leave ample room below the portable 255-byte component limit for the
    // separator and fixed-width identity, without splitting a UTF-8 scalar.
    let mut basename = safe_project_root_name(project_root);
    while basename.len() > 120 {
        basename.pop();
    }
    if basename.is_empty() {
        basename.push_str("project");
    }
    let identity = project_identity(project_root);
    Ok(by_project_path(home).join(format!("{basename}-{identity}")))
}

fn project_identity(project_root: &Path) -> String {
    #[cfg(unix)]
    let root_bytes = {
        use std::os::unix::ffi::OsStrExt;

        project_root.as_os_str().as_bytes().to_vec()
    };
    #[cfg(not(unix))]
    let root_bytes = project_root.to_string_lossy().into_owned().into_bytes();
    format!("{:x}", Sha256::digest(root_bytes))
}

fn legacy_project_catalog_dir(home: &Path, project_root: &Path) -> PathBuf {
    by_project_path(home).join(safe_project_root_name(project_root))
}

#[derive(Debug, Clone)]
struct CatalogMeta {
    name: String,
    root: String,
    identity: Option<String>,
    skills: BTreeSet<String>,
}

impl CatalogMeta {
    fn by_project(project_root: &Path) -> Self {
        Self {
            name: safe_project_root_name(project_root),
            root: project_root.to_string_lossy().into_owned(),
            identity: Some(project_identity(project_root)),
            skills: BTreeSet::new(),
        }
    }

    fn from_value(project_root: &Path, value: &Value) -> Self {
        Self {
            name: value
                .get("name")
                .and_then(|v| v.as_str())
                .map(str::to_string)
                .unwrap_or_else(|| safe_project_root_name(project_root)),
            root: value
                .get("root")
                .and_then(|v| v.as_str())
                .map(str::to_string)
                .unwrap_or_default(),
            identity: value
                .get("identity")
                .and_then(|v| v.as_str())
                .map(str::to_string),
            skills: value
                .get("skills")
                .and_then(|v| v.as_array())
                .map(|values| {
                    values
                        .iter()
                        .filter_map(|entry| entry.as_str())
                        .map(str::to_string)
                        .collect()
                })
                .unwrap_or_default(),
        }
    }

    fn read(meta_path: &Path, project_root: &Path) -> Result<Self, Error> {
        let raw = fs::read_to_string(meta_path).map_err(|e| map_io(meta_path, e))?;
        let value = serde_json::from_str(&raw).map_err(|e| {
            Error::msg(format!(
                "Invalid catalog meta {}: {e}",
                output::display_path(meta_path)
            ))
        })?;
        Ok(Self::from_value(project_root, &value))
    }

    fn write(&self, meta_path: &Path) -> Result<(), Error> {
        let body = json!({
            "name": self.name,
            "root": self.root,
            "identity": self.identity,
            "skills": self.skills.iter().cloned().collect::<Vec<_>>()
        });
        let text = format!(
            "{}\n",
            serde_json::to_string_pretty(&body).map_err(|e| Error::msg(e.to_string()))?
        );
        let parent = meta_path
            .parent()
            .ok_or_else(|| Error::msg("Catalog metadata path has no parent"))?;
        let mut staged = tempfile::Builder::new()
            .prefix(".meta-")
            .tempfile_in(parent)
            .map_err(|e| map_io(parent, e))?;
        staged
            .write_all(text.as_bytes())
            .map_err(|e| map_io(staged.path(), e))?;
        staged.flush().map_err(|e| map_io(staged.path(), e))?;
        staged
            .persist(meta_path)
            .map_err(|e| map_io(meta_path, e.error))?;
        Ok(())
    }

    /// Whether withdraw/forget may mutate this entry for `project_root`.
    ///
    /// `meta.root` must canonicalize to `project_root` (already canonical).
    /// Missing, empty, or unresolvable roots are not ownership proof.
    fn catalog_owned_by(&self, project_root: &Path) -> bool {
        if let Some(identity) = &self.identity {
            return identity == &project_identity(project_root);
        }
        if self.root.is_empty() {
            return false;
        }
        match Path::new(&self.root).canonicalize() {
            Ok(canon) => canon == project_root,
            Err(_) => false,
        }
    }

    fn add_skill(&mut self, skill_name: &str) {
        self.skills.insert(skill_name.to_string());
    }

    fn withdraw_skill(&mut self, skill_name: &str) -> bool {
        self.skills.remove(skill_name)
    }
}

fn safe_project_root_name(project_root: &Path) -> String {
    safe_project_dirname(
        project_root
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("project"),
    )
}

fn remove_catalog_dir(catalog: &Path) -> Result<(), Error> {
    if !catalog.exists() && !catalog.is_symlink() {
        return Ok(());
    }
    refuse_symlink(catalog)?;
    if !catalog.is_dir() {
        return Err(Error::msg(format!(
            "Refusing to remove non-directory catalog: {}",
            output::display_path(catalog)
        )));
    }
    fs::remove_dir_all(catalog).map_err(|e| map_io(catalog, e))?;
    Ok(())
}

fn validate_catalog_dir(catalog: &Path) -> Result<Option<PathBuf>, Error> {
    if !catalog.exists() && !catalog.is_symlink() {
        return Ok(None);
    }
    refuse_symlink(catalog)?;
    if !catalog.is_dir() {
        return Err(Error::msg(format!(
            "Refusing non-directory catalog: {}",
            output::display_path(catalog)
        )));
    }
    let meta = catalog.join("meta.json");
    require_file(&meta)?;
    if !meta.is_file() {
        return Ok(None);
    }
    Ok(Some(catalog.to_path_buf()))
}

/// Move a basename-keyed entry only when its stored canonical root proves it
/// belongs to this project. Ambiguous and foreign entries remain untouched.
fn migrate_owned_legacy_catalog(
    home: &Path,
    project_root: &Path,
    destination: &Path,
) -> Result<(), Error> {
    if destination.exists() || destination.is_symlink() {
        return Ok(());
    }
    let legacy = legacy_project_catalog_dir(home, project_root);
    let Some(legacy) = validate_catalog_dir(&legacy)? else {
        return Ok(());
    };
    let meta_path = legacy.join("meta.json");
    let meta = CatalogMeta::read(&meta_path, project_root)?;
    if !meta.catalog_owned_by(project_root) {
        if meta.identity.is_none()
            && (meta.root.is_empty() || Path::new(&meta.root).canonicalize().is_err())
        {
            return Err(Error::msg(format!(
                "Cannot migrate ambiguous legacy catalog {}; restore its canonical root field or remove the stale entry",
                output::display_path(&legacy)
            )));
        }
        return Ok(());
    }
    fs::rename(&legacy, destination).map_err(|e| map_io(destination, e))
}

/// Find an existing project catalog only after validating the owned home and
/// every directory boundary that a destructive operation would traverse.
fn existing_project_catalog(
    home: Option<&Path>,
    project_root: &Path,
) -> Result<Option<PathBuf>, Error> {
    let home = match home {
        Some(path) => path.to_path_buf(),
        // Cleanup remains a soft success when the implicit home cannot be
        // resolved (for example, HOME is unset). An explicit home is strict.
        None => match resolve_home() {
            Ok(path) => path,
            Err(_) => return Ok(None),
        },
    };
    let Some(home) = existing_inventory_root(Some(&home))? else {
        return Ok(None);
    };
    let by_project = by_project_path(&home);
    if !by_project.exists() && !by_project.is_symlink() {
        return Ok(None);
    }
    refuse_symlink(&by_project)?;
    if !by_project.is_dir() {
        return Err(Error::msg(format!(
            "Refusing non-directory catalog: {}",
            output::display_path(&by_project)
        )));
    }

    let catalog = project_catalog_dir(&home, project_root)?;
    if let Some(catalog) = validate_catalog_dir(&catalog)? {
        return Ok(Some(catalog));
    }

    let legacy = legacy_project_catalog_dir(&home, project_root);
    let Some(legacy) = validate_catalog_dir(&legacy)? else {
        return Ok(None);
    };
    let meta = CatalogMeta::read(&legacy.join("meta.json"), project_root)?;
    Ok(meta.catalog_owned_by(project_root).then_some(legacy))
}

/// Validate every catalog boundary that a later deposit will traverse.
/// Inventory creation may occur, but no project entry or skill name is written.
pub(crate) fn preflight_deposit_skill_at(
    home: Option<&Path>,
    project_root: &Path,
) -> Result<(), Error> {
    let project_root = project_root
        .canonicalize()
        .map_err(|e| map_io(project_root, e))?;
    let (home, _) = ensure_inventory_root(home)?;
    let catalog = project_catalog_dir(&home, &project_root)?;
    require_directory(&catalog)?;
    if let Some(existing) = validate_catalog_dir(&catalog)? {
        let meta = CatalogMeta::read(&existing.join("meta.json"), &project_root)?;
        if !meta.catalog_owned_by(&project_root) {
            return Err(Error::msg(format!(
                "Catalog identity belongs to another project: {}",
                output::display_path(&catalog)
            )));
        }
        return Ok(());
    }

    let legacy = legacy_project_catalog_dir(&home, &project_root);
    let Some(legacy) = validate_catalog_dir(&legacy)? else {
        return Ok(());
    };
    let meta = CatalogMeta::read(&legacy.join("meta.json"), &project_root)?;
    if !meta.catalog_owned_by(&project_root)
        && meta.identity.is_none()
        && (meta.root.is_empty() || Path::new(&meta.root).canonicalize().is_err())
    {
        return Err(Error::msg(format!(
            "Cannot migrate ambiguous legacy catalog {}; restore its canonical root field or remove the stale entry",
            output::display_path(&legacy)
        )));
    }
    Ok(())
}

/// Record that `skill_name` is installed in `project_root`.
///
/// Failure fails the caller.
pub(crate) fn deposit_skill_at(
    home: Option<&Path>,
    project_root: &Path,
    skill_name: &str,
) -> Result<PathBuf, Error> {
    let project_root = project_root
        .canonicalize()
        .map_err(|e| map_io(project_root, e))?;
    let (home, _) = ensure_inventory_root(home)?;
    let catalog = project_catalog_dir(&home, &project_root)?;
    migrate_owned_legacy_catalog(&home, &project_root, &catalog)?;
    require_directory(&catalog)?;
    mkdir_p(&catalog)?;

    let meta_path = catalog.join("meta.json");
    require_file(&meta_path)?;

    let mut meta = if meta_path.is_file() {
        let meta = CatalogMeta::read(&meta_path, &project_root)?;
        if !meta.catalog_owned_by(&project_root) {
            return Err(Error::msg(format!(
                "Catalog identity belongs to another project: {}",
                output::display_path(&catalog)
            )));
        }
        meta
    } else {
        CatalogMeta::by_project(&project_root)
    };
    meta.name = safe_project_root_name(&project_root);
    meta.root = project_root.to_string_lossy().into_owned();
    meta.identity = Some(project_identity(&project_root));
    meta.add_skill(skill_name);
    meta.write(&meta_path)?;
    Ok(catalog)
}

/// Drop `skill_name` from the by-project catalog for `project_root`.
///
/// Soft success when home, project entry, or name is already absent, or when
/// stored ownership cannot be proven. Never creates inventory. Preserves
/// existing meta `name`/`root` when siblings remain. Removes the project
/// catalog directory when no names remain.
pub fn withdraw_skill(project_root: &Path, skill_name: &str) -> Result<(), Error> {
    withdraw_skill_at(None, project_root, skill_name)
}

/// Like [`withdraw_skill`], with an optional home root (tests).
pub(crate) fn withdraw_skill_at(
    home: Option<&Path>,
    project_root: &Path,
    skill_name: &str,
) -> Result<(), Error> {
    let project_root = match project_root.canonicalize() {
        Ok(path) => path,
        Err(_) => return Ok(()),
    };
    let Some(catalog) = existing_project_catalog(home, &project_root)? else {
        return Ok(());
    };
    let meta_path = catalog.join("meta.json");
    if !meta_path.is_file() {
        return Ok(());
    }
    refuse_symlink(&meta_path)?;

    let mut meta = CatalogMeta::read(&meta_path, &project_root)?;
    if !meta.catalog_owned_by(&project_root) {
        return Ok(());
    }
    if !meta.withdraw_skill(skill_name) {
        return Ok(());
    }
    if meta.skills.is_empty() {
        return remove_catalog_dir(&catalog);
    }
    meta.write(&meta_path)
}

/// Remove this project's by-project catalog entry entirely.
///
/// Soft success when absent, or when `meta.root` belongs to a different
/// project sharing this basename. Does not create inventory or touch library.
pub fn forget_project(project_root: &Path) -> Result<(), Error> {
    forget_project_at(None, project_root)
}

/// Validate catalog cleanup before destructive project filesystem changes.
pub(crate) fn preflight_forget_project(project_root: &Path) -> Result<(), Error> {
    preflight_forget_project_at(None, project_root)
}

pub(crate) fn preflight_forget_project_at(
    home: Option<&Path>,
    project_root: &Path,
) -> Result<(), Error> {
    let project_root = match project_root.canonicalize() {
        Ok(path) => path,
        Err(_) => return Ok(()),
    };
    let Some(catalog) = existing_project_catalog(home, &project_root)? else {
        return Ok(());
    };
    let meta_path = catalog.join("meta.json");
    if meta_path.is_file() {
        refuse_symlink(&meta_path)?;
        let _ = CatalogMeta::read(&meta_path, &project_root)?;
    }
    Ok(())
}

/// Like [`forget_project`], with an optional home root (tests).
pub(crate) fn forget_project_at(home: Option<&Path>, project_root: &Path) -> Result<(), Error> {
    let project_root = match project_root.canonicalize() {
        Ok(path) => path,
        Err(_) => return Ok(()),
    };
    let Some(catalog) = existing_project_catalog(home, &project_root)? else {
        return Ok(());
    };
    let meta_path = catalog.join("meta.json");
    if meta_path.is_file() {
        refuse_symlink(&meta_path)?;
        let meta = CatalogMeta::read(&meta_path, &project_root)?;
        if !meta.catalog_owned_by(&project_root) {
            return Ok(());
        }
    }
    remove_catalog_dir(&catalog)
}

/// One skill row from the offline by-project name catalog.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct CatalogEntry {
    pub project: String,
    pub root: String,
    pub skill: String,
}

/// Read `$TINK_HOME` / `~/.tink` by-project catalogs (read-only; creates nothing).
pub fn list_catalog(home: Option<&Path>) -> Result<Vec<CatalogEntry>, Error> {
    let Some(home) = existing_inventory_root(home)? else {
        return Ok(Vec::new());
    };
    let new = by_project_path(&home);
    let legacy = home.join("skills").join(BY_PROJECT);
    let legacy_catalog = looks_like_legacy_catalog(&legacy);
    if new.exists() && legacy_catalog {
        return Err(Error::msg(format!(
            "Catalog split across {} and {}: keep catalog/by-project, remove skills/by-project, then re-run",
            output::display_path(&legacy),
            output::display_path(&new)
        )));
    }
    // Prefer new location; fall back to legacy catalog until migration runs.
    // Ignore skills/by-project when it is a skill tree (has SKILL.md).
    let by_project = if new.exists() {
        new
    } else if legacy_catalog {
        legacy
    } else {
        new
    };
    if !by_project.exists() {
        return Ok(Vec::new());
    }
    refuse_symlink(&by_project)?;
    if !by_project.is_dir() {
        return Err(Error::msg(format!(
            "Refusing to read non-directory catalog: {}",
            output::display_path(&by_project)
        )));
    }

    let mut entries = Vec::new();
    let mut dirs: Vec<_> = fs::read_dir(&by_project)
        .map_err(|e| map_io(&by_project, e))?
        .map(|entry| {
            entry
                .map(|entry| entry.path())
                .map_err(|e| map_io(&by_project, e))
        })
        .collect::<Result<_, _>>()?;
    dirs.sort();

    for dir in dirs {
        let name = dir
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("")
            .to_string();
        if name.is_empty() {
            continue;
        }
        if dir.is_symlink() || !dir.is_dir() {
            continue;
        }
        let meta_path = dir.join("meta.json");
        if meta_path.is_symlink() || !meta_path.is_file() {
            continue;
        }
        let Ok(meta) = CatalogMeta::read(&meta_path, &dir) else {
            continue;
        };
        for skill_name in meta.skills {
            if !skill_name.is_empty() {
                entries.push(CatalogEntry {
                    project: meta.name.clone(),
                    root: meta.root.clone(),
                    skill: skill_name,
                });
            }
        }
    }

    entries.sort();
    Ok(entries)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Value;
    use tempfile::TempDir;

    #[test]
    fn list_catalog_refuses_split_paths() {
        let temp = TempDir::new().unwrap();
        let home = temp.path().join("home");
        fs::create_dir_all(home.join("skills").join(BY_PROJECT).join("app")).unwrap();
        fs::create_dir_all(by_project_path(&home).join("app")).unwrap();
        fs::write(
            home.join(crate::home::LAYOUT_FILENAME),
            format!("{{\"kind\":\"{}\"}}", crate::home::LAYOUT_KIND),
        )
        .unwrap();
        let err = list_catalog(Some(&home)).unwrap_err();
        assert!(err.to_string().contains("Catalog split"), "{err}");
    }

    #[test]
    fn list_catalog_requires_owned_home_before_reading_catalog() {
        let temp = TempDir::new().unwrap();
        let home = temp.path().join("home");
        fs::create_dir_all(by_project_path(&home)).unwrap();

        let err = list_catalog(Some(&home)).unwrap_err();

        assert!(err.to_string().contains("Not a Tink home"), "{err}");
    }

    #[test]
    fn catalog_meta_models_skill_set_and_ownership() {
        let temp = tempfile::TempDir::new().unwrap();
        let project = temp.path().join("app");
        std::fs::create_dir_all(&project).unwrap();
        let project = project.canonicalize().unwrap();

        let mut meta = CatalogMeta::by_project(&project);
        assert!(meta.skills.is_empty());
        assert!(meta.catalog_owned_by(&project));

        meta.add_skill("beta");
        meta.add_skill("alpha");
        let ordered: Vec<_> = meta.skills.iter().map(String::as_str).collect();
        assert_eq!(ordered, vec!["alpha", "beta"]);

        assert!(meta.withdraw_skill("alpha"));
        assert_eq!(meta.skills.iter().collect::<Vec<_>>().len(), 1);
        assert!(!meta.withdraw_skill("missing"));

        // Empty root is ambiguous and never proves ownership.
        meta.identity = None;
        meta.root.clear();
        assert!(!meta.catalog_owned_by(&project));
        // Unresolvable path: soft-refuse.
        meta.root = project
            .join("does-not-exist-ownership-probe")
            .to_string_lossy()
            .into_owned();
        assert!(!meta.catalog_owned_by(&project));
        // Different existing path: soft-refuse.
        let other = temp.path().join("other");
        fs::create_dir_all(&other).unwrap();
        meta.root = other.canonicalize().unwrap().to_string_lossy().into_owned();
        assert!(!meta.catalog_owned_by(&project));

        meta.root = project.to_string_lossy().into_owned();
        let path_root = project.join("meta.json");
        meta.write(&path_root).unwrap();
        let reloaded = CatalogMeta::read(&path_root, &project).unwrap();
        assert_eq!(reloaded.name, "app");
        assert_eq!(reloaded.root, project.to_string_lossy());
        assert_eq!(reloaded.skills.len(), 1);
    }

    #[test]
    fn destructive_operations_do_not_claim_empty_root_metadata() {
        let temp = TempDir::new().unwrap();
        let home = temp.path().join("home");
        let project = temp.path().join("app");
        fs::create_dir_all(&project).unwrap();
        let catalog = deposit_skill_at(Some(&home), &project, "alpha").unwrap();
        deposit_skill_at(Some(&home), &project, "beta").unwrap();

        let meta_path = catalog.join("meta.json");
        let mut value: Value =
            serde_json::from_str(&fs::read_to_string(&meta_path).unwrap()).unwrap();
        value["root"] = json!("");
        value.as_object_mut().unwrap().remove("identity");
        fs::write(
            &meta_path,
            format!("{}\n", serde_json::to_string_pretty(&value).unwrap()),
        )
        .unwrap();
        let before = fs::read(&meta_path).unwrap();

        withdraw_skill_at(Some(&home), &project, "alpha").unwrap();
        forget_project_at(Some(&home), &project).unwrap();

        assert_eq!(fs::read(&meta_path).unwrap(), before);
    }

    #[test]
    fn skill_shaped_by_project_dir_is_not_legacy_catalog() {
        let temp = TempDir::new().unwrap();
        let root = temp.path().join("inv");
        ensure_inventory_root(Some(&root)).unwrap();
        let skill_shaped = root.join("skills").join(BY_PROJECT);
        fs::create_dir_all(&skill_shaped).unwrap();
        fs::write(
            skill_shaped.join("SKILL.md"),
            "---\nname: by-project\ndescription: x\n---\n",
        )
        .unwrap();
        fs::create_dir_all(by_project_path(&root).join("app")).unwrap();
        ensure_inventory_root(Some(&root)).unwrap();
        assert!(skill_shaped.join("SKILL.md").is_file());
        assert!(list_catalog(Some(&root)).unwrap().is_empty());
    }

    #[test]
    fn deposit_records_name_not_under_catalog_tree() {
        let temp = TempDir::new().unwrap();
        let home = temp.path().join("home");
        let project = temp.path().join("app");
        fs::create_dir_all(&project).unwrap();
        let catalog = deposit_skill_at(Some(&home), &project, "grill-me").unwrap();
        let meta: Value =
            serde_json::from_str(&fs::read_to_string(catalog.join("meta.json")).unwrap()).unwrap();
        assert_eq!(meta["name"], "app");
        assert!(
            meta["skills"]
                .as_array()
                .unwrap()
                .iter()
                .any(|s| s == "grill-me")
        );
        assert!(!catalog.join("grill-me").exists());
    }

    #[test]
    fn projects_with_the_same_basename_have_distinct_catalog_identities() {
        let temp = TempDir::new().unwrap();
        let home = temp.path().join("home");
        let first = temp.path().join("first").join("app");
        let second = temp.path().join("second").join("app");
        fs::create_dir_all(&first).unwrap();
        fs::create_dir_all(&second).unwrap();

        let first_catalog = deposit_skill_at(Some(&home), &first, "alpha").unwrap();
        let second_catalog = deposit_skill_at(Some(&home), &second, "beta").unwrap();

        assert_ne!(first_catalog, second_catalog);
        let rows = list_catalog(Some(&home)).unwrap();
        assert_eq!(rows.len(), 2);
        assert!(rows.iter().any(|row| {
            row.root == first.canonicalize().unwrap().to_string_lossy() && row.skill == "alpha"
        }));
        assert!(rows.iter().any(|row| {
            row.root == second.canonicalize().unwrap().to_string_lossy() && row.skill == "beta"
        }));
    }

    #[test]
    fn long_project_basename_stays_within_component_limit() {
        let temp = TempDir::new().unwrap();
        let home = temp.path().join("home");
        let project = temp.path().join("p".repeat(200));
        fs::create_dir_all(&project).unwrap();

        let catalog = deposit_skill_at(Some(&home), &project, "alpha").unwrap();

        assert!(
            catalog.file_name().unwrap().to_string_lossy().len() <= 255,
            "catalog component exceeded common filesystem NAME_MAX: {}",
            catalog.display()
        );
        deposit_skill_at(Some(&home), &project, "beta").unwrap();
        assert_eq!(list_catalog(Some(&home)).unwrap().len(), 2);
    }

    #[cfg(unix)]
    #[test]
    fn non_utf8_project_root_keeps_catalog_ownership() {
        use std::ffi::OsString;
        use std::os::unix::ffi::OsStringExt;

        let temp = TempDir::new().unwrap();
        let home = temp.path().join("home");
        let project = temp
            .path()
            .join(OsString::from_vec(b"project-\x80".to_vec()));
        if let Err(error) = fs::create_dir_all(&project) {
            assert_eq!(
                error.raw_os_error(),
                Some(92),
                "unexpected fixture error: {error}"
            );
            return;
        }

        deposit_skill_at(Some(&home), &project, "alpha").unwrap();
        deposit_skill_at(Some(&home), &project, "beta").unwrap();
        withdraw_skill_at(Some(&home), &project, "alpha").unwrap();
        assert_eq!(
            list_catalog(Some(&home))
                .unwrap()
                .iter()
                .map(|entry| entry.skill.as_str())
                .collect::<Vec<_>>(),
            vec!["beta"]
        );
        forget_project_at(Some(&home), &project).unwrap();
        assert!(list_catalog(Some(&home)).unwrap().is_empty());
    }

    #[test]
    fn deposit_refuses_ambiguous_legacy_catalog() {
        let temp = TempDir::new().unwrap();
        let home = temp.path().join("home");
        let project = temp.path().join("app");
        fs::create_dir_all(&project).unwrap();
        let project = project.canonicalize().unwrap();
        ensure_inventory_root(Some(&home)).unwrap();
        let legacy = legacy_project_catalog_dir(&home, &project);
        fs::create_dir_all(&legacy).unwrap();
        fs::write(
            legacy.join("meta.json"),
            "{\"name\":\"app\",\"skills\":[\"legacy\"]}\n",
        )
        .unwrap();

        let error = deposit_skill_at(Some(&home), &project, "alpha").unwrap_err();

        assert!(
            error.to_string().contains("ambiguous legacy catalog"),
            "{error}"
        );
        assert!(legacy.join("meta.json").is_file());
        assert!(!project_catalog_dir(&home, &project).unwrap().exists());
    }

    #[test]
    fn deposit_surfaces_malformed_legacy_catalog() {
        let temp = TempDir::new().unwrap();
        let home = temp.path().join("home");
        let project = temp.path().join("app");
        fs::create_dir_all(&project).unwrap();
        let project = project.canonicalize().unwrap();
        ensure_inventory_root(Some(&home)).unwrap();
        let legacy = legacy_project_catalog_dir(&home, &project);
        fs::create_dir_all(&legacy).unwrap();
        fs::write(legacy.join("meta.json"), "{not-json}\n").unwrap();

        let error = deposit_skill_at(Some(&home), &project, "alpha").unwrap_err();

        assert!(
            error.to_string().contains("Invalid catalog meta"),
            "{error}"
        );
        assert!(!project_catalog_dir(&home, &project).unwrap().exists());
    }

    #[cfg(unix)]
    #[test]
    fn catalog_identity_does_not_normalize_backslash_into_separator() {
        let temp = TempDir::new().unwrap();
        let home = temp.path().join("home");
        let backslash = temp.path().join("a\\b").join("app");
        let separator = temp.path().join("a").join("b").join("app");
        fs::create_dir_all(&backslash).unwrap();
        fs::create_dir_all(&separator).unwrap();

        let first = deposit_skill_at(Some(&home), &backslash, "alpha").unwrap();
        let second = deposit_skill_at(Some(&home), &separator, "beta").unwrap();

        assert_ne!(first, second);
    }

    #[test]
    fn deposit_migrates_legacy_catalog_with_matching_root() {
        let temp = TempDir::new().unwrap();
        let home = temp.path().join("home");
        let project = temp.path().join("app");
        fs::create_dir_all(&project).unwrap();
        let project = project.canonicalize().unwrap();
        ensure_inventory_root(Some(&home)).unwrap();
        let legacy = legacy_project_catalog_dir(&home, &project);
        fs::create_dir_all(&legacy).unwrap();
        let mut meta = CatalogMeta::by_project(&project);
        meta.add_skill("alpha");
        meta.write(&legacy.join("meta.json")).unwrap();

        let catalog = deposit_skill_at(Some(&home), &project, "beta").unwrap();

        assert_ne!(catalog, legacy);
        assert!(!legacy.exists());
        assert_eq!(
            list_catalog(Some(&home))
                .unwrap()
                .iter()
                .map(|entry| entry.skill.as_str())
                .collect::<Vec<_>>(),
            vec!["alpha", "beta"]
        );
    }

    #[test]
    fn deposit_leaves_foreign_legacy_catalog_untouched() {
        let temp = TempDir::new().unwrap();
        let home = temp.path().join("home");
        let first = temp.path().join("first").join("app");
        let second = temp.path().join("second").join("app");
        fs::create_dir_all(&first).unwrap();
        fs::create_dir_all(&second).unwrap();
        let first = first.canonicalize().unwrap();
        ensure_inventory_root(Some(&home)).unwrap();
        let legacy = legacy_project_catalog_dir(&home, &first);
        fs::create_dir_all(&legacy).unwrap();
        let mut meta = CatalogMeta::by_project(&first);
        meta.add_skill("alpha");
        let meta_path = legacy.join("meta.json");
        meta.write(&meta_path).unwrap();
        let before = fs::read(&meta_path).unwrap();

        let second_catalog = deposit_skill_at(Some(&home), &second, "beta").unwrap();

        assert_ne!(second_catalog, legacy);
        assert_eq!(fs::read(&meta_path).unwrap(), before);
        assert_eq!(list_catalog(Some(&home)).unwrap().len(), 2);
    }

    #[test]
    fn list_catalog_returns_sorted_rows() {
        let temp = TempDir::new().unwrap();
        let home = temp.path().join("home");
        let app = temp.path().join("app");
        let other = temp.path().join("other");
        fs::create_dir_all(&app).unwrap();
        fs::create_dir_all(&other).unwrap();
        deposit_skill_at(Some(&home), &app, "zebra").unwrap();
        deposit_skill_at(Some(&home), &app, "alpha").unwrap();
        deposit_skill_at(Some(&home), &other, "beta").unwrap();
        let rows = list_catalog(Some(&home)).unwrap();
        assert_eq!(
            rows.iter()
                .map(|e| (e.project.as_str(), e.skill.as_str()))
                .collect::<Vec<_>>(),
            vec![("app", "alpha"), ("app", "zebra"), ("other", "beta")]
        );
    }

    #[test]
    fn withdraw_keeps_siblings_and_preserves_meta() {
        let temp = TempDir::new().unwrap();
        let home = temp.path().join("home");
        let app = temp.path().join("app");
        fs::create_dir_all(&app).unwrap();
        let catalog = deposit_skill_at(Some(&home), &app, "keep").unwrap();
        deposit_skill_at(Some(&home), &app, "drop").unwrap();

        // Custom name with matching root: withdraw must preserve name/root.
        let before: Value =
            serde_json::from_str(&fs::read_to_string(catalog.join("meta.json")).unwrap()).unwrap();
        let mut patched = before.clone();
        patched["name"] = json!("preserved-name");
        fs::write(
            catalog.join("meta.json"),
            format!("{}\n", serde_json::to_string_pretty(&patched).unwrap()),
        )
        .unwrap();

        withdraw_skill_at(Some(&home), &app, "drop").unwrap();
        let after: Value =
            serde_json::from_str(&fs::read_to_string(catalog.join("meta.json")).unwrap()).unwrap();
        assert_eq!(after["name"], "preserved-name");
        assert_eq!(after["root"], before["root"]);
        assert_eq!(
            after["skills"]
                .as_array()
                .unwrap()
                .iter()
                .filter_map(|s| s.as_str())
                .collect::<Vec<_>>(),
            vec!["keep"]
        );
    }

    #[test]
    fn withdraw_and_forget_skip_foreign_basename_entry() {
        let temp = TempDir::new().unwrap();
        let home = temp.path().join("home");
        let clone_a = temp.path().join("clone-a").join("app");
        let clone_b = temp.path().join("clone-b").join("app");
        fs::create_dir_all(&clone_a).unwrap();
        fs::create_dir_all(&clone_b).unwrap();
        deposit_skill_at(Some(&home), &clone_a, "alpha").unwrap();
        deposit_skill_at(Some(&home), &clone_a, "beta").unwrap();

        withdraw_skill_at(Some(&home), &clone_b, "alpha").unwrap();
        assert_eq!(
            list_catalog(Some(&home))
                .unwrap()
                .iter()
                .map(|e| e.skill.as_str())
                .collect::<Vec<_>>(),
            vec!["alpha", "beta"]
        );

        forget_project_at(Some(&home), &clone_b).unwrap();
        assert_eq!(
            list_catalog(Some(&home))
                .unwrap()
                .iter()
                .map(|e| e.skill.as_str())
                .collect::<Vec<_>>(),
            vec!["alpha", "beta"]
        );
    }

    #[test]
    fn withdraw_last_name_removes_project_entry() {
        let temp = TempDir::new().unwrap();
        let home = temp.path().join("home");
        let app = temp.path().join("app");
        fs::create_dir_all(&app).unwrap();
        deposit_skill_at(Some(&home), &app, "only").unwrap();
        withdraw_skill_at(Some(&home), &app, "only").unwrap();
        assert!(list_catalog(Some(&home)).unwrap().is_empty());
        assert!(
            !project_catalog_dir(&home, &app.canonicalize().unwrap())
                .unwrap()
                .exists()
        );
    }

    #[test]
    fn forget_project_clears_entry() {
        let temp = TempDir::new().unwrap();
        let home = temp.path().join("home");
        let app = temp.path().join("app");
        fs::create_dir_all(&app).unwrap();
        deposit_skill_at(Some(&home), &app, "alpha").unwrap();
        deposit_skill_at(Some(&home), &app, "beta").unwrap();
        forget_project_at(Some(&home), &app).unwrap();
        assert!(list_catalog(Some(&home)).unwrap().is_empty());
    }

    #[test]
    fn withdraw_and_forget_soft_ok_when_nothing_to_do() {
        let temp = TempDir::new().unwrap();
        let missing_home = temp.path().join("no-home");
        let app = temp.path().join("app");
        fs::create_dir_all(&app).unwrap();
        withdraw_skill_at(Some(&missing_home), &app, "any").unwrap();
        forget_project_at(Some(&missing_home), &app).unwrap();

        let home = temp.path().join("home");
        deposit_skill_at(Some(&home), &app, "keep").unwrap();
        withdraw_skill_at(Some(&home), &app, "absent").unwrap();
        assert_eq!(
            list_catalog(Some(&home))
                .unwrap()
                .iter()
                .map(|e| e.skill.as_str())
                .collect::<Vec<_>>(),
            vec!["keep"]
        );
        forget_project_at(Some(&home), &temp.path().join("other-missing")).unwrap();
    }

    #[test]
    fn destructive_operations_refuse_unowned_home_without_mutation() {
        let temp = TempDir::new().unwrap();
        let home = temp.path().join("home");
        let app = temp.path().join("app");
        fs::create_dir_all(&app).unwrap();
        let catalog = by_project_path(&home).join("app");
        fs::create_dir_all(&catalog).unwrap();
        let meta = catalog.join("meta.json");
        fs::write(&meta, "unowned catalog\n").unwrap();
        let before = fs::read(&meta).unwrap();

        for operation in [
            withdraw_skill_at(Some(&home), &app, "alpha"),
            forget_project_at(Some(&home), &app),
        ] {
            let err = operation.unwrap_err();
            assert!(err.to_string().contains("Not a Tink home"), "{err}");
        }

        assert_eq!(fs::read(&meta).unwrap(), before);
        assert!(catalog.is_dir());
    }

    #[test]
    fn destructive_operations_refuse_catalog_symlinks_without_following() {
        let temp = TempDir::new().unwrap();
        let home = temp.path().join("home");
        let app = temp.path().join("app");
        fs::create_dir_all(&app).unwrap();
        ensure_inventory_root(Some(&home)).unwrap();

        let outside = temp.path().join("outside");
        let outside_catalog = outside.join("app");
        fs::create_dir_all(&outside_catalog).unwrap();
        let meta = outside_catalog.join("meta.json");
        fs::write(&meta, "external catalog\n").unwrap();
        fs::remove_dir_all(by_project_path(&home)).unwrap();
        std::os::unix::fs::symlink(&outside, by_project_path(&home)).unwrap();

        for operation in [
            withdraw_skill_at(Some(&home), &app, "alpha"),
            forget_project_at(Some(&home), &app),
        ] {
            let err = operation.unwrap_err();
            assert!(err.to_string().contains("symlink"), "{err}");
        }

        assert!(outside_catalog.is_dir());
        assert_eq!(fs::read_to_string(&meta).unwrap(), "external catalog\n");
    }

    #[test]
    fn destructive_operations_refuse_project_catalog_symlink_without_following() {
        let temp = TempDir::new().unwrap();
        let home = temp.path().join("home");
        let app = temp.path().join("app");
        fs::create_dir_all(&app).unwrap();
        ensure_inventory_root(Some(&home)).unwrap();

        let outside_catalog = temp.path().join("outside-app");
        fs::create_dir_all(&outside_catalog).unwrap();
        let meta = outside_catalog.join("meta.json");
        fs::write(&meta, "external catalog\n").unwrap();
        std::os::unix::fs::symlink(&outside_catalog, by_project_path(&home).join("app")).unwrap();

        for operation in [
            withdraw_skill_at(Some(&home), &app, "alpha"),
            forget_project_at(Some(&home), &app),
        ] {
            let err = operation.unwrap_err();
            assert!(err.to_string().contains("symlink"), "{err}");
        }

        assert!(outside_catalog.is_dir());
        assert_eq!(fs::read_to_string(&meta).unwrap(), "external catalog\n");
    }
}
