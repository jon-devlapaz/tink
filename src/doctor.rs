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
    let rows = vec![
        probe_git(root),
        probe_home(home),
        probe_skills(root),
        probe_manifest(root),
        probe_network(root),
    ];
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
            detail: root.display().to_string(),
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
            detail: error.to_string(),
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

    fn outcomes(rows: &[ProbeRow]) -> Vec<(&'static str, ProbeOutcome)> {
        rows.iter().map(|row| (row.name, row.outcome)).collect()
    }

    #[test]
    fn healthy_project_reports_no_failures() {
        let temp = tempfile::tempdir().unwrap();
        let home = temp.path().join("home");
        let project = temp.path().join("app");
        crate::init::init_project_at(
            Some(&home),
            &project,
            crate::init::InitOptions {
                with_tink_skills: Some(false),
                with_manage_tink: Some(false),
            },
        )
        .unwrap();
        write_skill(&project, "alpha");

        let rows = doctor_at(Some(&home), &project).unwrap();
        assert!(healthy(&rows), "{rows:?}");
        // Local-only project: network and manifest probes skip, nothing fails.
        assert!(outcomes(&rows).contains(&("network", ProbeOutcome::Skip)));
        assert!(outcomes(&rows).contains(&("manifest", ProbeOutcome::Skip)));
    }

    #[test]
    fn invalid_tree_names_the_skills_probe() {
        let temp = tempfile::tempdir().unwrap();
        let home = temp.path().join("home");
        let project = temp.path().join("app");
        crate::init::init_project_at(
            Some(&home),
            &project,
            crate::init::InitOptions {
                with_tink_skills: Some(false),
                with_manage_tink: Some(false),
            },
        )
        .unwrap();
        write_skill(&project, "alpha");
        fs::write(
            project.join(".agents/skills/alpha/SKILL.md"),
            "no frontmatter here\n",
        )
        .unwrap();

        let rows = doctor_at(Some(&home), &project).unwrap();
        assert!(!healthy(&rows));
        let skills = rows.iter().find(|row| row.name == "skills").unwrap();
        assert_eq!(skills.outcome, ProbeOutcome::Fail);
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

    #[test]
    fn unreachable_remote_fails_fast() {
        // A bare word is treated as a local path, so git fails without
        // touching the network: instant on any machine, and — critically —
        // without holding the shared subprocess supervision lock, which
        // would starve the timing-sensitive tests in this binary.
        // Timeout bounding itself is covered by `update::tests`.
        let started = std::time::Instant::now();
        let result = ls_remote_head("tink-doctor-probe-invalid-remote");
        assert!(result.is_err());
        assert!(
            started.elapsed() < std::time::Duration::from_secs(30),
            "network probe must not hang"
        );
    }
}
