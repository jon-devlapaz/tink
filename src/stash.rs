//! Home skill stash (`$TINK_HOME/skills/<name>/`).
//!
//! Rebuildable dump of skill trees from successful installs. Not an agent
//! discovery root — promote into a project with `tink skill add --stash`.

use std::fs;
use std::path::{Path, PathBuf};

use crate::error::Error;
use crate::home::{ensure_inventory_root, skills_stash_path, BY_PROJECT};
use crate::paths::{map_io, mkdir_p, refuse_symlink};
use crate::provenance::{self, Provenance};
use crate::skills::{self, PreflightOutcome, Skill};

/// Result of writing a skill into the home stash.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StashWrite {
    /// Stash entry was missing; tree was created.
    Created,
    /// Stash already matched the incoming tree (including receipt).
    Unchanged,
    /// Stash diverged; replaced with the incoming tree.
    Repaired,
}

/// Result of a create-only stash write (never repairs).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CreateOnlyWrite {
    /// Stash entry was missing; tree was created.
    Created,
    /// Stash already matched (exact, or body equal except receipt).
    Unchanged,
    /// Divergent or unreadable; no write. Optional reason for the caller.
    Skipped(Option<String>),
}

fn clear_path(target: &Path) -> Result<(), Error> {
    refuse_symlink(target)?;
    if target.is_dir() {
        fs::remove_dir_all(target).map_err(|e| map_io(target, e))?;
    } else if target.exists() {
        fs::remove_file(target).map_err(|e| map_io(target, e))?;
    }
    Ok(())
}

fn stash_root(home: Option<&Path>) -> Result<PathBuf, Error> {
    let (home, _) = ensure_inventory_root(home)?;
    let root = skills_stash_path(&home);
    mkdir_p(&root)?;
    Ok(root)
}

/// Validated skill trees currently in the stash (skips reserved / unreadable).
fn iter_stash_skills(stash: &Path) -> Result<Vec<Skill>, Error> {
    if !stash.exists() {
        return Ok(Vec::new());
    }
    refuse_symlink(stash)?;
    if !stash.is_dir() {
        return Err(Error::msg(format!(
            "Refusing to read non-directory stash: {}",
            stash.display()
        )));
    }

    let mut skills = Vec::new();
    for entry in fs::read_dir(stash).map_err(|e| map_io(stash, e))? {
        let entry = entry.map_err(|e| map_io(stash, e))?;
        let path = entry.path();
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if name == BY_PROJECT || name.starts_with('.') || name == "README.md" {
            continue;
        }
        if path.is_symlink() || !path.is_dir() {
            continue;
        }
        if let Ok(skill) = skills::read_skill(&path, true) {
            skills.push(skill);
        }
    }
    skills.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(skills)
}

/// Copy skill tree into `~/.tink/skills/<name>/`.
///
/// Identical → noop; missing → create; divergent → replace (caller should warn).
/// Project installs still refuse overwrites separately.
pub fn deposit(
    skill: &Skill,
    provenance: Option<&Provenance>,
) -> Result<(PathBuf, StashWrite), Error> {
    let stash = stash_root(None)?;
    match skills::preflight_install(skill, &stash, provenance)? {
        PreflightOutcome::Ready => {
            let (path, _) = skills::install_local(skill, &stash, provenance)?;
            Ok((path, StashWrite::Created))
        }
        PreflightOutcome::Identical => Ok((stash.join(&skill.name), StashWrite::Unchanged)),
        PreflightOutcome::Divergent => {
            clear_path(&stash.join(&skill.name))?;
            let (path, _) = skills::install_local(skill, &stash, provenance)?;
            Ok((path, StashWrite::Repaired))
        }
    }
}

/// Copy skill tree into the home stash only when missing or identical.
///
/// Divergent trees are skipped (no repair). Unreadable/unsafe trees surface as
/// [`CreateOnlyWrite::Skipped`] with the error detail — same create-only
/// contract harvest used before this lived in `stash`.
pub fn deposit_create_only(skill: &Skill) -> Result<(PathBuf, CreateOnlyWrite), Error> {
    let stash = stash_root(None)?;
    let target = stash.join(&skill.name);
    match skills::preflight_install(skill, &stash, None) {
        Err(err) => Ok((target, CreateOnlyWrite::Skipped(Some(err.to_string())))),
        Ok(PreflightOutcome::Ready) => {
            let (path, _) = skills::install_local(skill, &stash, None)?;
            Ok((path, CreateOnlyWrite::Created))
        }
        Ok(PreflightOutcome::Identical) => Ok((target, CreateOnlyWrite::Unchanged)),
        Ok(PreflightOutcome::Divergent) => {
            if target.is_dir()
                && skills::skill_contents_equal_except(
                    &target,
                    &skill.path,
                    &[".tink-source.json"],
                )?
            {
                Ok((target, CreateOnlyWrite::Unchanged))
            } else {
                Ok((
                    target,
                    CreateOnlyWrite::Skipped(Some("stash differs; create-only".into())),
                ))
            }
        }
    }
}

/// When the stash already holds the exact tree we would install, return it.
pub fn matching(
    skill: &Skill,
    provenance: Option<&Provenance>,
) -> Result<Option<Skill>, Error> {
    let stash = stash_root(None)?;
    let target = stash.join(&skill.name);
    if !target.is_dir() {
        return Ok(None);
    }
    match skills::preflight_install(skill, &stash, provenance)? {
        PreflightOutcome::Identical => Ok(Some(skills::read_skill(&target, true)?)),
        PreflightOutcome::Ready | PreflightOutcome::Divergent => Ok(None),
    }
}

/// List skill names present in the home stash.
///
/// Creates nothing; empty when home or stash root is missing.
pub fn list_names(home: Option<&Path>) -> Result<Vec<String>, Error> {
    let home = match home {
        Some(path) => path.to_path_buf(),
        None => crate::home::resolve_home()?,
    };
    if !home.exists() {
        return Ok(Vec::new());
    }
    refuse_symlink(&home)?;
    let stash = skills_stash_path(&home);
    Ok(iter_stash_skills(&stash)?
        .into_iter()
        .map(|skill| skill.name)
        .collect())
}

/// Load one skill from the home stash by directory name.
pub fn load(name: &str) -> Result<Skill, Error> {
    let stash = stash_root(None)?;
    let path = stash.join(name);
    if path.is_symlink() {
        return Err(Error::msg(format!(
            "Refusing to follow symlink: {}",
            path.display()
        )));
    }
    if !path.is_dir() {
        return Err(Error::msg(format!("Stash skill not found: {name}")));
    }
    skills::read_skill(&path, true)
}

/// Find a stash skill whose receipt matches this remote URL + revision tip.
pub fn for_remote_tip(
    source_url: &str,
    revision: &str,
    selected_name: Option<&str>,
) -> Result<Option<Skill>, Error> {
    let (home, _) = ensure_inventory_root(None)?;
    let stash = skills_stash_path(&home);
    if !stash.is_dir() {
        return Ok(None);
    }

    let mut hits = Vec::new();
    for skill in iter_stash_skills(&stash)? {
        if let Some(want) = selected_name {
            if skill.name != want {
                continue;
            }
        }
        let Ok(Some(provenance)) = provenance::read(&skill) else {
            continue;
        };
        if provenance.get("source").map(String::as_str) == Some(source_url)
            && provenance.get("revision").map(String::as_str) == Some(revision)
        {
            hits.push(skill);
        }
    }
    match hits.len() {
        0 => Ok(None),
        1 => Ok(Some(hits.remove(0))),
        _ => {
            let commands = hits
                .iter()
                .map(|skill| format!("  tink skill add {source_url} --skill {}", skill.name))
                .collect::<Vec<_>>()
                .join("\n");
            Err(Error::msg(format!(
                "Stash has multiple skills for this revision. Choose one:\n{commands}"
            )))
        }
    }
}

fn tracks_project(stash_skill: &Path, project_skill: &Path) -> Result<bool, Error> {
    if skills::skill_contents_equal(stash_skill, project_skill)? {
        return Ok(true);
    }
    // Allow a missing/different receipt when the skill body still matches.
    skills::skill_contents_equal_except(stash_skill, project_skill, &[".tink-source.json"])
}

/// Before refreshing a project skill, ensure the stash can accept `new`
/// (missing, already new, or still equal to the current project install).
pub fn preflight_refresh(
    project_installed: &Path,
    new_skill: &Skill,
    new_provenance: &Provenance,
) -> Result<(), Error> {
    let stash = stash_root(None)?;
    match skills::preflight_install(new_skill, &stash, Some(new_provenance))? {
        PreflightOutcome::Ready | PreflightOutcome::Identical => Ok(()),
        PreflightOutcome::Divergent => {
            let home_skill = stash.join(&new_skill.name);
            if home_skill.is_dir() && tracks_project(&home_skill, project_installed)? {
                Ok(())
            } else {
                Err(Error::msg(format!(
                    "Refusing to refresh {}: stash diverges",
                    new_skill.name
                )))
            }
        }
    }
}

/// Keep `$TINK_HOME/skills/<name>/` aligned with the installed project skill.
pub fn sync_from_installed(installed: &Skill) -> Result<(), Error> {
    deposit(installed, None).map(|_| ())
}

/// After a project refresh passed [`preflight_refresh`], write the new tree
/// into the stash (create, noop, or repair — same rules as [`deposit`]).
pub fn deposit_refresh(new_skill: &Skill, new_provenance: &Provenance) -> Result<(), Error> {
    deposit(new_skill, Some(new_provenance)).map(|_| ())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::home::TINK_HOME_ENV;
    use std::sync::{Mutex, MutexGuard, OnceLock};
    use tempfile::TempDir;

    fn env_lock() -> &'static Mutex<()> {
        static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        LOCK.get_or_init(|| Mutex::new(()))
    }

    /// Isolates `TINK_HOME` for in-process stash writes (serialized).
    struct TempHome {
        home: PathBuf,
        root: PathBuf,
        _temp: TempDir,
        prev: Option<std::ffi::OsString>,
        _guard: MutexGuard<'static, ()>,
    }

    impl TempHome {
        fn new() -> Self {
            let guard = env_lock().lock().unwrap_or_else(|e| e.into_inner());
            let temp = TempDir::new().unwrap();
            let root = temp.path().to_path_buf();
            let home = root.join("tink-home");
            let prev = std::env::var_os(TINK_HOME_ENV);
            // SAFETY: exclusive via env_lock for all tests in this module.
            unsafe { std::env::set_var(TINK_HOME_ENV, &home) };
            Self {
                home,
                root,
                _temp: temp,
                prev,
                _guard: guard,
            }
        }

        fn stash_skill(&self, name: &str) -> PathBuf {
            skills_stash_path(&self.home).join(name)
        }
    }

    impl Drop for TempHome {
        fn drop(&mut self) {
            // SAFETY: still holding env_lock via `_guard`.
            unsafe {
                match &self.prev {
                    Some(value) => std::env::set_var(TINK_HOME_ENV, value),
                    None => std::env::remove_var(TINK_HOME_ENV),
                }
            }
        }
    }

    fn write_skill(dir: &Path, name: &str, body: &str) -> Skill {
        fs::create_dir_all(dir).unwrap();
        let text = format!(
            "---\nname: {name}\ndescription: Unit fixture {name}.\n---\n\n# {name}\n\n{body}\n"
        );
        fs::write(dir.join("SKILL.md"), text).unwrap();
        skills::read_skill(dir, true).unwrap()
    }

    fn skill_md(path: &Path) -> String {
        fs::read_to_string(path.join("SKILL.md")).unwrap()
    }

    fn sample_provenance() -> Provenance {
        let mut provenance = Provenance::new();
        provenance.insert(
            "source".into(),
            "https://github.com/example/repo.git".into(),
        );
        provenance.insert(
            "revision".into(),
            "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa".into(),
        );
        provenance.insert("path".into(), "skills/demo-skill".into());
        provenance
    }

    #[test]
    fn create_only_missing_creates() {
        let home = TempHome::new();
        let src = home.root.join("src").join("demo-skill");
        let skill = write_skill(&src, "demo-skill", "fresh body");

        let (path, write) = deposit_create_only(&skill).unwrap();
        assert_eq!(write, CreateOnlyWrite::Created);
        assert_eq!(path, home.stash_skill("demo-skill"));
        assert!(skill_md(&path).contains("fresh body"));
    }

    #[test]
    fn create_only_identical_unchanged() {
        let home = TempHome::new();
        let src = home.root.join("src").join("demo-skill");
        let skill = write_skill(&src, "demo-skill", "same body");

        assert_eq!(
            deposit_create_only(&skill).unwrap().1,
            CreateOnlyWrite::Created
        );
        let before = skill_md(&home.stash_skill("demo-skill"));

        let (path, write) = deposit_create_only(&skill).unwrap();
        assert_eq!(write, CreateOnlyWrite::Unchanged);
        assert_eq!(skill_md(&path), before);
    }

    #[test]
    fn create_only_diverge_skips() {
        let home = TempHome::new();
        let first = home.root.join("first").join("demo-skill");
        let second = home.root.join("second").join("demo-skill");
        let original = write_skill(&first, "demo-skill", "original body");
        let incoming = write_skill(&second, "demo-skill", "incoming body");

        assert_eq!(
            deposit_create_only(&original).unwrap().1,
            CreateOnlyWrite::Created
        );
        let before = skill_md(&home.stash_skill("demo-skill"));

        let (path, write) = deposit_create_only(&incoming).unwrap();
        assert!(
            matches!(write, CreateOnlyWrite::Skipped(Some(ref detail)) if detail.contains("create-only")),
            "{write:?}"
        );
        assert_eq!(skill_md(&path), before);
        assert!(before.contains("original body"));
    }

    #[test]
    fn deposit_diverge_repairs() {
        let home = TempHome::new();
        let first = home.root.join("first").join("demo-skill");
        let second = home.root.join("second").join("demo-skill");
        let original = write_skill(&first, "demo-skill", "original body");
        let incoming = write_skill(&second, "demo-skill", "incoming body");

        assert_eq!(deposit(&original, None).unwrap().1, StashWrite::Created);

        let (path, write) = deposit(&incoming, None).unwrap();
        assert_eq!(write, StashWrite::Repaired);
        assert!(skill_md(&path).contains("incoming body"));
        assert!(!skill_md(&path).contains("original body"));
    }

    #[test]
    fn create_only_receipt_only_diff_unchanged() {
        let home = TempHome::new();
        let src = home.root.join("src").join("demo-skill");
        let skill = write_skill(&src, "demo-skill", "shared body");
        let provenance = sample_provenance();

        assert_eq!(
            deposit(&skill, Some(&provenance)).unwrap().1,
            StashWrite::Created
        );
        let stash = home.stash_skill("demo-skill");
        assert!(stash.join(".tink-source.json").is_file());
        let before = skill_md(&stash);

        // Incoming tree matches body but has no receipt — create-only must not rewrite.
        let (path, write) = deposit_create_only(&skill).unwrap();
        assert_eq!(write, CreateOnlyWrite::Unchanged);
        assert_eq!(skill_md(&path), before);
        assert!(path.join(".tink-source.json").is_file());
    }
}

