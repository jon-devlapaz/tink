//! Skill discovery, validation, and install.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};

use crate::error::Error;
use crate::paths::{map_io, refuse_symlink};
use crate::provenance::{self, Provenance};

#[derive(Debug, Clone)]
pub struct Skill {
    pub name: String,
    pub path: PathBuf,
}

pub fn valid_skill_name(name: &str) -> bool {
    if name.is_empty() || name.len() > 64 {
        return false;
    }
    // Reserved: home used to store the name catalog at skills/by-project/.
    if name == "by-project" {
        return false;
    }
    let mut parts = name.split('-');
    let Some(first) = parts.next() else {
        return false;
    };
    if first.is_empty()
        || !first
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit())
    {
        return false;
    }
    for part in parts {
        if part.is_empty()
            || !part
                .chars()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit())
        {
            return false;
        }
    }
    true
}

fn frontmatter_value(lines: &[&str], key: &str) -> Option<String> {
    let prefix = format!("{key}:");
    for (index, line) in lines.iter().enumerate() {
        if !line.starts_with(&prefix) {
            continue;
        }
        let value = line[prefix.len()..].trim();
        if matches!(value, "|" | ">" | "|-" | ">-" | "|+" | ">+") {
            let mut block = Vec::new();
            for following in &lines[index + 1..] {
                if !following.is_empty() && !following.chars().next().unwrap().is_whitespace() {
                    break;
                }
                let trimmed = following.trim();
                if !trimmed.is_empty() {
                    block.push(trimmed);
                }
            }
            return Some(block.join(" "));
        }
        if value.len() >= 2 {
            let bytes = value.as_bytes();
            let quote = bytes[0];
            if (quote == b'"' || quote == b'\'') && bytes[bytes.len() - 1] == quote {
                return Some(value[1..value.len() - 1].to_string());
            }
        }
        return Some(value.to_string());
    }
    None
}

pub fn read_skill(path: &Path, require_directory_name: bool) -> Result<Skill, Error> {
    refuse_symlink(path)?;
    let skill_file = path.join("SKILL.md");
    refuse_symlink(&skill_file)?;
    if !skill_file.is_file() {
        return Err(Error::msg(format!(
            "Missing regular SKILL.md: {}",
            skill_file.display()
        )));
    }
    let text = fs::read_to_string(&skill_file).map_err(|e| map_io(&skill_file, e))?;
    let lines: Vec<&str> = text.lines().collect();
    if lines.first().copied() != Some("---") {
        return Err(Error::msg(format!(
            "SKILL.md must start with YAML frontmatter: {}",
            skill_file.display()
        )));
    }
    let closing = lines
        .iter()
        .skip(1)
        .position(|line| *line == "---")
        .map(|i| i + 1)
        .ok_or_else(|| {
            Error::msg(format!(
                "SKILL.md frontmatter is not closed: {}",
                skill_file.display()
            ))
        })?;
    let frontmatter = &lines[1..closing];
    let name = frontmatter_value(frontmatter, "name").unwrap_or_default();
    let description = frontmatter_value(frontmatter, "description").unwrap_or_default();
    if !valid_skill_name(&name) {
        return Err(Error::msg(format!(
            "Invalid skill name in {}",
            skill_file.display()
        )));
    }
    if require_directory_name {
        let dir_name = path.file_name().and_then(|s| s.to_str()).unwrap_or("");
        if name != dir_name {
            return Err(Error::msg(format!(
                "Skill name {name:?} must match directory {dir_name:?}"
            )));
        }
    }
    if description.is_empty() || description.len() > 1024 {
        return Err(Error::msg(format!(
            "Invalid skill description in {}",
            skill_file.display()
        )));
    }
    Ok(Skill {
        name,
        path: path.to_path_buf(),
    })
}

pub fn discover(source: &Path) -> Result<Vec<Skill>, Error> {
    let source = source.canonicalize().map_err(|e| map_io(source, e))?;
    if source.join("SKILL.md").exists() {
        return Ok(vec![read_skill(&source, false)?]);
    }
    let skills_root = source.join("skills");
    refuse_symlink(&skills_root)?;
    if !skills_root.is_dir() {
        return Err(Error::msg(format!(
            "No skill found at {}",
            source.display()
        )));
    }
    let mut entries: Vec<_> = fs::read_dir(&skills_root)
        .map_err(|e| map_io(&skills_root, e))?
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.join("SKILL.md").exists())
        .collect();
    entries.sort();
    let mut skills = Vec::new();
    for path in entries {
        skills.push(read_skill(&path, true)?);
    }
    if skills.is_empty() {
        return Err(Error::msg(format!(
            "No skill found at {}",
            source.display()
        )));
    }
    Ok(skills)
}

#[derive(Debug, PartialEq, Eq)]
enum EntryKind {
    Dir,
    File(Vec<u8>),
}

/// One skill-tree walk: skip `.git`, refuse symlinks/specials, optionally load bytes.
enum Collected {
    Tree(BTreeMap<String, EntryKind>),
    Unsupported { path: PathBuf, what: &'static str },
}

fn collect_tree(root: &Path) -> Result<Collected, Error> {
    refuse_symlink(root)?;
    let mut contents = BTreeMap::new();
    fn walk(
        root: &Path,
        dir: &Path,
        contents: &mut BTreeMap<String, EntryKind>,
    ) -> Result<Option<(PathBuf, &'static str)>, Error> {
        for entry in fs::read_dir(dir).map_err(|e| map_io(dir, e))? {
            let entry = entry.map_err(|e| map_io(dir, e))?;
            let path = entry.path();
            let relative = path
                .strip_prefix(root)
                .map_err(|_| Error::msg(format!("path escape: {}", path.display())))?
                .to_string_lossy()
                .replace('\\', "/");
            if relative == ".git" || relative.starts_with(".git/") {
                continue;
            }
            if path.is_symlink() {
                return Ok(Some((path, "symlink")));
            }
            let ft = entry.file_type().map_err(|e| map_io(&path, e))?;
            if ft.is_dir() {
                contents.insert(relative, EntryKind::Dir);
                if let Some(bad) = walk(root, &path, contents)? {
                    return Ok(Some(bad));
                }
            } else if ft.is_file() {
                let bytes = fs::read(&path).map_err(|e| map_io(&path, e))?;
                contents.insert(relative, EntryKind::File(bytes));
            } else {
                return Ok(Some((path, "special file")));
            }
        }
        Ok(None)
    }
    match walk(root, root, &mut contents)? {
        None => Ok(Collected::Tree(contents)),
        Some((path, what)) => Ok(Collected::Unsupported { path, what }),
    }
}

fn require_safe_tree(root: &Path) -> Result<BTreeMap<String, EntryKind>, Error> {
    match collect_tree(root)? {
        Collected::Tree(tree) => Ok(tree),
        Collected::Unsupported { path, what } => Err(Error::msg(format!(
            "Refusing to copy {what}: {}",
            path.display()
        ))),
    }
}

/// Reject symlinks and special files anywhere in a skill tree, except `.git`.
pub fn validate_skill_tree(root: &Path) -> Result<(), Error> {
    require_safe_tree(root).map(|_| ())
}

fn tree_contents(root: &Path) -> Result<Option<BTreeMap<String, EntryKind>>, Error> {
    match collect_tree(root)? {
        Collected::Tree(tree) => Ok(Some(tree)),
        Collected::Unsupported { .. } => Ok(None),
    }
}

fn materialize_tree(tree: &BTreeMap<String, EntryKind>, destination: &Path) -> Result<(), Error> {
    fs::create_dir_all(destination).map_err(|e| map_io(destination, e))?;
    for (relative, kind) in tree {
        let to = destination.join(relative);
        match kind {
            EntryKind::Dir => {
                fs::create_dir_all(&to).map_err(|e| map_io(&to, e))?;
            }
            EntryKind::File(bytes) => {
                if let Some(parent) = to.parent() {
                    fs::create_dir_all(parent).map_err(|e| map_io(parent, e))?;
                }
                fs::write(&to, bytes).map_err(|e| map_io(&to, e))?;
            }
        }
    }
    Ok(())
}

pub fn skill_contents_equal(left: &Path, right: &Path) -> Result<bool, Error> {
    match (tree_contents(left)?, tree_contents(right)?) {
        (Some(a), Some(b)) => Ok(a == b),
        // Unreadable trees (symlinks/specials) are never "equal".
        _ => Ok(false),
    }
}

/// Like [`skill_contents_equal`], but ignore relative paths (e.g. `.tink-source.json`).
pub fn skill_contents_equal_except(
    left: &Path,
    right: &Path,
    ignore: &[&str],
) -> Result<bool, Error> {
    let Some(mut a) = tree_contents(left)? else {
        return Ok(false);
    };
    let Some(mut b) = tree_contents(right)? else {
        return Ok(false);
    };
    for key in ignore {
        a.remove(*key);
        b.remove(*key);
    }
    Ok(a == b)
}

pub fn copy_skill_tree(
    source: &Path,
    destination: &Path,
    root_ignore: &[&str],
) -> Result<(), Error> {
    let mut tree = require_safe_tree(source)?;
    for name in root_ignore {
        tree.remove(*name);
        let prefix = format!("{name}/");
        tree.retain(|key, _| !key.starts_with(&prefix));
    }
    materialize_tree(&tree, destination)
}

/// Compute the canonical digest used by opaque managed skillset trees.
pub fn tree_digest(root: &Path, root_ignore: &[&str]) -> Result<String, Error> {
    let mut tree = require_safe_tree(root)?;
    for name in root_ignore {
        tree.remove(*name);
        let prefix = format!("{name}/");
        tree.retain(|key, _| !key.starts_with(&prefix));
    }

    let mut hasher = Sha256::new();
    for (relative, kind) in tree {
        let path = relative.as_bytes();
        hasher.update((path.len() as u64).to_be_bytes());
        hasher.update(path);
        match kind {
            EntryKind::Dir => hasher.update([b'd']),
            EntryKind::File(bytes) => {
                hasher.update([b'f']);
                hasher.update((bytes.len() as u64).to_be_bytes());
                hasher.update(bytes);
            }
        }
    }
    Ok(format!("{:x}", hasher.finalize()))
}

/// Result of comparing a candidate skill tree to an install target.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PreflightOutcome {
    /// Target missing — install may proceed.
    Ready,
    /// Target already matches (including optional receipt) — noop.
    Identical,
    /// Skill body matches; only `.tink-source.json` presence or bytes differ.
    ReceiptMismatch,
    /// Target exists and body differs — caller decides refuse vs repair.
    Divergent,
}

impl PreflightOutcome {
    /// Project installs refuse body divergence; receipt-only drift may repair.
    pub fn require_compatible(
        self,
        skill_name: &str,
        destination_root: &Path,
    ) -> Result<Self, Error> {
        match self {
            Self::Divergent => Err(Error::msg(format!(
                "Refusing to overwrite existing skill: {}",
                destination_root.join(skill_name).display()
            ))),
            other => Ok(other),
        }
    }
}

fn expected_install_tree(
    skill: &Skill,
    provenance: Option<&Provenance>,
) -> Result<BTreeMap<String, EntryKind>, Error> {
    let mut expected = require_safe_tree(&skill.path)?;
    if let Some(provenance) = provenance {
        if expected.contains_key(provenance::SIDECAR_FILE) {
            return Err(Error::msg(format!(
                "Remote skill already contains reserved .tink-source.json: {}",
                skill.path.display()
            )));
        }
        expected.insert(
            provenance::SIDECAR_FILE.into(),
            EntryKind::File(provenance::to_bytes(provenance)?),
        );
    }
    Ok(expected)
}

fn equal_except_receipt(
    left: &BTreeMap<String, EntryKind>,
    right: &BTreeMap<String, EntryKind>,
) -> bool {
    left.iter()
        .filter(|(key, _)| key.as_str() != provenance::SIDECAR_FILE)
        .eq(right
            .iter()
            .filter(|(key, _)| key.as_str() != provenance::SIDECAR_FILE))
}

/// Align destination receipt to `expected` without rewriting skill body files.
fn repair_receipt(target: &Path, expected: &BTreeMap<String, EntryKind>) -> Result<(), Error> {
    refuse_symlink(target)?;
    let sidecar = target.join(provenance::SIDECAR_FILE);
    match expected.get(provenance::SIDECAR_FILE) {
        Some(EntryKind::File(bytes)) => {
            refuse_symlink(&sidecar)?;
            fs::write(&sidecar, bytes).map_err(|e| map_io(&sidecar, e))?;
        }
        Some(EntryKind::Dir) => {
            return Err(Error::msg(format!(
                "Invalid receipt entry (directory): {}",
                sidecar.display()
            )));
        }
        None => {
            if sidecar.exists() || sidecar.is_symlink() {
                refuse_symlink(&sidecar)?;
                fs::remove_file(&sidecar).map_err(|e| map_io(&sidecar, e))?;
            }
        }
    }
    Ok(())
}

/// Compare candidate skill to `destination_root/<name>` without writing.
pub fn preflight_install(
    skill: &Skill,
    destination_root: &Path,
    provenance: Option<&Provenance>,
) -> Result<PreflightOutcome, Error> {
    let expected = expected_install_tree(skill, provenance)?;
    let target = destination_root.join(&skill.name);
    refuse_symlink(&target)?;
    if !target.exists() && !target.is_symlink() {
        return Ok(PreflightOutcome::Ready);
    }
    if target.is_dir() {
        if let Some(existing) = tree_contents(&target)? {
            if existing == expected {
                return Ok(PreflightOutcome::Identical);
            }
            if equal_except_receipt(&existing, &expected) {
                return Ok(PreflightOutcome::ReceiptMismatch);
            }
        }
    }
    Ok(PreflightOutcome::Divergent)
}

/// Install skill; returns `(installed_path, created)`.
pub fn install_local(
    skill: &Skill,
    destination_root: &Path,
    provenance: Option<&Provenance>,
) -> Result<(PathBuf, bool), Error> {
    let outcome = preflight_install(skill, destination_root, provenance)?
        .require_compatible(&skill.name, destination_root)?;
    let target = destination_root.join(&skill.name);
    match outcome {
        PreflightOutcome::Identical => Ok((target, false)),
        PreflightOutcome::ReceiptMismatch => {
            let expected = expected_install_tree(skill, provenance)?;
            repair_receipt(&target, &expected)?;
            Ok((target, false))
        }
        PreflightOutcome::Ready => {
            let staging = tempfile::Builder::new()
                .prefix(".tink-stage-")
                .tempdir_in(destination_root)
                .map_err(|e| Error::msg(format!("staging dir: {e}")))?;
            let staged = staging.path().join(&skill.name);
            copy_skill_tree(&skill.path, &staged, &[".git"])?;
            if let Some(provenance) = provenance {
                provenance::write_file(&staged.join(provenance::SIDECAR_FILE), provenance)?;
            }
            fs::rename(&staged, &target).map_err(|e| map_io(&target, e))?;
            Ok((target, true))
        }
        PreflightOutcome::Divergent => unreachable!("require_compatible rejects Divergent"),
    }
}

/// Replace an existing imported skill after dirty-tree preflight elsewhere.
pub fn replace_verified(
    skill: &Skill,
    destination_root: &Path,
    provenance: &Provenance,
) -> Result<PathBuf, Error> {
    require_safe_tree(&skill.path)?;
    let target = destination_root.join(&skill.name);
    refuse_symlink(&target)?;
    if !target.is_dir() {
        return Err(Error::msg(format!(
            "Refusing to replace missing or unsafe skill: {}",
            target.display()
        )));
    }
    let staging = tempfile::Builder::new()
        .prefix(".tink-update-")
        .tempdir_in(destination_root)
        .map_err(|e| Error::msg(format!("update staging: {e}")))?;
    let staged = staging.path().join("new");
    let backup = staging.path().join("old");
    copy_skill_tree(&skill.path, &staged, &[".git"])?;
    if staged.join(provenance::SIDECAR_FILE).exists() {
        return Err(Error::msg(format!(
            "Remote skill contains reserved .tink-source.json: {}",
            skill.path.display()
        )));
    }
    provenance::write_file(&staged.join(provenance::SIDECAR_FILE), provenance)?;
    fs::rename(&target, &backup).map_err(|e| map_io(&target, e))?;
    if let Err(err) = fs::rename(&staged, &target) {
        let _ = fs::rename(&backup, &target);
        return Err(map_io(&target, err));
    }
    let _ = fs::remove_dir_all(&backup);
    Ok(target)
}
