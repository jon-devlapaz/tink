//! `tink skill remove` — delete one project skill directory.

use std::fs;
use std::path::{Path, PathBuf};

use crate::catalog;
use crate::error::Error;
use crate::paths::{map_io, refuse_symlink};
use crate::skills;

#[derive(Debug)]
pub struct RemoveReport {
    pub removed: PathBuf,
}

/// Drop `<name>` from the by-project catalog, then delete
/// `<project>/.agents/skills/<name>/`. Does not touch `$TINK_HOME` library trees.
/// Catalog sync runs before disk delete; sync errors leave the skill tree intact.
pub fn remove_skill(project_root: &Path, name: &str) -> Result<RemoveReport, Error> {
    if !skills::valid_skill_name(name) {
        return Err(Error::msg(format!("Invalid skill name: {name}")));
    }

    let agents = crate::home::project_agents_path(project_root);
    let skills_root = crate::home::project_skills_path(project_root);
    let target = skills_root.join(name);

    if agents.exists() || agents.is_symlink() {
        refuse_symlink(&agents)?;
    }
    if skills_root.exists() || skills_root.is_symlink() {
        refuse_symlink(&skills_root)?;
    }

    if !target.exists() && !target.is_symlink() {
        return Err(Error::msg(format!("Skill not found: {name}")));
    }
    refuse_symlink(&target)?;
    if !target.is_dir() {
        return Err(Error::msg(format!(
            "Refusing to remove non-directory: {}",
            target.display()
        )));
    }
    if target.join(crate::skillsets::RECEIPT_FILE).exists()
        || target.join(crate::skillsets::RECEIPT_FILE).is_symlink()
    {
        return Err(Error::msg(format!(
            "Skillset root detected; use `tink skillset remove {name}`"
        )));
    }

    catalog::withdraw_skill(project_root, name)?;
    fs::remove_dir_all(&target).map_err(|e| map_io(&target, e))?;
    Ok(RemoveReport { removed: target })
}
