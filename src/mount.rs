use std::fs;
use std::path::{Path, PathBuf};

use crate::error::Error;
use crate::home;
use crate::paths::{canonicalize_beneath, map_io, mkdir_p, refuse_symlink};
use crate::skills;

#[derive(Debug, PartialEq, Eq)]
pub enum MountOutcome {
    Mounted(PathBuf),
}

#[derive(Debug, PartialEq, Eq)]
pub enum UnmountOutcome {
    Unmounted,
    NotMounted,
}

fn require_valid_skill_name(name: &str) -> Result<(), Error> {
    if !skills::valid_skill_name(name) {
        return Err(Error::msg(format!(
            "Invalid skill name '{name}': invalid syntax or traversal characters not permitted"
        )));
    }
    Ok(())
}

pub fn mount_skill(
    project_root: &Path,
    skill_name: &str,
    home: Option<&Path>,
) -> Result<MountOutcome, Error> {
    require_valid_skill_name(skill_name)?;

    let (home_root, _) = home::ensure_inventory_root(home)?;
    let library_root = home::skills_library_path(&home_root);

    let src = canonicalize_beneath(&library_root, Path::new(skill_name))
        .map_err(|_| Error::msg(format!("Skill '{skill_name}' not found in library")))?;

    let active_dir = home::project_active_skills_path(project_root);
    mkdir_p(&active_dir)?;
    refuse_symlink(&active_dir)?;

    let target = active_dir.join(skill_name);

    if target.exists() || target.is_symlink() {
        if target.is_dir() && !target.is_symlink() {
            return Err(Error::msg(format!(
                "Refusing to overwrite non-symlink directory: {}",
                target.display()
            )));
        }
        fs::remove_file(&target).map_err(|e| map_io(&target, e))?;
    }

    #[cfg(unix)]
    std::os::unix::fs::symlink(&src, &target).map_err(|e| map_io(&target, e))?;

    #[cfg(windows)]
    std::os::windows::fs::symlink_dir(&src, &target).map_err(|e| map_io(&target, e))?;

    Ok(MountOutcome::Mounted(target))
}

pub fn unmount_skill(project_root: &Path, skill_name: &str) -> Result<UnmountOutcome, Error> {
    require_valid_skill_name(skill_name)?;

    let target = home::project_active_skills_path(project_root).join(skill_name);
    if !target.exists() && !target.is_symlink() {
        return Ok(UnmountOutcome::NotMounted);
    }

    if target.is_dir() && !target.is_symlink() {
        return Err(Error::msg(format!(
            "Refusing to remove non-symlink directory: {}",
            target.display()
        )));
    }

    fs::remove_file(&target).map_err(|e| map_io(&target, e))?;

    Ok(UnmountOutcome::Unmounted)
}
