//! `tink destroy` — remove project agent scaffolding.

use std::fs;
use std::io::{self, IsTerminal, Write};
use std::path::{Path, PathBuf};

use crate::catalog;
use crate::error::Error;
use crate::paths::{map_io, refuse_symlink};
use crate::style::CliStyle;

#[derive(Debug, Default)]
pub struct DestroyReport {
    pub removed: Vec<PathBuf>,
}

/// Remove this project's `.agents/skills/`, then drop its by-project catalog
/// entry. An empty `.agents/` directory is removed, but unrelated siblings are
/// preserved.
/// `ZEN.md` and `AGENTS.md` are preserved because the current
/// project layout has no durable proof that Tink created either file. Does not
/// touch the home library. Catalog cleanup is preflighted before disk deletes;
/// expected catalog refusals leave project files intact. Refuses symlinks.
/// Requires `--yes` or an
/// interactive `y` confirmation (default no).
pub fn destroy_project(project_root: &Path, yes: bool) -> Result<DestroyReport, Error> {
    destroy_project_at(project_root, yes, None)
}

fn destroy_project_at(
    project_root: &Path,
    yes: bool,
    catalog_home: Option<&Path>,
) -> Result<DestroyReport, Error> {
    if !yes {
        confirm_destroy()?;
    }

    let agents = crate::home::project_agents_path(project_root);
    let skills = crate::home::project_skills_path(project_root);
    if agents.exists() || agents.is_symlink() {
        refuse_symlink(&agents)?;
        if !agents.is_dir() {
            return Err(Error::msg(format!(
                "Refusing to remove non-directory: {}",
                agents.display()
            )));
        }
    }
    if skills.exists() || skills.is_symlink() {
        refuse_symlink(&skills)?;
        if !skills.is_dir() {
            return Err(Error::msg(format!(
                "Refusing to remove non-directory: {}",
                skills.display()
            )));
        }
    }

    match catalog_home {
        Some(home) => catalog::preflight_forget_project_at(Some(home), project_root)?,
        None => catalog::preflight_forget_project(project_root)?,
    }

    let mut removed = Vec::new();
    if skills.is_dir() {
        fs::remove_dir_all(&skills).map_err(|e| map_io(&skills, e))?;
        removed.push(skills);
    }
    if agents.is_dir()
        && fs::read_dir(&agents)
            .map_err(|e| map_io(&agents, e))?
            .next()
            .is_none()
    {
        fs::remove_dir(&agents).map_err(|e| map_io(&agents, e))?;
        removed.push(agents);
    }

    match catalog_home {
        Some(home) => catalog::forget_project_at(Some(home), project_root)?,
        None => catalog::forget_project(project_root)?,
    }

    Ok(DestroyReport { removed })
}

fn confirm_destroy() -> Result<(), Error> {
    if !io::stdin().is_terminal() {
        return Err(Error::msg(
            "Refusing to destroy without confirmation (pass --yes, or run in a terminal)",
        ));
    }
    let style = CliStyle::auto_stdout();
    let mut stdout = io::stdout();
    write!(
        stdout,
        "{} {}",
        style.warn("Delete .agents/skills/ in this project?"),
        style.accent("[y/N]")
    )
    .map_err(|e| Error::msg(format!("prompt: {e}")))?;
    write!(stdout, " ").map_err(|e| Error::msg(format!("prompt: {e}")))?;
    stdout
        .flush()
        .map_err(|e| Error::msg(format!("prompt: {e}")))?;
    let mut line = String::new();
    io::stdin()
        .read_line(&mut line)
        .map_err(|e| Error::msg(format!("prompt: {e}")))?;
    let answer = line.trim();
    if answer.eq_ignore_ascii_case("y") || answer.eq_ignore_ascii_case("yes") {
        return Ok(());
    }
    Err(Error::msg("Destroy cancelled"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn destroy_preserves_preexisting_project_guidance() {
        let temp = tempfile::tempdir().unwrap();
        let project = temp.path().join("project");
        let home = temp.path().join("home");
        fs::create_dir_all(crate::home::project_skills_path(&project)).unwrap();
        let agents_body = "# Existing agent guidance\n";
        let zen_body = "# Existing maintainability guidance\n";
        fs::write(project.join("AGENTS.md"), agents_body).unwrap();
        fs::write(project.join("ZEN.md"), zen_body).unwrap();
        catalog::deposit_skill_at(Some(&home), &project, "alpha").unwrap();

        let report = destroy_project_at(&project, true, Some(&home)).unwrap();
        assert!(!crate::home::project_agents_path(&project).exists());
        assert!(catalog::list_catalog(Some(&home)).unwrap().is_empty());
        assert!(
            project.join("AGENTS.md").is_file(),
            "destroy removed pre-existing AGENTS.md"
        );
        assert!(
            project.join("ZEN.md").is_file(),
            "destroy removed pre-existing ZEN.md"
        );
        assert_eq!(
            fs::read_to_string(project.join("AGENTS.md")).unwrap(),
            agents_body
        );
        assert_eq!(
            fs::read_to_string(project.join("ZEN.md")).unwrap(),
            zen_body
        );
        assert_eq!(
            report.removed,
            vec![
                crate::home::project_skills_path(&project),
                crate::home::project_agents_path(&project)
            ]
        );
    }

    #[test]
    fn destroy_preserves_unrelated_agents_siblings() {
        let temp = tempfile::tempdir().unwrap();
        let project = temp.path().join("project");
        let home = temp.path().join("home");
        let agents = crate::home::project_agents_path(&project);
        fs::create_dir_all(crate::home::project_skills_path(&project)).unwrap();
        fs::write(agents.join("foreign-config.json"), b"keep\n").unwrap();
        catalog::deposit_skill_at(Some(&home), &project, "alpha").unwrap();

        destroy_project_at(&project, true, Some(&home)).unwrap();

        assert!(!crate::home::project_skills_path(&project).exists());
        assert_eq!(
            fs::read(agents.join("foreign-config.json")).unwrap(),
            b"keep\n"
        );
        assert!(agents.is_dir());
        assert!(catalog::list_catalog(Some(&home)).unwrap().is_empty());
    }
}
