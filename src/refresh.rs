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
    let mut relative = PathBuf::from(".");
    if source_path != "." {
        relative.clear();
        for part in source_path.split('/') {
            if part.is_empty() || part == "." || part == ".." {
                return Err(Error::msg(format!(
                    "Upstream skill path is missing: {source_path}"
                )));
            }
            relative.push(part);
        }
    }
    let path = crate::paths::canonicalize_beneath(repository, &relative)?;
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

enum RefreshPlan {
    Local {
        name: String,
    },
    Unchanged {
        installed: Skill,
    },
    Update {
        installed: Skill,
        new_skill: Skill,
        next: Provenance,
        tree_changed: bool,
        _clone_temp: TempDir,
    },
}

impl RefreshPlan {
    fn name(&self) -> &str {
        match self {
            Self::Local { name } => name,
            Self::Unchanged { installed } | Self::Update { installed, .. } => &installed.name,
        }
    }
}

fn prepare_refresh(installed: Skill) -> Result<RefreshPlan, Error> {
    let name = installed.name.clone();
    let Some(provenance) = provenance::read(&installed)? else {
        return Ok(RefreshPlan::Local { name });
    };
    let remote = sources::parse_remote(&provenance["source"])?;
    let destination_root = installed
        .path
        .parent()
        .ok_or_else(|| Error::msg("skill has no parent"))?
        .to_path_buf();

    // Keep clone (and optional worktree) alive for the whole update.
    let (clone_temp, current_repository, current_revision) = git::checkout(&remote)?;
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
        return Ok(RefreshPlan::Unchanged { installed });
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
    Ok(RefreshPlan::Update {
        installed,
        new_skill,
        next,
        tree_changed,
        _clone_temp: clone_temp,
    })
}

/// Returns whether the installed skill tree changed (receipt-only bumps are false).
fn apply_refresh(plan: RefreshPlan) -> Result<Option<bool>, Error> {
    match plan {
        RefreshPlan::Local { .. } => Ok(None),
        RefreshPlan::Unchanged { installed } => {
            // No upstream move — still keep the library honest.
            library::sync_from_installed(&installed)?;
            Ok(Some(false))
        }
        RefreshPlan::Update {
            installed,
            new_skill,
            next,
            tree_changed,
            ..
        } => {
            let destination_root = installed
                .path
                .parent()
                .ok_or_else(|| Error::msg("skill has no parent"))?;
            let _installed = skills::replace_verified(&new_skill, destination_root, Some(&next))?;
            library::deposit_refresh(&new_skill, &next)?;
            Ok(Some(tree_changed))
        }
    }
}

pub fn refresh_skill(root: &Path, name: &str) -> Result<bool, Error> {
    let skills: BTreeMap<_, _> = check::check_project(root)?
        .into_iter()
        .map(|s| (s.name.clone(), s))
        .collect();
    let installed = skills
        .get(name)
        .ok_or_else(|| Error::msg(format!("Installed skill not found: {name}")))?;
    match apply_refresh(prepare_refresh(installed.clone())?)? {
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
    let plans = installed
        .into_iter()
        .map(prepare_refresh)
        .collect::<Result<Vec<_>, _>>()?;
    let mut refreshed = Vec::new();
    for plan in plans {
        let name = plan.name().to_string();
        match apply_refresh(plan)? {
            Some(true) => {
                catalog::deposit_skill(root, &name)?;
                refreshed.push(name);
            }
            Some(false) => {
                catalog::deposit_skill(root, &name)?;
            }
            None => {}
        }
    }
    Ok(refreshed)
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::*;

    #[cfg(unix)]
    #[test]
    fn skill_at_refuses_symlinked_ancestor() {
        let temp = tempfile::tempdir().unwrap();
        let repository = temp.path().join("repository");
        let outside = temp.path().join("outside");
        fs::create_dir_all(&repository).unwrap();
        fs::create_dir_all(outside.join("skills/alpha")).unwrap();
        std::os::unix::fs::symlink(&outside, repository.join("jump")).unwrap();

        let error = skill_at(&repository, "jump/skills/alpha")
            .expect_err("ancestor symlink must be refused");

        assert!(error.to_string().contains("symlink"), "{error}");
    }
}
