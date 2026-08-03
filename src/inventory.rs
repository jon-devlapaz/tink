//! Offline home inventory root (`~/.tink` or `TINK_HOME`).
//!
//! Not an agent discovery root. Live skills stay under the project's
//! `.agents/skills/`. Home keeps:
//! - `skills/<name>/` — archive of skill trees from successful adds
//! - `catalog/by-project/<project>/meta.json` — grow-only name index

use std::collections::BTreeSet;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};

use serde_json::{json, Value};

use crate::error::Error;
use crate::paths::{map_io, mkdir_p, refuse_symlink, require_directory, require_file};
use crate::skills::{self, Provenance, Skill};

pub const TINK_HOME_ENV: &str = "TINK_HOME";
pub const TINK_HOME_NAME: &str = ".tink";
pub const LAYOUT_FILENAME: &str = "layout.json";
pub const LAYOUT_KIND: &str = "tink-skill-inventory";
pub const BY_PROJECT: &str = "by-project";

const INVENTORY_README: &str = "\
# Tink home (`~/.tink`)

Tink home directory. This is **not** an agent skill discovery root. Agents load
skills only from a project's `.agents/skills/`.

Successful installs:
- archive skill trees under `skills/<name>/`
- record skill **names** under `catalog/by-project/<project>/meta.json`

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

/// Path to the skill-tree archive root (`skills/`).
pub fn skills_archive_path(home: &Path) -> PathBuf {
    home.join("skills")
}

/// Ensure inventory root + archive dir + catalog + layout marker.
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
    mkdir_p(&skills_archive_path(&root))?;
    migrate_catalog_if_needed(&root)?;
    mkdir_p(&by_project_path(&root))?;
    write_layout_marker(&root)?;
    Ok((root, created))
}

/// True when `skills/by-project` looks like the old name catalog (not a skill tree).
fn looks_like_legacy_catalog(path: &Path) -> bool {
    path.is_dir() && !path.join("SKILL.md").is_file()
}

/// Older homes kept the name catalog at `skills/by-project/`; move it out so
/// `skills/<name>/` can hold archived trees.
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
    // A skill tree mistakenly archived as by-project has SKILL.md — leave it.
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
/// Failure fails the caller.
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

/// Refuse if home archive would diverge; no-op if identical or missing.
pub fn preflight_archive(skill: &Skill, provenance: Option<&Provenance>) -> Result<(), Error> {
    let (home, _) = ensure_inventory_root(None)?;
    let archive = skills_archive_path(&home);
    mkdir_p(&archive)?;
    skills::preflight_install(skill, &archive, provenance)?;
    Ok(())
}

/// Copy skill tree into `~/.tink/skills/<name>/` (identical → noop).
pub fn deposit_archive(skill: &Skill, provenance: Option<&Provenance>) -> Result<PathBuf, Error> {
    let (home, _) = ensure_inventory_root(None)?;
    let archive = skills_archive_path(&home);
    mkdir_p(&archive)?;
    let (path, _) = skills::install_local(skill, &archive, provenance)?;
    Ok(path)
}

fn archive_tracks_project(home_skill: &Path, project_skill: &Path) -> Result<bool, Error> {
    if skills::skill_contents_equal(home_skill, project_skill)? {
        return Ok(true);
    }
    // Allow a missing/different receipt when the skill body still matches.
    skills::skill_contents_equal_except(
        home_skill,
        project_skill,
        &[".tink-source.json"],
    )
}

/// Before refreshing a project skill, ensure the home archive can accept `new`
/// (missing, already new, or still equal to the current project install).
pub fn preflight_archive_refresh(
    project_installed: &Path,
    new_skill: &Skill,
    new_provenance: &Provenance,
) -> Result<(), Error> {
    let (home, _) = ensure_inventory_root(None)?;
    let archive = skills_archive_path(&home);
    mkdir_p(&archive)?;
    match skills::preflight_install(new_skill, &archive, Some(new_provenance)) {
        Ok(_) => Ok(()),
        Err(err) if err.to_string().contains("Refusing to overwrite") => {
            let home_skill = archive.join(&new_skill.name);
            if home_skill.is_dir() && archive_tracks_project(&home_skill, project_installed)? {
                Ok(())
            } else {
                Err(Error::msg(format!(
                    "Refusing to refresh {}: home archive diverges",
                    new_skill.name
                )))
            }
        }
        Err(err) => Err(err),
    }
}

/// Keep `$TINK_HOME/skills/<name>/` aligned with the installed project skill.
///
/// On same-revision refresh the project install is source of truth: backfill or
/// repair a stale archive (including after a failed post-refresh deposit).
pub fn sync_archive_from_installed(installed: &Skill) -> Result<(), Error> {
    let (home, _) = ensure_inventory_root(None)?;
    let archive = skills_archive_path(&home);
    mkdir_p(&archive)?;
    let target = archive.join(&installed.name);
    if target.is_dir() && skills::skill_contents_equal(&target, &installed.path)? {
        return Ok(());
    }
    if target.exists() || target.is_symlink() {
        refuse_symlink(&target)?;
        if target.is_dir() {
            fs::remove_dir_all(&target).map_err(|e| map_io(&target, e))?;
        } else {
            fs::remove_file(&target).map_err(|e| map_io(&target, e))?;
        }
    }
    skills::install_local(installed, &archive, None)?;
    Ok(())
}

/// After a project refresh passed [`preflight_archive_refresh`], write the new
/// tree into the home archive (replace if present, install if missing).
pub fn deposit_archive_refresh(
    new_skill: &Skill,
    new_provenance: &Provenance,
) -> Result<(), Error> {
    let (home, _) = ensure_inventory_root(None)?;
    let archive = skills_archive_path(&home);
    mkdir_p(&archive)?;
    let target = archive.join(&new_skill.name);
    if target.is_dir() {
        skills::replace_verified(new_skill, &archive, new_provenance)?;
    } else {
        skills::install_local(new_skill, &archive, Some(new_provenance))?;
    }
    Ok(())
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
        assert!(skills_archive_path(&root).is_dir());
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
        fs::write(legacy.join("meta.json"), "{\"name\":\"app\",\"root\":\"/tmp/app\",\"skills\":[\"x\"]}\n")
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
        assert!(err.to_string().contains("non-directory legacy catalog"), "{err}");
    }

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
        fs::write(skill_shaped.join("SKILL.md"), "---\nname: by-project\ndescription: x\n---\n").unwrap();
        fs::create_dir_all(by_project_path(&root).join("app")).unwrap();
        ensure_inventory_root(Some(&root)).unwrap();
        assert!(skill_shaped.join("SKILL.md").is_file());
        assert!(list_catalog(Some(&root)).unwrap().is_empty());
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

    #[test]
    fn deposit_records_name_not_under_catalog_tree() {
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
