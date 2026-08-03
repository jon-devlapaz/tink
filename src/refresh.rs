//! `tink skill refresh` — update clean GitHub-imported skills.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use crate::check::{self, read_provenance};
use crate::error::Error;
use crate::git;
use crate::inventory;
use crate::skills::{self, Provenance, Skill};
use crate::sources;

fn skill_at(repository: &Path, source_path: &str) -> Result<PathBuf, Error> {
    let mut path = repository.to_path_buf();
    if source_path != "." {
        for part in source_path.split('/') {
            if part.is_empty() || part == "." || part == ".." {
                return Err(Error::msg(format!(
                    "Upstream skill path is missing: {source_path}"
                )));
            }
            path.push(part);
        }
    }
    if !path.is_dir() {
        return Err(Error::msg(format!(
            "Upstream skill path is missing: {source_path}"
        )));
    }
    Ok(path)
}

/// Returns whether the installed skill tree changed (receipt-only bumps are false).
fn refresh_one(installed: &Skill) -> Result<Option<bool>, Error> {
    let name = installed.name.clone();
    let Some(provenance) = read_provenance(installed)? else {
        return Ok(None);
    };
    let remote = sources::parse_remote(&provenance["source"])?;
    let destination_root = installed
        .path
        .parent()
        .ok_or_else(|| Error::msg("skill has no parent"))?
        .to_path_buf();

    // Keep clone (and optional worktree) alive for the whole update.
    let (_clone_temp, current_repository, current_revision) = git::checkout(&remote)?;
    let current_repository = current_repository
        .canonicalize()
        .map_err(|e| Error::msg(format!("repository: {e}")))?;
    let source_is_repository_root = provenance["path"] == ".";

    let mut _old_temp = None;
    let old_repository = if current_revision == provenance["revision"] {
        current_repository.clone()
    } else {
        let (temp, path) = git::checkout_revision(&current_repository, &provenance["revision"])?;
        _old_temp = Some(temp);
        path
    };

    let old_skill = skills::read_skill(
        &skill_at(&old_repository, &provenance["path"])?,
        !source_is_repository_root,
    )?;

    match skills::preflight_install(&old_skill, &destination_root, Some(&provenance)) {
        Ok(true) => {
            return Err(Error::msg(format!(
                "Refusing to update missing installed skill: {name}"
            )));
        }
        Ok(false) => {}
        Err(err) if err.to_string().contains("Refusing to overwrite") => {
            return Err(Error::msg(format!(
                "Refusing to update {name}: local modifications are present"
            )));
        }
        Err(err) => return Err(err),
    }

    if current_revision == provenance["revision"] {
        // No upstream move — still keep the home archive honest.
        inventory::sync_archive_from_installed(installed)?;
        return Ok(Some(false));
    }

    let new_skill = skills::read_skill(
        &skill_at(&current_repository, &provenance["path"])?,
        !source_is_repository_root,
    )?;
    if new_skill.name != name {
        return Err(Error::msg(format!(
            "Upstream skill name changed from {name} to {}",
            new_skill.name
        )));
    }

    let mut next: Provenance = provenance.clone();
    next.insert("revision".into(), current_revision);

    if skills::skill_contents_equal(&old_skill.path, &new_skill.path)? {
        // Tree unchanged; bump receipt + sync archive.
        inventory::preflight_archive_refresh(&installed.path, &new_skill, &next)?;
        let _installed = skills::replace_verified(&new_skill, &destination_root, &next)?;
        inventory::deposit_archive_refresh(&new_skill, &next)?;
        return Ok(Some(false));
    }

    inventory::preflight_archive_refresh(&installed.path, &new_skill, &next)?;
    let _installed = skills::replace_verified(&new_skill, &destination_root, &next)?;
    inventory::deposit_archive_refresh(&new_skill, &next)?;
    Ok(Some(true))
}

pub fn refresh_skill(root: &Path, name: &str) -> Result<bool, Error> {
    let skills: BTreeMap<_, _> = check::check_project(root)?
        .into_iter()
        .map(|s| (s.name.clone(), s))
        .collect();
    let installed = skills
        .get(name)
        .ok_or_else(|| Error::msg(format!("Installed skill not found: {name}")))?;
    match refresh_one(installed)? {
        None => Err(Error::msg(format!("Local skill has no remote source: {name}"))),
        Some(changed) => {
            inventory::deposit_skill(root, name)?;
            Ok(changed)
        }
    }
}

pub fn refresh_all(root: &Path) -> Result<Vec<String>, Error> {
    let installed = check::check_project(root)?;
    let mut refreshed = Vec::new();
    for skill in &installed {
        match refresh_one(skill)? {
            Some(true) => {
                inventory::deposit_skill(root, &skill.name)?;
                refreshed.push(skill.name.clone());
            }
            Some(false) => {
                inventory::deposit_skill(root, &skill.name)?;
            }
            None => {}
        }
    }
    Ok(refreshed)
}
