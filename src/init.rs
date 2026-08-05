//! Minimal project skill home: `.agents/skills/` (+ optional ZEN / tink-skills / manage-tink).

use std::io::{self, BufRead, IsTerminal, Write};
use std::path::{Path, PathBuf};

use crate::add;
use crate::error::Error;
use crate::home;
use crate::manage_tink;
use crate::paths::{map_io, mkdir_p, require_directory, require_file};
use crate::style::CliStyle;
use crate::templates::{
    self, TINK_SKILLS, TINK_SKILLS_SOURCE, ZEN, ZEN_REFERENCE, ZEN_REFERENCE_MARKER,
};

const SKILLS_README: &str = "\
# Project skills

Complete, repository-owned Agent Skills live in this directory. Each skill is a
directory containing a `SKILL.md` file and any resources it needs.
";

#[derive(Debug, Clone, Copy, Default)]
pub struct InitOptions {
    /// `None` = ask when interactive; default no when non-interactive.
    pub with_zen: Option<bool>,
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
    pub zen_written: bool,
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
        println!("{}", style.muted(hint));
    }
    print!("{} {}", question, style.accent("[y/N] "));
    io::stdout()
        .flush()
        .map_err(|e| Error::msg(format!("prompt: {e}")))?;
    let mut line = String::new();
    io::stdin()
        .lock()
        .read_line(&mut line)
        .map_err(|e| Error::msg(format!("prompt: {e}")))?;
    let answer = line.trim().to_ascii_lowercase();
    Ok(answer == "y" || answer == "yes")
}

/// Like [`opt_in`], but offers `r` to print embedded `ZEN.md` before deciding.
fn opt_in_zen(explicit: Option<bool>) -> Result<bool, Error> {
    if let Some(value) = explicit {
        return Ok(value);
    }
    if !io::stdin().is_terminal() {
        return Ok(false);
    }
    let style = CliStyle::auto_stdout();
    let question = style.warn("Add tink's maintainability principles (ZEN.md)?");
    let hint = style.muted("r = print ZEN.md without adding it");
    loop {
        println!("{hint}");
        print!("{} {}", question, style.accent("[y/N/r] "));
        io::stdout()
            .flush()
            .map_err(|e| Error::msg(format!("prompt: {e}")))?;
        let mut line = String::new();
        io::stdin()
            .lock()
            .read_line(&mut line)
            .map_err(|e| Error::msg(format!("prompt: {e}")))?;
        let answer = line.trim().to_ascii_lowercase();
        match answer.as_str() {
            "y" | "yes" => return Ok(true),
            "r" | "read" => {
                println!();
                println!("{}", style.muted(ZEN.trim_end()));
                println!();
            }
            _ => return Ok(false),
        }
    }
}

fn write_zen(project_root: &Path) -> Result<bool, Error> {
    let zen = project_root.join(templates::ZEN_FILENAME);
    let agents_file = project_root.join("AGENTS.md");
    require_file(&zen)?;
    require_file(&agents_file)?;

    let mut wrote = false;
    if !zen.exists() {
        std::fs::write(&zen, ZEN).map_err(|e| map_io(&zen, e))?;
        wrote = true;
    }
    if !agents_file.exists() {
        let body = format!("# Agent instructions\n\n{ZEN_REFERENCE}");
        std::fs::write(&agents_file, body).map_err(|e| map_io(&agents_file, e))?;
        wrote = true;
    } else {
        let current = std::fs::read_to_string(&agents_file).map_err(|e| map_io(&agents_file, e))?;
        if !current.contains(ZEN_REFERENCE_MARKER) {
            let separator = if current.is_empty() || current.ends_with("\n\n") {
                ""
            } else if current.ends_with('\n') {
                "\n"
            } else {
                "\n\n"
            };
            let body = format!("{current}{separator}{ZEN_REFERENCE}");
            std::fs::write(&agents_file, body).map_err(|e| map_io(&agents_file, e))?;
            wrote = true;
        }
    }
    Ok(wrote)
}

/// Create `.agents/skills/`, optionally ZEN/tink-skills/manage-tink, and ensure home root.
pub fn init_project(project_root: &Path, options: InitOptions) -> Result<InitReport, Error> {
    let agents = project_root.join(".agents");
    let skills = agents.join("skills");
    let readme = skills.join("README.md");

    require_directory(&agents)?;
    require_directory(&skills)?;
    require_file(&readme)?;

    let style = CliStyle::auto_stdout();
    let with_zen = opt_in_zen(options.with_zen)?;
    let with_tink_skills = opt_in(
        options.with_tink_skills,
        &format!(
            "{}{} and {}{}{}?",
            style.warn("Add "),
            style.skill("skill-scout"),
            style.skill("skill-eval-loop"),
            style.warn(" from "),
            style.link(
                &format!("https://github.com/{TINK_SKILLS_SOURCE}"),
                "tink-skills",
            ),
        ),
        Some("Optional GitHub bundle — click tink-skills to open the repo"),
    )?;
    let with_manage_tink = options.with_manage_tink.unwrap_or(true);

    if with_zen {
        require_file(&project_root.join(templates::ZEN_FILENAME))?;
        require_file(&project_root.join("AGENTS.md"))?;
    }

    let skills_created = !skills.is_dir();
    mkdir_p(&agents)?;
    mkdir_p(&skills)?;
    if !readme.exists() {
        std::fs::write(&readme, SKILLS_README).map_err(|e| map_io(&readme, e))?;
    }

    let zen_written = if with_zen {
        write_zen(project_root)?
    } else {
        false
    };

    let manage_tink_added = if with_manage_tink {
        let outcome = manage_tink::install_manage_tink(project_root)?;
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
            let outcome = add::add_skill_quiet(project_root, TINK_SKILLS_SOURCE, Some(name))?;
            tink_skills_added.push(InstalledSkill {
                name: outcome.name,
                created: outcome.created,
            });
        }
    }

    let (home, home_created) = home::ensure_inventory_root(None)?;
    Ok(InitReport {
        skills_path: skills,
        skills_created,
        inventory_home: home,
        inventory_created: home_created,
        zen_written,
        tink_skills_added,
        manage_tink_added,
    })
}

/// Bootstrap used by `add` — skills dir + home only, no prompts or bundled skills.
pub fn ensure_project_layout(project_root: &Path) -> Result<(), Error> {
    let agents = project_root.join(".agents");
    let skills = agents.join("skills");
    let readme = skills.join("README.md");

    require_directory(&agents)?;
    require_directory(&skills)?;
    require_file(&readme)?;

    mkdir_p(&agents)?;
    mkdir_p(&skills)?;
    if !readme.exists() {
        std::fs::write(&readme, SKILLS_README).map_err(|e| map_io(&readme, e))?;
    }
    let _ = home::ensure_inventory_root(None)?;
    Ok(())
}
