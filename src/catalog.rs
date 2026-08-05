//! By-project name catalog under `$TINK_HOME/catalog/by-project/`.
//!
//! Names are recorded on install and dropped on `skill remove`; a project's
//! catalog entry is removed on `destroy` (or when the last name is withdrawn).

use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

use serde_json::{json, Value};

use crate::error::Error;
use crate::home::{
    by_project_path, ensure_inventory_root, looks_like_legacy_catalog, resolve_home, BY_PROJECT,
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
    let label = project_root
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("project");
    Ok(by_project_path(home).join(safe_project_dirname(label)))
}

fn skills_from_meta(value: &Value) -> BTreeSet<String> {
    let mut skills = BTreeSet::new();
    if let Some(arr) = value.get("skills").and_then(|v| v.as_array()) {
        for item in arr {
            if let Some(name) = item.as_str() {
                skills.insert(name.to_string());
            }
        }
    }
    skills
}

fn parse_meta(meta_path: &Path) -> Result<Value, Error> {
    let raw = fs::read_to_string(meta_path).map_err(|e| map_io(meta_path, e))?;
    serde_json::from_str(&raw).map_err(|e| {
        Error::msg(format!(
            "Invalid catalog meta {}: {e}",
            meta_path.display()
        ))
    })
}

fn write_meta(meta_path: &Path, body: &Value) -> Result<(), Error> {
    let text = format!(
        "{}\n",
        serde_json::to_string_pretty(body).map_err(|e| Error::msg(e.to_string()))?
    );
    fs::write(meta_path, text).map_err(|e| map_io(meta_path, e))?;
    Ok(())
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

    let mut skills = if meta_path.is_file() {
        skills_from_meta(&parse_meta(&meta_path)?)
    } else {
        BTreeSet::new()
    };
    skills.insert(skill_name.to_string());

    let label = project_root
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("project");
    let body = json!({
        "name": safe_project_dirname(label),
        "root": project_root.to_string_lossy(),
        "skills": skills.into_iter().collect::<Vec<_>>(),
    });
    write_meta(&meta_path, &body)?;
    Ok(catalog)
}

/// Drop `skill_name` from the by-project catalog for `project_root`.
///
/// Soft success when home, project entry, or name is already absent. Never
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

    let value = parse_meta(&meta_path)?;
    let mut skills = skills_from_meta(&value);
    if !skills.remove(skill_name) {
        return Ok(());
    }
    if skills.is_empty() {
        return remove_catalog_dir(&catalog);
    }

    let label = value
        .get("name")
        .and_then(|v| v.as_str())
        .map(str::to_string)
        .unwrap_or_else(|| {
            project_root
                .file_name()
                .and_then(|s| s.to_str())
                .map(safe_project_dirname)
                .unwrap_or_else(|| "project".into())
        });
    let root = value
        .get("root")
        .and_then(|v| v.as_str())
        .map(str::to_string)
        .unwrap_or_else(|| project_root.to_string_lossy().into_owned());
    let body = json!({
        "name": label,
        "root": root,
        "skills": skills.into_iter().collect::<Vec<_>>(),
    });
    write_meta(&meta_path, &body)
}

/// Remove this project's by-project catalog entry entirely.
///
/// Soft success when absent. Does not create inventory or touch stash trees.
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
        let Ok(raw) = fs::read_to_string(&meta_path) else {
            continue;
        };
        let Ok(value) = serde_json::from_str::<Value>(&raw) else {
            continue;
        };
        let project = value
            .get("name")
            .and_then(|v| v.as_str())
            .unwrap_or(&name)
            .to_string();
        let root = value
            .get("root")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        let Some(skills) = value.get("skills").and_then(|v| v.as_array()) else {
            continue;
        };
        for skill in skills {
            if let Some(skill_name) = skill.as_str() {
                if !skill_name.is_empty() {
                    entries.push(CatalogEntry {
                        project: project.clone(),
                        root: root.clone(),
                        skill: skill_name.to_string(),
                    });
                }
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
    fn skill_shaped_by_project_dir_is_not_legacy_catalog() {
        let temp = TempDir::new().unwrap();
        let root = temp.path().join("inv");
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
        assert!(meta["skills"]
            .as_array()
            .unwrap()
            .iter()
            .any(|s| s == "grill-me"));
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
            rows
                .iter()
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

        // Simulate a divergent root already stored; withdraw must not rewrite it.
        let before: Value =
            serde_json::from_str(&fs::read_to_string(catalog.join("meta.json")).unwrap()).unwrap();
        let mut patched = before.clone();
        patched["root"] = json!("/preserved/root");
        patched["name"] = json!("preserved-name");
        write_meta(&catalog.join("meta.json"), &patched).unwrap();

        withdraw_skill_at(Some(&home), &app, "drop").unwrap();
        let after: Value =
            serde_json::from_str(&fs::read_to_string(catalog.join("meta.json")).unwrap()).unwrap();
        assert_eq!(after["name"], "preserved-name");
        assert_eq!(after["root"], "/preserved/root");
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
    fn withdraw_last_name_removes_project_entry() {
        let temp = TempDir::new().unwrap();
        let home = temp.path().join("home");
        let app = temp.path().join("app");
        fs::create_dir_all(&app).unwrap();
        deposit_skill_at(Some(&home), &app, "only").unwrap();
        withdraw_skill_at(Some(&home), &app, "only").unwrap();
        assert!(list_catalog(Some(&home)).unwrap().is_empty());
        assert!(!project_catalog_dir(&home, &app.canonicalize().unwrap())
            .unwrap()
            .exists());
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
