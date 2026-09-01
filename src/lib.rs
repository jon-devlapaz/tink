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
mod inspect;
mod library;
mod manage_tink;
mod manifest;
mod output;
mod paths;
mod process;
mod provenance;
mod read;
mod refresh;
mod remove;
mod skills;
mod skillsets;
mod sources;
mod style;
mod templates;
mod update;

use clap::{Parser, Subcommand};
use clap_complete::{
    CompletionCandidate,
    engine::{ArgValueCompleter, PathCompleter, ValueCompleter},
};
use std::collections::BTreeSet;
use std::ffi::OsStr;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use crate::error::Error;
use crate::init::InitOptions;
use crate::style::CliStyle;

// This dispatch module is intentionally output-heavy. Shadow the infallible
// standard macros here so every write is propagated through the CLI error path.
macro_rules! println {
    () => {
        crate::output::stdout_line(format_args!(""))?
    };
    ($($arg:tt)*) => {
        crate::output::stdout_line(format_args!($($arg)*))?
    };
}

macro_rules! eprintln {
    ($($arg:tt)*) => {
        crate::output::warning_line(format_args!($($arg)*))
    };
}

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
        /// Add skill-scout and triangulate-me from GitHub (tink-skills)
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
    /// Inspect the reusable standalone skill library
    Library {
        #[command(subcommand)]
        command: LibraryCommand,
    },
    /// Install grouped member skills as one nested project tree
    Skillset {
        #[command(subcommand)]
        command: SkillsetCommand,
    },
    /// Inspect skills and source-defined skillsets in a public GitHub URL
    Inspect { url: String },
    /// Remove `.agents/skills/`, an empty `.agents/`, and this project's catalog entry (not guidance or library)
    Destroy {
        /// Skip the confirmation prompt
        #[arg(long)]
        yes: bool,
    },
    /// Replace this binary with a newer verified GitHub Release
    Update,
}

#[derive(Debug, Subcommand)]
pub enum SkillCommand {
    /// Copy one complete skill into the project
    Add {
        /// Local path, `owner/repo`, public GitHub HTTPS or skill tree URL, or library skill name
        #[arg(add = ArgValueCompleter::new(add_source_candidates))]
        source: String,
        /// Unique skill name or repository-relative skill path
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
    /// Read a standalone skill's description and metadata
    Read {
        /// Canonical standalone skill name
        #[arg(add = ArgValueCompleter::new(read_skill_candidates))]
        name: String,
        /// Read from the home library instead of the current project
        #[arg(long, short = 'l')]
        library: bool,
        /// Output only the unstyled description text
        #[arg(long, short = 'r')]
        raw: bool,
    },
    /// Validate project skills without changing anything
    Check,
    /// Verify project skills against `.tink/skills.toml` and `.tink/skills.lock`
    Verify,
    /// Generate `.tink/skills.toml` and `.tink/skills.lock` from installed skills
    Lock {
        /// Source mapping for local skills (`NAME=PATH`); repeatable
        #[arg(long = "source", value_name = "NAME=PATH")]
        source: Vec<String>,
    },
    /// Install missing or pinned skills from the project manifest and lockfile
    Sync,
    /// Refresh clean GitHub imports or the reserved embedded manage-tink
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
    /// Publish one standalone project skill to the reusable library
    Promote {
        /// Directory name below `.agents/skills/`
        name: String,
        /// Replace a divergent reusable library copy
        #[arg(long)]
        replace: bool,
    },
}

#[derive(Debug, Subcommand)]
pub enum LibraryCommand {
    /// List standalone skill names in the library
    List,
}

#[derive(Debug, Subcommand)]
pub enum SkillsetCommand {
    /// List receipt-backed project skillsets and their members
    List {
        /// List receipt-backed library skillsets and their members
        #[arg(long)]
        library: bool,
    },
    /// Install the pinned skillset named in `$TINK_HOME/catalog/by-skillset/`
    Add { name: String },
    /// Update one clean installed skillset to its pinned catalog definition
    Refresh { name: String },
    /// Remove one installed skillset without deleting its shared catalog definition
    Remove { name: String },
}

fn read_skill_candidates(current: &OsStr) -> Vec<CompletionCandidate> {
    let Some(prefix) = current.to_str() else {
        return Vec::new();
    };
    let mut names = BTreeSet::new();
    if let Ok(cwd) = std::env::current_dir() {
        names.extend(project_standalone_completion_names(&cwd));
    }
    names.extend(library::list_names(None).unwrap_or_default());
    names
        .into_iter()
        .filter(|name| name.starts_with(prefix))
        .map(CompletionCandidate::new)
        .collect()
}

fn project_standalone_completion_names(cwd: &Path) -> Vec<String> {
    let skills_root = home::project_skills_path(cwd);
    let Ok(entries) = std::fs::read_dir(&skills_root) else {
        return Vec::new();
    };
    let mut names = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        let Some(name) = entry.file_name().to_str().map(str::to_owned) else {
            continue;
        };
        if name == "README.md" || name.starts_with('.') {
            continue;
        }
        if path.is_symlink() || !path.is_dir() {
            continue;
        }
        if skillsets::has_receipt_entry(&path) {
            continue;
        }
        if skills::valid_skill_name(&name) {
            names.push(name);
        }
    }
    names
}

fn add_source_candidates(current: &OsStr) -> Vec<CompletionCandidate> {
    let mut candidates = PathCompleter::any().complete(current);
    let Some(prefix) = current.to_str() else {
        return candidates;
    };
    candidates.extend(
        library::list_names(None)
            .unwrap_or_default()
            .into_iter()
            .filter(|name| name.starts_with(prefix))
            .map(CompletionCandidate::new),
    );
    candidates.sort();
    candidates.dedup();
    candidates
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
        Err(err) if err.is_stdout_broken_pipe() => ExitCode::SUCCESS,
        Err(err) => {
            let style = CliStyle::auto_stderr();
            match output::stderr_line(format_args!("{}", style.error(&err))) {
                Ok(()) => ExitCode::from(if err.is_conflict() { 3 } else { 1 }),
                // Downstream closure is expected for CLI pipelines. Never convert
                // it into Rust's default printing panic / exit status 101.
                Err(_) => ExitCode::from(1),
            }
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
        Command::Library { command } => match command {
            LibraryCommand::List => dispatch_skill_list_library(),
        },
        Command::Skillset { command } => dispatch_skillset(&cwd, command),
        Command::Inspect { url } => dispatch_inspect(&url),
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
            update::print_report(&report)?;
            Ok(())
        }
    }
}

fn dispatch_inspect(url: &str) -> Result<(), Error> {
    let style = CliStyle::auto_stdout();
    let report = inspect::inspect(url)?;
    println!("Source");
    println!("  Repository: {}", style.accent(&report.repository));
    println!("  Revision:   {}", style.accent(&report.revision));
    println!("  Boundary:   {}", style.accent(&report.boundary));
    println!();
    let member_count: usize = report
        .skillsets
        .iter()
        .map(|skillset| skillset.members)
        .sum();
    println!(
        "Skillsets ({}, {} member skills)",
        report.skillsets.len(),
        member_count
    );
    let mut grouped_paths = BTreeSet::new();
    for (index, skillset) in report.skillsets.iter().enumerate() {
        if index > 0 {
            println!();
        }
        let name = skillset.name.as_deref().unwrap_or("(unnamed proposal)");
        let padded_name = format!("{name:<28}");
        let displayed_name = if skillset.name.is_some() {
            style.skillset(padded_name)
        } else {
            padded_name
        };
        let source_path = if skillset.path == "." {
            "./".to_string()
        } else {
            format!("{}/", skillset.path)
        };
        let noun = if skillset.members == 1 {
            "skill"
        } else {
            "skills"
        };
        println!(
            "  {} ({} {})  {}",
            displayed_name,
            skillset.members,
            noun,
            style.accent(source_path)
        );
        if skillset.members == 0 {
            println!("    (empty structural candidate)");
            continue;
        }
        let prefix = if skillset.path == "." {
            None
        } else {
            Some(format!("{}/", skillset.path))
        };
        for skill in &report.skills {
            if prefix
                .as_ref()
                .map(|prefix| skill.path.starts_with(prefix))
                .unwrap_or(true)
            {
                println!("    {}", style.skill(&skill.name));
                grouped_paths.insert(skill.path.as_str());
            }
        }
    }
    println!();
    let standalone: Vec<_> = report
        .skills
        .iter()
        .filter(|skill| !grouped_paths.contains(skill.path.as_str()))
        .collect();
    println!("Standalone skills ({})", standalone.len());
    for skill in standalone {
        let padded_name = format!("{:<28}", skill.name);
        println!(
            "  {} {}",
            style.skill(padded_name),
            style.accent(&skill.path)
        );
    }
    if !report.diagnostics.is_empty() {
        println!();
        println!(
            "{}",
            style.warn(format!("Diagnostics ({})", report.diagnostics.len()))
        );
        for diagnostic in &report.diagnostics {
            println!("  {diagnostic}");
        }
    }
    Ok(())
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
        SkillCommand::Read { name, library, raw } => dispatch_skill_read(cwd, &name, library, raw),
        SkillCommand::Check => dispatch_skill_check(cwd),
        SkillCommand::Verify => dispatch_skill_verify(cwd),
        SkillCommand::Lock { source } => dispatch_skill_lock(cwd, &source),
        SkillCommand::Sync => dispatch_skill_sync(cwd),
        SkillCommand::Refresh { name } => dispatch_skill_refresh(cwd, name.as_deref()),
        SkillCommand::Remove { name } => dispatch_skill_remove(cwd, &name),
        SkillCommand::Harvest => dispatch_skill_harvest(cwd),
        SkillCommand::Promote { name, replace } => dispatch_skill_promote(cwd, &name, replace),
    }
}

fn dispatch_skillset(cwd: &Path, command: SkillsetCommand) -> Result<(), Error> {
    match command {
        SkillsetCommand::List { library } => {
            let style = CliStyle::auto_stdout();
            let skillsets = if library {
                skillsets::list_library(None)?
            } else {
                skillsets::list_installed(cwd)?
            };
            if skillsets.is_empty() {
                let message = if library {
                    "(no library skillsets)"
                } else {
                    "(no skillsets)"
                };
                println!("{}", style.muted(message));
            } else {
                for (index, skillset) in skillsets.iter().enumerate() {
                    if index > 0 {
                        println!();
                    }
                    let noun = if skillset.members.len() == 1 {
                        "skill"
                    } else {
                        "skills"
                    };
                    println!(
                        "{} {}",
                        style.skillset(&skillset.name),
                        style.muted(format!("({} {noun})", skillset.members.len()))
                    );
                    for member in &skillset.members {
                        println!("  {}", style.skill(member));
                    }
                }
            }
            Ok(())
        }
        SkillsetCommand::Add { name } => {
            let style = CliStyle::auto_stdout();
            let (path, created, library_write) = skillsets::add_skillset(cwd, &name)?;
            if library_write == skillsets::LibraryWrite::Repaired {
                let err = CliStyle::auto_stderr();
                eprintln!("{}", err.warn(format!("Updated home copy of {name}")));
            }
            if created {
                println!(
                    "{} {}",
                    style.success("Installed"),
                    style.accent(path.display())
                );
            } else {
                println!("{} {}", style.muted("Unchanged"), style.skillset(name));
            }
            Ok(())
        }
        SkillsetCommand::Refresh { name } => {
            let style = CliStyle::auto_stdout();
            if skillsets::refresh_skillset(cwd, &name)? {
                println!("{} {}", style.success("Refreshed"), style.skillset(name));
            } else {
                println!("{} {}", style.muted("Unchanged"), style.skillset(name));
            }
            Ok(())
        }
        SkillsetCommand::Remove { name } => {
            let style = CliStyle::auto_stdout();
            let path = skillsets::remove_skillset(cwd, &name)?;
            println!(
                "{} {}",
                style.success("Removed"),
                style.accent(path.display())
            );
            Ok(())
        }
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
        print_init_skill(&style, skill)?;
    }
    for skill in &report.tink_skills_added {
        print_init_skill(&style, skill)?;
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

fn print_init_skill(style: &CliStyle, skill: &init::InstalledSkill) -> Result<(), Error> {
    if skill.created {
        println!("{} {}", style.success("Added"), style.skill(&skill.name));
    } else {
        println!(
            "{} {}",
            style.muted("Already present"),
            style.skill(&skill.name)
        );
    }
    Ok(())
}

fn dispatch_skill_add(cwd: &Path, source: &str, skill: Option<&str>) -> Result<(), Error> {
    add::add_skill(cwd, source, skill).map(|_| ())
}

fn dispatch_skill_sync(cwd: &Path) -> Result<(), Error> {
    let style = CliStyle::auto_stdout();
    let count = manifest::sync(cwd)?;
    println!(
        "{} {} manifest skill(s)",
        style.success("Synced"),
        style.accent(count)
    );
    Ok(())
}

fn dispatch_skill_lock(cwd: &Path, sources: &[String]) -> Result<(), Error> {
    let style = CliStyle::auto_stdout();
    let count = manifest::lock(cwd, sources)?;
    println!(
        "{} {} manifest skill(s)",
        style.success("Wrote"),
        style.accent(count)
    );
    Ok(())
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
    let (skillsets, members) = skillsets::project_counts(cwd)?;
    if skillsets == 0 {
        println!(
            "{} {} skill(s)",
            style.success("OK"),
            style.accent(skills.len())
        );
    } else {
        println!(
            "{} {} skill(s), {} skillset(s), {} member skill(s)",
            style.success("OK"),
            style.accent(skills.len()),
            style.accent(skillsets),
            style.accent(members)
        );
    }
    Ok(())
}

fn dispatch_skill_read(cwd: &Path, name: &str, library: bool, raw: bool) -> Result<(), Error> {
    let report = read::read_skill_report(cwd, name, library)?;
    read::print_report(&report, raw, CliStyle::auto_stdout())
}

fn dispatch_skill_list(cwd: &Path) -> Result<(), Error> {
    let out = CliStyle::auto_stdout();
    let err = CliStyle::auto_stderr();
    let skills = check::load_project_skills(cwd)?;
    if let Err(zen_err) = check::check_zen_coupling(cwd) {
        eprintln!("{}", err.warn(zen_err.to_string()));
    }
    if skills.is_empty() {
        let (skillsets, _) = skillsets::project_counts(cwd)?;
        if skillsets > 0 {
            println!(
                "{}",
                out.muted("(no standalone skills; use `tink skillset list`)")
            );
        } else {
            println!("{}", out.muted("(no skills)"));
        }
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
            style.muted(tsv_field(&entry.project)),
            style.muted(tsv_field(&entry.root)),
            style.skill(tsv_field(&entry.skill))
        );
    }
    Ok(())
}

fn tsv_field(value: &str) -> String {
    output::escape_untrusted(value)
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

fn dispatch_skill_promote(cwd: &Path, name: &str, replace: bool) -> Result<(), Error> {
    let style = CliStyle::auto_stdout();
    let outcome = library::promote(cwd, name, replace)?;
    let action = match outcome.write {
        library::PromotionWrite::Created => style.success("Created"),
        library::PromotionWrite::Unchanged => style.muted("Unchanged"),
        library::PromotionWrite::Replaced => style.success("Replaced"),
    };
    println!(
        "{} {} {}\n{} {}\n{} {}",
        action,
        style.skill(name),
        style.accent(outcome.destination.display()),
        style.muted("Origin"),
        style.accent(format!("project skill {name}")),
        style.muted("Digest"),
        style.accent(outcome.digest),
    );
    Ok(())
}

fn dispatch_skill_refresh(cwd: &Path, name: Option<&str>) -> Result<(), Error> {
    let style = CliStyle::auto_stdout();
    match name {
        Some("manage-tink") => {
            let outcome = manage_tink::refresh_manage_tink(cwd)?;
            let action = match outcome {
                manage_tink::RefreshOutcome::Installed => style.success("Installed"),
                manage_tink::RefreshOutcome::Unchanged => style.muted("Unchanged"),
                manage_tink::RefreshOutcome::Refreshed => style.success("Refreshed"),
            };
            println!("{} {}", action, style.skill("manage-tink"));
            Ok(())
        }
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
