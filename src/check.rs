//! Offline project skill validation (`tink skill check`).

use std::fs;
use std::path::Path;

use crate::error::Error;
use crate::paths::{map_io, refuse_symlink};
use crate::provenance;
use crate::skills::{self, Skill};

fn is_ignored_skill_entry(name: &str) -> bool {
    name == "README.md" || name.starts_with('.')
}

fn read_skill_entry(path: &Path) -> Result<Option<Skill>, Error> {
    let name = path.file_name().and_then(|s| s.to_str()).unwrap_or("");
    if is_ignored_skill_entry(name) {
        return Ok(None);
    }
    if path.is_symlink() || !path.is_dir() {
        return Err(Error::msg(format!(
            "Unexpected entry in .agents/skills: {name}"
        )));
    }
    if crate::skillsets::has_receipt_entry(path) {
        crate::skillsets::validate_installed(path)?;
        return Ok(None);
    }
    let skill = skills::read_skill(path, true)?;
    skills::validate_skill_tree(path)?;
    let provenance = provenance::read(&skill)?;
    if skill.name == "manage-tink" && provenance.is_none() {
        crate::manage_tink::require_current(&skill)?;
    }
    Ok(Some(skill))
}

/// Load and validate project skills under `.agents/skills/`.
pub fn load_project_skills(root: &Path) -> Result<Vec<Skill>, Error> {
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

    let mut skills = Vec::new();
    for path in entries {
        if let Some(skill) = read_skill_entry(&path)? {
            skills.push(skill);
        }
    }
    Ok(skills)
}

/// Validate project skills. No writes. No network for local skills; provenance
/// shape is checked without fetching.
pub fn check_project(root: &Path) -> Result<Vec<Skill>, Error> {
    load_project_skills(root)
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
        let _socket = std::os::unix::net::UnixListener::bind(skill.join("socket")).unwrap();

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
