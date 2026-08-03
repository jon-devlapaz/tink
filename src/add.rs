//! `tink skill add` — install one skill into `.agents/skills/`.

use std::path::{Path, PathBuf};

use crate::error::Error;
use crate::git;
use crate::init;
use crate::inventory;
use crate::skills::{self, Provenance, Skill};
use crate::sources::{self, RemoteSource};
use crate::style::CliStyle;

#[derive(Debug)]
#[allow(dead_code)] // Returned for callers / future CLI reporting.
pub struct AddOutcome {
    pub name: String,
    pub created: bool,
    pub project_path: PathBuf,
}

fn place_skill(
    project_root: &Path,
    skill: &Skill,
    destination_root: &Path,
    provenance: Option<&Provenance>,
) -> Result<AddOutcome, Error> {
    // Preflight both, then archive before project so a project failure leaves a
    // recoverable home archive (re-add completes the project install).
    inventory::preflight_archive(skill, provenance)?;
    skills::preflight_install(skill, destination_root, provenance)?;
    inventory::deposit_archive(skill, provenance)?;
    let (installed, created) = skills::install_local(skill, destination_root, provenance)?;
    // Catalog even on identical noop so the name index can catch up.
    inventory::deposit_skill(project_root, &skill.name)?;
    Ok(AddOutcome {
        name: skill.name.clone(),
        created,
        project_path: installed,
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
    place_skill(project_root, &skill, destination_root, provenance.as_ref())
}

pub fn add_skill(
    project_root: &Path,
    source_value: &str,
    selected_name: Option<&str>,
) -> Result<AddOutcome, Error> {
    init::ensure_project_skills(project_root)?;
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
    if outcome.created {
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
