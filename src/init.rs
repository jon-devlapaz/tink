//! Minimal project skill home: `.agents/skills/` (+ optional tink-skills / manage-tink).

use std::io::{self, BufRead, IsTerminal};
use std::path::{Path, PathBuf};

use crate::add;
use crate::error::Error;
use crate::home;
use crate::manage_tink;
use crate::output;
use crate::paths::{map_io, mkdir_p, require_directory, require_file};
use crate::style::CliStyle;

const TINK_SKILLS_SOURCE: &str = "jon-devlapaz/tink-skills";
const TINK_SKILLS: &[&str] = &["skill-scout", "triangulate-me"];

const AGENTS_FILENAME: &str = "AGENTS.md";
const AGENTS_MD: &str = "\
This project uses Tink to manage Agent Skills under `.agents/skills/`.
";

const SKILLS_README: &str = "\
# Project skills

Complete, repository-owned Agent Skills live in this directory. Each skill is a
directory containing a `SKILL.md` file and any resources it needs.
";

#[derive(Debug, Clone, Copy, Default)]
pub struct InitOptions {
    /// `None` = ask when interactive; default no when non-interactive.
    pub with_tink_skills: Option<bool>,
    /// `None` = install manage-tink (default on).
    pub with_manage_tink: Option<bool>,
}

#[derive(Debug)]
pub struct InstalledSkill {
    pub name: String,
    pub created: bool,
}

#[derive(Debug)]
pub struct InitReport {
    pub skills_path: PathBuf,
    pub skills_created: bool,
    pub inventory_home: PathBuf,
    pub inventory_created: bool,
    pub agents_written: bool,
    pub tink_skills_added: Vec<InstalledSkill>,
    pub manage_tink_added: Option<InstalledSkill>,
}

/// Ask `[y/N]` when `explicit` is unset and stdin is a TTY; otherwise use
/// `explicit` or default `false`.
///
/// `question` is printed as-is (callers apply styling, including purple skill names).
/// Optional `hint` is printed above the prompt in muted style.
pub fn opt_in(explicit: Option<bool>, question: &str, hint: Option<&str>) -> Result<bool, Error> {
    if let Some(value) = explicit {
        return Ok(value);
    }
    if !io::stdin().is_terminal() {
        return Ok(false);
    }
    let style = CliStyle::auto_stdout();
    if let Some(hint) = hint {
        output::stdout_line(format_args!("{}", style.muted(hint)))?;
    }
    output::stdout(format_args!("{} {}", question, style.accent("[y/N] ")))?;
    output::flush_stdout()?;
    let mut line = String::new();
    io::stdin()
        .lock()
        .read_line(&mut line)
        .map_err(|e| Error::msg(format!("prompt: {e}")))?;
    let answer = line.trim().to_ascii_lowercase();
    Ok(answer == "y" || answer == "yes")
}

fn write_agents_md(project_root: &Path) -> Result<bool, Error> {
    let agents_file = project_root.join(AGENTS_FILENAME);
    require_file(&agents_file)?;
    if agents_file.exists() {
        return Ok(false);
    }
    std::fs::write(&agents_file, AGENTS_MD).map_err(|e| map_io(&agents_file, e))?;
    Ok(true)
}

/// Create `.agents/skills/`, optionally tink-skills/manage-tink, and ensure home root.
pub fn init_project(project_root: &Path, options: InitOptions) -> Result<InitReport, Error> {
    init_project_at(None, project_root, options)
}

pub(crate) fn init_project_at(
    home: Option<&Path>,
    project_root: &Path,
    options: InitOptions,
) -> Result<InitReport, Error> {
    let agents = crate::home::project_agents_path(project_root);
    let skills = crate::home::project_skills_path(project_root);
    let readme = skills.join("README.md");

    require_directory(&agents)?;
    require_directory(&skills)?;
    require_file(&readme)?;

    let style = CliStyle::auto_stdout();
    let with_tink_skills = opt_in(
        options.with_tink_skills,
        &format!(
            "{}{} and {}{}{}?",
            style.warn("Add "),
            style.skill("skill-scout"),
            style.skill("triangulate-me"),
            style.warn(" from "),
            style.link(
                &format!("https://github.com/{TINK_SKILLS_SOURCE}"),
                "tink-skills",
            ),
        ),
        Some("Optional GitHub bundle — click tink-skills to open the repo"),
    )?;
    let with_manage_tink = options.with_manage_tink.unwrap_or(true);

    let (inventory_home, home_created) = home::ensure_inventory_root(home)?;

    let skills_created = !skills.is_dir();
    mkdir_p(&agents)?;
    mkdir_p(&skills)?;
    if !readme.exists() {
        std::fs::write(&readme, SKILLS_README).map_err(|e| map_io(&readme, e))?;
    }

    let agents_written = write_agents_md(project_root)?;

    let manage_tink_added = if with_manage_tink {
        let outcome = manage_tink::install_manage_tink_at(home, project_root)?;
        Some(InstalledSkill {
            name: outcome.name,
            created: outcome.created,
        })
    } else {
        None
    };

    let mut tink_skills_added = Vec::new();
    if with_tink_skills {
        for name in TINK_SKILLS {
            let outcome =
                add::add_skill_quiet_at(home, project_root, TINK_SKILLS_SOURCE, Some(name))?;
            tink_skills_added.push(InstalledSkill {
                name: outcome.name,
                created: outcome.created,
            });
        }
    }

    Ok(InitReport {
        skills_path: skills,
        skills_created,
        inventory_home,
        inventory_created: home_created,
        agents_written,
        tink_skills_added,
        manage_tink_added,
    })
}

/// Bootstrap used by `add` — skills dir + home only, no prompts or bundled skills.
pub(crate) fn ensure_project_layout_at(
    home: Option<&Path>,
    project_root: &Path,
) -> Result<(), Error> {
    let agents = crate::home::project_agents_path(project_root);
    let skills = crate::home::project_skills_path(project_root);
    let readme = skills.join("README.md");

    require_directory(&agents)?;
    require_directory(&skills)?;
    require_file(&readme)?;
    let _ = home::ensure_inventory_root(home)?;

    mkdir_p(&agents)?;
    mkdir_p(&skills)?;
    if !readme.exists() {
        std::fs::write(&readme, SKILLS_README).map_err(|e| map_io(&readme, e))?;
    }
    Ok(())
}
