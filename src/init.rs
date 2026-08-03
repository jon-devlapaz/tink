//! Minimal project skill home: `.agents/skills/` (+ inventory ensure).

use std::path::Path;

use crate::error::Error;
use crate::inventory;
use crate::paths::{map_io, mkdir_p, require_directory, require_file};

const SKILLS_README: &str = "\
# Project skills

Complete, repository-owned Agent Skills live in this directory. Each skill is a
directory containing a `SKILL.md` file and any resources it needs.
";

/// Create `.agents/skills/` if needed and ensure the home inventory root exists.
///
/// Does not write `AGENTS.md`, `ZEN.md`, or GitHub workflows.
/// Returns `(skills_path, skills_created, inventory_home, inventory_created)`.
pub fn init_project(
    project_root: &Path,
) -> Result<(std::path::PathBuf, bool, std::path::PathBuf, bool), Error> {
    let agents = project_root.join(".agents");
    let skills = agents.join("skills");
    let readme = skills.join("README.md");

    require_directory(&agents)?;
    require_directory(&skills)?;
    require_file(&readme)?;

    let skills_created = !skills.is_dir();
    mkdir_p(&agents)?;
    mkdir_p(&skills)?;
    if !readme.exists() {
        std::fs::write(&readme, SKILLS_README).map_err(|e| map_io(&readme, e))?;
    }

    let (home, home_created) = inventory::ensure_inventory_root(None)?;
    Ok((skills, skills_created, home, home_created))
}
