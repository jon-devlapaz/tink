//! Embedded `manage-tink` skill shipped with the Tink binary.

use std::path::Path;

use crate::add;
use crate::error::Error;
use crate::library;
use crate::paths::{map_io, refuse_symlink};
use crate::provenance;
use crate::skills::{self, Skill};

const SKILL_MD: &str = include_str!("../skills/manage-tink/SKILL.md");
const COMMANDS_MD: &str = include_str!("../skills/manage-tink/references/commands.md");
const TINK_JEV_MD: &str = include_str!("../skills/manage-tink/references/tink-jev.md");

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RefreshOutcome {
    Installed,
    Unchanged,
    Refreshed,
}

/// Expected embedded layout for the read-only freshness check.
const EMBEDDED_DIRS: &[&str] = &["references"];
const EMBEDDED_FILES: &[(&str, &[u8])] = &[
    ("SKILL.md", SKILL_MD.as_bytes()),
    ("references/commands.md", COMMANDS_MD.as_bytes()),
    ("references/tink-jev.md", TINK_JEV_MD.as_bytes()),
];

/// Materialize the embedded tree for installation or refresh publication.
/// The returned guard owns the bytes referenced by `Skill`.
pub(crate) fn prepare_manage_tink() -> Result<(tempfile::TempDir, Skill), Error> {
    let staging = tempfile::Builder::new()
        .prefix(".tink-manage-tink-")
        .tempdir()
        .map_err(|e| Error::msg(format!("manage-tink staging: {e}")))?;
    let skill_root = staging.path().join("manage-tink");
    let references = skill_root.join("references");
    std::fs::create_dir_all(&references).map_err(|e| map_io(&references, e))?;
    std::fs::write(skill_root.join("SKILL.md"), SKILL_MD)
        .map_err(|e| map_io(&skill_root.join("SKILL.md"), e))?;
    std::fs::write(references.join("commands.md"), COMMANDS_MD)
        .map_err(|e| map_io(&references.join("commands.md"), e))?;
    std::fs::write(references.join("tink-jev.md"), TINK_JEV_MD)
        .map_err(|e| map_io(&references.join("tink-jev.md"), e))?;
    let skill = skills::read_skill(&skill_root, true)?;
    Ok((staging, skill))
}

pub(crate) fn is_current(installed: &Skill) -> Result<bool, Error> {
    // In-memory compare: `skill check` is read-only and must not stage tempdirs.
    skills::skill_dir_matches_embedded(&installed.path, EMBEDDED_DIRS, EMBEDDED_FILES)
}

/// Require an installed embedded copy to match the payload in this binary.
pub(crate) fn require_current(installed: &Skill) -> Result<(), Error> {
    if is_current(installed)? {
        return Ok(());
    }
    Err(Error::msg(
        "manage-tink differs from this Tink binary; run `tink skill refresh manage-tink`",
    ))
}

fn refuse_remote_library_collision(home: Option<&Path>) -> Result<(), Error> {
    let Some(home) = crate::home::existing_inventory_root(home)? else {
        return Ok(());
    };
    let target = crate::home::skills_library_path(&home).join("manage-tink");
    refuse_symlink(&target)?;
    if !target.is_dir() {
        return Ok(());
    }
    let library_skill = skills::read_skill(&target, true)?;
    if provenance::read(&library_skill)?.is_some() {
        return Err(Error::msg(
            "Refusing to replace library manage-tink with remote provenance",
        ));
    }
    Ok(())
}

pub(crate) fn refresh_manage_tink(project_root: &Path) -> Result<RefreshOutcome, Error> {
    refresh_manage_tink_at(None, project_root)
}

pub(crate) fn refresh_manage_tink_at(
    home: Option<&Path>,
    project_root: &Path,
) -> Result<RefreshOutcome, Error> {
    let agents = crate::home::project_agents_path(project_root);
    let skills_root = crate::home::project_skills_path(project_root);
    let target = skills_root.join("manage-tink");
    refuse_symlink(&agents)?;
    refuse_symlink(&skills_root)?;
    refuse_symlink(&target)?;
    refuse_remote_library_collision(home)?;

    if !target.exists() {
        install_manage_tink_at(home, project_root)?;
        return Ok(RefreshOutcome::Installed);
    }
    if !target.is_dir() {
        return Err(Error::msg("Installed manage-tink is not a directory"));
    }

    let installed = skills::read_skill(&target, true)?;
    skills::validate_skill_tree(&target)?;
    if provenance::read(&installed)?.is_some() {
        return Err(Error::msg(
            "Refusing to replace manage-tink with remote provenance",
        ));
    }
    let (_staging, embedded) = prepare_manage_tink()?;
    if !skills::skill_contents_equal(&installed.path, &embedded.path)? {
        library::preflight_deposit_at(home, &embedded, None)?;
        skills::replace_embedded_verified(&embedded, &skills_root)?;
        library::deposit_at(home, &embedded, None)?;

        let refreshed = skills::read_skill(&target, true)?;
        require_current(&refreshed)?;
        return Ok(RefreshOutcome::Refreshed);
    }

    library::preflight_deposit_at(home, &installed, None)?;
    library::sync_from_installed_at(home, &installed)?;
    Ok(RefreshOutcome::Unchanged)
}

/// Stage the embedded skill and install it into the project via `add`.
///
/// Uses the quiet add path so init can own the closing narrative.
/// Returns the install outcome (name + whether the project tree was created).
pub(crate) fn install_manage_tink_at(
    home: Option<&Path>,
    project_root: &Path,
) -> Result<add::AddOutcome, Error> {
    let (_staging, skill) = prepare_manage_tink()?;
    add::add_skill_quiet_at(
        home,
        project_root,
        skill
            .path
            .to_str()
            .ok_or_else(|| Error::msg("manage-tink path is not UTF-8"))?,
        None,
    )
}
