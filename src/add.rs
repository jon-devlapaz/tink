//! `tink skill add` — install one skill into `.agents/skills/`.

use std::path::{Path, PathBuf};

use crate::catalog;
use crate::error::Error;
use crate::git;
use crate::init;
use crate::library::{self, LibraryWrite};
use crate::provenance::Provenance;
use crate::skills::{self, Skill};
use crate::sources::{self, RemoteSource};
use crate::style::CliStyle;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AddOrigin {
    /// Installed from a local path or fresh remote checkout.
    Source,
    /// Installed from the library (`skills/<name>/`), including tip reuse.
    Library,
}

#[derive(Debug)]
pub(crate) struct AddOutcome {
    pub name: String,
    pub created: bool,
    project_path: PathBuf,
    origin: AddOrigin,
}

fn place_skill(
    project_root: &Path,
    skill: &Skill,
    destination_root: &Path,
    provenance: Option<&Provenance>,
) -> Result<AddOutcome, Error> {
    // Protect the project tree first. Library is a rebuildable collection: repair on
    // diverge, then install project (re-add recovers if that fails).
    skills::preflight_install(skill, destination_root, provenance)?
        .require_compatible(&skill.name, destination_root)?;
    let (_, write) = library::deposit(skill, provenance)?;
    if write == LibraryWrite::Repaired {
        let err = CliStyle::auto_stderr();
        eprintln!(
            "{}",
            err.warn(format!("Updated home copy of {}", skill.name))
        );
    }
    let (installed, created) = skills::install_local(skill, destination_root, provenance)?;
    // Catalog even on identical noop so the name index can catch up.
    catalog::deposit_skill(project_root, &skill.name)?;
    Ok(AddOutcome {
        name: skill.name.clone(),
        created,
        project_path: installed,
        origin: AddOrigin::Source,
    })
}

/// Install into the project from an already-complete library tree (receipt included).
fn place_from_library(
    project_root: &Path,
    skill: &Skill,
    destination_root: &Path,
) -> Result<AddOutcome, Error> {
    skills::preflight_install(skill, destination_root, None)?
        .require_compatible(&skill.name, destination_root)?;
    let (installed, created) = skills::install_local(skill, destination_root, None)?;
    catalog::deposit_skill(project_root, &skill.name)?;
    Ok(AddOutcome {
        name: skill.name.clone(),
        created,
        project_path: installed,
        origin: AddOrigin::Library,
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
    locked_path: Option<&str>,
) -> Result<AddOutcome, Error> {
    let source_root = source_root
        .canonicalize()
        .map_err(|e| crate::paths::map_io(source_root, e))?;
    let skill = if let Some(locked_path) = locked_path {
        if locked_path == "."
            || locked_path.is_empty()
            || locked_path.contains("..")
            || locked_path.contains('\\')
            || locked_path.starts_with('/')
        {
            return Err(Error::msg(format!(
                "Invalid locked skill path: {locked_path}"
            )));
        }
        let path = source_root.join(locked_path);
        let skill = skills::read_skill(&path, true)?;
        if selected_name != Some(skill.name.as_str()) {
            return Err(Error::msg(format!(
                "Locked skill path does not contain {selected_name:?}"
            )));
        }
        skill
    } else {
        select_one_skill(&source_root, source_display, selected_name)?
    };
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
            let rel = if rel.is_empty() { ".".to_string() } else { rel };
            let mut map = Provenance::new();
            map.insert("source".into(), url.to_string());
            map.insert("revision".into(), rev.to_string());
            map.insert("path".into(), rel);
            Some(map)
        }
        _ => None,
    };
    if let Some(cached) = library::matching(&skill, provenance.as_ref())? {
        return place_from_library(project_root, &cached, destination_root);
    }
    place_skill(project_root, &skill, destination_root, provenance.as_ref())
}

pub(crate) fn add_locked_skill(
    project_root: &Path,
    name: &str,
    source: &str,
    revision: Option<&str>,
) -> Result<AddOutcome, Error> {
    if revision.is_none() {
        return add_skill(project_root, source, Some(name));
    }
    let remote = sources::parse_remote(source)?;
    let (_clone, repository, tip) = git::checkout(&remote)?;
    let (_old_checkout, source_root) = if tip == revision.unwrap() {
        (None, repository)
    } else {
        let (temp, checkout) = git::checkout_revision(&repository, revision.unwrap())?;
        (Some(temp), checkout)
    };
    install_from_checkout(
        project_root,
        &source_root,
        &remote.display,
        &crate::home::project_skills_path(project_root),
        Some(name),
        Some(&remote.url),
        Some(revision.unwrap()),
        Some("."),
    )
}

pub fn add_skill(
    project_root: &Path,
    source_value: &str,
    selected_name: Option<&str>,
) -> Result<AddOutcome, Error> {
    add_skill_inner(project_root, source_value, selected_name, true)
}

/// Like [`add_skill`] but skips per-skill stdout (caller owns the narrative).
pub(crate) fn add_skill_quiet(
    project_root: &Path,
    source_value: &str,
    selected_name: Option<&str>,
) -> Result<AddOutcome, Error> {
    add_skill_inner(project_root, source_value, selected_name, false)
}

fn add_skill_inner(
    project_root: &Path,
    source_value: &str,
    selected_name: Option<&str>,
    report: bool,
) -> Result<AddOutcome, Error> {
    init::ensure_project_layout(project_root)?;
    let destination_root = crate::home::project_skills_path(project_root);
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
            None,
        )?;
        if report {
            report_add(&outcome);
        }
        return Ok(outcome);
    }
    if looks_like_filesystem_path(source_value) {
        return Err(Error::msg(format!("Path does not exist: {source_value}")));
    }
    // Slash or URL → remote only. Bare name → library promote.
    if looks_like_remote_source(source_value) {
        let remote = sources::parse_remote(source_value)?;
        let outcome = add_from_remote(project_root, &destination_root, &remote, selected_name)?;
        if report {
            report_add(&outcome);
        }
        return Ok(outcome);
    }
    if selected_name.is_some() {
        return Err(Error::msg(
            "Do not combine --skill with a library skill name; pass only the library name",
        ));
    }
    let skill = library::load(source_value)?;
    let outcome = place_from_library(project_root, &skill, &destination_root)?;
    if report {
        report_add(&outcome);
    }
    Ok(outcome)
}

fn looks_like_remote_source(value: &str) -> bool {
    value.contains('/') || value.contains("://")
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
    match (outcome.created, outcome.origin) {
        (true, AddOrigin::Library) => println!(
            "{} {} → {} {}",
            style.success("Installed"),
            style.skill(&outcome.name),
            style.accent(outcome.project_path.display()),
            style.muted("(from library)")
        ),
        (true, AddOrigin::Source) => println!(
            "{} {} → {}",
            style.success("Installed"),
            style.skill(&outcome.name),
            style.accent(outcome.project_path.display())
        ),
        (false, _) => println!(
            "{} {}",
            style.muted("Unchanged"),
            style.skill(&outcome.name)
        ),
    }
}

fn add_from_remote(
    project_root: &Path,
    destination_root: &Path,
    remote: &RemoteSource,
    selected_name: Option<&str>,
) -> Result<AddOutcome, Error> {
    // Cheap tip check: if library already has this exact remote revision, copy it.
    let tip = git::remote_head(remote)?;
    if let Some(cached) = library::for_remote_tip(&remote.url, &tip, selected_name)? {
        return place_from_library(project_root, &cached, destination_root);
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
        None,
    )
}
