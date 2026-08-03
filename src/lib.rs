//! Tink — project-local Agent Skill installer (Rust core).
//!
//! Acceptance boundary: [`../ACCEPTANCE.md`](../ACCEPTANCE.md).

mod add;
mod check;
mod error;
mod git;
mod init;
mod inventory;
mod manage_tink;
mod paths;
mod refresh;
mod skills;
mod sources;
mod templates;

use clap::{Parser, Subcommand};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use crate::error::Error;
use crate::init::InitOptions;

#[derive(Debug, Parser)]
#[command(name = "tink", version, about = "Install Agent Skills into .agents/skills/")]
pub struct Cli {
    #[command(subcommand)]
    pub command: Command,
}

#[derive(Debug, Subcommand)]
pub enum Command {
    /// Create `.agents/skills/` and ensure the home inventory root exists
    Init {
        /// Add ZEN.md and reference it from AGENTS.md
        #[arg(long, group = "zen")]
        with_zen: bool,
        /// Skip ZEN.md
        #[arg(long, group = "zen")]
        no_zen: bool,
        /// Add Twotink skill-scout and skill-eval-loop from GitHub
        #[arg(long, group = "twotink")]
        with_twotink: bool,
        /// Skip Twotink skills
        #[arg(long, group = "twotink")]
        no_twotink: bool,
        /// Install the embedded manage-tink skill (default)
        #[arg(long, group = "manage_tink")]
        with_manage_tink: bool,
        /// Skip the embedded manage-tink skill
        #[arg(long, group = "manage_tink")]
        no_manage_tink: bool,
    },
    /// Manage project Agent Skills (canonical surface)
    Skill {
        #[command(subcommand)]
        command: SkillCommand,
    },
    /// Alias of `tink skill add`
    Add {
        /// Local skill/repository path, `owner/repo`, or public GitHub HTTPS URL
        source: String,
        /// Skill name when the source contains several skills
        #[arg(long)]
        skill: Option<String>,
    },
    /// Alias of `tink skill check`
    Check,
    /// Alias of `tink skill refresh`
    Refresh {
        /// Optional skill name; default refreshes all imported skills
        name: Option<String>,
    },
}

#[derive(Debug, Subcommand)]
pub enum SkillCommand {
    /// Copy one complete skill into the project
    Add {
        /// Local skill/repository path, `owner/repo`, or public GitHub HTTPS URL
        source: String,
        /// Skill name when the source contains several skills
        #[arg(long)]
        skill: Option<String>,
    },
    /// List installed project skills
    List,
    /// Validate project skills without changing anything
    Check,
    /// Refresh clean GitHub-imported skills; refuse local modifications
    Refresh {
        /// Optional skill name; default refreshes all imported skills
        name: Option<String>,
    },
}

fn flag_tri(yes: bool, no: bool) -> Option<bool> {
    match (yes, no) {
        (true, _) => Some(true),
        (_, true) => Some(false),
        _ => None,
    }
}

/// Run the CLI. `cwd` is the process working directory.
pub fn run(cli: Cli, cwd: PathBuf) -> ExitCode {
    match dispatch(cli, cwd) {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            eprintln!("{err}");
            ExitCode::from(1)
        }
    }
}

fn dispatch(cli: Cli, cwd: PathBuf) -> Result<(), Error> {
    match cli.command {
        Command::Init {
            with_zen,
            no_zen,
            with_twotink,
            no_twotink,
            with_manage_tink,
            no_manage_tink,
        } => dispatch_init(
            &cwd,
            flag_tri(with_zen, no_zen),
            flag_tri(with_twotink, no_twotink),
            flag_tri(with_manage_tink, no_manage_tink),
        ),
        Command::Skill { command } => dispatch_skill(&cwd, command),
        Command::Add { source, skill } => {
            dispatch_skill_add(&cwd, &source, skill.as_deref())
        }
        Command::Check => dispatch_skill_check(&cwd),
        Command::Refresh { name } => dispatch_skill_refresh(&cwd, name.as_deref()),
    }
}

fn dispatch_skill(cwd: &Path, command: SkillCommand) -> Result<(), Error> {
    match command {
        SkillCommand::Add { source, skill } => {
            dispatch_skill_add(cwd, &source, skill.as_deref())
        }
        SkillCommand::List => dispatch_skill_list(cwd),
        SkillCommand::Check => dispatch_skill_check(cwd),
        SkillCommand::Refresh { name } => dispatch_skill_refresh(cwd, name.as_deref()),
    }
}

fn dispatch_init(
    cwd: &Path,
    with_zen: Option<bool>,
    with_twotink: Option<bool>,
    with_manage_tink: Option<bool>,
) -> Result<(), Error> {
    let report = init::init_project(
        cwd,
        InitOptions {
            with_zen,
            with_twotink,
            with_manage_tink,
        },
    )?;
    if report.skills_created {
        println!("Created {}", report.skills_path.display());
    } else {
        println!("Ready {}", report.skills_path.display());
    }
    if report.zen_written {
        println!("Added ZEN.md maintainability principles");
    }
    if let Some(name) = &report.manage_tink_added {
        println!("Added {name}");
    }
    for name in &report.twotink_added {
        println!("Added {name}");
    }
    if report.inventory_created {
        println!("New home inventory at {}", report.inventory_home.display());
    } else {
        println!("Home inventory at {}", report.inventory_home.display());
    }
    Ok(())
}

fn dispatch_skill_add(cwd: &Path, source: &str, skill: Option<&str>) -> Result<(), Error> {
    add::add_skill(cwd, source, skill).map(|_| ())
}

fn dispatch_skill_check(cwd: &Path) -> Result<(), Error> {
    let skills = check::check_project(cwd)?;
    println!("OK {} skill(s)", skills.len());
    Ok(())
}

fn dispatch_skill_list(cwd: &Path) -> Result<(), Error> {
    let skills = check::check_project(cwd)?;
    if skills.is_empty() {
        println!("(no skills)");
    } else {
        for skill in &skills {
            println!("{}", skill.name);
        }
    }
    Ok(())
}

fn dispatch_skill_refresh(cwd: &Path, name: Option<&str>) -> Result<(), Error> {
    match name {
        Some(name) => {
            let changed = refresh::refresh_skill(cwd, name)?;
            if changed {
                println!("Refreshed {name}");
            } else {
                println!("Unchanged {name}");
            }
            Ok(())
        }
        None => {
            let refreshed = refresh::refresh_all(cwd)?;
            if refreshed.is_empty() {
                println!("Unchanged (no imported skills updated)");
            } else {
                println!("Refreshed {}", refreshed.join(", "));
            }
            Ok(())
        }
    }
}
