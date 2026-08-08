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

/// Drop this project's by-project catalog entry, then remove `.agents/`,
/// `ZEN.md`, and `AGENTS.md` from the project root. Does not touch the home
/// library. Catalog sync runs before disk deletes; sync errors leave project
/// files intact. Refuses symlinks. Requires `--yes` or an interactive `y`
/// confirmation (default no).
pub fn destroy_project(project_root: &Path, yes: bool) -> Result<DestroyReport, Error> {
    if !yes {
        confirm_destroy()?;
    }

    let mut to_remove = Vec::new();
    let agents = crate::home::project_agents_path(project_root);
    if agents.exists() || agents.is_symlink() {
        refuse_symlink(&agents)?;
        if !agents.is_dir() {
            return Err(Error::msg(format!(
                "Refusing to remove non-directory: {}",
                agents.display()
            )));
        }
        to_remove.push(agents);
    }

    for name in ["ZEN.md", "AGENTS.md"] {
        let path = project_root.join(name);
        if !path.exists() && !path.is_symlink() {
            continue;
        }
        refuse_symlink(&path)?;
        if !path.is_file() {
            return Err(Error::msg(format!(
                "Refusing to remove non-file: {}",
                path.display()
            )));
        }
        to_remove.push(path);
    }

    catalog::forget_project(project_root)?;

    let mut removed = Vec::new();
    for path in to_remove {
        if path.is_dir() {
            fs::remove_dir_all(&path).map_err(|e| map_io(&path, e))?;
        } else {
            fs::remove_file(&path).map_err(|e| map_io(&path, e))?;
        }
        removed.push(path);
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
        style.warn("Delete .agents/, ZEN.md, and AGENTS.md in this project?"),
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
