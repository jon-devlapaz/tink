//! Offline home inventory (`~/.tink` or `TINK_HOME`).
//!
//! Not an agent discovery root. Compatible layout with Python tink-agents.

use std::env;
use std::fs;
use std::path::{Path, PathBuf};

use serde_json::json;

use crate::error::Error;
use crate::paths::{map_io, mkdir_p, refuse_symlink, require_directory, require_file};
use crate::skills;

pub const TINK_HOME_ENV: &str = "TINK_HOME";
/// Python override; accepted so both tools can share a test inventory root.
pub const TINK_DUMP_DIR_ENV: &str = "TINK_DUMP_DIR";
pub const TINK_HOME_NAME: &str = ".tink";
pub const LAYOUT_FILENAME: &str = "layout.json";
pub const LAYOUT_KIND: &str = "tink-skill-inventory";

const INVENTORY_ROOT_IGNORE: &[&str] = &[
    ".git",
    "__pycache__",
    ".pytest_cache",
    ".mypy_cache",
    "build",
    "dist",
];

const INVENTORY_README: &str = "\
# Tink home (`~/.tink`)

Offline skill memory managed by Tink. This directory is **not** an agent skill
discovery root. Agents load skills only from a project's `.agents/skills/`
(and optional tool-specific user skill homes).

## Layout

- `layout.json` — inventory marker (`kind`: tink-skill-inventory)
- `skills/by-project/<project-name>/meta.json` — project root pointer
- `skills/by-project/<project-name>/skills/` — deposited skill trees

Default location: `~/.tink` (override with `TINK_HOME`).
";

#[derive(Debug, Clone)]
pub struct ProjectCatalog {
    pub name: String,
    pub root: PathBuf,
    pub path: PathBuf,
}

impl ProjectCatalog {
    pub fn skills_path(&self) -> PathBuf {
        self.path.join("skills")
    }
}

#[derive(Debug, Clone)]
#[allow(dead_code)] // path retained for future inventory inspect commands.
pub struct InventorySkill {
    pub name: String,
    pub skill_path: PathBuf,
}

/// Resolve the offline inventory root.
pub fn resolve_home() -> Result<PathBuf, Error> {
    if let Ok(custom) = env::var(TINK_HOME_ENV) {
        if !custom.is_empty() {
            return Ok(PathBuf::from(custom));
        }
    }
    if let Ok(custom) = env::var(TINK_DUMP_DIR_ENV) {
        if !custom.is_empty() {
            return Ok(PathBuf::from(custom));
        }
    }
    let home = env::var_os("HOME").ok_or_else(|| Error::msg("HOME is not set"))?;
    Ok(PathBuf::from(home).join(TINK_HOME_NAME))
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
    let skills_namespace = root.join("skills");
    mkdir_p(&skills_namespace)?;
    let by_project = skills_namespace.join("by-project");
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

pub fn project_identity(project_root: &Path) -> Result<ProjectCatalog, Error> {
    let project_root = project_root
        .canonicalize()
        .unwrap_or_else(|_| project_root.to_path_buf());
    let folder = project_root
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("project");
    let name = safe_project_dirname(folder);
    let dump_root = resolve_home()?;
    Ok(ProjectCatalog {
        name: name.clone(),
        root: project_root,
        path: dump_root.join("skills").join("by-project").join(name),
    })
}

fn project_root_from_skills(destination_root: &Path) -> PathBuf {
    let destination_root = destination_root
        .canonicalize()
        .unwrap_or_else(|_| destination_root.to_path_buf());
    if destination_root.file_name().and_then(|s| s.to_str()) == Some("skills") {
        if let Some(agents) = destination_root.parent() {
            if agents.file_name().and_then(|s| s.to_str()) == Some(".agents") {
                if let Some(project) = agents.parent() {
                    return project.to_path_buf();
                }
            }
        }
    }
    destination_root
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or(destination_root)
}

fn ensure_project_catalog(project_root: &Path) -> Result<ProjectCatalog, Error> {
    ensure_inventory_root(None)?;
    let catalog = project_identity(project_root)?;
    require_directory(&catalog.path)?;
    mkdir_p(&catalog.path)?;
    let skills_path = catalog.skills_path();
    require_directory(&skills_path)?;
    mkdir_p(&skills_path)?;
    Ok(catalog)
}

fn write_catalog_meta(catalog: &ProjectCatalog) -> Result<(), Error> {
    let meta_path = catalog.path.join("meta.json");
    require_file(&meta_path)?;
    let body = json!({
        "name": catalog.name,
        "root": catalog.root.to_string_lossy(),
    });
    let text = format!(
        "{}\n",
        serde_json::to_string_pretty(&body).map_err(|e| Error::msg(e.to_string()))?
    );
    fs::write(&meta_path, text).map_err(|e| map_io(&meta_path, e))?;
    Ok(())
}

/// Copy an installed project skill into the project-partitioned inventory.
pub fn deposit_skill(destination_root: &Path, installed_skill: &Path) -> Result<PathBuf, Error> {
    let installed_skill = installed_skill
        .canonicalize()
        .map_err(|e| map_io(installed_skill, e))?;
    refuse_symlink(&installed_skill)?;
    if !installed_skill.is_dir() {
        return Err(Error::msg(format!(
            "Installed skill must be a regular directory: {}",
            installed_skill.display()
        )));
    }
    if !installed_skill.join("SKILL.md").is_file() {
        return Err(Error::msg(format!(
            "Installed skill is missing SKILL.md: {}",
            installed_skill.display()
        )));
    }

    let catalog = ensure_project_catalog(&project_root_from_skills(destination_root))?;
    let skill_name = installed_skill
        .file_name()
        .ok_or_else(|| Error::msg("skill has no name"))?;
    let target = catalog.skills_path().join(skill_name);
    let staging = tempfile::Builder::new()
        .prefix(".tink-dump-")
        .tempdir_in(catalog.skills_path())
        .map_err(|e| Error::msg(format!("inventory staging: {e}")))?;
    let staged = staging.path().join(skill_name);
    skills::copy_skill_tree(&installed_skill, &staged, INVENTORY_ROOT_IGNORE)?;
    if target.exists() || target.is_symlink() {
        if target.is_dir() && !target.is_symlink() {
            fs::remove_dir_all(&target).map_err(|e| map_io(&target, e))?;
        } else {
            fs::remove_file(&target).map_err(|e| map_io(&target, e))?;
        }
    }
    fs::rename(&staged, &target).map_err(|e| map_io(&target, e))?;
    write_catalog_meta(&catalog)?;
    Ok(target)
}

pub fn list_project_skills(
    project_root: &Path,
) -> Result<(ProjectCatalog, Vec<InventorySkill>), Error> {
    let catalog = project_identity(project_root)?;
    let skills_path = catalog.skills_path();
    if !skills_path.is_dir() {
        return Ok((catalog, Vec::new()));
    }
    let mut entries: Vec<_> = fs::read_dir(&skills_path)
        .map_err(|e| map_io(&skills_path, e))?
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .collect();
    entries.sort();
    let mut dumped = Vec::new();
    for path in entries {
        let name = path
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("")
            .to_string();
        if name.starts_with('.') || path.is_symlink() || !path.is_dir() {
            continue;
        }
        if !path.join("SKILL.md").is_file() {
            continue;
        }
        dumped.push(InventorySkill {
            name,
            skill_path: path,
        });
    }
    Ok((catalog, dumped))
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
        assert!(root.join("skills").join("by-project").is_dir());
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
