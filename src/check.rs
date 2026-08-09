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
    if path.join(crate::skillsets::RECEIPT_FILE).exists()
        || path.join(crate::skillsets::RECEIPT_FILE).is_symlink()
    {
        crate::skillsets::validate_installed(path)?;
        return Ok(None);
    }
    let skill = skills::read_skill(path, true)?;
    provenance::read(&skill)?;
    Ok(Some(skill))
}

/// Load and validate project skills under `.agents/skills/`.
/// Does not enforce ZEN.md / AGENTS.md coupling (see [`check_zen_coupling`]).
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
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .collect();
    entries.sort();

    let mut skills = Vec::new();
    for path in entries {
        if let Some(skill) = read_skill_entry(&path)? {
            skills.push(skill);
        }
    }
    Ok(skills)
}

/// When `ZEN.md` is present, require a regular `AGENTS.md` that references it.
pub fn check_zen_coupling(root: &Path) -> Result<(), Error> {
    let zen = root.join("ZEN.md");
    if !zen.exists() && !zen.is_symlink() {
        return Ok(());
    }
    refuse_symlink(&zen)?;
    if !zen.is_file() {
        return Err(Error::msg("ZEN.md must be a regular file"));
    }
    let agents_file = root.join("AGENTS.md");
    refuse_symlink(&agents_file)?;
    if !agents_file.is_file() {
        return Err(Error::msg(
            "ZEN.md is not referenced by a regular AGENTS.md",
        ));
    }
    let agents_text = fs::read_to_string(&agents_file).map_err(|e| map_io(&agents_file, e))?;
    if !agents_text.contains(crate::templates::ZEN_REFERENCE_MARKER) {
        return Err(Error::msg(format!(
            "AGENTS.md does not reference {}",
            crate::templates::ZEN_REFERENCE_MARKER
        )));
    }
    Ok(())
}

/// Validate project skills. No writes. No network for local skills; provenance
/// shape is checked without fetching. Hard-fails on ZEN/AGENTS coupling errors.
pub fn check_project(root: &Path) -> Result<Vec<Skill>, Error> {
    let skills = load_project_skills(root)?;
    check_zen_coupling(root)?;
    Ok(skills)
}
