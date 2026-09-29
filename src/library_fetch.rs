//! `tink library fetch <PIN>...`: deposit a reviewed skillset pin's members into
//! the standalone library.
//!
//! A pin (a committed project file under `.tink/skillsets/` or a home pin) is the
//! trust anchor: its `source` + full `revision` name exactly which bytes were
//! reviewed. Fetch clones the source, checks out that revision, verifies every
//! member (regular files only, identity, no divergence from the library), and only
//! after ALL members of ALL pins pass does it write. Writes go through the same
//! [`library::deposit_at`] path as `skill add`, so `.tink-source.json` provenance
//! and approve-on-write digests behave identically. tink never writes pins.

use std::collections::{BTreeMap, HashMap};
use std::fs;
use std::io;
use std::path::{Component, Path, PathBuf};

use serde::Serialize;
use tempfile::TempDir;

use crate::git;
use crate::home;
use crate::library;
use crate::mount::{self, MountRefusal};
use crate::output::display_path;
use crate::provenance::Provenance;
use crate::skills::{self, PreflightOutcome, Skill, SnapshotRefusal};
use crate::skillsets::{self, SkillsetMeta};

pub const CONTRACT_VERSION: u32 = 1;

#[derive(Debug, Serialize)]
pub struct SkillReport {
    pub name: String,
    pub status: &'static str,
    pub tree_digest: String,
}

#[derive(Debug, Serialize)]
pub struct PinReport {
    pub pin: String,
    pub source: String,
    pub revision: String,
    pub skills: Vec<SkillReport>,
}

#[derive(Debug, Serialize)]
pub struct Report {
    pub contract_version: u32,
    pub pins: Vec<PinReport>,
}

fn refuse(code: &'static str, message: impl Into<String>) -> MountRefusal {
    MountRefusal::new(code, message)
}

/// Exit code for a refusal: listed codes are trust refusals (2), plain I/O is 1.
pub fn exit_code(refusal: &MountRefusal) -> u8 {
    if refusal.code == "error" { 1 } else { 2 }
}

struct PinFile {
    path: PathBuf,
    meta: SkillsetMeta,
}

fn looks_like_path(arg: &str) -> bool {
    arg.contains('/') || arg.contains('\\') || arg.ends_with(".json")
}

fn load_pin(cwd: &Path, home_root: &Path, arg: &str) -> Result<PinFile, MountRefusal> {
    let path = if looks_like_path(arg) {
        let path = Path::new(arg);
        let path = if path.is_absolute() {
            path.to_path_buf()
        } else {
            cwd.join(path)
        };
        if !path.exists() && !path.is_symlink() {
            return Err(refuse(
                "pin_not_found",
                format!("Pin file not found: {}", display_path(&path)),
            ));
        }
        path
    } else {
        match skillsets::find_pin(cwd, Some(home_root), arg)
            .map_err(|e| refuse("pin_invalid", e.to_string()))?
        {
            Some(path) => path,
            None => {
                let canonical = skillsets::canonicalize_skillset_name(arg)
                    .map_err(|e| refuse("pin_invalid", e.to_string()))?;
                return Err(refuse(
                    "pin_not_found",
                    format!(
                        "Skillset pin {canonical} not found: looked for {} and {}",
                        display_path(
                            &home::project_skillset_pins_path(cwd)
                                .join(format!("{canonical}.json"))
                        ),
                        display_path(&home::skillset_pin_path(home_root, &canonical))
                    ),
                ));
            }
        }
    };
    let meta = skillsets::read_pin_file(&path).map_err(|e| refuse("pin_invalid", e.to_string()))?;
    // The `.tink-source.json` provenance contract only admits canonical GitHub HTTPS
    // URLs; a deposit with any other source would break later project reads.
    let canonical = crate::sources::parse_remote(&meta.source)
        .map(|remote| remote.url == meta.source)
        .unwrap_or(false);
    if !canonical {
        return Err(refuse(
            "pin_invalid",
            format!(
                "Pin source must be a canonical public GitHub HTTPS URL (https://github.com/<owner>/<repo>.git) for `library fetch` (got {:?} in {})",
                display_path(Path::new(&meta.source)),
                display_path(&path)
            ),
        ));
    }
    Ok(PinFile { path, meta })
}

/// Walk `relative` below `base` one component at a time without following links.
fn resolve_beneath(base: &Path, relative: &Path, what: &str) -> Result<PathBuf, MountRefusal> {
    let mut current = base.to_path_buf();
    for component in relative.components() {
        match component {
            Component::Normal(part) => current.push(part),
            Component::CurDir => continue,
            _ => {
                return Err(refuse(
                    "member_not_found",
                    format!("{what} escapes the repository"),
                ));
            }
        }
        match fs::symlink_metadata(&current) {
            Ok(meta) if meta.file_type().is_symlink() => {
                return Err(refuse(
                    "symlink_refused",
                    format!("Refusing symlink in {what}: {}", display_path(&current)),
                ));
            }
            Ok(meta) if meta.is_dir() => {}
            Ok(_) => {
                return Err(refuse(
                    "member_not_found",
                    format!("{what} is not a directory: {}", display_path(&current)),
                ));
            }
            Err(e) if e.kind() == io::ErrorKind::NotFound => {
                return Err(refuse(
                    "member_not_found",
                    format!("{what} not found in the pinned revision"),
                ));
            }
            Err(e) => return Err(refuse("error", format!("{}: {e}", display_path(&current)))),
        }
    }
    Ok(current)
}

struct Planned {
    skill: Skill,
    provenance: Provenance,
    body_digest: String,
    write: bool,
}

/// Checkouts shared across pins; guards keep the trees alive until writes finish.
#[derive(Default)]
struct Checkouts {
    clones: HashMap<String, (PathBuf, String)>,
    trees: HashMap<(String, String), PathBuf>,
    guards: Vec<TempDir>,
}

impl Checkouts {
    fn tree(&mut self, meta: &SkillsetMeta) -> Result<PathBuf, MountRefusal> {
        let key = (meta.source.clone(), meta.revision.clone());
        if let Some(path) = self.trees.get(&key) {
            return Ok(path.clone());
        }
        if !self.clones.contains_key(&meta.source) {
            let remote = skillsets::parse_pin_source(&meta.source)
                .map_err(|e| refuse("pin_invalid", e.to_string()))?;
            let (guard, repository, tip) =
                git::checkout(&remote).map_err(|e| refuse("clone_failed", e.to_string()))?;
            self.guards.push(guard);
            self.clones.insert(meta.source.clone(), (repository, tip));
        }
        let (repository, tip) = self.clones[&meta.source].clone();
        let path = if tip == meta.revision {
            repository
        } else {
            let (guard, path) = git::checkout_revision(&repository, &meta.revision)
                .map_err(|e| refuse("revision_not_found", e.to_string()))?;
            self.guards.push(guard);
            path
        };
        self.trees.insert(key, path.clone());
        Ok(path)
    }
}

fn plan_member(
    checkout: &Path,
    meta: &SkillsetMeta,
    member: &str,
    library_root: &Path,
) -> Result<Planned, MountRefusal> {
    let source_root =
        skillsets::pin_source_root(meta).map_err(|e| refuse("pin_invalid", e.to_string()))?;
    let relative = source_root.join(member);
    let dir = resolve_beneath(checkout, &relative, &format!("member '{member}'"))?;
    let snapshot = match skills::snapshot_tree(&dir) {
        Ok(Ok(snapshot)) => snapshot,
        Ok(Err(SnapshotRefusal::Symlink(path))) => {
            return Err(refuse(
                "symlink_refused",
                format!(
                    "Refusing symlink in member '{member}': {}",
                    display_path(&path)
                ),
            ));
        }
        Ok(Err(SnapshotRefusal::Special(path))) => {
            return Err(refuse(
                "symlink_refused",
                format!(
                    "Refusing special file in member '{member}': {}",
                    display_path(&path)
                ),
            ));
        }
        Err(e) => return Err(refuse("error", e.to_string())),
    };
    let Some(skill_md) = snapshot.files.get(Path::new("SKILL.md")) else {
        return Err(refuse(
            "member_not_found",
            format!("Member '{member}' has no SKILL.md in the pinned revision"),
        ));
    };
    match skills::frontmatter_name(&String::from_utf8_lossy(skill_md)) {
        Some(declared) if declared == member => {}
        Some(declared) => {
            return Err(refuse(
                "identity_mismatch",
                format!(
                    "Member directory '{member}' declares name \"{}\"; refusing identity mismatch",
                    crate::output::escape_untrusted(&declared)
                ),
            ));
        }
        None => {
            return Err(refuse(
                "identity_mismatch",
                format!("Member '{member}' declares no frontmatter name"),
            ));
        }
    }
    let skill = skills::read_skill(&dir, true)
        .map_err(|e| refuse("identity_mismatch", format!("Member '{member}': {e}")))?;
    let rel_path = relative.to_string_lossy().replace('\\', "/");
    let rel_path = rel_path.strip_prefix("./").unwrap_or(&rel_path).to_string();
    let mut provenance = Provenance::new();
    provenance.insert("source".into(), meta.source.clone());
    provenance.insert("revision".into(), meta.revision.clone());
    provenance.insert("path".into(), rel_path);

    let target = library_root.join(member);
    let divergent = || {
        refuse(
            "library_divergent",
            format!(
                "Library skill '{member}' differs from the pinned tree; refusing to overwrite it. \
To accept the pin, remove or rename {} and re-run `tink library fetch`",
                display_path(&target)
            ),
        )
    };
    if target.is_symlink() || skillsets::has_receipt_entry(&target) {
        return Err(divergent());
    }
    let write = match skills::preflight_install(&skill, library_root, Some(&provenance))
        .map_err(|e| refuse("identity_mismatch", format!("Member '{member}': {e}")))?
    {
        PreflightOutcome::Ready | PreflightOutcome::ReceiptMismatch => true,
        PreflightOutcome::Identical => false,
        PreflightOutcome::Divergent => return Err(divergent()),
    };
    Ok(Planned {
        skill,
        provenance,
        body_digest: snapshot.digest,
        write,
    })
}

pub fn run(cwd: &Path, pins: &[String]) -> Result<Report, MountRefusal> {
    let home_root = home::resolve_home()?;
    let library_root = home::skills_library_path(&home_root);

    // Phase 1: validate everything; nothing is written.
    let mut checkouts = Checkouts::default();
    let mut planned: BTreeMap<String, Planned> = BTreeMap::new();
    let mut layout: Vec<(PinFile, Vec<(String, bool)>)> = Vec::new();
    for arg in pins {
        let pin = load_pin(cwd, &home_root, arg)?;
        let checkout = checkouts.tree(&pin.meta)?;
        let mut members = Vec::new();
        for member in &pin.meta.members {
            let plan = plan_member(&checkout, &pin.meta, member, &library_root)?;
            let first_time = match planned.get(member) {
                None => true,
                Some(prior)
                    if prior.body_digest == plan.body_digest
                        && prior.provenance == plan.provenance =>
                {
                    false
                }
                Some(_) => {
                    return Err(refuse(
                        "pin_invalid",
                        format!(
                            "Pins disagree about member '{member}' (different revision or contents); fetch them separately"
                        ),
                    ));
                }
            };
            let fresh = first_time && plan.write;
            if first_time {
                planned.insert(member.clone(), plan);
            }
            members.push((member.clone(), fresh));
        }
        layout.push((pin, members));
    }

    // Phase 2: every member of every pin verified; deposit through the `skill add` path.
    for plan in planned.values().filter(|plan| plan.write) {
        library::deposit_at(None, &plan.skill, Some(&plan.provenance))?;
    }

    let mut reports = Vec::new();
    for (pin, members) in layout {
        let mut skills = Vec::new();
        for (name, fetched) in members {
            let verified = mount::verify_library_skill(&library_root, &name)?;
            skills.push(SkillReport {
                name,
                status: if fetched { "fetched" } else { "unchanged" },
                tree_digest: verified.tree_digest(),
            });
        }
        reports.push(PinReport {
            pin: pin.path.to_string_lossy().into_owned(),
            source: pin.meta.source,
            revision: pin.meta.revision,
            skills,
        });
    }
    Ok(Report {
        contract_version: CONTRACT_VERSION,
        pins: reports,
    })
}
