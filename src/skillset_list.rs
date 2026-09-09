//! Read-only grouped skillset views.
//!
//! Listing builds on the receipt foundations in [`crate::skillsets`] but never
//! mutates: per-tree validation errors become row `error` values so one
//! divergent skillset cannot blank the rest of the inventory.

use std::fs;
use std::path::Path;

use crate::error::Error;
use crate::home;
use crate::output;
use crate::paths::{map_io, refuse_symlink};
use crate::skillsets::{
    EntryClass, classify_entry, has_receipt_entry, read_installed, read_owned_receipt,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ListedSkillset {
    pub name: String,
    pub members: Vec<String>,
    /// `None` when the tree matches its receipt; otherwise the validation error.
    pub error: Option<String>,
}

fn entry_name(path: &Path) -> String {
    path.file_name()
        .and_then(|value| value.to_str())
        .unwrap_or("")
        .to_string()
}

/// List one receipt-backed root without failing the surrounding inventory walk.
fn list_skillset_entry(path: &Path) -> ListedSkillset {
    match read_installed(path) {
        Ok(installed) => ListedSkillset {
            name: installed.name,
            members: installed.receipt.members,
            error: None,
        },
        Err(error) => {
            let members = read_owned_receipt(path, "installed skillset receipt")
                .map(|receipt| receipt.members)
                .unwrap_or_default();
            ListedSkillset {
                name: entry_name(path),
                members,
                error: Some(error.to_string()),
            }
        }
    }
}

/// List receipt-backed skillsets installed in a project.
///
/// Structural project problems (missing skills root, unexpected entries) still
/// fail the command. Per-tree validation errors are returned as row `error`
/// values so one divergent skillset cannot blank the rest of the inventory.
pub fn list_installed(project_root: &Path) -> Result<Vec<ListedSkillset>, Error> {
    let agents = home::project_agents_path(project_root);
    let skills_root = home::project_skills_path(project_root);
    refuse_symlink(&agents)?;
    refuse_symlink(&skills_root)?;
    if !skills_root.is_dir() {
        return Err(Error::msg(
            "Not a Tink project (missing .agents/skills); run `tink init` first",
        ));
    }

    let mut entries: Vec<_> = fs::read_dir(&skills_root)
        .map_err(|e| map_io(&skills_root, e))?
        .map(|entry| {
            entry
                .map(|entry| entry.path())
                .map_err(|e| map_io(&skills_root, e))
        })
        .collect::<Result<_, _>>()?;
    entries.sort();

    let mut skillsets = Vec::new();
    for path in entries {
        let name = entry_name(&path);
        match classify_entry(&path) {
            EntryClass::Ignored => continue,
            EntryClass::Unexpected => {
                return Err(Error::msg(format!(
                    "Unexpected entry in .agents/skills: {name}"
                )));
            }
            EntryClass::Skillset => {
                skillsets.push(list_skillset_entry(&path));
            }
            EntryClass::Standalone => {}
        }
    }
    skillsets.sort_by(|left, right| left.name.cmp(&right.name));
    Ok(skillsets)
}

/// List receipt-backed skillsets in the home library without creating it.
pub fn list_library(home_root: Option<&Path>) -> Result<Vec<ListedSkillset>, Error> {
    let home = match home_root {
        Some(path) => path.to_path_buf(),
        None => home::resolve_home()?,
    };
    if !home.exists() {
        return Ok(Vec::new());
    }
    refuse_symlink(&home)?;
    let library = home::skillsets_library_path(&home);
    if !library.exists() {
        return Ok(Vec::new());
    }
    refuse_symlink(&library)?;
    if !library.is_dir() {
        return Err(Error::msg(format!(
            "Refusing to read non-directory skillsets library: {}",
            output::display_path(&library)
        )));
    }

    let mut entries: Vec<_> = fs::read_dir(&library)
        .map_err(|e| map_io(&library, e))?
        .map(|entry| {
            entry
                .map(|entry| entry.path())
                .map_err(|e| map_io(&library, e))
        })
        .collect::<Result<_, _>>()?;
    entries.sort();
    let mut skillsets = Vec::new();
    for path in entries {
        if path.is_symlink() || !path.is_dir() {
            continue;
        }
        if has_receipt_entry(&path) {
            skillsets.push(list_skillset_entry(&path));
        }
    }
    Ok(skillsets)
}
