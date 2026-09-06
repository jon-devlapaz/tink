//! Standalone inventory publish seam: layout → preflight → library →
//! project install → catalog.
//!
//! Callers keep classification/selection and warn rendering. Skillset
//! staging stays out (see `.agents/specs/inventory-publish-seam.md`).

use std::path::{Path, PathBuf};

use crate::catalog;
use crate::error::Error;
use crate::init;
use crate::library::{self, LibraryWrite};
use crate::provenance::Provenance;
use crate::skills::{self, Skill};

/// Result of publishing one standalone skill into project + inventory.
#[derive(Debug)]
pub(crate) struct PublishOutcome {
    pub name: String,
    pub created: bool,
    pub library_write: LibraryWrite,
    pub project_path: PathBuf,
}

/// Publish from a source tree path (local or checkout). Repair library first.
pub(crate) fn publish(
    home: Option<&Path>,
    project_root: &Path,
    skill: &Skill,
    provenance: Option<&Provenance>,
) -> Result<PublishOutcome, Error> {
    let destination_root = crate::home::project_skills_path(project_root);
    init::ensure_project_layout_at(home, project_root)?;
    // Protect the project tree first. Library is a rebuildable collection: repair on
    // diverge, then install project (re-add recovers if that fails).
    skills::preflight_install(skill, &destination_root, provenance)?
        .require_compatible(&skill.name, &destination_root)?;
    let (_, library_write) = library::deposit_at(home, skill, provenance)?;
    let (installed, created) = skills::install_local(skill, &destination_root, provenance)?;
    // Catalog even on identical noop so the name index can catch up.
    catalog::deposit_skill_at(home, project_root, &skill.name)?;
    Ok(PublishOutcome {
        name: skill.name.clone(),
        created,
        library_write,
        project_path: installed,
    })
}

/// Install into the project from an already-complete library tree.
pub(crate) fn publish_from_library(
    home: Option<&Path>,
    project_root: &Path,
    skill: &Skill,
) -> Result<PublishOutcome, Error> {
    let destination_root = crate::home::project_skills_path(project_root);
    init::ensure_project_layout_at(home, project_root)?;
    skills::preflight_install(skill, &destination_root, None)?
        .require_compatible(&skill.name, &destination_root)?;
    let (installed, created) = skills::install_local(skill, &destination_root, None)?;
    catalog::deposit_skill_at(home, project_root, &skill.name)?;
    Ok(PublishOutcome {
        name: skill.name.clone(),
        created,
        library_write: LibraryWrite::Unchanged,
        project_path: installed,
    })
}
