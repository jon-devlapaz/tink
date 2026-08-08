//! Tink — project-local Agent Skill installer (Rust core).
//!
//! Acceptance boundary: [`../ACCEPTANCE.md`](../ACCEPTANCE.md).

mod add;
mod catalog;
mod check;
mod destroy;
mod error;
mod git;
mod harvest;
mod home;
mod init;
mod library;
mod manage_tink;
mod manifest;
mod paths;
mod provenance;
mod refresh;
mod remove;
mod skills;
mod sources;
mod style;
mod templates;
mod update;

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
        #[arg(long = "with-tink-skills", group = "tink_skills")]
        with_tink_skills: bool,
        /// Skip tink-skills bundle
        #[arg(long = "no-tink-skills", group = "tink_skills")]
        no_tink_skills: bool,
        /// Install the embedded manage-tink skill (default)
        #[arg(long, group = "manage_tink")]
        with_manage_tink: bool,
        /// Skip the embedded manage-tink skill
        #[arg(long, group = "manage_tink")]
        no_manage_tink: bool,
    },
    /// Manage project Agent Skills
    Skill {
        #[command(subcommand)]
        command: SkillCommand,
    },
    /// Remove `.agents/`, `ZEN.md`, `AGENTS.md`, and this project's catalog entry (not library)
    Destroy {
        /// Skip the confirmation prompt
        #[arg(long)]
        yes: bool,
    },
    /// Replace this binary with the latest GitHub Release
    Update,
}

#[derive(Debug, Subcommand)]
pub enum SkillCommand {
    /// Copy one complete skill into the project
    Add {
        /// Local path, `owner/repo`, public GitHub HTTPS URL, or library skill name
        source: String,
        /// Skill name when the source contains several skills
        #[arg(long)]
        skill: Option<String>,
    },
    /// List installed project skills, the by-project catalog, or the library
    List {
        /// List offline catalog under `$TINK_HOME` / `~/.tink` as `project\\troot\\tskill` TSV
        #[arg(long, group = "list_source")]
        catalog: bool,
        /// List skill names in the library (`skills/<name>/`)
        #[arg(long, group = "list_source")]
        library: bool,
    },
    /// Validate project skills without changing anything
    Check,
    /// Verify project skills against `.tink/skills.toml`
    Verify,
    /// Refresh clean GitHub-imported skills; refuse local modifications
    Refresh {
        /// Optional skill name; default refreshes all imported skills
        name: Option<String>,
    },
    /// Delete one project skill directory and drop it from the by-project catalog (not library)
    Remove {
        /// Skill directory name under `.agents/skills/`
        name: String,
    },
    /// Copy harness skill trees into the library (create-only)
    Harvest,
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
        Command::Update => {
            let report = update::update_binary()?;
            update::print_report(&report);
            Ok(())
        }
    }
}

fn dispatch_skill(cwd: &Path, command: SkillCommand) -> Result<(), Error> {
    match command {
        SkillCommand::Add { source, skill } => dispatch_skill_add(cwd, &source, skill.as_deref()),
        SkillCommand::List { catalog, library } => {
            if catalog {
                dispatch_skill_list_catalog()
            } else if library {
                dispatch_skill_list_library()
            } else {
                dispatch_skill_list(cwd)
            }
        }
        SkillCommand::Check => dispatch_skill_check(cwd),
        SkillCommand::Verify => dispatch_skill_verify(cwd),
        SkillCommand::Refresh { name } => dispatch_skill_refresh(cwd, name.as_deref()),
        SkillCommand::Remove { name } => dispatch_skill_remove(cwd, &name),
        SkillCommand::Harvest => dispatch_skill_harvest(cwd),
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
            "{} {} {}",
            style.success("Added"),
            style.rainbow("ZEN.md"),
            style.accent("maintainability principles")
        );
    }
    if let Some(skill) = &report.manage_tink_added {
        print_init_skill(&style, skill);
    }
    for skill in &report.tink_skills_added {
        print_init_skill(&style, skill);
    }
    if report.inventory_created {
        println!(
            "{} {}",
            style.success("New home at"),
            style.accent(report.inventory_home.display())
        );
    } else {
        println!(
            "{} {}",
            style.muted("Home at"),
            style.accent(report.inventory_home.display())
        );
    }
    Ok(())
}

fn print_init_skill(style: &CliStyle, skill: &init::InstalledSkill) {
    if skill.created {
        println!("{} {}", style.success("Added"), style.skill(&skill.name));
    } else {
        println!(
            "{} {}",
            style.muted("Already present"),
            style.skill(&skill.name)
        );
    }
}

fn dispatch_skill_add(cwd: &Path, source: &str, skill: Option<&str>) -> Result<(), Error> {
    add::add_skill(cwd, source, skill).map(|_| ())
}

fn dispatch_skill_verify(cwd: &Path) -> Result<(), Error> {
    let style = CliStyle::auto_stdout();
    let count = manifest::verify(cwd)?;
    println!(
        "{} {} manifest skill(s)",
        style.success("OK"),
        style.accent(count)
    );
    Ok(())
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
    let out = CliStyle::auto_stdout();
    let err = CliStyle::auto_stderr();
    let skills = check::load_project_skills(cwd)?;
    if let Err(zen_err) = check::check_zen_coupling(cwd) {
        eprintln!("{}", err.warn(zen_err.to_string()));
    }
    if skills.is_empty() {
        println!("{}", out.muted("(no skills)"));
    } else {
        for skill in &skills {
            println!("{}", out.skill(&skill.name));
        }
    }
    Ok(())
}

fn dispatch_skill_list_catalog() -> Result<(), Error> {
    let style = CliStyle::auto_stdout();
    let entries = catalog::list_catalog(None)?;
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
                style.skill(&entry.skill)
            );
        }
    }
    Ok(())
}

fn dispatch_skill_list_library() -> Result<(), Error> {
    let style = CliStyle::auto_stdout();
    let names = library::list_names(None)?;
    if names.is_empty() {
        println!("{}", style.muted("(no library skills)"));
    } else {
        for name in &names {
            println!("{}", style.skill(name));
        }
    }
    Ok(())
}

fn dispatch_skill_remove(cwd: &Path, name: &str) -> Result<(), Error> {
    let style = CliStyle::auto_stdout();
    let report = remove::remove_skill(cwd, name)?;
    println!(
        "{} {}",
        style.success("Removed"),
        style.accent(report.removed.display())
    );
    Ok(())
}

fn dispatch_skill_harvest(cwd: &Path) -> Result<(), Error> {
    let out = CliStyle::auto_stdout();
    let err = CliStyle::auto_stderr();
    let report = harvest::harvest(cwd)?;
    for event in &report.events {
        match event.action {
            harvest::HarvestAction::Created => {
                println!(
                    "{} {} {}",
                    out.success("Harvested"),
                    out.skill(&event.name),
                    out.muted(event.source.display())
                );
            }
            harvest::HarvestAction::Unchanged => {
                // Quiet no-ops; counts appear in the summary.
            }
            harvest::HarvestAction::Skipped => {
                let detail = event.detail.as_deref().unwrap_or("skipped");
                eprintln!(
                    "{} {} {} ({})",
                    err.warn("Skipped"),
                    err.skill(&event.name),
                    err.muted(event.source.display()),
                    detail
                );
            }
        }
    }
    let home = home::resolve_home()?;
    println!(
        "{} {} {} · {} {} · {} {}",
        out.success("Harvest"),
        out.accent(report.created),
        out.muted("harvested"),
        out.accent(report.unchanged),
        out.muted("already present"),
        out.accent(report.skipped),
        out.muted("skipped")
    );
    println!("{} {}", out.muted("Home"), out.accent(home.display()));
    Ok(())
}

fn dispatch_skill_refresh(cwd: &Path, name: Option<&str>) -> Result<(), Error> {
    let style = CliStyle::auto_stdout();
    match name {
        Some(name) => {
            let changed = refresh::refresh_skill(cwd, name)?;
            if changed {
                println!("{} {}", style.success("Refreshed"), style.skill(name));
            } else {
                println!("{} {}", style.muted("Unchanged"), style.skill(name));
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
                    style.skill(refreshed.join(", "))
                );
            }
            Ok(())
        }
    }
}
