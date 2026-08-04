//! `tink skill remove` — delete one project skill directory.

use std::fs;
use std::path::{Path, PathBuf};

use crate::error::Error;
use crate::paths::{map_io, refuse_symlink};
use crate::skills;

#[derive(Debug)]
pub struct RemoveReport {
    pub removed: PathBuf,
}

/// Delete `<project>/.agents/skills/<name>/` only.
///
/// Does not touch `$TINK_HOME` stash trees or the by-project catalog.
pub fn remove_skill(project_root: &Path, name: &str) -> Result<RemoveReport, Error> {
    if !skills::valid_skill_name(name) {
        return Err(Error::msg(format!("Invalid skill name: {name}")));
    }

    let agents = project_root.join(".agents");
    let skills_root = agents.join("skills");
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

    fs::remove_dir_all(&target).map_err(|e| map_io(&target, e))?;
    Ok(RemoveReport { removed: target })
}
