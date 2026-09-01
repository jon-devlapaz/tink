//! Embedded `manage-tink` skill shipped with the Tink binary.

use std::path::Path;

use crate::add;
use crate::catalog;
use crate::check;
use crate::error::Error;
use crate::library;
use crate::paths::{map_io, refuse_symlink};
use crate::provenance;
use crate::skills::{self, Skill};

const SKILL_MD: &str = include_str!("../skills/manage-tink/SKILL.md");
const OPENAI_YAML: &str = include_str!("../skills/manage-tink/agents/openai.yaml");
const COMMANDS_MD: &str = include_str!("../skills/manage-tink/references/commands.md");

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RefreshOutcome {
    Installed,
    Unchanged,
    Refreshed,
}

/// Materialize the embedded tree for read-only validation or later publication.
/// The returned guard owns the bytes referenced by `Skill`.
pub(crate) fn prepare_manage_tink() -> Result<(tempfile::TempDir, Skill), Error> {
    let staging = tempfile::Builder::new()
        .prefix(".tink-manage-tink-")
        .tempdir()
        .map_err(|e| Error::msg(format!("manage-tink staging: {e}")))?;
    let skill_root = staging.path().join("manage-tink");
    let agents = skill_root.join("agents");
    let references = skill_root.join("references");
    std::fs::create_dir_all(&agents).map_err(|e| map_io(&agents, e))?;
    std::fs::create_dir_all(&references).map_err(|e| map_io(&references, e))?;
    std::fs::write(skill_root.join("SKILL.md"), SKILL_MD)
        .map_err(|e| map_io(&skill_root.join("SKILL.md"), e))?;
    std::fs::write(agents.join("openai.yaml"), OPENAI_YAML)
        .map_err(|e| map_io(&agents.join("openai.yaml"), e))?;
    std::fs::write(references.join("commands.md"), COMMANDS_MD)
        .map_err(|e| map_io(&references.join("commands.md"), e))?;
    let skill = skills::read_skill(&skill_root, true)?;
    Ok((staging, skill))
}

pub(crate) fn is_current(installed: &Skill) -> Result<bool, Error> {
    let (_staging, embedded) = prepare_manage_tink()?;
    skills::skill_contents_equal(&installed.path, &embedded.path)
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
    check::check_zen_coupling(project_root)?;
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
        catalog::preflight_deposit_skill_at(home, project_root)?;
        skills::replace_embedded_verified(&embedded, &skills_root)?;
        library::deposit_at(home, &embedded, None)?;
        catalog::deposit_skill_at(home, project_root, "manage-tink")?;

        let refreshed = skills::read_skill(&target, true)?;
        require_current(&refreshed)?;
        return Ok(RefreshOutcome::Refreshed);
    }

    library::preflight_deposit_at(home, &installed, None)?;
    catalog::preflight_deposit_skill_at(home, project_root)?;
    library::sync_from_installed_at(home, &installed)?;
    catalog::deposit_skill_at(home, project_root, "manage-tink")?;
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use tempfile::TempDir;

    struct TempHome {
        home: PathBuf,
        root: PathBuf,
        _temp: TempDir,
    }

    impl TempHome {
        fn new() -> Self {
            let temp = TempDir::new().unwrap();
            let root = temp.path().to_path_buf();
            let home = root.join("tink-home");
            Self {
                home,
                root,
                _temp: temp,
            }
        }
    }

    #[test]
    fn refresh_manage_tink_at_deposits_into_given_home() {
        let home = TempHome::new();
        let project = home.root.join("project");
        std::fs::create_dir_all(&project).unwrap();

        crate::init::init_project_at(
            Some(&home.home),
            &project,
            crate::init::InitOptions {
                with_zen: Some(false),
                with_tink_skills: Some(false),
                with_manage_tink: Some(false),
            },
        )
        .unwrap();

        let outcome = refresh_manage_tink_at(Some(&home.home), &project).unwrap();
        assert_eq!(outcome, RefreshOutcome::Installed);
        assert!(
            crate::home::project_skills_path(&project)
                .join("manage-tink/SKILL.md")
                .is_file()
        );
        assert!(
            crate::home::skills_library_path(&home.home)
                .join("manage-tink/SKILL.md")
                .is_file()
        );
        let catalog = crate::catalog::list_catalog(Some(&home.home)).unwrap();
        assert!(
            catalog.iter().any(|entry| entry.skill == "manage-tink"),
            "catalog skills: {:?}",
            catalog
                .iter()
                .map(|entry| entry.skill.as_str())
                .collect::<Vec<_>>(),
        );
    }
}
