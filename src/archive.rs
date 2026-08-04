//! Home skill-tree archive cache (`$TINK_HOME/skills/<name>/`).

use std::fs;
use std::path::{Path, PathBuf};

use crate::error::Error;
use crate::home::{ensure_inventory_root, skills_archive_path, BY_PROJECT};
use crate::paths::{map_io, mkdir_p, refuse_symlink};
use crate::provenance::{self, Provenance};
use crate::skills::{self, PreflightOutcome, Skill};

/// Result of writing a skill into the home archive cache.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ArchiveWrite {
    /// Archive was missing; tree was created.
    Created,
    /// Archive already matched the incoming tree (including receipt).
    Unchanged,
    /// Archive diverged; replaced with the incoming tree.
    Repaired,
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

/// Copy skill tree into `~/.tink/skills/<name>/`.
///
/// The archive is a rebuildable cache: identical → noop; missing → create;
/// divergent → replace (caller should warn). Project installs still refuse
/// overwrites separately.
pub fn deposit_archive(
    skill: &Skill,
    provenance: Option<&Provenance>,
) -> Result<(PathBuf, ArchiveWrite), Error> {
    let (home, _) = ensure_inventory_root(None)?;
    let archive = skills_archive_path(&home);
    mkdir_p(&archive)?;
    match skills::preflight_install(skill, &archive, provenance)? {
        PreflightOutcome::Ready => {
            let (path, _) = skills::install_local(skill, &archive, provenance)?;
            Ok((path, ArchiveWrite::Created))
        }
        PreflightOutcome::Identical => Ok((archive.join(&skill.name), ArchiveWrite::Unchanged)),
        PreflightOutcome::Divergent => {
            clear_path(&archive.join(&skill.name))?;
            let (path, _) = skills::install_local(skill, &archive, provenance)?;
            Ok((path, ArchiveWrite::Repaired))
        }
    }
}

/// When the home archive already holds the exact tree we would install, return it.
pub fn matching_archive(
    skill: &Skill,
    provenance: Option<&Provenance>,
) -> Result<Option<Skill>, Error> {
    let (home, _) = ensure_inventory_root(None)?;
    let archive = skills_archive_path(&home);
    let target = archive.join(&skill.name);
    if !target.is_dir() {
        return Ok(None);
    }
    match skills::preflight_install(skill, &archive, provenance)? {
        PreflightOutcome::Identical => Ok(Some(skills::read_skill(&target, true)?)),
        PreflightOutcome::Ready | PreflightOutcome::Divergent => Ok(None),
    }
}

/// Find a home archive whose receipt matches this remote URL + revision tip.
pub fn archive_for_remote_tip(
    source_url: &str,
    revision: &str,
    selected_name: Option<&str>,
) -> Result<Option<Skill>, Error> {
    let (home, _) = ensure_inventory_root(None)?;
    let archive = skills_archive_path(&home);
    if !archive.is_dir() {
        return Ok(None);
    }
    let mut hits = Vec::new();
    for entry in fs::read_dir(&archive).map_err(|e| map_io(&archive, e))? {
        let entry = entry.map_err(|e| map_io(&archive, e))?;
        let path = entry.path();
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if name == BY_PROJECT || name.starts_with('.') || name == "README.md" {
            continue;
        }
        if let Some(want) = selected_name {
            if name != want {
                continue;
            }
        }
        if path.is_symlink() || !path.is_dir() {
            continue;
        }
        let skill = match skills::read_skill(&path, true) {
            Ok(skill) => skill,
            Err(_) => continue,
        };
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
                "Home archive has multiple skills for this revision. Choose one:\n{commands}"
            )))
        }
    }
}

fn archive_tracks_project(home_skill: &Path, project_skill: &Path) -> Result<bool, Error> {
    if skills::skill_contents_equal(home_skill, project_skill)? {
        return Ok(true);
    }
    // Allow a missing/different receipt when the skill body still matches.
    skills::skill_contents_equal_except(home_skill, project_skill, &[".tink-source.json"])
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
    match skills::preflight_install(new_skill, &archive, Some(new_provenance))? {
        PreflightOutcome::Ready | PreflightOutcome::Identical => Ok(()),
        PreflightOutcome::Divergent => {
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
        clear_path(&target)?;
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
