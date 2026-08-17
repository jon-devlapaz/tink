//! `tink skill add` — install one skill into `.agents/skills/`.

use std::path::{Path, PathBuf};

use crate::catalog;
use crate::error::Error;
use crate::git;
use crate::init;
use crate::library::{self, LibraryWrite};
use crate::output;
use crate::provenance::Provenance;
use crate::skills::{self, Skill};
use crate::sources::{self, AddSource, LockedSource, RemoteSource};
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

/// A locked source resolved exactly once. Checkout/staging guards keep the
/// selected bytes alive through cross-skill validation and quiet publication.
pub(crate) struct PreparedLockedSkill {
    _guards: Vec<tempfile::TempDir>,
    skill: Skill,
    provenance: Option<Provenance>,
}

impl PreparedLockedSkill {
    pub(crate) fn skill(&self) -> &Skill {
        &self.skill
    }

    pub(crate) fn provenance(&self) -> Option<&Provenance> {
        self.provenance.as_ref()
    }

    pub(crate) fn publish(self, project_root: &Path) -> Result<AddOutcome, Error> {
        self.publish_at(project_root, None)
    }

    pub(crate) fn publish_at(
        self,
        project_root: &Path,
        home: Option<&Path>,
    ) -> Result<AddOutcome, Error> {
        let destination_root = crate::home::project_skills_path(project_root);
        place_skill_inner(
            project_root,
            home,
            &self.skill,
            &destination_root,
            self.provenance.as_ref(),
            false,
        )
    }
}

fn place_skill(
    project_root: &Path,
    skill: &Skill,
    destination_root: &Path,
    provenance: Option<&Provenance>,
) -> Result<AddOutcome, Error> {
    place_skill_inner(
        project_root,
        None,
        skill,
        destination_root,
        provenance,
        true,
    )
}

fn place_skill_inner(
    project_root: &Path,
    home: Option<&Path>,
    skill: &Skill,
    destination_root: &Path,
    provenance: Option<&Provenance>,
    report_library_repair: bool,
) -> Result<AddOutcome, Error> {
    crate::skillsets::ensure_standalone_source(&skill.path, &skill.name)?;
    init::ensure_project_layout(project_root)?;
    // Protect the project tree first. Library is a rebuildable collection: repair on
    // diverge, then install project (re-add recovers if that fails).
    skills::preflight_install(skill, destination_root, provenance)?
        .require_compatible(&skill.name, destination_root)?;
    let (_, write) = library::deposit_at(home, skill, provenance)?;
    let (installed, created) = skills::install_local(skill, destination_root, provenance)?;
    // Catalog even on identical noop so the name index can catch up.
    catalog::deposit_skill_at(home, project_root, &skill.name)?;
    if write == LibraryWrite::Repaired && report_library_repair {
        let err = CliStyle::auto_stderr();
        output::warning_line(format_args!(
            "{}",
            err.warn(format!("Updated home copy of {}", skill.name))
        ));
    }
    Ok(AddOutcome {
        name: skill.name.clone(),
        created,
        project_path: installed,
        origin: AddOrigin::Source,
    })
}

fn select_locked_skill(
    source_root: &Path,
    name: &str,
    source_path: Option<&str>,
) -> Result<Skill, Error> {
    let skill = if let Some(source_path) = source_path {
        if source_path.is_empty()
            || source_path.contains("..")
            || source_path.contains('\\')
            || source_path.starts_with('/')
        {
            return Err(Error::msg(format!(
                "Invalid locked skill path: {source_path}"
            )));
        }
        let path = crate::paths::canonicalize_beneath(source_root, Path::new(source_path))?;
        let skill = skills::read_skill(&path, source_path != ".")?;
        if skill.name != name {
            return Err(Error::msg(format!(
                "Locked skill path does not contain {name:?}"
            )));
        }
        skill
    } else {
        select_one_skill(source_root, &output::display_path(source_root), Some(name))?
    };
    crate::skillsets::ensure_standalone_source(&skill.path, &skill.name)?;
    Ok(skill)
}

pub(crate) fn prepare_locked_skill(
    name: &str,
    source: LockedSource,
) -> Result<PreparedLockedSkill, Error> {
    match source {
        LockedSource::LocalPath {
            declared,
            project_root,
        } => {
            let source_root =
                crate::paths::canonicalize_beneath(&project_root, Path::new(&declared))?;
            let selected = select_locked_skill(&source_root, name, None)?;
            let guard = tempfile::Builder::new()
                .prefix(".tink-locked-local-")
                .tempdir()
                .map_err(|e| Error::msg(format!("local skill snapshot: {e}")))?;
            let snapshot = guard.path().join(name);
            skills::copy_skill_tree(&selected.path, &snapshot, &[".git"])?;
            let skill = skills::read_skill(&snapshot, true)?;
            Ok(PreparedLockedSkill {
                _guards: vec![guard],
                skill,
                provenance: None,
            })
        }
        LockedSource::Github {
            remote,
            revision,
            path,
        } => {
            let (clone_guard, repository, tip) = git::checkout(&remote)?;
            let mut guards = vec![clone_guard];
            let source_root = if tip == revision {
                repository
            } else {
                let (worktree_guard, worktree) = git::checkout_revision(&repository, &revision)?;
                guards.push(worktree_guard);
                worktree
            };
            let skill = select_locked_skill(&source_root, name, Some(&path))?;
            let mut provenance = Provenance::new();
            provenance.insert("source".into(), remote.url);
            provenance.insert("revision".into(), revision);
            provenance.insert("path".into(), path);
            Ok(PreparedLockedSkill {
                _guards: guards,
                skill,
                provenance: Some(provenance),
            })
        }
        LockedSource::EmbeddedManageTink => {
            let (guard, skill) = crate::manage_tink::prepare_manage_tink()?;
            if skill.name != name {
                return Err(Error::msg(format!(
                    "Embedded source does not provide skill: {name}"
                )));
            }
            Ok(PreparedLockedSkill {
                _guards: vec![guard],
                skill,
                provenance: None,
            })
        }
    }
}

/// Install into the project from an already-complete library tree (receipt included).
fn place_from_library(
    project_root: &Path,
    skill: &Skill,
    destination_root: &Path,
) -> Result<AddOutcome, Error> {
    crate::skillsets::ensure_standalone_source(&skill.path, &skill.name)?;
    init::ensure_project_layout(project_root)?;
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

fn repository_relative_path(repository: &Path, skill: &Skill) -> Result<String, Error> {
    let relative = skill
        .path
        .strip_prefix(repository)
        .map_err(|_| {
            Error::msg(format!(
                "skill path {} is outside source {}",
                output::display_path(&skill.path),
                output::display_path(repository)
            ))
        })?
        .to_string_lossy()
        .replace('\\', "/");
    Ok(if relative.is_empty() {
        ".".to_string()
    } else {
        relative
    })
}

fn select_remote_skill_path(
    source_root: &Path,
    source_display: &str,
    selector: &str,
) -> Result<String, Error> {
    let discovery = skills::discover_recursive(source_root)?;
    let mut candidates = discovery
        .skills
        .into_iter()
        .map(|skill| {
            let path = repository_relative_path(source_root, &skill)?;
            Ok((skill, path))
        })
        .collect::<Result<Vec<_>, Error>>()?;

    if selector == "." || selector.contains('/') {
        candidates.retain(|(_, path)| path == selector);
    } else {
        candidates.retain(|(skill, path)| {
            skill.name == selector
                && (path == "."
                    || skill.path.file_name().and_then(|name| name.to_str())
                        == Some(skill.name.as_str()))
        });
    }
    if candidates.is_empty() {
        return Err(Error::msg(format!("Skill not found: {selector}")));
    }
    if candidates.len() != 1 {
        let commands = candidates
            .iter()
            .map(|(_, path)| format!("  tink skill add {source_display} --skill {path}"))
            .collect::<Vec<_>>()
            .join("\n");
        return Err(Error::msg(format!(
            "Skill selector {selector:?} matches multiple skills. Choose one by repository path:\n{commands}"
        )));
    }
    Ok(candidates.remove(0).1)
}

struct CheckoutInstallOptions<'a> {
    source_display: &'a str,
    selected_name: Option<&'a str>,
    source_url: Option<&'a str>,
    revision: Option<&'a str>,
    source_path: Option<&'a str>,
}

fn install_from_checkout(
    project_root: &Path,
    source_root: &Path,
    destination_root: &Path,
    options: CheckoutInstallOptions<'_>,
) -> Result<AddOutcome, Error> {
    let source_root = source_root
        .canonicalize()
        .map_err(|e| crate::paths::map_io(source_root, e))?;
    let skill = if let Some(source_path) = options.source_path {
        if source_path.is_empty()
            || source_path.contains("..")
            || source_path.contains('\\')
            || source_path.starts_with('/')
        {
            return Err(Error::msg(format!(
                "Invalid locked skill path: {source_path}"
            )));
        }
        let path = crate::paths::canonicalize_beneath(&source_root, Path::new(source_path))?;
        let skill = skills::read_skill(&path, source_path != ".")?;
        if let Some(selected_name) = options.selected_name
            && selected_name != skill.name
            && selected_name != source_path
        {
            return Err(Error::msg(format!(
                "Skill selector {selected_name:?} does not match {} at {source_path}",
                skill.name
            )));
        }
        skill
    } else {
        select_one_skill(&source_root, options.source_display, options.selected_name)?
    };
    crate::skillsets::ensure_standalone_source(&skill.path, &skill.name)?;
    let provenance = match (options.source_url, options.revision) {
        (Some(url), Some(rev)) => {
            let rel = skill
                .path
                .strip_prefix(&source_root)
                .map_err(|_| {
                    Error::msg(format!(
                        "skill path {} is outside source {}",
                        output::display_path(&skill.path),
                        output::display_path(&source_root)
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
    let destination_root = crate::home::project_skills_path(project_root);
    match sources::classify_add_input(source_value)? {
        AddSource::LocalPath(local_source) => {
            crate::paths::refuse_symlink(&local_source)?;
            let source_root = local_source
                .canonicalize()
                .map_err(|e| crate::paths::map_io(&local_source, e))?;
            let outcome = install_from_checkout(
                project_root,
                &source_root,
                &destination_root,
                CheckoutInstallOptions {
                    source_display: &output::display_path(&source_root),
                    selected_name,
                    source_url: None,
                    revision: None,
                    source_path: None,
                },
            )?;
            if report {
                report_add(&outcome)?;
            }
            Ok(outcome)
        }
        AddSource::Github(source) => {
            let outcome = add_from_remote(project_root, &destination_root, &source, selected_name)?;
            if report {
                report_add(&outcome)?;
            }
            Ok(outcome)
        }
        AddSource::LibraryName(name) => {
            if selected_name.is_some() {
                return Err(Error::msg(
                    "Do not combine --skill with a library skill name; pass only the library name",
                ));
            }
            let skill = library::load(&name)?;
            let outcome = place_from_library(project_root, &skill, &destination_root)?;
            if report {
                report_add(&outcome)?;
            }
            Ok(outcome)
        }
    }
}

fn report_add(outcome: &AddOutcome) -> Result<(), Error> {
    let style = CliStyle::auto_stdout();
    match (outcome.created, outcome.origin) {
        (true, AddOrigin::Library) => output::stdout_line(format_args!(
            "{} {} → {} {}",
            style.success("Installed"),
            style.skill(&outcome.name),
            style.accent(output::display_path(&outcome.project_path)),
            style.muted("(from library)")
        )),
        (true, AddOrigin::Source) => output::stdout_line(format_args!(
            "{} {} → {}",
            style.success("Installed"),
            style.skill(&outcome.name),
            style.accent(output::display_path(&outcome.project_path))
        )),
        (false, _) => output::stdout_line(format_args!(
            "{} {}",
            style.muted("Unchanged"),
            style.skill(&outcome.name)
        )),
    }
}

fn add_from_remote(
    project_root: &Path,
    destination_root: &Path,
    source: &sources::GithubAddSource,
    selected_name: Option<&str>,
) -> Result<AddOutcome, Error> {
    if selected_name.is_some() && source.skill_path.is_some() {
        return Err(Error::msg(
            "Do not combine --skill with a GitHub tree URL; the URL already selects the skill",
        ));
    }
    if let (Some(tree_ref), Some(skill_path)) =
        (source.tree_ref.as_deref(), source.skill_path.as_deref())
    {
        reject_ambiguous_tree_ref(&source.remote, tree_ref, skill_path)?;
    }
    // A selected name may be ambiguous in the repository, so discovery must run
    // before the library can be trusted. Preserve the root-skill no-clone path.
    if selected_name.is_none() && source.skill_path.is_none() {
        let tip = git::remote_head(&source.remote)?;
        if let Some(cached) = library::for_remote_tip(&source.remote.url, &tip, None)? {
            return place_from_library(project_root, &cached, destination_root);
        }
    }
    let (_temp, source_root, revision) = git::checkout(&source.remote)?;
    let selected_path = if let Some(skill_path) = source.skill_path.as_deref() {
        Some(explicit_remote_skill_path(
            &source_root,
            &source.remote,
            skill_path,
        )?)
    } else {
        selected_name
            .map(|selector| {
                select_remote_skill_path(&source_root, &source.remote.display, selector)
            })
            .transpose()?
    };
    install_from_checkout(
        project_root,
        &source_root,
        destination_root,
        CheckoutInstallOptions {
            source_display: &source.remote.display,
            selected_name,
            source_url: Some(&source.remote.url),
            revision: Some(&revision),
            source_path: selected_path.as_deref(),
        },
    )
}

fn reject_ambiguous_tree_ref(
    remote: &RemoteSource,
    requested_ref: &str,
    relative_path: &str,
) -> Result<(), Error> {
    let remote_refs = git::remote_ref_names(remote)?;
    let mut candidate = requested_ref.to_string();
    for segment in relative_path.split('/') {
        candidate.push('/');
        candidate.push_str(segment);
        if remote_refs.contains(&candidate) {
            return Err(Error::msg(format!(
                "GitHub URL is ambiguous because Git ref `{candidate}` contains `/`; use a ref without `/`"
            )));
        }
    }
    Ok(())
}

fn explicit_remote_skill_path(
    source_root: &Path,
    remote: &RemoteSource,
    url_path: &str,
) -> Result<String, Error> {
    let path = crate::paths::canonicalize_beneath(source_root, Path::new(url_path))
        .map_err(|error| Error::msg(format!("GitHub tree path {url_path} is invalid: {error}")))?;
    if path.join("SKILL.md").is_file() {
        return Ok(url_path.to_string());
    }
    let discovery = skills::discover_recursive(&path)?;
    if discovery.skills.is_empty() {
        return Err(Error::msg(format!(
            "GitHub tree path is not a skill: {url_path}"
        )));
    }
    let commands = discovery
        .skills
        .iter()
        .map(|skill| {
            let relative = repository_relative_path(source_root, skill)?;
            Ok(format!(
                "  tink skill add {} --skill {relative}",
                remote.url
            ))
        })
        .collect::<Result<Vec<_>, Error>>()?
        .join("\n");
    Err(Error::msg(format!(
        "GitHub tree path is not a skill. Choose one:\n{commands}"
    )))
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::*;

    #[test]
    fn prepared_local_skill_owns_snapshot_bytes() {
        let temp = tempfile::tempdir().unwrap();
        let source = temp.path().join("alpha");
        fs::create_dir_all(&source).unwrap();
        let original = "---\nname: alpha\ndescription: Original fixture.\n---\n\n# Alpha\n";
        fs::write(source.join("SKILL.md"), original).unwrap();
        let prepared = prepare_locked_skill(
            "alpha",
            LockedSource::LocalPath {
                declared: "alpha".into(),
                project_root: temp.path().to_path_buf(),
            },
        )
        .unwrap();

        fs::write(
            source.join("SKILL.md"),
            "---\nname: alpha\ndescription: Mutated fixture.\n---\n",
        )
        .unwrap();

        assert_eq!(
            fs::read_to_string(prepared.skill().path.join("SKILL.md")).unwrap(),
            original
        );
    }

    #[test]
    fn prepared_local_skill_refuses_receipt_owned_source() {
        let temp = tempfile::tempdir().unwrap();
        let source = temp.path().join("common-skillset");
        fs::create_dir_all(&source).unwrap();
        fs::write(
            source.join("SKILL.md"),
            "---\nname: common-skillset\ndescription: Receipt fixture.\n---\n",
        )
        .unwrap();
        fs::write(source.join(".tink-skillset.json"), "{}\n").unwrap();

        let error = prepare_locked_skill(
            "common-skillset",
            LockedSource::LocalPath {
                declared: "common-skillset".into(),
                project_root: temp.path().to_path_buf(),
            },
        )
        .err()
        .expect("receipt-owned source must be refused");

        assert!(
            error
                .to_string()
                .contains("tink skillset add common-skillset"),
            "{error}"
        );
    }

    #[cfg(unix)]
    #[test]
    fn prepared_local_skill_refuses_symlinked_ancestor() {
        let temp = tempfile::tempdir().unwrap();
        let project = temp.path().join("project");
        let outside = temp.path().join("outside");
        let source = outside.join("alpha");
        fs::create_dir_all(&project).unwrap();
        fs::create_dir_all(&source).unwrap();
        fs::write(
            source.join("SKILL.md"),
            "---\nname: alpha\ndescription: Outside fixture.\n---\n",
        )
        .unwrap();
        std::os::unix::fs::symlink(&outside, project.join("jump")).unwrap();

        let error = prepare_locked_skill(
            "alpha",
            LockedSource::LocalPath {
                declared: "jump/alpha".into(),
                project_root: project,
            },
        )
        .err()
        .expect("ancestor symlink must be refused");

        assert!(error.to_string().contains("symlink"), "{error}");
    }

    #[cfg(unix)]
    #[test]
    fn locked_remote_skill_refuses_symlinked_ancestor() {
        let temp = tempfile::tempdir().unwrap();
        let checkout = temp.path().join("checkout");
        let outside = temp.path().join("outside");
        let source = outside.join("alpha");
        fs::create_dir_all(&checkout).unwrap();
        fs::create_dir_all(&source).unwrap();
        fs::write(
            source.join("SKILL.md"),
            "---\nname: alpha\ndescription: Outside fixture.\n---\n",
        )
        .unwrap();
        std::os::unix::fs::symlink(&outside, checkout.join("jump")).unwrap();

        let error = select_locked_skill(&checkout, "alpha", Some("jump/alpha"))
            .expect_err("ancestor symlink must be refused");

        assert!(error.to_string().contains("symlink"), "{error}");
    }
}
