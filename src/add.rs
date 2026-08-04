//! `tink skill add` — install one skill into `.agents/skills/`.

use std::path::{Path, PathBuf};

use crate::archive::{self, ArchiveWrite};
use crate::catalog;
use crate::error::Error;
use crate::git;
use crate::init;
use crate::provenance::Provenance;
use crate::skills::{self, Skill};
use crate::sources::{self, RemoteSource};
use crate::style::CliStyle;

#[derive(Debug)]
pub(crate) struct AddOutcome {
    pub name: String,
    created: bool,
    project_path: PathBuf,
    from_home_archive: bool,
}

fn place_skill(
    project_root: &Path,
    skill: &Skill,
    destination_root: &Path,
    provenance: Option<&Provenance>,
) -> Result<AddOutcome, Error> {
    // Protect the project tree first. Home archive is a rebuildable cache:
    // repair on diverge, then install project (re-add recovers if that fails).
    skills::preflight_install(skill, destination_root, provenance)?
        .require_compatible(&skill.name, destination_root)?;
    let (_, archive_write) = archive::deposit_archive(skill, provenance)?;
    if archive_write == ArchiveWrite::Repaired {
        let err = CliStyle::auto_stderr();
        eprintln!(
            "{}",
            err.warn(format!(
                "Repaired divergent home archive for {}",
                skill.name
            ))
        );
    }
    let (installed, created) = skills::install_local(skill, destination_root, provenance)?;
    // Catalog even on identical noop so the name index can catch up.
    catalog::deposit_skill(project_root, &skill.name)?;
    Ok(AddOutcome {
        name: skill.name.clone(),
        created,
        project_path: installed,
        from_home_archive: false,
    })
}

/// Install into the project from an already-complete home archive tree (receipt included).
fn place_from_home_archive(
    project_root: &Path,
    archived: &Skill,
    destination_root: &Path,
) -> Result<AddOutcome, Error> {
    skills::preflight_install(archived, destination_root, None)?
        .require_compatible(&archived.name, destination_root)?;
    let (installed, created) = skills::install_local(archived, destination_root, None)?;
    catalog::deposit_skill(project_root, &archived.name)?;
    Ok(AddOutcome {
        name: archived.name.clone(),
        created,
        project_path: installed,
        from_home_archive: true,
    })
}

fn select_one_skill(
    source_root: &Path,
    source_display: &str,
    selected_name: Option<&str>,
) -> Result<Skill, Error> {
    let mut candidates = skills::discover(source_root)?;
    if let Some(name) = selected_name {
        candidates.retain(|skill| skill.name == name);
        if candidates.is_empty() {
            return Err(Error::msg(format!("Skill not found: {name}")));
        }
    }
    if candidates.len() != 1 {
        let commands = candidates
            .iter()
            .map(|skill| format!("  tink skill add {source_display} --skill {}", skill.name))
            .collect::<Vec<_>>()
            .join("\n");
        return Err(Error::msg(format!(
            "Source contains multiple skills. Choose one:\n{commands}"
        )));
    }
    Ok(candidates.remove(0))
}

fn install_from_checkout(
    project_root: &Path,
    source_root: &Path,
    source_display: &str,
    destination_root: &Path,
    selected_name: Option<&str>,
    source_url: Option<&str>,
    revision: Option<&str>,
) -> Result<AddOutcome, Error> {
    let source_root = source_root
        .canonicalize()
        .map_err(|e| crate::paths::map_io(source_root, e))?;
    let skill = select_one_skill(&source_root, source_display, selected_name)?;
    let provenance = match (source_url, revision) {
        (Some(url), Some(rev)) => {
            let rel = skill
                .path
                .strip_prefix(&source_root)
                .map_err(|_| {
                    Error::msg(format!(
                        "skill path {} is outside source {}",
                        skill.path.display(),
                        source_root.display()
                    ))
                })?
                .to_string_lossy()
                .replace('\\', "/");
            // Repo-root skills strip to ""; refresh already treats "." as root.
            let rel = if rel.is_empty() {
                ".".to_string()
            } else {
                rel
            };
            let mut map = Provenance::new();
            map.insert("source".into(), url.to_string());
            map.insert("revision".into(), rev.to_string());
            map.insert("path".into(), rel);
            Some(map)
        }
        _ => None,
    };
    if let Some(cached) = archive::matching_archive(&skill, provenance.as_ref())? {
        return place_from_home_archive(project_root, &cached, destination_root);
    }
    place_skill(project_root, &skill, destination_root, provenance.as_ref())
}

pub fn add_skill(
    project_root: &Path,
    source_value: &str,
    selected_name: Option<&str>,
) -> Result<AddOutcome, Error> {
    init::ensure_project_layout(project_root)?;
    let destination_root = project_root.join(".agents").join("skills");
    let local_source = Path::new(source_value);
    if local_source.exists() {
        let source_root = local_source
            .canonicalize()
            .map_err(|e| crate::paths::map_io(local_source, e))?;
        let outcome = install_from_checkout(
            project_root,
            &source_root,
            &source_root.display().to_string(),
            &destination_root,
            selected_name,
            None,
            None,
        )?;
        report_add(&outcome);
        return Ok(outcome);
    }
    if looks_like_filesystem_path(source_value) {
        return Err(Error::msg(format!("Path does not exist: {source_value}")));
    }
    let remote = sources::parse_remote(source_value)?;
    let outcome = add_from_remote(project_root, &destination_root, &remote, selected_name)?;
    report_add(&outcome);
    Ok(outcome)
}

fn looks_like_filesystem_path(value: &str) -> bool {
    let path = Path::new(value);
    path.is_absolute()
        || value.starts_with("./")
        || value.starts_with("../")
        || value.starts_with("~/")
        || value.contains('\\')
}

fn report_add(outcome: &AddOutcome) {
    let style = CliStyle::auto_stdout();
    if outcome.from_home_archive && outcome.created {
        println!(
            "{} {} → {} {}",
            style.success("Installed"),
            style.accent(&outcome.name),
            style.accent(outcome.project_path.display()),
            style.muted("(from home archive)")
        );
    } else if outcome.created {
        println!(
            "{} {} → {}",
            style.success("Installed"),
            style.accent(&outcome.name),
            style.accent(outcome.project_path.display())
        );
    } else {
        println!(
            "{} {}",
            style.muted("Unchanged"),
            style.accent(&outcome.name)
        );
    }
}

fn add_from_remote(
    project_root: &Path,
    destination_root: &Path,
    remote: &RemoteSource,
    selected_name: Option<&str>,
) -> Result<AddOutcome, Error> {
    // Cheap tip check: if home already has this exact remote revision, copy it.
    let tip = git::remote_head(remote)?;
    if let Some(cached) =
        archive::archive_for_remote_tip(&remote.url, &tip, selected_name)?
    {
        return place_from_home_archive(project_root, &cached, destination_root);
    }
    let (_temp, source_root, revision) = git::checkout(remote)?;
    install_from_checkout(
        project_root,
        &source_root,
        &remote.display,
        destination_root,
        selected_name,
        Some(&remote.url),
        Some(&revision),
    )
}
