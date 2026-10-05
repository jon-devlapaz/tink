//! Read-only environment and consistency diagnostics (`tink doctor`).
//!
//! Every probe reuses the same functions as the commands it diagnoses.
//! Probes never write; a failing probe reports its row, never the command.

use std::collections::BTreeSet;
use std::path::Path;
use std::process::Command;
use std::time::Duration;

use crate::check;
use crate::error::Error;
use crate::git;
use crate::home;
use crate::manifest;
use crate::provenance;

const NETWORK_TIMEOUT: Duration = Duration::from_secs(15);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProbeOutcome {
    Pass,
    /// Reported loudly but does not fail `doctor`.
    Warn,
    Fail,
    Skip,
}

#[derive(Debug, Clone)]
pub struct ProbeRow {
    pub name: &'static str,
    pub outcome: ProbeOutcome,
    pub detail: String,
}

pub fn doctor(root: &Path) -> Result<Vec<ProbeRow>, Error> {
    doctor_at(None, root)
}

pub(crate) fn doctor_at(home: Option<&Path>, root: &Path) -> Result<Vec<ProbeRow>, Error> {
    let mut rows = vec![
        probe_git(root),
        probe_home(home),
        probe_skills(root),
        probe_manifest(root),
        probe_library(home),
    ];
    rows.extend(probe_pins(home, root));
    rows.push(probe_network(root));
    Ok(rows)
}

pub fn healthy(rows: &[ProbeRow]) -> bool {
    rows.iter().all(|row| row.outcome != ProbeOutcome::Fail)
}

fn probe_git(root: &Path) -> ProbeRow {
    match git::git_stdout(root, &["--version"]) {
        Ok(version) => ProbeRow {
            name: "git",
            outcome: ProbeOutcome::Pass,
            detail: version.lines().next().unwrap_or("git").to_string(),
        },
        Err(error) => ProbeRow {
            name: "git",
            outcome: ProbeOutcome::Fail,
            detail: error.to_string(),
        },
    }
}

fn probe_home(home: Option<&Path>) -> ProbeRow {
    match home::existing_inventory_root(home) {
        Ok(Some(root)) => ProbeRow {
            name: "home",
            outcome: ProbeOutcome::Pass,
            detail: crate::output::display_path(&root),
        },
        Ok(None) => ProbeRow {
            name: "home",
            outcome: ProbeOutcome::Skip,
            detail: "no home yet; created on demand".to_string(),
        },
        Err(error) => ProbeRow {
            name: "home",
            outcome: ProbeOutcome::Fail,
            detail: error.to_string(),
        },
    }
}

fn probe_skills(root: &Path) -> ProbeRow {
    match check::load_project_skills(root) {
        Ok(skills) => ProbeRow {
            name: "skills",
            outcome: ProbeOutcome::Pass,
            detail: format!("{} valid project skill(s)", skills.len()),
        },
        Err(error) => ProbeRow {
            name: "skills",
            outcome: ProbeOutcome::Fail,
            detail: {
                let message = error.to_string();
                if message == "Missing .agents/skills" {
                    format!("{message}; run `tink init`")
                } else {
                    message
                }
            },
        },
    }
}

fn probe_manifest(root: &Path) -> ProbeRow {
    if !root
        .join(manifest::DIRECTORY)
        .join(manifest::MANIFEST_FILE)
        .exists()
    {
        return ProbeRow {
            name: "manifest",
            outcome: ProbeOutcome::Skip,
            detail: "no manifest; run `tink skill lock` to create one".to_string(),
        };
    }
    match manifest::verify(root) {
        Ok(count) => ProbeRow {
            name: "manifest",
            outcome: ProbeOutcome::Pass,
            detail: format!("{count} manifest skill(s) verified"),
        },
        Err(error) => ProbeRow {
            name: "manifest",
            outcome: ProbeOutcome::Fail,
            detail: error.to_string(),
        },
    }
}

/// Library trust: symlinked entries (mount refuses them) and skills whose
/// current tree digest is not the approved one (payload delivery refuses them).
fn probe_library(home: Option<&Path>) -> ProbeRow {
    let row = |outcome, detail| ProbeRow {
        name: "library",
        outcome,
        detail,
    };
    let entries = match crate::library::scan_entries(home) {
        Ok(entries) => entries,
        Err(error) => return row(ProbeOutcome::Fail, error.to_string()),
    };
    if entries.standalone.is_empty() && entries.symlinked.is_empty() {
        return row(ProbeOutcome::Skip, "no library skills".to_string());
    }
    let approved = match home::existing_inventory_root(home) {
        Ok(Some(root)) => crate::approvals::load(&root),
        Ok(None) => Ok(Default::default()),
        Err(error) => Err(error),
    };
    let approved = match approved {
        Ok(approved) => approved,
        Err(error) => return row(ProbeOutcome::Fail, error.to_string()),
    };
    let library = match home::existing_inventory_root(home) {
        Ok(Some(root)) => home::skills_library_path(&root),
        _ => return row(ProbeOutcome::Skip, "no library".to_string()),
    };
    let mut unapproved = 0;
    let mut refused = Vec::new();
    for name in &entries.standalone {
        match crate::mount::verify_library_skill(&library, name) {
            Ok(skill) if approved.get(name) == Some(&skill.tree_digest()) => {}
            Ok(_) => unapproved += 1,
            Err(refusal) => refused.push(format!("{name} ({})", refusal.code)),
        }
    }
    let total = entries.standalone.len() + entries.symlinked.len();
    let mut detail = format!("{total} library skill(s); {unapproved} unapproved");
    if !entries.symlinked.is_empty() {
        detail.push_str(&format!("; symlinked: {}", entries.symlinked.join(", ")));
    }
    if !refused.is_empty() {
        detail.push_str(&format!("; refused: {}", refused.join(", ")));
    }
    if unapproved > 0 {
        detail.push_str("; review, then run `tink library approve <name>` (or --all)");
    }
    let clean = unapproved == 0 && entries.symlinked.is_empty() && refused.is_empty();
    row(
        if clean {
            ProbeOutcome::Pass
        } else {
            ProbeOutcome::Warn
        },
        detail,
    )
}

/// Committed project pins (`.tink/skillsets/*.json`): how many members are not yet
/// in the library. Warns only; an unreadable pin counts as no members. Absent when
/// the project has no pins.
fn probe_pins(home: Option<&Path>, root: &Path) -> Option<ProbeRow> {
    let files = crate::skillsets::project_pin_files(root);
    if files.is_empty() {
        return None;
    }
    let library = match home::existing_inventory_root(home) {
        Ok(Some(home_root)) => Some(home::skills_library_path(&home_root)),
        _ => None,
    };
    let mut missing = BTreeSet::new();
    let mut first_missing_pin: Option<String> = None;
    for file in &files {
        let Ok(meta) = crate::skillsets::read_pin_file(file) else {
            continue;
        };
        for member in &meta.members {
            let present = library
                .as_ref()
                .is_some_and(|dir| dir.join(member).is_dir());
            if !present {
                missing.insert(member.clone());
                first_missing_pin.get_or_insert_with(|| {
                    file.file_name()
                        .map(|name| name.to_string_lossy().into_owned())
                        .unwrap_or_default()
                });
            }
        }
    }
    let mut detail = format!(
        "{} project pin(s); {} member(s) not in the library",
        files.len(),
        missing.len()
    );
    let outcome = match first_missing_pin {
        Some(name) => {
            detail.push_str(&format!(
                "; run `tink library fetch .tink/skillsets/{name}`"
            ));
            ProbeOutcome::Warn
        }
        None => ProbeOutcome::Pass,
    };
    Some(ProbeRow {
        name: "pins",
        outcome,
        detail,
    })
}

fn first_remote_source(root: &Path) -> Option<String> {
    let skills = check::load_project_skills(root).ok()?;
    let mut seen = BTreeSet::new();
    for skill in &skills {
        let provenance = provenance::read(skill).ok()??;
        if let Some(source) = provenance.get("source")
            && source.starts_with("https://")
            && seen.insert(source.clone())
        {
            return Some(source.clone());
        }
    }
    None
}

fn probe_network(root: &Path) -> ProbeRow {
    let Some(source) = first_remote_source(root) else {
        return ProbeRow {
            name: "network",
            outcome: ProbeOutcome::Skip,
            detail: "no remote sources installed".to_string(),
        };
    };
    match ls_remote_head(&source) {
        Ok(_) => ProbeRow {
            name: "network",
            outcome: ProbeOutcome::Pass,
            detail: "remote reachable".to_string(),
        },
        Err(error) => ProbeRow {
            name: "network",
            outcome: ProbeOutcome::Fail,
            detail: error.to_string(),
        },
    }
}

/// Bounded reachability check with doctor's own timeout (shorter than the
/// shared Git transport budget so doctor stays snappy).
fn ls_remote_head(url: &str) -> Result<(), Error> {
    let mut command = Command::new("git");
    command
        .args(["ls-remote", "--quiet", url, "HEAD"])
        .env("GIT_TERMINAL_PROMPT", "0");
    let output = crate::process::run_bounded(
        &mut command,
        NETWORK_TIMEOUT,
        "git ls-remote",
        Some("Git is required to reach remote skill sources"),
    )?;
    if output.status.success() {
        return Ok(());
    }
    let detail = String::from_utf8_lossy(&output.stderr);
    Err(Error::msg(format!(
        "remote unreachable: {}",
        detail.lines().last().unwrap_or(url).trim()
    )))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn write_skill(root: &Path, name: &str) {
        let dir = root.join(".agents/skills").join(name);
        fs::create_dir_all(&dir).unwrap();
        fs::write(
            dir.join("SKILL.md"),
            format!("---\nname: {name}\ndescription: Fixture.\n---\n"),
        )
        .unwrap();
    }

    #[test]
    fn broken_home_marker_fails_only_the_home_probe() {
        let temp = tempfile::tempdir().unwrap();
        let home = temp.path().join("home");
        let project = temp.path().join("app");
        crate::init::init_project_at(
            Some(&home),
            &project,
            crate::init::InitOptions {
                with_tink_skills: Some(false),
                with_manage_tink: Some(false),
                ..Default::default()
            },
        )
        .unwrap();
        write_skill(&project, "alpha");
        fs::write(home.join("layout.json"), "{not-json}\n").unwrap();

        let rows = doctor_at(Some(&home), &project).unwrap();
        assert!(!healthy(&rows));
        let home_row = rows.iter().find(|row| row.name == "home").unwrap();
        assert_eq!(home_row.outcome, ProbeOutcome::Fail);
        let skills = rows.iter().find(|row| row.name == "skills").unwrap();
        assert_eq!(skills.outcome, ProbeOutcome::Pass);
    }
}
