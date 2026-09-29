use std::fs;
use std::path::{Path, PathBuf};

use serde::Serialize;

use crate::approvals;
use crate::error::Error;
use crate::home;
use crate::paths::{canonicalize_beneath, map_io, mkdir_p, refuse_symlink};
use crate::skills::{self, SnapshotRefusal, TreeSnapshot};

/// Version of the `tink mount --json` contract.
pub const CONTRACT_VERSION: u32 = 1;

#[derive(Debug, PartialEq, Eq)]
pub enum MountOutcome {
    Mounted(PathBuf),
}

#[derive(Debug, PartialEq, Eq)]
pub enum UnmountOutcome {
    Unmounted,
    NotMounted,
}

/// A mount refusal with a stable machine-readable code.
#[derive(Debug)]
pub struct MountRefusal {
    pub code: &'static str,
    pub message: String,
}

impl MountRefusal {
    pub(crate) fn new(code: &'static str, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
        }
    }

    /// Trust refusals (exit 2 under `--json`) as opposed to ordinary failures.
    pub fn is_security(&self) -> bool {
        matches!(
            self.code,
            "symlink_refused"
                | "identity_mismatch"
                | "unsafe_tree"
                | "unapproved"
                | "digest_mismatch"
        )
    }
}

impl From<Error> for MountRefusal {
    fn from(error: Error) -> Self {
        Self::new("error", error.to_string())
    }
}

impl From<MountRefusal> for Error {
    fn from(refusal: MountRefusal) -> Self {
        Error::msg(refusal.message)
    }
}

/// A library skill read once, without following links, and checked for identity.
#[derive(Debug)]
pub struct VerifiedSkill {
    pub name: String,
    /// Canonical library directory.
    pub dir: PathBuf,
    snapshot: TreeSnapshot,
}

impl VerifiedSkill {
    pub fn tree_digest(&self) -> String {
        format!("sha256:{}", self.snapshot.digest)
    }

    pub(crate) fn skill_md_text(&self) -> Option<String> {
        self.snapshot
            .files
            .get(Path::new("SKILL.md"))
            .map(|bytes| String::from_utf8_lossy(bytes).into_owned())
    }

    fn files_under(&self, top: &str) -> Vec<String> {
        self.snapshot
            .files
            .keys()
            .filter(|path| path.starts_with(top))
            .map(|path| path.to_string_lossy().replace('\\', "/"))
            .collect()
    }

    pub fn scripts(&self) -> Vec<String> {
        self.files_under("scripts")
    }

    pub fn references(&self) -> Vec<String> {
        self.files_under("references")
    }

    /// SKILL.md followed by every `references/**` file, from the hashed bytes.
    pub fn payload(&self) -> Result<String, MountRefusal> {
        let text = |relative: &Path| -> Result<&str, MountRefusal> {
            let bytes = &self.snapshot.files[relative];
            std::str::from_utf8(bytes).map_err(|_| {
                MountRefusal::new(
                    "invalid_utf8",
                    format!(
                        "Skill '{}' file is not UTF-8 text: {}",
                        self.name,
                        crate::output::display_path(relative)
                    ),
                )
            })
        };
        let mut content = text(Path::new("SKILL.md"))?.to_string();
        for relative in self
            .snapshot
            .files
            .keys()
            .filter(|path| path.starts_with("references"))
        {
            let header = relative.to_string_lossy().replace('\\', "/");
            content.push_str(&format!("\n\n--- {header} ---\n"));
            content.push_str(text(relative)?);
        }
        Ok(content)
    }
}

fn require_valid_skill_name(name: &str) -> Result<(), Error> {
    if !skills::valid_skill_name(name) {
        return Err(Error::msg(format!(
            "Invalid skill name '{name}': invalid syntax or traversal characters not permitted"
        )));
    }
    Ok(())
}

/// Resolve a library skill strictly by directory name and verify its tree:
/// no symlinks anywhere (including the skill dir and SKILL.md), no special
/// files, and frontmatter `name:` (when present) equal to the directory name.
pub fn verify_library_skill(
    library_root: &Path,
    skill_name: &str,
) -> Result<VerifiedSkill, MountRefusal> {
    require_valid_skill_name(skill_name)
        .map_err(|e| MountRefusal::new("invalid_name", e.to_string()))?;
    let not_found = || {
        MountRefusal::new(
            "not_found",
            format!("Skill '{skill_name}' not found in library"),
        )
    };
    let entry = library_root.join(skill_name);
    if entry.is_symlink() {
        return Err(MountRefusal::new(
            "symlink_refused",
            format!(
                "Skill '{skill_name}' not found in library (refusing symlinked library entry: {})",
                crate::output::display_path(&entry)
            ),
        ));
    }
    let dir = canonicalize_beneath(library_root, Path::new(skill_name)).map_err(|_| not_found())?;
    if !dir.is_dir() {
        return Err(not_found());
    }
    let snapshot = match skills::snapshot_tree(&dir)? {
        Ok(snapshot) => snapshot,
        Err(SnapshotRefusal::Symlink(path)) => {
            return Err(MountRefusal::new(
                "symlink_refused",
                format!(
                    "Refusing symlink in skill '{skill_name}': {}",
                    crate::output::display_path(&path)
                ),
            ));
        }
        Err(SnapshotRefusal::Special(path)) => {
            return Err(MountRefusal::new(
                "unsafe_tree",
                format!(
                    "Refusing special file in skill '{skill_name}': {}",
                    crate::output::display_path(&path)
                ),
            ));
        }
    };
    if let Some(bytes) = snapshot.files.get(Path::new("SKILL.md"))
        && let Some(declared) = skills::frontmatter_name(&String::from_utf8_lossy(bytes))
        && declared != skill_name
    {
        return Err(MountRefusal::new(
            "identity_mismatch",
            format!(
                "Skill directory '{skill_name}' declares name \"{}\"; refusing identity mismatch",
                crate::output::escape_untrusted(&declared)
            ),
        ));
    }
    Ok(VerifiedSkill {
        name: skill_name.to_string(),
        dir,
        snapshot,
    })
}

/// Refuse unless the verified tree digest is the approved one.
pub(crate) fn require_approved(
    home_root: &Path,
    skill: &VerifiedSkill,
) -> Result<(), MountRefusal> {
    let approved = approvals::load(home_root)?;
    match approved.get(&skill.name) {
        None => Err(MountRefusal::new(
            "unapproved",
            format!(
                "Skill '{}' is not approved; review it, then run `tink library approve {}`",
                skill.name, skill.name
            ),
        )),
        Some(digest) if *digest != skill.tree_digest() => Err(MountRefusal::new(
            "digest_mismatch",
            format!(
                "Skill '{}' changed since it was approved; review it, then run `tink library approve {}`",
                skill.name, skill.name
            ),
        )),
        Some(_) => Ok(()),
    }
}

fn link_into_active(project_root: &Path, skill_name: &str, src: &Path) -> Result<PathBuf, Error> {
    let active_dir = home::project_active_skills_path(project_root);
    mkdir_p(&active_dir)?;
    refuse_symlink(&active_dir)?;
    if let Some(tink_dir) = active_dir.parent() {
        crate::init::ensure_tink_gitignore(tink_dir)?;
    }

    let target = active_dir.join(skill_name);

    if target.is_dir() && !target.is_symlink() {
        return Err(Error::msg(format!(
            "Refusing to overwrite non-symlink directory: {}",
            target.display()
        )));
    }

    #[cfg(unix)]
    {
        // Stage a fresh link and rename(2) it over the target: symlink -> symlink
        // replacement is atomic, so concurrent readers never see a missing link.
        let staged = active_dir.join(format!(
            ".tmp-{skill_name}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|duration| duration.as_nanos())
                .unwrap_or(0)
        ));
        std::os::unix::fs::symlink(src, &staged).map_err(|e| map_io(&staged, e))?;
        if let Err(error) = fs::rename(&staged, &target) {
            let _ = fs::remove_file(&staged);
            return Err(map_io(&target, error));
        }
    }

    #[cfg(windows)]
    {
        if target.exists() || target.is_symlink() {
            fs::remove_file(&target).map_err(|e| map_io(&target, e))?;
        }
        std::os::windows::fs::symlink_dir(src, &target).map_err(|e| map_io(&target, e))?;
    }

    Ok(target)
}

pub fn mount_skill(
    project_root: &Path,
    skill_name: &str,
    home: Option<&Path>,
) -> Result<MountOutcome, Error> {
    require_valid_skill_name(skill_name)?;

    let (home_root, _) = home::ensure_inventory_root(home)?;
    let library_root = home::skills_library_path(&home_root);

    let skill = verify_library_skill(&library_root, skill_name)?;
    let target = link_into_active(project_root, skill_name, &skill.dir)?;
    Ok(MountOutcome::Mounted(target))
}

#[derive(Debug, Serialize)]
pub struct Payload {
    pub content: String,
    pub chars: usize,
}

/// `tink mount --json` success document (contract version 1).
#[derive(Debug, Serialize)]
pub struct MountReport {
    pub contract_version: u32,
    pub skill: String,
    pub target: Option<String>,
    pub entrypoint: String,
    pub has_scripts: bool,
    pub has_references: bool,
    pub references: Vec<String>,
    pub scripts: Vec<String>,
    pub tree_digest: String,
    pub mounted: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub payload: Option<Payload>,
}

/// Verify a library skill, link it only when it has `scripts/`, and optionally
/// deliver it whole. Payload delivery requires an approved tree digest.
pub fn mount_skill_report(
    project_root: &Path,
    skill_name: &str,
    home: Option<&Path>,
    with_payload: bool,
) -> Result<MountReport, MountRefusal> {
    require_valid_skill_name(skill_name)
        .map_err(|e| MountRefusal::new("invalid_name", e.to_string()))?;
    let (home_root, _) = home::ensure_inventory_root(home)?;
    let library_root = home::skills_library_path(&home_root);

    let skill = verify_library_skill(&library_root, skill_name)?;
    if !skill.snapshot.files.contains_key(Path::new("SKILL.md")) {
        return Err(MountRefusal::new(
            "invalid_skill",
            format!("Skill '{skill_name}' has no regular SKILL.md"),
        ));
    }
    let payload = if with_payload {
        require_approved(&home_root, &skill)?;
        let content = skill.payload()?;
        Some(Payload {
            chars: content.chars().count(),
            content,
        })
    } else {
        None
    };

    let scripts = skill.scripts();
    let references = skill.references();
    let target = if scripts.is_empty() {
        None
    } else {
        Some(
            link_into_active(project_root, skill_name, &skill.dir)
                .map_err(|e| MountRefusal::new("mount_failed", e.to_string()))?,
        )
    };

    Ok(MountReport {
        contract_version: CONTRACT_VERSION,
        skill: skill.name.clone(),
        mounted: target.is_some(),
        target: target.map(|path| path.to_string_lossy().into_owned()),
        entrypoint: skill.dir.join("SKILL.md").to_string_lossy().into_owned(),
        has_scripts: !scripts.is_empty(),
        has_references: !references.is_empty(),
        references,
        scripts,
        tree_digest: skill.tree_digest(),
        payload,
    })
}

/// Approve library skills by recording their current verified tree digest.
/// Returns `(name, digest)` pairs that were recorded.
pub fn approve_library_skills(
    home: Option<&Path>,
    names: &[String],
) -> Result<Vec<(String, String)>, MountRefusal> {
    let (home_root, _) = home::ensure_inventory_root(home)?;
    let library_root = home::skills_library_path(&home_root);
    let mut approved = Vec::new();
    for name in names {
        let skill = verify_library_skill(&library_root, name)?;
        approved.push((skill.name.clone(), skill.tree_digest()));
    }
    approvals::record(&home_root, &approved)?;
    Ok(approved)
}

/// Approve-on-write: record the digest of a library skill tink just wrote.
/// A tree that fails verification is left unapproved (mount refuses it anyway).
pub(crate) fn record_written_library_skill(home: Option<&Path>, name: &str) -> Result<(), Error> {
    let (home_root, _) = home::ensure_inventory_root(home)?;
    let library_root = home::skills_library_path(&home_root);
    if let Ok(skill) = verify_library_skill(&library_root, name) {
        approvals::record(&home_root, &[(skill.name.clone(), skill.tree_digest())])?;
    }
    Ok(())
}

pub fn unmount_skill(project_root: &Path, skill_name: &str) -> Result<UnmountOutcome, Error> {
    require_valid_skill_name(skill_name)?;

    let target = home::project_active_skills_path(project_root).join(skill_name);
    if !target.exists() && !target.is_symlink() {
        return Ok(UnmountOutcome::NotMounted);
    }

    if target.is_dir() && !target.is_symlink() {
        return Err(Error::msg(format!(
            "Refusing to remove non-symlink directory: {}",
            target.display()
        )));
    }

    fs::remove_file(&target).map_err(|e| map_io(&target, e))?;

    Ok(UnmountOutcome::Unmounted)
}
