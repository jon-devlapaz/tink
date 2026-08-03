//! Tink — project-local Agent Skill installer (Rust core).
//!
//! Acceptance boundary: [`../ACCEPTANCE.md`](../ACCEPTANCE.md).

mod add;
mod check;
mod destroy;
mod error;
mod git;
mod init;
mod inventory;
mod manage_tink;
mod paths;
mod refresh;
mod skills;
mod sources;
mod style;
mod templates;

use clap::{Parser, Subcommand};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use crate::error::Error;
use crate::init::InitOptions;
use crate::style::CliStyle;

#[derive(Debug, Parser)]
#[command(
    name = "tink",
    version,
    about = "Install Agent Skills into .agents/skills/",
    styles = crate::style::CLAP_STYLES
)]
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
        /// Add skill-scout and skill-eval-loop from GitHub (tink-skills)
        #[arg(
            long = "with-tink-skills",
            visible_alias = "with-twotink",
            group = "tink_skills"
        )]
        with_tink_skills: bool,
        /// Skip tink-skills bundle
        #[arg(
            long = "no-tink-skills",
            visible_alias = "no-twotink",
            group = "tink_skills"
        )]
        no_tink_skills: bool,
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
    /// Remove `.agents/`, `ZEN.md`, and `AGENTS.md` from this project
    Destroy {
        /// Skip the confirmation prompt
        #[arg(long)]
        yes: bool,
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
    /// List installed project skills, or the home by-project catalog
    List {
        /// List offline catalog under `$TINK_HOME` / `~/.tink` as `project\\troot\\tskill` TSV
        #[arg(long)]
        home: bool,
    },
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
            let style = CliStyle::auto_stderr();
            eprintln!("{}", style.error(&err));
            ExitCode::from(1)
        }
    }
}

fn dispatch(cli: Cli, cwd: PathBuf) -> Result<(), Error> {
    match cli.command {
        Command::Init {
            with_zen,
            no_zen,
            with_tink_skills,
            no_tink_skills,
            with_manage_tink,
            no_manage_tink,
        } => dispatch_init(
            &cwd,
            flag_tri(with_zen, no_zen),
            flag_tri(with_tink_skills, no_tink_skills),
            flag_tri(with_manage_tink, no_manage_tink),
        ),
        Command::Skill { command } => dispatch_skill(&cwd, command),
        Command::Add { source, skill } => {
            dispatch_skill_add(&cwd, &source, skill.as_deref())
        }
        Command::Check => dispatch_skill_check(&cwd),
        Command::Refresh { name } => dispatch_skill_refresh(&cwd, name.as_deref()),
        Command::Destroy { yes } => {
            let style = CliStyle::auto_stdout();
            let report = destroy::destroy_project(&cwd, yes)?;
            if report.removed.is_empty() {
                println!("{}", style.muted("Nothing to destroy"));
            } else {
                for path in &report.removed {
                    println!(
                        "{} {}",
                        style.success("Removed"),
                        style.accent(path.display())
                    );
                }
            }
            Ok(())
        }
    }
}

fn dispatch_skill(cwd: &Path, command: SkillCommand) -> Result<(), Error> {
    match command {
        SkillCommand::Add { source, skill } => {
            dispatch_skill_add(cwd, &source, skill.as_deref())
        }
        SkillCommand::List { home } => {
            if home {
                dispatch_skill_list_home()
            } else {
                dispatch_skill_list(cwd)
            }
        }
        SkillCommand::Check => dispatch_skill_check(cwd),
        SkillCommand::Refresh { name } => dispatch_skill_refresh(cwd, name.as_deref()),
    }
}

fn dispatch_init(
    cwd: &Path,
    with_zen: Option<bool>,
    with_tink_skills: Option<bool>,
    with_manage_tink: Option<bool>,
) -> Result<(), Error> {
    let style = CliStyle::auto_stdout();
    let report = init::init_project(
        cwd,
        InitOptions {
            with_zen,
            with_tink_skills,
            with_manage_tink,
        },
    )?;
    if report.skills_created {
        println!(
            "{} {}",
            style.success("Created"),
            style.accent(report.skills_path.display())
        );
    } else {
        println!(
            "{} {}",
            style.success("Ready"),
            style.accent(report.skills_path.display())
        );
    }
    if report.zen_written {
        println!(
            "{} {}",
            style.success("Added"),
            style.accent("ZEN.md maintainability principles")
        );
    }
    if let Some(name) = &report.manage_tink_added {
        println!("{} {}", style.success("Added"), style.accent(name));
    }
    for name in &report.tink_skills_added {
        println!("{} {}", style.success("Added"), style.accent(name));
    }
    if report.inventory_created {
        println!(
            "{} {}",
            style.success("New home inventory at"),
            style.accent(report.inventory_home.display())
        );
    } else {
        println!(
            "{} {}",
            style.muted("Home inventory at"),
            style.accent(report.inventory_home.display())
        );
    }
    Ok(())
}

fn dispatch_skill_add(cwd: &Path, source: &str, skill: Option<&str>) -> Result<(), Error> {
    add::add_skill(cwd, source, skill).map(|_| ())
}

fn dispatch_skill_check(cwd: &Path) -> Result<(), Error> {
    let style = CliStyle::auto_stdout();
    let skills = check::check_project(cwd)?;
    println!(
        "{} {} skill(s)",
        style.success("OK"),
        style.accent(skills.len())
    );
    Ok(())
}

fn dispatch_skill_list(cwd: &Path) -> Result<(), Error> {
    let style = CliStyle::auto_stdout();
    let skills = check::check_project(cwd)?;
    if skills.is_empty() {
        println!("{}", style.muted("(no skills)"));
    } else {
        for skill in &skills {
            println!("{}", style.accent(&skill.name));
        }
    }
    Ok(())
}

fn dispatch_skill_list_home() -> Result<(), Error> {
    let style = CliStyle::auto_stdout();
    let entries = inventory::list_catalog(None)?;
    if entries.is_empty() {
        println!("{}", style.muted("(no catalog entries)"));
    } else {
        // Header + TSV rows: plain when piped; lightly role-colored on a TTY.
        println!(
            "{}\t{}\t{}",
            style.muted("project"),
            style.muted("root"),
            style.muted("skill")
        );
        for entry in &entries {
            println!(
                "{}\t{}\t{}",
                style.muted(&entry.project),
                style.muted(&entry.root),
                style.accent(&entry.skill)
            );
        }
    }
    Ok(())
}

fn dispatch_skill_refresh(cwd: &Path, name: Option<&str>) -> Result<(), Error> {
    let style = CliStyle::auto_stdout();
    match name {
        Some(name) => {
            let changed = refresh::refresh_skill(cwd, name)?;
            if changed {
                println!("{} {}", style.success("Refreshed"), style.accent(name));
            } else {
                println!("{} {}", style.muted("Unchanged"), style.accent(name));
            }
            Ok(())
        }
        None => {
            let refreshed = refresh::refresh_all(cwd)?;
            if refreshed.is_empty() {
                println!("{}", style.muted("Unchanged (no imported skills updated)"));
            } else {
                println!(
                    "{} {}",
                    style.success("Refreshed"),
                    style.accent(refreshed.join(", "))
                );
            }
            Ok(())
        }
    }
}
