//! Tink — project-local Agent Skill installer (Rust core).
//!
//! Acceptance boundary: [`../ACCEPTANCE.md`](../ACCEPTANCE.md).

mod add;
mod check;
mod error;
mod git;
mod init;
mod inventory;
mod paths;
mod refresh;
mod skills;
mod sources;
mod templates;

use clap::{Parser, Subcommand};
use std::path::PathBuf;
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
    },
    /// Copy one complete skill into the project
    Add {
        /// Local skill/repository path, `owner/repo`, or public GitHub HTTPS URL
        source: String,
        /// Skill name when the source contains several skills
        #[arg(long)]
        skill: Option<String>,
    },
    /// Validate project skills without changing anything
    Check,
    /// Refresh clean GitHub-imported skills; refuse local modifications
    Refresh {
        /// Optional skill name; default refreshes all imported skills
        name: Option<String>,
    },
    /// Inspect the offline home inventory (not agent discovery)
    Inventory {
        #[command(subcommand)]
        command: InventoryCommand,
    },
}

#[derive(Debug, Subcommand)]
pub enum InventoryCommand {
    /// List inventory skills for this project
    List,
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
        } => {
            let report = init::init_project(
                &cwd,
                InitOptions {
                    with_zen: flag_tri(with_zen, no_zen),
                    with_twotink: flag_tri(with_twotink, no_twotink),
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
        Command::Add { source, skill } => {
            add::add_skill(&cwd, &source, skill.as_deref()).map(|_| ())
        }
        Command::Check => {
            let skills = check::check_project(&cwd)?;
            println!("OK {} skill(s)", skills.len());
            Ok(())
        }
        Command::Refresh { name } => match name {
            Some(name) => {
                let changed = refresh::refresh_skill(&cwd, &name)?;
                if changed {
                    println!("Refreshed {name}");
                } else {
                    println!("Unchanged {name}");
                }
                Ok(())
            }
            None => {
                let refreshed = refresh::refresh_all(&cwd)?;
                if refreshed.is_empty() {
                    println!("Unchanged (no imported skills updated)");
                } else {
                    println!("Refreshed {}", refreshed.join(", "));
                }
                Ok(())
            }
        },
        Command::Inventory {
            command: InventoryCommand::List,
        } => inventory_list(&cwd),
    }
}

fn inventory_list(cwd: &std::path::Path) -> Result<(), Error> {
    let (_catalog, skills) = inventory::list_project_skills(cwd)?;
    if skills.is_empty() {
        println!("(no inventory skills for this project)");
        return Ok(());
    }
    for skill in skills {
        println!("{}", skill.name);
    }
    Ok(())
}
