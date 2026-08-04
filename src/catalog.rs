//! Grow-only by-project name catalog under `$TINK_HOME/catalog/by-project/`.

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

/// Record that `skill_name` is installed in `project_root` (grow-only name list).
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

    let mut skills: BTreeSet<String> = BTreeSet::new();
    if meta_path.is_file() {
        let raw = fs::read_to_string(&meta_path).map_err(|e| map_io(&meta_path, e))?;
        let value: Value = serde_json::from_str(&raw).map_err(|e| {
            Error::msg(format!(
                "Invalid catalog meta {}: {e}",
                meta_path.display()
            ))
        })?;
        if let Some(arr) = value.get("skills").and_then(|v| v.as_array()) {
            for item in arr {
                if let Some(name) = item.as_str() {
                    skills.insert(name.to_string());
                }
            }
        }
    }
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
    let text = format!(
        "{}\n",
        serde_json::to_string_pretty(&body).map_err(|e| Error::msg(e.to_string()))?
    );
    fs::write(&meta_path, text).map_err(|e| map_io(&meta_path, e))?;
    Ok(catalog)
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
}
