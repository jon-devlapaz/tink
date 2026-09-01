//! Pinned nested skillset lifecycle.
//!
//! Receipt entry presence classifies a root as a skillset before receipt contents are
//! trusted. The project tree is authoritative; library copies are derived from a
//! validated project tree.

use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::error::Error;
use crate::git;
use crate::home;
use crate::init;
use crate::output;
use crate::paths::{canonicalize_beneath, map_io, refuse_symlink};
use crate::skills;
use crate::sources;

const RECEIPT_FILE: &str = ".tink-skillset.json";
const NAME_SUFFIX: &str = "-skillset";
const DIGEST_VERSION: u32 = 2;

fn legacy_digest_version() -> u32 {
    1
}

/// Whether `path` contains a skillset receipt entry.
///
/// This is classification, not validation. Receipt presence claims the root for the
/// skillset domain and prevents standalone handling; callers that need valid contents
/// must validate them separately.
pub(crate) fn has_receipt_entry(path: &Path) -> bool {
    let receipt = path.join(RECEIPT_FILE);
    receipt.exists() || receipt.is_symlink()
}

/// Refuse a skillset-owned tree at a standalone-skill boundary.
///
/// Receipt entry presence establishes ownership before receipt contents are trusted.
pub(crate) fn ensure_standalone_source(path: &Path, name: &str) -> Result<(), Error> {
    if has_receipt_entry(path) {
        return Err(Error::msg(format!(
            "Source is owned by a skillset; use `tink skillset add {name}`"
        )));
    }
    Ok(())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LibraryWrite {
    Created,
    Unchanged,
    Repaired,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
struct SkillsetMeta {
    source: String,
    revision: String,
    #[serde(rename = "sourceRoot")]
    source_root: String,
    members: Vec<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
struct SkillsetReceipt {
    source: String,
    revision: String,
    #[serde(rename = "sourceRoot")]
    source_root: String,
    members: Vec<String>,
    #[serde(rename = "digestVersion", default = "legacy_digest_version")]
    digest_version: u32,
    digest: String,
}

#[derive(Debug)]
struct InstalledSkillset {
    name: String,
    receipt: SkillsetReceipt,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ListedSkillset {
    pub name: String,
    pub members: Vec<String>,
}

fn read_json<T: for<'de> Deserialize<'de>>(path: &Path, label: &str) -> Result<T, Error> {
    refuse_symlink(path)?;
    if !path.is_file() {
        return Err(Error::msg(format!(
            "Missing {label}: {}",
            output::display_path(path)
        )));
    }
    let text = fs::read_to_string(path).map_err(|e| map_io(path, e))?;
    serde_json::from_str(&text).map_err(|e| Error::msg(format!("Invalid {label}: {e}")))
}

fn validate_revision(revision: &str) -> Result<(), Error> {
    if !(revision.len() == 40 || revision.len() == 64)
        || !revision.chars().all(|c| c.is_ascii_hexdigit())
    {
        return Err(Error::msg("Skillset revision must be a full Git object ID"));
    }
    Ok(())
}

fn validate_digest(digest: &str) -> Result<(), Error> {
    if digest.len() != 64 || !digest.chars().all(|c| c.is_ascii_hexdigit()) {
        return Err(Error::msg("Skillset digest must be a SHA-256 hex string"));
    }
    Ok(())
}

fn normalized_source_root(source_root: &str) -> Result<PathBuf, Error> {
    if source_root.is_empty() || source_root.starts_with('/') || source_root.contains('\\') {
        return Err(Error::msg(
            "Skillset sourceRoot must be a non-empty relative POSIX path",
        ));
    }
    let mut path = PathBuf::new();
    for segment in source_root.split('/') {
        if segment.is_empty() || segment == "." || segment == ".." {
            return Err(Error::msg(
                "Skillset sourceRoot must be normalized and repository-relative",
            ));
        }
        path.push(segment);
    }
    Ok(path)
}

fn validate_members(members: &[String]) -> Result<(), Error> {
    if members.is_empty() {
        return Err(Error::msg("Skillset members must not be empty"));
    }
    let mut seen = BTreeSet::new();
    for member in members {
        if !skills::valid_skill_name(member) {
            return Err(Error::msg(format!(
                "Invalid skillset member name: {member}"
            )));
        }
        if !seen.insert(member) {
            return Err(Error::msg(format!("Duplicate skillset member: {member}")));
        }
    }
    Ok(())
}

fn validate_skillset_name(name: &str) -> Result<(), Error> {
    if !skills::valid_skill_name(name) {
        return Err(Error::msg(format!("Invalid skillset name: {name}")));
    }
    if !name.ends_with(NAME_SUFFIX) {
        return Err(Error::msg(format!(
            "Skillset name must end in {NAME_SUFFIX}: {name}"
        )));
    }
    Ok(())
}

fn parse_source(source: &str) -> Result<sources::RemoteSource, Error> {
    let rest = source
        .strip_prefix("https://")
        .ok_or_else(|| Error::msg("Skillset source must be an absolute HTTPS Git URL"))?;
    let (authority, path) = rest
        .split_once('/')
        .ok_or_else(|| Error::msg("Skillset source must be an absolute HTTPS Git URL"))?;
    if authority.is_empty()
        || authority.contains('@')
        || authority.chars().any(char::is_whitespace)
        || path.is_empty()
        || path.contains('?')
        || path.contains('#')
        || path.contains("//")
    {
        return Err(Error::msg(
            "Skillset source must be an absolute HTTPS Git URL",
        ));
    }
    Ok(sources::RemoteSource {
        display: source.to_string(),
        url: source.to_string(),
    })
}

fn validate_meta(meta: &SkillsetMeta) -> Result<sources::RemoteSource, Error> {
    let remote = parse_source(&meta.source)?;
    validate_revision(&meta.revision)?;
    normalized_source_root(&meta.source_root)?;
    validate_members(&meta.members)?;
    Ok(remote)
}

fn receipt_for(meta: &SkillsetMeta, digest: String) -> SkillsetReceipt {
    SkillsetReceipt {
        source: meta.source.clone(),
        revision: meta.revision.clone(),
        source_root: meta.source_root.clone(),
        members: meta.members.clone(),
        digest_version: DIGEST_VERSION,
        digest,
    }
}

fn receipt_meta(receipt: &SkillsetReceipt) -> SkillsetMeta {
    SkillsetMeta {
        source: receipt.source.clone(),
        revision: receipt.revision.clone(),
        source_root: receipt.source_root.clone(),
        members: receipt.members.clone(),
    }
}

fn read_owned_receipt(path: &Path, label: &str) -> Result<SkillsetReceipt, Error> {
    let receipt: SkillsetReceipt = read_json(&path.join(RECEIPT_FILE), label)?;
    validate_meta(&receipt_meta(&receipt))?;
    if receipt.digest_version != 1 && receipt.digest_version != DIGEST_VERSION {
        return Err(Error::msg(format!(
            "Unsupported skillset digest version: {}",
            receipt.digest_version
        )));
    }
    validate_digest(&receipt.digest)?;
    Ok(receipt)
}

fn validate_member_trees(path: &Path, receipt: &SkillsetReceipt) -> Result<(), Error> {
    for member in &receipt.members {
        let member_path = path.join(member);
        refuse_symlink(&member_path)?;
        if !member_path.is_dir() {
            return Err(Error::msg(format!("Missing skillset member: {member}")));
        }
        skills::read_skill(&member_path, true)?;
    }
    Ok(())
}

fn validate_installed_tree(path: &Path, receipt: &SkillsetReceipt) -> Result<(), Error> {
    validate_member_trees(path, receipt)?;
    if receipt.digest_version != DIGEST_VERSION {
        return Err(Error::msg(format!(
            "Skillset receipt uses legacy digest version {}; run `tink skillset refresh {}` to migrate it",
            receipt.digest_version,
            path.file_name()
                .and_then(|name| name.to_str())
                .unwrap_or("NAME-skillset")
        )));
    }
    let digest = skills::tree_digest(path, &[RECEIPT_FILE])?;
    if digest != receipt.digest {
        return Err(Error::msg(format!(
            "Skillset tree digest mismatch: {}",
            output::display_path(path)
        )));
    }
    Ok(())
}

fn validate_legacy_tree_for_refresh(path: &Path, receipt: &SkillsetReceipt) -> Result<(), Error> {
    validate_member_trees(path, receipt)?;
    let digest = skills::tree_digest_legacy(path, &[RECEIPT_FILE])?;
    if digest != receipt.digest {
        return Err(Error::msg(format!(
            "Skillset tree digest mismatch: {}",
            output::display_path(path)
        )));
    }
    Ok(())
}

fn read_catalog(home: Option<&Path>, name: &str) -> Result<SkillsetMeta, Error> {
    validate_skillset_name(name)?;
    let home = match home {
        Some(home) => home.to_path_buf(),
        None => home::resolve_home()?,
    };
    let catalog = home::by_skillset_path(&home).join(name);
    refuse_symlink(&catalog)?;
    let meta: SkillsetMeta = read_json(&catalog.join("meta.json"), "skillset catalog meta")?;
    validate_meta(&meta)?;
    Ok(meta)
}

fn source_member_root(
    checkout: &Path,
    meta: &SkillsetMeta,
    member: &str,
) -> Result<PathBuf, Error> {
    let source_root = canonicalize_beneath(checkout, &normalized_source_root(&meta.source_root)?)?;
    if !source_root.is_dir() {
        return Err(Error::msg(format!(
            "Skillset sourceRoot is not a directory: {}",
            output::display_path(&source_root)
        )));
    }
    let member_root = canonicalize_beneath(&source_root, Path::new(member))?;
    if !member_root.is_dir() {
        return Err(Error::msg(format!("Skillset member not found: {member}")));
    }
    skills::read_skill(&member_root, true)?;
    Ok(member_root)
}

fn install_from_checkout(
    checkout: &Path,
    meta: &SkillsetMeta,
    destination_root: &Path,
    name: &str,
) -> Result<(PathBuf, bool), Error> {
    let target = destination_root.join(name);
    if target.exists() || target.is_symlink() {
        refuse_symlink(&target)?;
        return Err(Error::msg(format!(
            "Refusing to overwrite existing skillset: {}",
            output::display_path(&target)
        )));
    }

    let (_staging, staged) = stage_from_checkout(checkout, meta, destination_root, name)?;
    fs::rename(&staged, &target).map_err(|e| map_io(&target, e))?;
    Ok((target, true))
}

fn stage_from_checkout(
    checkout: &Path,
    meta: &SkillsetMeta,
    destination_root: &Path,
    name: &str,
) -> Result<(tempfile::TempDir, PathBuf), Error> {
    let staging = tempfile::Builder::new()
        .prefix(".tink-skillset-stage-")
        .tempdir_in(destination_root)
        .map_err(|e| Error::msg(format!("skillset staging dir: {e}")))?;
    let staged = staging.path().join(name);
    fs::create_dir_all(&staged).map_err(|e| map_io(&staged, e))?;
    for member in &meta.members {
        let source = source_member_root(checkout, meta, member)?;
        skills::copy_skill_tree(&source, &staged.join(member), &[".git"])?;
    }
    let digest = skills::tree_digest(&staged, &[RECEIPT_FILE])?;
    let receipt = receipt_for(meta, digest);
    let receipt_path = staged.join(RECEIPT_FILE);
    let receipt_text = serde_json::to_string_pretty(&receipt)
        .map_err(|e| Error::msg(format!("serialize skillset receipt: {e}")))?;
    fs::write(&receipt_path, format!("{receipt_text}\n")).map_err(|e| map_io(&receipt_path, e))?;
    read_installed(&staged)?;
    Ok((staging, staged))
}

fn replace_from_checkout(
    checkout: &Path,
    meta: &SkillsetMeta,
    destination_root: &Path,
    name: &str,
) -> Result<PathBuf, Error> {
    let target = destination_root.join(name);
    let (staging, staged) = stage_from_checkout(checkout, meta, destination_root, name)?;

    skills::publish_staged_tree(staging, staged, &target)
}

pub fn add_skillset(
    project_root: &Path,
    name: &str,
) -> Result<(PathBuf, bool, LibraryWrite), Error> {
    add_skillset_at(None, project_root, name)
}

pub(crate) fn add_skillset_at(
    home: Option<&Path>,
    project_root: &Path,
    name: &str,
) -> Result<(PathBuf, bool, LibraryWrite), Error> {
    validate_skillset_name(name)?;
    let meta = read_catalog(home, name)?;
    preflight_library_target(home, name)?;
    let target = home::project_skills_path(project_root).join(name);
    if target.exists() || target.is_symlink() {
        refuse_symlink(&target)?;
        if !target.is_dir() {
            return Err(Error::msg(format!(
                "Refusing to overwrite non-directory skillset: {}",
                output::display_path(&target)
            )));
        }
        let receipt = read_owned_receipt(&target, "installed skillset receipt")?;
        if receipt.digest_version != DIGEST_VERSION {
            return Err(Error::msg(format!(
                "Skillset receipt uses a legacy digest; run `tink skillset refresh {name}` to migrate it"
            )));
        }
        if validate_installed_tree(&target, &receipt).is_err() {
            return Err(Error::msg(format!(
                "Refusing to add {name}: local modifications are present; remove it first to discard them"
            )));
        }
        if receipt_meta(&receipt) != meta {
            return Err(Error::msg(format!(
                "Skillset catalog changed for {name}; run `tink skillset refresh {name}`"
            )));
        }
        let library_write = sync_library_from_project(home, &target)?;
        return Ok((target, false, library_write));
    }

    init::ensure_project_layout_at(home, project_root)?;
    let remote = validate_meta(&meta)?;
    let (_clone, repository, tip) = git::checkout(&remote)?;
    let (_old_checkout, checkout) = if tip == meta.revision {
        (None, repository)
    } else {
        let (temp, checkout) = git::checkout_revision(&repository, &meta.revision)?;
        (Some(temp), checkout)
    };
    let (installed, created) = install_from_checkout(
        &checkout,
        &meta,
        &home::project_skills_path(project_root),
        name,
    )?;
    let library_write = sync_library_from_project(home, &installed)?;
    Ok((installed, created, library_write))
}

pub fn refresh_skillset(project_root: &Path, name: &str) -> Result<bool, Error> {
    refresh_skillset_at(None, project_root, name)
}

pub(crate) fn refresh_skillset_at(
    home: Option<&Path>,
    project_root: &Path,
    name: &str,
) -> Result<bool, Error> {
    validate_skillset_name(name)?;
    let meta = read_catalog(home, name)?;
    let skills_root = home::project_skills_path(project_root);
    let target = skills_root.join(name);
    refuse_symlink(&target)?;
    if !target.is_dir() {
        return Err(Error::msg(format!("Skillset not found: {name}")));
    }
    let receipt = read_owned_receipt(&target, "installed skillset receipt")?;
    let legacy_receipt = receipt.digest_version != DIGEST_VERSION;
    let validation = if legacy_receipt {
        validate_legacy_tree_for_refresh(&target, &receipt)
    } else {
        validate_installed_tree(&target, &receipt)
    };
    if validation.is_err() {
        return Err(Error::msg(format!(
            "Refusing to refresh {name}: local modifications are present"
        )));
    }
    preflight_library_target(home, name)?;
    if !legacy_receipt && receipt_meta(&receipt) == meta {
        sync_library_from_project(home, &target)?;
        return Ok(false);
    }

    let remote = validate_meta(&meta)?;
    let (_clone, repository, tip) = git::checkout(&remote)?;
    let (_old_checkout, checkout) = if tip == meta.revision {
        (None, repository)
    } else {
        let (temp, checkout) = git::checkout_revision(&repository, &meta.revision)?;
        (Some(temp), checkout)
    };
    let installed = replace_from_checkout(&checkout, &meta, &skills_root, name)?;
    sync_library_from_project(home, &installed)?;
    Ok(true)
}

pub fn remove_skillset(project_root: &Path, name: &str) -> Result<PathBuf, Error> {
    validate_skillset_name(name)?;
    let agents = home::project_agents_path(project_root);
    let skills_root = home::project_skills_path(project_root);
    refuse_symlink(&agents)?;
    refuse_symlink(&skills_root)?;
    let target = skills_root.join(name);
    refuse_symlink(&target)?;
    if !target.is_dir() {
        return Err(Error::msg(format!("Skillset not found: {name}")));
    }
    read_owned_receipt(&target, "installed skillset receipt")?;
    fs::remove_dir_all(&target).map_err(|e| map_io(&target, e))?;
    Ok(target)
}

fn read_installed(path: &Path) -> Result<InstalledSkillset, Error> {
    refuse_symlink(path)?;
    let name = path
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or("");
    validate_skillset_name(name)?;
    let receipt = read_owned_receipt(path, "installed skillset receipt")?;
    validate_installed_tree(path, &receipt)?;
    Ok(InstalledSkillset {
        name: name.to_string(),
        receipt,
    })
}

/// Validate an installed skillset without consulting the network or catalog.
pub fn validate_installed(path: &Path) -> Result<(), Error> {
    read_installed(path).map(|_| ())
}

/// List receipt-backed skillsets installed in a project.
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
        let name = path
            .file_name()
            .and_then(|value| value.to_str())
            .unwrap_or("");
        if name == "README.md" || name.starts_with('.') {
            continue;
        }
        if path.is_symlink() || !path.is_dir() {
            return Err(Error::msg(format!(
                "Unexpected entry in .agents/skills: {name}"
            )));
        }
        if has_receipt_entry(&path) {
            let installed = read_installed(&path)?;
            skillsets.push(ListedSkillset {
                name: installed.name,
                members: installed.receipt.members,
            });
        }
    }
    skillsets.sort_by(|left, right| left.name.cmp(&right.name));
    Ok(skillsets)
}

/// Count validated project skillsets and their declared member skills.
pub fn project_counts(project_root: &Path) -> Result<(usize, usize), Error> {
    let skills_root = home::project_skills_path(project_root);
    let mut skillset_count = 0;
    let mut member_count = 0;
    for entry in fs::read_dir(&skills_root).map_err(|e| map_io(&skills_root, e))? {
        let path = entry.map_err(|e| map_io(&skills_root, e))?.path();
        if has_receipt_entry(&path) {
            let installed = read_installed(&path)?;
            skillset_count += 1;
            member_count += installed.receipt.members.len();
        }
    }
    Ok((skillset_count, member_count))
}

fn validate_library_receipt(path: &Path) -> Result<(), Error> {
    read_owned_receipt(path, "library skillset receipt").map(|_| ())
}

fn library_root(home: Option<&Path>) -> Result<PathBuf, Error> {
    let (home, _) = home::ensure_inventory_root(home)?;
    Ok(home::skills_library_path(&home))
}

fn preflight_library_target(home: Option<&Path>, name: &str) -> Result<(), Error> {
    let target = library_root(home)?.join(name);
    if !target.exists() && !target.is_symlink() {
        return Ok(());
    }
    refuse_symlink(&target)?;
    if !target.is_dir() {
        return Err(Error::msg(format!(
            "Library name collision for skillset: {}",
            output::display_path(&target)
        )));
    }
    validate_library_receipt(&target).map_err(|_| {
        Error::msg(format!(
            "Library name collision for skillset: {}; existing entry is not an owned skillset",
            output::display_path(&target)
        ))
    })
}

fn copy_project_tree(
    project: &Path,
    library: &Path,
    name: &str,
    replace: bool,
) -> Result<(), Error> {
    let staging = tempfile::Builder::new()
        .prefix(".tink-skillset-library-")
        .tempdir_in(library)
        .map_err(|e| Error::msg(format!("skillset library staging: {e}")))?;
    let staged = staging.path().join(name);
    skills::copy_skill_tree(project, &staged, &[])?;
    read_installed(&staged)?;
    let target = library.join(name);
    if !replace {
        fs::rename(&staged, &target).map_err(|e| map_io(&target, e))?;
        return Ok(());
    }

    skills::publish_staged_tree(staging, staged, &target).map(|_| ())
}

fn sync_library_from_project(home: Option<&Path>, project: &Path) -> Result<LibraryWrite, Error> {
    let name = read_installed(project)?.name;
    let library = library_root(home)?;
    let target = library.join(&name);
    if !target.exists() && !target.is_symlink() {
        copy_project_tree(project, &library, &name, false)?;
        return Ok(LibraryWrite::Created);
    }
    preflight_library_target(home, &name)?;
    if skills::skill_contents_equal(project, &target)? {
        return Ok(LibraryWrite::Unchanged);
    }
    copy_project_tree(project, &library, &name, true)?;
    Ok(LibraryWrite::Repaired)
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
    let library = home::skills_library_path(&home);
    if !library.exists() {
        return Ok(Vec::new());
    }
    refuse_symlink(&library)?;
    if !library.is_dir() {
        return Err(Error::msg(format!(
            "Refusing to read non-directory library: {}",
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
            let installed = read_installed(&path)?;
            skillsets.push(ListedSkillset {
                name: installed.name,
                members: installed.receipt.members,
            });
        }
    }
    Ok(skillsets)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn legacy_receipt_is_only_accepted_for_refresh_migration() {
        let temp = tempfile::tempdir().unwrap();
        let installed = temp.path().join("demo-skillset");
        let member = installed.join("demo");
        fs::create_dir_all(&member).unwrap();
        fs::write(
            member.join("SKILL.md"),
            "---\nname: demo\ndescription: Legacy receipt fixture.\n---\n",
        )
        .unwrap();
        let digest = skills::tree_digest_legacy(&installed, &[RECEIPT_FILE]).unwrap();
        let receipt = SkillsetReceipt {
            source: "https://github.com/example/skills.git".into(),
            revision: "a".repeat(40),
            source_root: "skills".into(),
            members: vec!["demo".into()],
            digest_version: 1,
            digest,
        };

        assert!(validate_legacy_tree_for_refresh(&installed, &receipt).is_ok());
        let error = validate_installed_tree(&installed, &receipt).unwrap_err();
        assert!(error.to_string().contains("skillset refresh"), "{error}");
    }

    #[test]
    fn source_root_rejects_escape_and_empty_segments() {
        for value in [
            "",
            ".",
            "skills/../other",
            "/skills",
            "skills//common",
            "skills\\common",
        ] {
            assert!(normalized_source_root(value).is_err(), "{value}");
        }
        assert_eq!(
            normalized_source_root("skills/common").unwrap(),
            PathBuf::from("skills/common")
        );
    }

    #[cfg(unix)]
    #[test]
    fn source_member_root_refuses_symlinked_source_root_ancestor() {
        let temp = tempfile::tempdir().unwrap();
        let checkout = temp.path().join("checkout");
        let outside = temp.path().join("outside");
        let member = outside.join("skills/alpha");
        fs::create_dir_all(&checkout).unwrap();
        fs::create_dir_all(&member).unwrap();
        fs::write(
            member.join("SKILL.md"),
            "---\nname: alpha\ndescription: Outside fixture.\n---\n",
        )
        .unwrap();
        std::os::unix::fs::symlink(&outside, checkout.join("jump")).unwrap();
        let meta = SkillsetMeta {
            source: "https://github.com/example/skills.git".into(),
            revision: "a".repeat(40),
            source_root: "jump/skills".into(),
            members: vec!["alpha".into()],
        };

        let error = source_member_root(&checkout, &meta, "alpha")
            .expect_err("ancestor symlink must be refused");

        assert!(error.to_string().contains("symlink"), "{error}");
    }

    #[test]
    fn validates_explicit_unique_member_names() {
        assert!(validate_members(&["alpha".into(), "beta-two".into()]).is_ok());
        assert!(validate_members(&["alpha".into(), "alpha".into()]).is_err());
        assert!(validate_members(&["Alpha".into()]).is_err());
    }

    #[test]
    fn accepts_absolute_https_sources_and_rejects_local_or_credentialed_ones() {
        assert!(parse_source("https://git.example.test/team/skills.git").is_ok());
        assert!(parse_source("owner/skills").is_err());
        assert!(parse_source("https://user@git.example.test/skills.git").is_err());
    }

    #[test]
    fn receipt_entry_presence_includes_dangling_symlinks() {
        let root = tempfile::tempdir().unwrap();
        let receipt = root.path().join(RECEIPT_FILE);

        assert!(!has_receipt_entry(root.path()));
        fs::write(&receipt, "receipt").unwrap();
        assert!(has_receipt_entry(root.path()));

        fs::remove_file(&receipt).unwrap();
        std::os::unix::fs::symlink(root.path().join("missing"), &receipt).unwrap();
        assert!(has_receipt_entry(root.path()));
    }
}
