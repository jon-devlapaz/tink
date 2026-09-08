//! Offline project skill validation (`tink skill check`).

use std::fs;
use std::path::{Path, PathBuf};

use crate::error::Error;
use crate::paths::{map_io, refuse_symlink};
use crate::provenance;
use crate::skills::{self, Skill};

/// Result of a project integrity walk that enumerates every root.
#[derive(Debug, Default)]
pub struct ProjectCheck {
    pub skills: Vec<Skill>,
    pub skillsets: usize,
    pub members: usize,
    pub failures: Vec<String>,
}

fn read_standalone(path: &Path, strict_manage_tink: bool) -> Result<Skill, Error> {
    let skill = skills::read_skill(path, true)?;
    skills::validate_skill_tree(path)?;
    let provenance = provenance::read(&skill)?;
    if strict_manage_tink && skill.name == "manage-tink" && provenance.is_none() {
        crate::manage_tink::require_current(&skill)?;
    }
    Ok(skill)
}

fn read_skill_entry(path: &Path, strict_manage_tink: bool) -> Result<Option<Skill>, Error> {
    let name = path.file_name().and_then(|s| s.to_str()).unwrap_or("");
    match crate::skillsets::classify_entry(path) {
        crate::skillsets::EntryClass::Ignored => return Ok(None),
        crate::skillsets::EntryClass::Unexpected => {
            return Err(Error::msg(format!(
                "Unexpected entry in .agents/skills: {name}"
            )));
        }
        crate::skillsets::EntryClass::Skillset => {
            crate::skillsets::validate_installed(path)?;
            return Ok(None);
        }
        crate::skillsets::EntryClass::Standalone => {}
    }
    Ok(Some(read_standalone(path, strict_manage_tink)?))
}

/// Load and validate project skills under `.agents/skills/`.
/// No writes. No network for local skills; provenance shape is checked
/// without fetching.
///
/// Fails closed on the first invalid root. Prefer [`check_project`] when a
/// full inventory report is needed.
pub fn load_project_skills(root: &Path) -> Result<Vec<Skill>, Error> {
    load_entries(skill_entries(root)?, true)
}

/// Load project skills, tolerating a stale embedded manage-tink so
/// read-only diagnostics (`outdated`) can report it instead of failing.
pub fn load_project_skills_lenient(root: &Path) -> Result<Vec<Skill>, Error> {
    load_entries(skill_entries(root)?, false)
}

/// Load standalone project skills only; receipt-backed skillset roots are skipped
/// without digest validation so one divergent skillset cannot blank standalone list.
pub fn load_standalone_skills(root: &Path) -> Result<Vec<Skill>, Error> {
    let mut skills = Vec::new();
    for path in skill_entries(root)? {
        let name = path.file_name().and_then(|s| s.to_str()).unwrap_or("");
        match crate::skillsets::classify_entry(&path) {
            crate::skillsets::EntryClass::Ignored => {}
            crate::skillsets::EntryClass::Unexpected => {
                return Err(Error::msg(format!(
                    "Unexpected entry in .agents/skills: {name}"
                )));
            }
            crate::skillsets::EntryClass::Skillset => {}
            crate::skillsets::EntryClass::Standalone => {
                skills.push(read_standalone(&path, true)?);
            }
        }
    }
    Ok(skills)
}
///
/// Structural problems (missing skills root, unexpected entries, root
/// symlinks) still return `Err`. Divergent skillset or standalone trees are
/// recorded in [`ProjectCheck::failures`] so healthy roots stay visible.
pub fn check_project(root: &Path) -> Result<ProjectCheck, Error> {
    let entries = skill_entries(root)?;
    let mut report = ProjectCheck::default();
    for path in entries {
        let name = path.file_name().and_then(|s| s.to_str()).unwrap_or("");
        match crate::skillsets::classify_entry(&path) {
            crate::skillsets::EntryClass::Ignored => {}
            crate::skillsets::EntryClass::Unexpected => {
                return Err(Error::msg(format!(
                    "Unexpected entry in .agents/skills: {name}"
                )));
            }
            crate::skillsets::EntryClass::Skillset => {
                match crate::skillsets::validate_installed(&path) {
                    Ok(members) => {
                        report.skillsets += 1;
                        report.members += members;
                    }
                    Err(error) => report.failures.push(error.to_string()),
                }
            }
            crate::skillsets::EntryClass::Standalone => match read_standalone(&path, true) {
                Ok(skill) => report.skills.push(skill),
                Err(error) => report.failures.push(error.to_string()),
            },
        }
    }
    Ok(report)
}

fn skill_entries(root: &Path) -> Result<Vec<PathBuf>, Error> {
    let agents = crate::home::project_agents_path(root);
    let skills_root = crate::home::project_skills_path(root);
    refuse_symlink(&agents)?;
    refuse_symlink(&skills_root)?;
    if !skills_root.is_dir() {
        return Err(Error::msg("Missing .agents/skills"));
    }

    let mut entries: Vec<_> = fs::read_dir(&skills_root)
        .map_err(|e| map_io(&skills_root, e))?
        .map(|entry| {
            entry
                .map(|entry| entry.path())
                .map_err(|e| map_io(&skills_root, e))
        })
        .collect::<Result<_, _>>()?;
    entries.sort();
    Ok(entries)
}

fn load_entries(entries: Vec<PathBuf>, strict_manage_tink: bool) -> Result<Vec<Skill>, Error> {
    let mut skills = Vec::new();
    for path in entries {
        if let Some(skill) = read_skill_entry(&path, strict_manage_tink)? {
            skills.push(skill);
        }
    }
    Ok(skills)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn write_skill(root: &Path) {
        fs::create_dir_all(root).unwrap();
        fs::write(
            root.join("SKILL.md"),
            "---\nname: demo-skill\ndescription: A valid test skill.\n---\n",
        )
        .unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn load_project_skills_refuses_nested_symlink() {
        let temp = TempDir::new().unwrap();
        let skill = temp.path().join(".agents/skills/demo-skill");
        write_skill(&skill);
        std::os::unix::fs::symlink("/tmp", skill.join("nested-link")).unwrap();

        let err = load_project_skills(temp.path()).unwrap_err();

        assert!(err.to_string().contains("symlink"), "{err}");
    }

    #[cfg(unix)]
    #[test]
    fn load_project_skills_refuses_nested_special_file() {
        let temp = TempDir::new().unwrap();
        let skill = temp.path().join(".agents/skills/demo-skill");
        write_skill(&skill);
        // FIFO, not a Unix socket: socket bind is denied in some sandboxes
        // while mkfifo is allowed, and both hit the same "special file" branch.
        let status = std::process::Command::new("mkfifo")
            .arg(skill.join("fifo"))
            .status()
            .unwrap();
        assert!(status.success(), "mkfifo fixture failed: {status}");

        let err = load_project_skills(temp.path()).unwrap_err();

        assert!(err.to_string().contains("special file"), "{err}");
    }

    #[cfg(unix)]
    #[test]
    fn load_project_skills_ignores_git_contents() {
        let temp = TempDir::new().unwrap();
        let skill = temp.path().join(".agents/skills/demo-skill");
        write_skill(&skill);
        fs::create_dir_all(skill.join(".git")).unwrap();
        std::os::unix::fs::symlink("/tmp", skill.join(".git/ignored-link")).unwrap();

        let skills = load_project_skills(temp.path()).unwrap();

        assert_eq!(skills.len(), 1);
        assert_eq!(skills[0].name, "demo-skill");
    }
}
