//! `tink skill harvest` — copy harness skill trees into the home stash.
//!
//! Create-only: never repair divergent or receipt-backed stash entries.
//! Stash remains offline inventory, not an agent discovery root.
//!
//! Root tables track documented Agent Skills (`SKILL.md`) locations from:
//! - agentskills.io client guidance (`.agents/skills` + client-specific)
//! - Cursor docs (`.agents`/`.cursor` + Claude/Codex compat)
//! - OpenAI Codex docs (`~/.agents`, `.agents`, `/etc/codex` skipped here)
//! - mdskills.ai install paths (Claude, Cursor, Copilot, Codex, Gemini)
//! - Windsurf/Devin, Cline, Gemini CLI, and community agent path tables

use std::collections::BTreeSet;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};

use crate::error::Error;
use crate::home::{ensure_inventory_root, skills_stash_path};
use crate::paths::{map_io, refuse_symlink};
use crate::skills;
use crate::stash::{self, CreateOnlyWrite};

/// Relative skill roots under `$HOME` (user/global harness locations).
///
/// Sorted for stable discovery order. Missing dirs are skipped.
const GLOBAL_SKILL_ROOTS: &[&str] = &[
    // Cross-agent / Codex user scope
    ".agents/skills",
    // Claude Code
    ".claude/skills",
    // Codex (also used; Cursor loads these for compat)
    ".codex/skills",
    // Cursor
    ".cursor/skills",
    ".cursor/skills-cursor",
    // GitHub Copilot (VS Code)
    ".copilot/skills",
    ".github/skills",
    // Windsurf / Cascade
    ".codeium/windsurf/skills",
    // Cline
    ".cline/skills",
    // Aider
    ".aider/skills",
    // Gemini CLI + Antigravity
    ".gemini/skills",
    ".gemini/antigravity/skills",
    ".gemini/antigravity/global_skills",
    // Roo / Kilo
    ".roo/skills",
    ".kilocode/skills",
    // Amazon Q, Augment, Tabnine, Sourcegraph
    ".amazonq/skills",
    ".augment/skills",
    ".tabnine/skills",
    ".sourcegraph/skills",
    // OpenCode (XDG-style)
    ".config/opencode/skills",
];

/// Relative skill roots under the current project (cwd).
const PROJECT_SKILL_ROOTS: &[&str] = &[
    // Cross-agent / Codex repo scope
    ".agents/skills",
    // Claude / Cursor / Codex project
    ".claude/skills",
    ".codex/skills",
    ".cursor/skills",
    // Copilot
    ".github/skills",
    // Windsurf
    ".windsurf/skills",
    // Cline (+ legacy)
    ".cline/skills",
    ".clinerules/skills",
    // Aider
    ".aider/skills",
    // Gemini CLI + Antigravity (note singular `.agent`)
    ".gemini/skills",
    ".agent/skills",
    // Roo / Kilo
    ".roo/skills",
    ".kilocode/skills",
    // Amazon Q, Augment, Tabnine, OpenCode, Sourcegraph
    ".amazonq/skills",
    ".augment/skills",
    ".tabnine/skills",
    ".opencode/skills",
    ".sourcegraph/skills",
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HarvestAction {
    Created,
    Unchanged,
    Skipped,
}

#[derive(Debug, Clone)]
pub struct HarvestEvent {
    pub name: String,
    pub source: PathBuf,
    pub action: HarvestAction,
    pub detail: Option<String>,
}

#[derive(Debug, Default)]
pub struct HarvestReport {
    pub events: Vec<HarvestEvent>,
    pub created: usize,
    pub unchanged: usize,
    pub skipped: usize,
}

fn user_home() -> Result<PathBuf, Error> {
    let home = env::var_os("HOME").ok_or_else(|| Error::msg("HOME is not set"))?;
    Ok(PathBuf::from(home))
}

fn is_under(path: &Path, ancestor: &Path) -> bool {
    path.starts_with(ancestor)
}

fn resolve_skill_root(path: &Path) -> Result<Option<PathBuf>, Error> {
    if path.is_symlink() {
        let target = fs::read_link(path).map_err(|e| map_io(path, e))?;
        let resolved = if target.is_absolute() {
            target
        } else {
            path.parent()
                .unwrap_or_else(|| Path::new("."))
                .join(target)
        };
        let canonical = match resolved.canonicalize() {
            Ok(p) => p,
            Err(_) => return Ok(None),
        };
        if canonical.join("SKILL.md").is_file() {
            return Ok(Some(canonical));
        }
        return Ok(None);
    }
    if path.is_dir() && path.join("SKILL.md").is_file() {
        match path.canonicalize() {
            Ok(p) => Ok(Some(p)),
            Err(_) => Ok(None),
        }
    } else {
        Ok(None)
    }
}

/// Recursively find skill directories (dirs containing `SKILL.md`) under `root`.
fn find_skills_under(root: &Path, out: &mut Vec<PathBuf>) -> Result<(), Error> {
    if !root.exists() {
        return Ok(());
    }
    let root = if root.is_symlink() {
        match root.canonicalize() {
            Ok(p) if p.is_dir() => p,
            _ => return Ok(()),
        }
    } else {
        refuse_symlink(root)?;
        if !root.is_dir() {
            return Ok(());
        }
        root.to_path_buf()
    };

    fn walk(dir: &Path, out: &mut Vec<PathBuf>) -> Result<(), Error> {
        for entry in fs::read_dir(dir).map_err(|e| map_io(dir, e))? {
            let entry = entry.map_err(|e| map_io(dir, e))?;
            let name = entry.file_name();
            let name = name.to_string_lossy();
            if name.starts_with('.') {
                continue;
            }
            let path = entry.path();
            if path.is_symlink() {
                if let Some(resolved) = resolve_skill_root(&path)? {
                    out.push(resolved);
                }
                continue;
            }
            if !path.is_dir() {
                continue;
            }
            if path.join("SKILL.md").is_file() {
                if let Some(resolved) = resolve_skill_root(&path)? {
                    out.push(resolved);
                }
                // Skill trees are leaves for discovery — do not nest-scan.
                continue;
            }
            walk(&path, out)?;
        }
        Ok(())
    }

    // Root itself may be a single skill directory.
    if root.join("SKILL.md").is_file() {
        if let Some(resolved) = resolve_skill_root(&root)? {
            out.push(resolved);
        }
        return Ok(());
    }
    walk(&root, out)
}

fn candidate_roots(cwd: &Path) -> Result<Vec<PathBuf>, Error> {
    let home = user_home()?;
    let mut roots = Vec::new();
    for rel in GLOBAL_SKILL_ROOTS {
        roots.push(home.join(rel));
    }
    for rel in PROJECT_SKILL_ROOTS {
        roots.push(cwd.join(rel));
    }
    Ok(roots)
}

fn map_create_only(write: CreateOnlyWrite) -> (HarvestAction, Option<String>) {
    match write {
        CreateOnlyWrite::Created => (HarvestAction::Created, None),
        CreateOnlyWrite::Unchanged => (HarvestAction::Unchanged, None),
        CreateOnlyWrite::Skipped(detail) => (HarvestAction::Skipped, detail),
    }
}

/// Scan harness skill locations and create missing trees in the home stash.
pub fn harvest(cwd: &Path) -> Result<HarvestReport, Error> {
    let (tink_home, _) = ensure_inventory_root(None)?;
    let stash = skills_stash_path(&tink_home);
    let stash_canon = stash.canonicalize().unwrap_or_else(|_| stash.clone());

    let mut skill_paths = Vec::new();
    for root in candidate_roots(cwd)? {
        find_skills_under(&root, &mut skill_paths)?;
    }
    skill_paths.sort();
    skill_paths.dedup();

    let mut seen_paths: BTreeSet<PathBuf> = BTreeSet::new();
    let mut claimed_names: BTreeSet<String> = BTreeSet::new();
    let mut report = HarvestReport::default();

    for path in skill_paths {
        // Skip only the stash inventory — not the whole TINK_HOME tree
        // (a workspace nested under TINK_HOME remains harvestable).
        if is_under(&path, &stash_canon) || is_under(&path, &stash) {
            report.events.push(HarvestEvent {
                name: path
                    .file_name()
                    .map(|s| s.to_string_lossy().into_owned())
                    .unwrap_or_else(|| path.display().to_string()),
                source: path,
                action: HarvestAction::Skipped,
                detail: Some("under home stash".into()),
            });
            report.skipped += 1;
            continue;
        }
        if !seen_paths.insert(path.clone()) {
            continue;
        }

        let skill = match skills::read_skill(&path, true) {
            Ok(skill) => skill,
            Err(err) => {
                report.events.push(HarvestEvent {
                    name: path
                        .file_name()
                        .map(|s| s.to_string_lossy().into_owned())
                        .unwrap_or_else(|| "unknown".into()),
                    source: path,
                    action: HarvestAction::Skipped,
                    detail: Some(err.to_string()),
                });
                report.skipped += 1;
                continue;
            }
        };

        if !skills::valid_skill_name(&skill.name) {
            report.events.push(HarvestEvent {
                name: skill.name,
                source: path,
                action: HarvestAction::Skipped,
                detail: Some("invalid or reserved name".into()),
            });
            report.skipped += 1;
            continue;
        }

        if claimed_names.contains(&skill.name) {
            report.events.push(HarvestEvent {
                name: skill.name,
                source: path,
                action: HarvestAction::Skipped,
                detail: Some("name already harvested".into()),
            });
            report.skipped += 1;
            continue;
        }

        let (_, write) = stash::deposit_create_only(&skill)?;
        let (action, detail) = map_create_only(write);
        match action {
            HarvestAction::Created => {
                claimed_names.insert(skill.name.clone());
                report.created += 1;
            }
            HarvestAction::Unchanged => {
                claimed_names.insert(skill.name.clone());
                report.unchanged += 1;
            }
            HarvestAction::Skipped => {
                report.skipped += 1;
            }
        }
        report.events.push(HarvestEvent {
            name: skill.name,
            source: path,
            action,
            detail,
        });
    }

    Ok(report)
}
