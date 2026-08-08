//! `tink skill refresh` — update clean GitHub-imported skills.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use crate::catalog;
use crate::check;
use crate::error::Error;
use crate::git;
use crate::library;
use crate::provenance::{self, Provenance};
use crate::skills::{self, Skill};
use crate::sources;

use tempfile::TempDir;

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

fn checkout_reference_skill(
    repository: &Path,
    tip_revision: &str,
    recorded_revision: &str,
) -> Result<(PathBuf, Option<TempDir>), Error> {
    if tip_revision == recorded_revision {
        return Ok((repository.to_path_buf(), None));
    }
    let (temp, checkout) = git::checkout_revision(repository, recorded_revision)?;
    Ok((checkout, Some(temp)))
}

/// Returns whether the installed skill tree changed (receipt-only bumps are false).
fn refresh_one(installed: &Skill) -> Result<Option<bool>, Error> {
    let name = installed.name.clone();
    let Some(provenance) = provenance::read(installed)? else {
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

    let (old_repository, _old_checkout) = checkout_reference_skill(
        &current_repository,
        &current_revision,
        &provenance["revision"],
    )?;

    let old_skill = skills::read_skill(
        &skill_at(&old_repository, &provenance["path"])?,
        !source_is_repository_root,
    )?;

    match skills::preflight_install(&old_skill, &destination_root, Some(&provenance))? {
        skills::PreflightOutcome::Ready => {
            return Err(Error::msg(format!(
                "Refusing to update missing installed skill: {name}"
            )));
        }
        // Receipt drift alone is not a content edit; refresh rewrites receipts later.
        skills::PreflightOutcome::Identical | skills::PreflightOutcome::ReceiptMismatch => {}
        skills::PreflightOutcome::Divergent => {
            return Err(Error::msg(format!(
                "Refusing to update {name}: local modifications are present"
            )));
        }
    }

    if current_revision == provenance["revision"] {
        // No upstream move — still keep the library honest.
        library::sync_from_installed(installed)?;
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

    let tree_changed = !skills::skill_contents_equal(&old_skill.path, &new_skill.path)?;
    library::preflight_refresh(&installed.path, &new_skill, &next)?;
    let _installed = skills::replace_verified(&new_skill, &destination_root, &next)?;
    library::deposit_refresh(&new_skill, &next)?;
    Ok(Some(tree_changed))
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
        None => Err(Error::msg(format!(
            "Local skill has no remote source: {name}"
        ))),
        Some(changed) => {
            catalog::deposit_skill(root, name)?;
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
                catalog::deposit_skill(root, &skill.name)?;
                refreshed.push(skill.name.clone());
            }
            Some(false) => {
                catalog::deposit_skill(root, &skill.name)?;
            }
            None => {}
        }
    }
    Ok(refreshed)
}
