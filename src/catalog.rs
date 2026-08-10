//! By-project name catalog under `$TINK_HOME/catalog/by-project/`.
//!
//! Names are recorded on install and dropped on `skill remove`; a project's
//! catalog entry is removed on `destroy` (or when the last name is withdrawn).

use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

use serde_json::{Value, json};

use crate::error::Error;
use crate::home::{
    BY_PROJECT, by_project_path, ensure_inventory_root, looks_like_legacy_catalog, resolve_home,
};
use crate::paths::{map_io, mkdir_p, refuse_symlink, require_directory, require_file};

fn safe_project_dirname(name: &str) -> String {
    let cleaned = name
        .trim()
        .replace('/', "-")
        .replace('\\', "-")
        .replace('\0', "");
    let cleaned = cleaned.trim_end_matches(['.', ' ']);
    if cleaned.is_empty() || cleaned == "." || cleaned == ".." {
        "project".into()
    } else {
        cleaned.to_string()
    }
}

fn project_catalog_dir(home: &Path, project_root: &Path) -> Result<PathBuf, Error> {
    Ok(by_project_path(home).join(safe_project_root_name(project_root)))
}

#[derive(Debug, Clone)]
struct CatalogMeta {
    name: String,
    root: String,
    skills: BTreeSet<String>,
}

impl CatalogMeta {
    fn by_project(project_root: &Path) -> Self {
        Self {
            name: safe_project_root_name(project_root),
            root: project_root.to_string_lossy().into_owned(),
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
            Error::msg(format!("Invalid catalog meta {}: {e}", meta_path.display()))
        })?;
        Ok(Self::from_value(project_root, &value))
    }

    fn write(&self, meta_path: &Path) -> Result<(), Error> {
        let body = json!({
            "name": self.name,
            "root": self.root,
            "skills": self.skills.iter().cloned().collect::<Vec<_>>()
        });
        let text = format!(
            "{}\n",
            serde_json::to_string_pretty(&body).map_err(|e| Error::msg(e.to_string()))?
        );
        fs::write(meta_path, text).map_err(|e| map_io(meta_path, e))?;
        Ok(())
    }

    /// Whether withdraw/forget may mutate this entry for `project_root`.
    ///
    /// Non-empty `meta.root` must canonicalize to `project_root` (already
    /// canonical). Missing/empty `root` keeps basename-keyed behavior for
    /// legacy metas. Unresolvable stored roots soft-refuse so a sibling basename
    /// cannot wipe another project's catalog.
    fn catalog_owned_by(&self, project_root: &Path) -> bool {
        if self.root.is_empty() {
            return true;
        }
        match Path::new(&self.root).canonicalize() {
            Ok(canon) => canon == project_root,
            Err(_) => false,
        }
    }

    /// Seal empty/missing root to the withdrawing project when siblings remain,
    /// matching pre-`CatalogMeta` write behavior (`root` defaulted to project).
    fn seal_root_if_empty(&mut self, project_root: &Path) {
        if self.root.is_empty() {
            self.root = project_root.to_string_lossy().into_owned();
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
            catalog.display()
        )));
    }
    fs::remove_dir_all(catalog).map_err(|e| map_io(catalog, e))?;
    Ok(())
}

/// Soft-resolve home for withdraw/forget: missing home is success, not create.
fn existing_home(home: Option<&Path>) -> Result<Option<PathBuf>, Error> {
    let home = match home {
        Some(path) => path.to_path_buf(),
        None => match resolve_home() {
            Ok(path) => path,
            Err(_) => return Ok(None),
        },
    };
    if !home.exists() {
        return Ok(None);
    }
    refuse_symlink(&home)?;
    Ok(Some(home))
}

/// Record that `skill_name` is installed in `project_root`.
///
/// Last writer wins on `meta.json` `root` when two projects share a basename.
/// Failure fails the caller.
pub fn deposit_skill(project_root: &Path, skill_name: &str) -> Result<PathBuf, Error> {
    deposit_skill_at(None, project_root, skill_name)
}

/// Like [`deposit_skill`], with an optional home root (tests).
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
    require_directory(&catalog)?;
    mkdir_p(&catalog)?;

    let meta_path = catalog.join("meta.json");
    require_file(&meta_path)?;

    let mut meta = if meta_path.is_file() {
        CatalogMeta::read(&meta_path, &project_root)?
    } else {
        CatalogMeta::by_project(&project_root)
    };
    meta.name = safe_project_root_name(&project_root);
    meta.root = project_root.to_string_lossy().into_owned();
    meta.add_skill(skill_name);
    meta.write(&meta_path)?;
    Ok(catalog)
}

/// Drop `skill_name` from the by-project catalog for `project_root`.
///
/// Soft success when home, project entry, or name is already absent, or when
/// `meta.root` belongs to a different project sharing this basename. Never
/// creates inventory. Preserves existing meta `name`/`root` when siblings
/// remain. Removes the project catalog directory when no names remain.
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
    let Some(home) = existing_home(home)? else {
        return Ok(());
    };
    let catalog = project_catalog_dir(&home, &project_root)?;
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
    // Legacy metas omit `root`; after partial withdraw, seal ownership so a
    // foreign basename checkout cannot soft-own remaining skill names.
    meta.seal_root_if_empty(&project_root);
    meta.write(&meta_path)
}

/// Remove this project's by-project catalog entry entirely.
///
/// Soft success when absent, or when `meta.root` belongs to a different
/// project sharing this basename. Does not create inventory or touch library.
pub fn forget_project(project_root: &Path) -> Result<(), Error> {
    forget_project_at(None, project_root)
}

/// Like [`forget_project`], with an optional home root (tests).
pub(crate) fn forget_project_at(home: Option<&Path>, project_root: &Path) -> Result<(), Error> {
    let project_root = match project_root.canonicalize() {
        Ok(path) => path,
        Err(_) => return Ok(()),
    };
    let Some(home) = existing_home(home)? else {
        return Ok(());
    };
    let catalog = project_catalog_dir(&home, &project_root)?;
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
    let home = match home {
        Some(path) => path.to_path_buf(),
        None => resolve_home()?,
    };
    if !home.exists() {
        return Ok(Vec::new());
    }
    refuse_symlink(&home)?;
    let new = by_project_path(&home);
    let legacy = home.join("skills").join(BY_PROJECT);
    let legacy_catalog = looks_like_legacy_catalog(&legacy);
    if new.exists() && legacy_catalog {
        return Err(Error::msg(format!(
            "Catalog split across {} and {}: keep catalog/by-project, remove skills/by-project, then re-run",
            legacy.display(),
            new.display()
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
            by_project.display()
        )));
    }

    let mut entries = Vec::new();
    let mut dirs: Vec<_> = fs::read_dir(&by_project)
        .map_err(|e| map_io(&by_project, e))?
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .collect();
    dirs.sort();

    for dir in dirs {
        let name = dir
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("")
            .to_string();
        if name.is_empty() || name.starts_with('.') {
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
        let err = list_catalog(Some(&home)).unwrap_err();
        assert!(err.to_string().contains("Catalog split"), "{err}");
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

        // Empty root: legacy basename soft-own.
        meta.root.clear();
        assert!(meta.catalog_owned_by(&project));
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
    fn withdraw_seals_empty_root_and_blocks_foreign_forget() {
        let temp = TempDir::new().unwrap();
        let home = temp.path().join("home");
        let clone_a = temp.path().join("clone-a").join("app");
        let clone_b = temp.path().join("clone-b").join("app");
        fs::create_dir_all(&clone_a).unwrap();
        fs::create_dir_all(&clone_b).unwrap();
        let catalog = deposit_skill_at(Some(&home), &clone_a, "alpha").unwrap();
        deposit_skill_at(Some(&home), &clone_a, "beta").unwrap();

        // Legacy: missing/empty root still soft-owns for the first withdraws.
        let meta_path = catalog.join("meta.json");
        let mut value: Value =
            serde_json::from_str(&fs::read_to_string(&meta_path).unwrap()).unwrap();
        value["root"] = json!("");
        fs::write(
            &meta_path,
            format!("{}\n", serde_json::to_string_pretty(&value).unwrap()),
        )
        .unwrap();

        withdraw_skill_at(Some(&home), &clone_a, "alpha").unwrap();
        let after: Value = serde_json::from_str(&fs::read_to_string(&meta_path).unwrap()).unwrap();
        let sealed = after["root"].as_str().unwrap_or("");
        assert!(
            !sealed.is_empty(),
            "empty root should seal after partial withdraw"
        );
        assert_eq!(
            Path::new(sealed).canonicalize().unwrap(),
            clone_a.canonicalize().unwrap()
        );
        assert_eq!(
            after["skills"]
                .as_array()
                .unwrap()
                .iter()
                .filter_map(|s| s.as_str())
                .collect::<Vec<_>>(),
            vec!["beta"]
        );

        // Foreign basename must not forget remaining names after seal.
        forget_project_at(Some(&home), &clone_b).unwrap();
        assert_eq!(
            list_catalog(Some(&home))
                .unwrap()
                .iter()
                .map(|e| e.skill.as_str())
                .collect::<Vec<_>>(),
            vec!["beta"]
        );
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
}
