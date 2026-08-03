//! Offline home inventory root (`~/.tink` or `TINK_HOME`).
//!
//! Not an agent discovery root. Live skills stay under the project's
//! `.agents/skills/`. Home keeps a lean by-project **name catalog** so you can
//! browse which skills a project has without opening it.

use std::collections::BTreeSet;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};

use serde_json::{json, Value};

use crate::error::Error;
use crate::paths::{map_io, mkdir_p, refuse_symlink, require_directory, require_file};

pub const TINK_HOME_ENV: &str = "TINK_HOME";
pub const TINK_HOME_NAME: &str = ".tink";
pub const LAYOUT_FILENAME: &str = "layout.json";
pub const LAYOUT_KIND: &str = "tink-skill-inventory";
pub const BY_PROJECT: &str = "by-project";

const INVENTORY_README: &str = "\
# Tink home (`~/.tink`)

Tink home directory. This is **not** an agent skill discovery root. Agents load
skills only from a project's `.agents/skills/`.

Successful installs record skill **names** under
`skills/by-project/<project>/meta.json` for offline inventory. That catalog is
not a second copy of skill trees.

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

/// Path to `skills/by-project` under a home root.
pub fn by_project_path(home: &Path) -> PathBuf {
    home.join("skills").join(BY_PROJECT)
}

/// Ensure inventory root + `skills/by-project` + layout marker.
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
    let skills = root.join("skills");
    mkdir_p(&skills)?;
    let by_project = by_project_path(&root);
    mkdir_p(&by_project)?;
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
/// Does **not** copy skill trees. Failure fails the caller.
pub fn deposit_skill(project_root: &Path, skill_name: &str) -> Result<PathBuf, Error> {
    deposit_skill_into(None, project_root, skill_name)
}

fn deposit_skill_into(
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
///
/// Skips unreadable or invalid `meta.json` entries after recording them as errors
/// only when the by-project root itself is unsafe; individual bad metas are skipped
/// with no fatal error so one corrupt catalog does not hide the rest.
pub fn list_catalog(home: Option<&Path>) -> Result<Vec<CatalogEntry>, Error> {
    let home = match home {
        Some(path) => path.to_path_buf(),
        None => resolve_home()?,
    };
    if !home.exists() {
        return Ok(Vec::new());
    }
    refuse_symlink(&home)?;
    let by_project = by_project_path(&home);
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
    use tempfile::TempDir;

    #[test]
    fn ensure_creates_layout_and_by_project() {
        let temp = TempDir::new().unwrap();
        let root = temp.path().join("inv");
        let (path, created) = ensure_inventory_root(Some(&root)).unwrap();
        assert!(created);
        assert_eq!(path, root);
        assert!(root.join("layout.json").is_file());
        assert!(by_project_path(&root).is_dir());
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

    #[test]
    fn deposit_records_name_not_tree() {
        let temp = TempDir::new().unwrap();
        let home = temp.path().join("home");
        let project = temp.path().join("app");
        fs::create_dir_all(&project).unwrap();
        let catalog = deposit_skill_into(Some(&home), &project, "grill-me").unwrap();
        let meta: Value =
            serde_json::from_str(&fs::read_to_string(catalog.join("meta.json")).unwrap()).unwrap();
        assert_eq!(meta["name"], "app");
        assert!(meta["skills"]
            .as_array()
            .unwrap()
            .iter()
            .any(|s| s == "grill-me"));
        assert!(!catalog.join("skills").join("grill-me").exists());
    }

    #[test]
    fn list_catalog_returns_sorted_rows() {
        let temp = TempDir::new().unwrap();
        let home = temp.path().join("home");
        let app = temp.path().join("app");
        let other = temp.path().join("other");
        fs::create_dir_all(&app).unwrap();
        fs::create_dir_all(&other).unwrap();
        deposit_skill_into(Some(&home), &app, "zebra").unwrap();
        deposit_skill_into(Some(&home), &app, "alpha").unwrap();
        deposit_skill_into(Some(&home), &other, "beta").unwrap();
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
