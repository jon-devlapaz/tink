//! Skill discovery, validation, and install.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};

use crate::error::Error;
use crate::output;
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
            output::display_path(&skill_file)
        )));
    }
    let text = fs::read_to_string(&skill_file).map_err(|e| map_io(&skill_file, e))?;
    let lines: Vec<&str> = text.lines().collect();
    if lines.first().copied() != Some("---") {
        return Err(Error::msg(format!(
            "SKILL.md must start with YAML frontmatter: {}",
            output::display_path(&skill_file)
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
                output::display_path(&skill_file)
            ))
        })?;
    let frontmatter = &lines[1..closing];
    let name = frontmatter_value(frontmatter, "name").unwrap_or_default();
    let description = frontmatter_value(frontmatter, "description").unwrap_or_default();
    if !valid_skill_name(&name) {
        return Err(Error::msg(format!(
            "Invalid skill name in {}",
            output::display_path(&skill_file)
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
            output::display_path(&skill_file)
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
            output::display_path(&source)
        )));
    }
    let mut entries: Vec<_> = fs::read_dir(&skills_root)
        .map_err(|e| map_io(&skills_root, e))?
        .map(|entry| {
            entry
                .map(|entry| entry.path())
                .map_err(|e| map_io(&skills_root, e))
        })
        .collect::<Result<_, _>>()?;
    entries.retain(|path| path.join("SKILL.md").exists());
    entries.sort();
    let mut skills = Vec::new();
    for path in entries {
        skills.push(read_skill(&path, true)?);
    }
    if skills.is_empty() {
        return Err(Error::msg(format!(
            "No skill found at {}",
            output::display_path(&source)
        )));
    }
    Ok(skills)
}

#[derive(Debug)]
pub struct InvalidSkill {
    pub path: PathBuf,
    pub detail: String,
}

#[derive(Debug)]
pub struct RecursiveDiscovery {
    pub skills: Vec<Skill>,
    pub invalid: Vec<InvalidSkill>,
}

/// Find every valid skill below `source` without following symlink directories.
pub fn discover_recursive(source: &Path) -> Result<RecursiveDiscovery, Error> {
    let source = source.canonicalize().map_err(|e| map_io(source, e))?;
    let mut discovery = RecursiveDiscovery {
        skills: Vec::new(),
        invalid: Vec::new(),
    };

    fn walk(directory: &Path, discovery: &mut RecursiveDiscovery) -> Result<(), Error> {
        let skill_file = directory.join("SKILL.md");
        match fs::symlink_metadata(&skill_file) {
            Ok(metadata) if metadata.file_type().is_symlink() => {}
            Ok(metadata) if metadata.is_file() => match read_skill(directory, false) {
                Ok(skill) => discovery.skills.push(skill),
                Err(error) => discovery.invalid.push(InvalidSkill {
                    path: directory.to_path_buf(),
                    detail: error.to_string(),
                }),
            },
            Ok(_) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(map_io(&skill_file, error)),
        }

        let mut children = Vec::new();
        for entry in fs::read_dir(directory).map_err(|error| map_io(directory, error))? {
            let entry = entry.map_err(|error| map_io(directory, error))?;
            let path = entry.path();
            if path.file_name().and_then(|name| name.to_str()) == Some(".git") {
                continue;
            }
            let metadata = fs::symlink_metadata(&path).map_err(|error| map_io(&path, error))?;
            if metadata.file_type().is_symlink() || !metadata.is_dir() {
                continue;
            }
            children.push(path);
        }
        children.sort();
        for child in children {
            walk(&child, discovery)?;
        }
        Ok(())
    }

    walk(&source, &mut discovery)?;
    Ok(discovery)
}

#[derive(Debug, PartialEq, Eq)]
enum EntryKind {
    Dir,
    File { bytes: Vec<u8>, mode: u32 },
}

/// One skill-tree walk: skip `.git`, refuse symlinks/specials, optionally load bytes.
enum Collected {
    Tree(BTreeMap<PathBuf, EntryKind>),
    Unsupported { path: PathBuf, what: &'static str },
}

#[cfg(unix)]
fn regular_file_mode(metadata: &fs::Metadata) -> u32 {
    use std::os::unix::fs::PermissionsExt;

    if metadata.permissions().mode() & 0o111 == 0 {
        0o644
    } else {
        0o755
    }
}

#[cfg(not(unix))]
fn regular_file_mode(_metadata: &fs::Metadata) -> u32 {
    0
}

#[cfg(unix)]
fn default_file_mode() -> u32 {
    0o644
}

#[cfg(not(unix))]
fn default_file_mode() -> u32 {
    0
}

/// Encode version-2 paths without collapsing legal Unix filename bytes.
/// Windows is not a claimed platform, but normalizing its separator keeps the
/// encoding well-defined for callers that compile there.
pub(crate) fn digest_path_bytes(relative: &Path) -> Vec<u8> {
    #[cfg(unix)]
    {
        use std::os::unix::ffi::OsStrExt;

        relative.as_os_str().as_bytes().to_vec()
    }
    #[cfg(not(unix))]
    {
        relative.to_string_lossy().replace('\\', "/").into_bytes()
    }
}

/// Preserve the pre-version-2 encoding exactly for receipt migration.
fn legacy_digest_path_bytes(relative: &Path) -> Vec<u8> {
    if let Some(relative) = relative.to_str() {
        return relative.replace('\\', "/").into_bytes();
    }
    #[cfg(unix)]
    {
        use std::os::unix::ffi::OsStrExt;

        relative.as_os_str().as_bytes().to_vec()
    }
    #[cfg(not(unix))]
    {
        relative.to_string_lossy().replace('\\', "/").into_bytes()
    }
}

fn collect_tree(root: &Path) -> Result<Collected, Error> {
    refuse_symlink(root)?;
    let mut contents = BTreeMap::new();
    fn walk(
        root: &Path,
        dir: &Path,
        contents: &mut BTreeMap<PathBuf, EntryKind>,
    ) -> Result<Option<(PathBuf, &'static str)>, Error> {
        for entry in fs::read_dir(dir).map_err(|e| map_io(dir, e))? {
            let entry = entry.map_err(|e| map_io(dir, e))?;
            let path = entry.path();
            let relative = path
                .strip_prefix(root)
                .map_err(|_| Error::msg(format!("path escape: {}", output::display_path(&path))))?
                .to_path_buf();
            if relative == Path::new(".git") || relative.starts_with(Path::new(".git")) {
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
                let metadata = entry.metadata().map_err(|e| map_io(&path, e))?;
                contents.insert(
                    relative,
                    EntryKind::File {
                        bytes,
                        mode: regular_file_mode(&metadata),
                    },
                );
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

fn require_safe_tree(root: &Path) -> Result<BTreeMap<PathBuf, EntryKind>, Error> {
    match collect_tree(root)? {
        Collected::Tree(tree) => Ok(tree),
        Collected::Unsupported { path, what } => Err(Error::msg(format!(
            "Refusing to copy {what}: {}",
            output::display_path(&path)
        ))),
    }
}

fn validate_tree_structure(root: &Path) -> Result<Option<(PathBuf, &'static str)>, Error> {
    refuse_symlink(root)?;
    fn walk(root: &Path, dir: &Path) -> Result<Option<(PathBuf, &'static str)>, Error> {
        for entry in fs::read_dir(dir).map_err(|e| map_io(dir, e))? {
            let entry = entry.map_err(|e| map_io(dir, e))?;
            let path = entry.path();
            let relative = path
                .strip_prefix(root)
                .map_err(|_| Error::msg(format!("path escape: {}", output::display_path(&path))))?;
            if relative == Path::new(".git") || relative.starts_with(".git") {
                continue;
            }
            if path.is_symlink() {
                return Ok(Some((path, "symlink")));
            }
            let ft = entry.file_type().map_err(|e| map_io(&path, e))?;
            if ft.is_dir() {
                if let Some(bad) = walk(root, &path)? {
                    return Ok(Some(bad));
                }
            } else if !ft.is_file() {
                return Ok(Some((path, "special file")));
            }
        }
        Ok(None)
    }
    walk(root, root)
}

/// Reject symlinks and special files anywhere in a skill tree, except `.git`.
pub fn validate_skill_tree(root: &Path) -> Result<(), Error> {
    match validate_tree_structure(root)? {
        None => Ok(()),
        Some((path, what)) => Err(Error::msg(format!(
            "Refusing to copy {what}: {}",
            output::display_path(&path)
        ))),
    }
}

fn tree_contents(root: &Path) -> Result<Option<BTreeMap<PathBuf, EntryKind>>, Error> {
    match collect_tree(root)? {
        Collected::Tree(tree) => Ok(Some(tree)),
        Collected::Unsupported { .. } => Ok(None),
    }
}

fn materialize_tree(tree: &BTreeMap<PathBuf, EntryKind>, destination: &Path) -> Result<(), Error> {
    fs::create_dir_all(destination).map_err(|e| map_io(destination, e))?;
    for (relative, kind) in tree {
        let to = destination.join(relative);
        match kind {
            EntryKind::Dir => {
                fs::create_dir_all(&to).map_err(|e| map_io(&to, e))?;
            }
            EntryKind::File { bytes, mode } => {
                if let Some(parent) = to.parent() {
                    fs::create_dir_all(parent).map_err(|e| map_io(parent, e))?;
                }
                fs::write(&to, bytes).map_err(|e| map_io(&to, e))?;
                #[cfg(unix)]
                {
                    use std::os::unix::fs::PermissionsExt;

                    fs::set_permissions(&to, fs::Permissions::from_mode(*mode & 0o777))
                        .map_err(|e| map_io(&to, e))?;
                }
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
        a.remove(Path::new(key));
        b.remove(Path::new(key));
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
        let ignored = Path::new(name);
        tree.retain(|key, _| key != ignored && !key.starts_with(ignored));
    }
    materialize_tree(&tree, destination)
}

fn digest_tree(root: &Path, root_ignore: &[&str], include_mode: bool) -> Result<String, Error> {
    let mut tree = require_safe_tree(root)?;
    for name in root_ignore {
        let ignored = Path::new(name);
        tree.retain(|key, _| key != ignored && !key.starts_with(ignored));
    }

    let mut hasher = Sha256::new();
    if include_mode {
        hasher.update(b"tink-tree-digest-v2\0");
    }
    for (relative, kind) in tree {
        let path = if include_mode {
            digest_path_bytes(&relative)
        } else {
            legacy_digest_path_bytes(&relative)
        };
        hasher.update((path.len() as u64).to_be_bytes());
        hasher.update(&path);
        match kind {
            EntryKind::Dir => hasher.update([b'd']),
            EntryKind::File { bytes, mode } => {
                hasher.update([b'f']);
                if include_mode {
                    hasher.update(mode.to_be_bytes());
                }
                hasher.update((bytes.len() as u64).to_be_bytes());
                hasher.update(bytes);
            }
        }
    }
    Ok(format!("{:x}", hasher.finalize()))
}

/// Version-2 digest for opaque managed trees. Fields are unambiguously framed,
/// and each regular file's canonical executable/non-executable mode is part of
/// the integrity contract.
pub fn tree_digest(root: &Path, root_ignore: &[&str]) -> Result<String, Error> {
    digest_tree(root, root_ignore, true)
}

/// Pre-version-2 digest retained only to validate and migrate old receipts.
/// New persisted state must use [`tree_digest`].
pub(crate) fn tree_digest_legacy(root: &Path, root_ignore: &[&str]) -> Result<String, Error> {
    digest_tree(root, root_ignore, false)
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
                output::display_path(&destination_root.join(skill_name))
            ))),
            other => Ok(other),
        }
    }
}

fn expected_install_tree(
    skill: &Skill,
    provenance: Option<&Provenance>,
) -> Result<BTreeMap<PathBuf, EntryKind>, Error> {
    let mut expected = require_safe_tree(&skill.path)?;
    if let Some(provenance) = provenance {
        if expected.contains_key(Path::new(provenance::SIDECAR_FILE)) {
            return Err(Error::msg(format!(
                "Remote skill already contains reserved .tink-source.json: {}",
                output::display_path(&skill.path)
            )));
        }
        expected.insert(
            PathBuf::from(provenance::SIDECAR_FILE),
            EntryKind::File {
                bytes: provenance::to_bytes(provenance)?,
                mode: default_file_mode(),
            },
        );
    }
    Ok(expected)
}

fn equal_except_receipt(
    left: &BTreeMap<PathBuf, EntryKind>,
    right: &BTreeMap<PathBuf, EntryKind>,
) -> bool {
    left.iter()
        .filter(|(key, _)| key.as_path() != Path::new(provenance::SIDECAR_FILE))
        .eq(right
            .iter()
            .filter(|(key, _)| key.as_path() != Path::new(provenance::SIDECAR_FILE)))
}

/// Align destination receipt to `expected` without rewriting skill body files.
fn repair_receipt(target: &Path, expected: &BTreeMap<PathBuf, EntryKind>) -> Result<(), Error> {
    refuse_symlink(target)?;
    let sidecar = target.join(provenance::SIDECAR_FILE);
    match expected.get(Path::new(provenance::SIDECAR_FILE)) {
        Some(EntryKind::File { bytes, mode }) => {
            refuse_symlink(&sidecar)?;
            fs::write(&sidecar, bytes).map_err(|e| map_io(&sidecar, e))?;
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;

                fs::set_permissions(&sidecar, fs::Permissions::from_mode(*mode & 0o777))
                    .map_err(|e| map_io(&sidecar, e))?;
            }
        }
        Some(EntryKind::Dir) => {
            return Err(Error::msg(format!(
                "Invalid receipt entry (directory): {}",
                output::display_path(&sidecar)
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
    if target.is_dir()
        && let Some(existing) = tree_contents(&target)?
    {
        if existing == expected {
            return Ok(PreflightOutcome::Identical);
        }
        if equal_except_receipt(&existing, &expected) {
            return Ok(PreflightOutcome::ReceiptMismatch);
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

fn rollback_or_retain_backup(
    staging: tempfile::TempDir,
    backup: &Path,
    target: &Path,
    publish_error: std::io::Error,
) -> Error {
    match fs::rename(backup, target) {
        Ok(()) => map_io(target, publish_error),
        Err(rollback_error) => {
            let recovery_root = staging.keep();
            let recovery = recovery_root.join(
                backup
                    .file_name()
                    .unwrap_or_else(|| std::ffi::OsStr::new("old")),
            );
            Error::msg(format!(
                "could not publish {} ({publish_error}); rollback failed ({rollback_error}); recovery backup: {}",
                output::display_path(target),
                output::display_path(&recovery)
            ))
        }
    }
}

/// Rename a fully staged replacement over an existing tree. If publication and
/// rollback both fail, retain the only original at an explicit recovery path.
pub(crate) fn publish_staged_tree(
    staging: tempfile::TempDir,
    staged: PathBuf,
    target: &Path,
) -> Result<PathBuf, Error> {
    let backup = staging.path().join("old");
    fs::rename(target, &backup).map_err(|e| map_io(target, e))?;
    if let Err(error) = fs::rename(&staged, target) {
        return Err(rollback_or_retain_backup(staging, &backup, target, error));
    }
    Ok(target.to_path_buf())
}

fn replace_verified_inner(
    skill: &Skill,
    destination_root: &Path,
    provenance: Option<&Provenance>,
) -> Result<PathBuf, Error> {
    require_safe_tree(&skill.path)?;
    let target = destination_root.join(&skill.name);
    refuse_symlink(&target)?;
    if !target.is_dir() {
        return Err(Error::msg(format!(
            "Refusing to replace missing or unsafe skill: {}",
            output::display_path(&target)
        )));
    }
    let staging = tempfile::Builder::new()
        .prefix(".tink-update-")
        .tempdir_in(destination_root)
        .map_err(|e| Error::msg(format!("update staging: {e}")))?;
    let staged = staging.path().join("new");
    copy_skill_tree(&skill.path, &staged, &[".git"])?;
    if staged.join(provenance::SIDECAR_FILE).exists() {
        return Err(Error::msg(format!(
            "Remote skill contains reserved .tink-source.json: {}",
            output::display_path(&skill.path)
        )));
    }
    if let Some(provenance) = provenance {
        provenance::write_file(&staged.join(provenance::SIDECAR_FILE), provenance)?;
    }
    publish_staged_tree(staging, staged, &target)
}

/// Replace an existing imported skill after dirty-tree preflight elsewhere.
pub fn replace_verified(
    skill: &Skill,
    destination_root: &Path,
    provenance: &Provenance,
) -> Result<PathBuf, Error> {
    replace_verified_inner(skill, destination_root, Some(provenance))
}

/// Replace the receipt-free embedded skill after its ownership preflight.
pub(crate) fn replace_embedded_verified(
    skill: &Skill,
    destination_root: &Path,
) -> Result<PathBuf, Error> {
    replace_verified_inner(skill, destination_root, None)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[cfg(unix)]
    #[test]
    fn skill_errors_escape_terminal_controls_in_paths() {
        let temp = TempDir::new().unwrap();
        let skill = temp.path().join("unsafe\u{1b}[31m");
        fs::create_dir_all(&skill).unwrap();
        fs::write(skill.join("SKILL.md"), "missing frontmatter\n").unwrap();

        let error = read_skill(&skill, false).unwrap_err().to_string();

        assert!(
            !error.contains('\u{1b}'),
            "raw escape reached diagnostic: {error:?}"
        );
        assert!(error.contains("unsafe\\x1b[31m"), "{error:?}");
    }

    #[cfg(unix)]
    #[test]
    fn copy_skill_tree_canonicalizes_executable_mode_and_strips_special_bits() {
        use std::os::unix::fs::PermissionsExt;

        let temp = TempDir::new().unwrap();
        let source = temp.path().join("source");
        let destination = temp.path().join("destination");
        fs::create_dir_all(&source).unwrap();
        let script = source.join("run.sh");
        fs::write(&script, "#!/bin/sh\nexit 0\n").unwrap();
        fs::set_permissions(&script, fs::Permissions::from_mode(0o6741)).unwrap();

        copy_skill_tree(&source, &destination, &[]).unwrap();

        let copied_mode = fs::metadata(destination.join("run.sh"))
            .unwrap()
            .permissions()
            .mode()
            & 0o777;
        assert_eq!(copied_mode, 0o755);
    }

    #[cfg(unix)]
    #[test]
    fn tree_identity_is_stable_across_non_executable_umask_modes() {
        use std::os::unix::fs::PermissionsExt;

        let temp = TempDir::new().unwrap();
        let source = temp.path().join("source");
        let destination = temp.path().join("destination");
        fs::create_dir_all(&source).unwrap();
        let file = source.join("notes.txt");
        fs::write(&file, "same bytes").unwrap();
        fs::set_permissions(&file, fs::Permissions::from_mode(0o600)).unwrap();
        let restrictive_digest = tree_digest(&source, &[]).unwrap();

        copy_skill_tree(&source, &destination, &[]).unwrap();
        assert_eq!(
            fs::metadata(destination.join("notes.txt"))
                .unwrap()
                .permissions()
                .mode()
                & 0o777,
            0o644
        );
        fs::set_permissions(&file, fs::Permissions::from_mode(0o644)).unwrap();

        assert_eq!(tree_digest(&source, &[]).unwrap(), restrictive_digest);
        assert!(skill_contents_equal(&source, &destination).unwrap());
    }

    #[cfg(unix)]
    #[test]
    fn copy_skill_tree_preserves_distinct_non_utf8_names() {
        use std::ffi::OsString;
        use std::os::unix::ffi::OsStringExt;

        let temp = TempDir::new().unwrap();
        let source = temp.path().join("source");
        let destination = temp.path().join("destination");
        fs::create_dir_all(&source).unwrap();
        let first = source.join(OsString::from_vec(vec![0x80]));
        let second = source.join(OsString::from_vec(vec![0x81]));
        for (path, body) in [
            (&first, b"first".as_slice()),
            (&second, b"second".as_slice()),
        ] {
            if let Err(error) = fs::write(path, body) {
                assert_eq!(
                    error.raw_os_error(),
                    Some(92),
                    "unexpected fixture error: {error}"
                );
                assert!(
                    !destination.exists(),
                    "OS rejection must happen before destination writes"
                );
                return;
            }
        }

        copy_skill_tree(&source, &destination, &[]).unwrap();

        assert_eq!(
            fs::read(destination.join(OsString::from_vec(vec![0x80]))).unwrap(),
            b"first"
        );
        assert_eq!(
            fs::read(destination.join(OsString::from_vec(vec![0x81]))).unwrap(),
            b"second"
        );
    }

    #[test]
    fn tree_digest_v2_is_domain_separated_from_legacy_encoding() {
        let temp = TempDir::new().unwrap();
        let tree = temp.path().join("skill");
        fs::create_dir_all(tree.join("resources")).unwrap();
        fs::write(tree.join("SKILL.md"), b"body").unwrap();
        fs::write(tree.join("resources/run.sh"), b"run").unwrap();

        assert_ne!(
            tree_digest(&tree, &[]).unwrap(),
            tree_digest_legacy(&tree, &[]).unwrap()
        );
        assert_eq!(
            tree_digest_legacy(&tree, &[]).unwrap(),
            "f612169400ea83a502473a69fac44e95b4dff9aa49319aebc0a073b61dd57a03"
        );
    }

    #[cfg(unix)]
    #[test]
    fn tree_digest_v2_includes_regular_file_mode() {
        use std::os::unix::fs::PermissionsExt;

        let temp = TempDir::new().unwrap();
        let tree = temp.path().join("skill");
        fs::create_dir_all(&tree).unwrap();
        let script = tree.join("run.sh");
        fs::write(&script, b"#!/bin/sh\n").unwrap();
        fs::set_permissions(&script, fs::Permissions::from_mode(0o755)).unwrap();
        let executable = tree_digest(&tree, &[]).unwrap();
        let legacy_executable = tree_digest_legacy(&tree, &[]).unwrap();

        fs::set_permissions(&script, fs::Permissions::from_mode(0o644)).unwrap();

        assert_ne!(tree_digest(&tree, &[]).unwrap(), executable);
        assert_eq!(tree_digest_legacy(&tree, &[]).unwrap(), legacy_executable);
    }

    #[cfg(unix)]
    #[test]
    fn tree_digest_v2_distinguishes_literal_backslash_from_path_separator() {
        let temp = tempfile::tempdir().unwrap();
        let flat = temp.path().join("flat");
        let nested = temp.path().join("nested");
        fs::create_dir_all(flat.join("a")).unwrap();
        fs::create_dir_all(nested.join("a")).unwrap();
        fs::write(flat.join("SKILL.md"), "same").unwrap();
        fs::write(nested.join("SKILL.md"), "same").unwrap();
        fs::write(flat.join("a\\b"), "payload").unwrap();
        fs::write(nested.join("a").join("b"), "payload").unwrap();

        assert_eq!(
            tree_digest_legacy(&flat, &[]).unwrap(),
            tree_digest_legacy(&nested, &[]).unwrap(),
            "fixture must reproduce the historical normalization collision"
        );
        assert_ne!(
            tree_digest(&flat, &[]).unwrap(),
            tree_digest(&nested, &[]).unwrap()
        );
    }

    #[test]
    fn rollback_failure_retains_recovery_backup() {
        let temp = TempDir::new().unwrap();
        let staging = tempfile::Builder::new()
            .prefix(".rollback-fixture-")
            .tempdir_in(temp.path())
            .unwrap();
        let staging_path = staging.path().to_path_buf();
        let backup = staging.path().join("old");
        fs::create_dir_all(&backup).unwrap();
        fs::write(backup.join("original"), "preserve me").unwrap();
        let target = temp.path().join("target");
        fs::write(&target, "rollback blocker").unwrap();

        let error = rollback_or_retain_backup(
            staging,
            &backup,
            &target,
            std::io::Error::other("publish fixture"),
        );

        assert!(error.to_string().contains("recovery backup"), "{error}");
        assert!(error.to_string().contains(&backup.display().to_string()));
        assert_eq!(
            fs::read(staging_path.join("old/original")).unwrap(),
            b"preserve me"
        );
    }

    #[cfg(unix)]
    #[test]
    fn validate_skill_tree_reads_only_metadata_for_regular_files() {
        use std::os::unix::fs::PermissionsExt;

        let temp = TempDir::new().unwrap();
        let tree = temp.path().join("skill");
        fs::create_dir_all(&tree).unwrap();
        let unreadable = tree.join("private.txt");
        fs::write(&unreadable, "contents must not be read\n").unwrap();
        fs::set_permissions(&unreadable, fs::Permissions::from_mode(0o000)).unwrap();

        let result = validate_skill_tree(&tree);
        fs::set_permissions(&unreadable, fs::Permissions::from_mode(0o600)).unwrap();

        assert!(result.is_ok(), "{result:?}");
    }
}
