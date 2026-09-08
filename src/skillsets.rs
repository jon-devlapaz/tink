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
/// Agent-authored skillset router. Not part of the Git member pin; ignored by
/// the receipt digest so manage-tink can write it without dirtying the tree.
const ROUTER_FILE: &str = "SKILL.md";
const NAME_SUFFIX: &str = "-skillset";
const DIGEST_VERSION: u32 = 2;
const DIGEST_ROOT_IGNORE: &[&str] = &[RECEIPT_FILE, ROUTER_FILE, ".DS_Store"];

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

/// How one entry under a skills root classifies for standalone handling.
///
/// Classification only: ignored names stay silent in every listing, a receipt
/// entry claims the root before its contents are trusted, and each caller maps
/// `Unexpected` to its own outcome (refusal or skip) with its own message.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum EntryClass {
    /// `README.md` or a dotfile: silently skipped by every listing.
    Ignored,
    /// Symlink or non-directory: refused by validation, skipped by listings.
    Unexpected,
    /// Skillset receipt entry: owned by the skillset lifecycle.
    Skillset,
    /// Candidate standalone skill directory.
    Standalone,
}

pub(crate) fn classify_entry(path: &Path) -> EntryClass {
    let name = path
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or("");
    if name == "README.md" || name.starts_with('.') {
        return EntryClass::Ignored;
    }
    if path.is_symlink() || !path.is_dir() {
        return EntryClass::Unexpected;
    }
    if has_receipt_entry(path) {
        return EntryClass::Skillset;
    }
    EntryClass::Standalone
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LibraryWrite {
    Created,
    Unchanged,
    Repaired,
}

#[derive(Debug)]
pub struct SkillsetAddOutcome {
    pub name: String,
    pub created: bool,
    pub library_write: LibraryWrite,
    pub member_count: usize,
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
    /// `None` when the tree matches its receipt; otherwise the validation error.
    pub error: Option<String>,
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
    if source_root == "." {
        return Ok(PathBuf::from("."));
    }
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

pub fn canonicalize_skillset_name(name: &str) -> Result<String, Error> {
    let trimmed = name.trim();
    let candidate = if trimmed.ends_with(NAME_SUFFIX) {
        trimmed.to_string()
    } else {
        format!("{trimmed}{NAME_SUFFIX}")
    };
    validate_skillset_name(&candidate)?;
    Ok(candidate)
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
    let digest = skills::tree_digest(path, DIGEST_ROOT_IGNORE)?;
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
    let digest = skills::tree_digest_legacy(path, DIGEST_ROOT_IGNORE)?;
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
) -> Result<(PathBuf, String), Error> {
    let source_root = if meta.source_root == "." {
        checkout.to_path_buf()
    } else {
        canonicalize_beneath(checkout, &normalized_source_root(&meta.source_root)?)?
    };
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
    let (_skill, desc) = skills::read_skill_and_description(&member_root, true)?;
    Ok((member_root, desc))
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
    let mut member_descriptions = Vec::with_capacity(meta.members.len());
    for member in &meta.members {
        let (source, desc) = source_member_root(checkout, meta, member)?;
        skills::copy_skill_tree(&source, &staged.join(member), &[".git"])?;
        member_descriptions.push((member.clone(), desc));
    }
    let digest = skills::tree_digest(&staged, DIGEST_ROOT_IGNORE)?;
    let receipt = receipt_for(meta, digest);
    let receipt_path = staged.join(RECEIPT_FILE);
    let receipt_text = serde_json::to_string_pretty(&receipt)
        .map_err(|e| Error::msg(format!("serialize skillset receipt: {e}")))?;
    fs::write(&receipt_path, format!("{receipt_text}\n")).map_err(|e| map_io(&receipt_path, e))?;

    let router_path = staged.join(ROUTER_FILE);
    if !router_path.exists() {
        let router_text = generate_baseline_router(name, &member_descriptions);
        fs::write(&router_path, router_text).map_err(|e| map_io(&router_path, e))?;
    }

    read_installed(&staged)?;
    Ok((staging, staged))
}

fn read_router_overlay(path: &Path) -> Result<Option<Vec<u8>>, Error> {
    let router = path.join(ROUTER_FILE);
    if !router.exists() && !router.is_symlink() {
        return Ok(None);
    }
    refuse_symlink(&router)?;
    if !router.is_file() {
        return Err(Error::msg(format!(
            "Skillset router must be a regular file: {}",
            output::display_path(&router)
        )));
    }
    Ok(Some(fs::read(&router).map_err(|e| map_io(&router, e))?))
}

fn write_router_overlay(path: &Path, bytes: &[u8]) -> Result<(), Error> {
    let router = path.join(ROUTER_FILE);
    if router.exists() || router.is_symlink() {
        refuse_symlink(&router)?;
    }
    fs::write(&router, bytes).map_err(|e| map_io(&router, e))
}

fn restore_router_overlay(path: &Path, overlay: Option<Vec<u8>>) -> Result<(), Error> {
    match overlay {
        Some(bytes) => write_router_overlay(path, &bytes),
        None => Ok(()),
    }
}

fn replace_from_checkout(
    checkout: &Path,
    meta: &SkillsetMeta,
    destination_root: &Path,
    name: &str,
) -> Result<PathBuf, Error> {
    let target = destination_root.join(name);
    let overlay = read_router_overlay(&target)?;
    let (staging, staged) = stage_from_checkout(checkout, meta, destination_root, name)?;
    restore_router_overlay(&staged, overlay)?;
    skills::publish_staged_tree(staging, staged, &target)
}

fn human_title(name: &str) -> String {
    name.split('-')
        .map(|part| {
            let mut chars = part.chars();
            match chars.next() {
                None => String::new(),
                Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn generate_baseline_router(name: &str, members: &[(String, String)]) -> String {
    let member_names: Vec<&str> = members.iter().map(|(n, _)| n.as_str()).collect();
    let members_joined = member_names.join(", ");
    let title = human_title(name);

    let mut out = String::new();
    out.push_str("---\n");
    out.push_str(&format!("name: {name}\n"));
    out.push_str("description: >\n");
    out.push_str(&format!(
        "  Router for the {name} skillset. Use when work involves {members_joined}.\n"
    ));
    out.push_str("  Do not use when a single named member skill is already the clear owner.\n");
    out.push_str("---\n\n");
    out.push_str(&format!("# {title}\n\n"));
    out.push_str(
        "Route. Prefer members under this skillset tree over any sibling standalone\n\
         skill with the same name.\n\n\
         ## 1. Classify the request\n\n\
         Pick the lightest owner that covers the ask:\n\n",
    );

    if members.len() >= 12 {
        out.push_str("### Members\n\n");
    }

    out.push_str("| Ask | Load |\n");
    out.push_str("| --- | --- |\n");
    for (member, desc) in members {
        let first_line = desc.lines().next().unwrap_or("").trim().replace('|', "\\|");
        let ask = if first_line.is_empty() {
            format!("Work involving {member}.")
        } else {
            first_line
        };
        out.push_str(&format!(
            "| {ask} | [{member}/SKILL.md]({member}/SKILL.md) |\n"
        ));
    }
    out.push('\n');

    out.push_str(
        "If the user names a member, load that member only.\n\n\
         If several domains are in play and a coordinator owns that workflow, load it.\n\
         Otherwise load only the owners needed for the change.\n\n\
         ## 2. Hand off\n\n\
         1. Read the chosen member `SKILL.md` in full.\n\
         2. Follow that skill's procedure, references, and reporting format.\n\
         3. Load sibling members only when the chosen skill names a handoff, or when a\n\
            coordinator requires its workers.\n\n\
         ## 3. Boundaries\n\n\
         - Leave `.tink-skillset.json` untouched. It is ownership and digest evidence.\n\
         - Skillset install, refresh, and remove stay with `manage-tink`.\n",
    );

    out
}

fn sanitize_for_skill_name(raw: &str) -> String {
    let mut result = String::with_capacity(raw.len());
    let mut last_was_hyphen = false;
    for c in raw.chars() {
        if c.is_ascii_alphanumeric() {
            result.push(c.to_ascii_lowercase());
            last_was_hyphen = false;
        } else if !last_was_hyphen {
            result.push('-');
            last_was_hyphen = true;
        }
    }
    result.trim_matches('-').to_string()
}

fn is_generic_folder(folder: &str) -> bool {
    matches!(
        folder,
        "" | "." | "skills" | "agent-skills" | "agents" | ".agents"
    )
}

fn derive_skillset_name(
    owner: &str,
    repo: &str,
    boundary: &str,
    custom_name: Option<&str>,
) -> Result<String, Error> {
    if let Some(custom) = custom_name {
        return canonicalize_skillset_name(custom);
    }

    let folder = boundary
        .trim_matches('/')
        .rsplit('/')
        .next()
        .unwrap_or("")
        .trim();

    let base = if is_generic_folder(folder) {
        let owner_clean = sanitize_for_skill_name(owner);
        let repo_clean = sanitize_for_skill_name(repo);
        format!("{owner_clean}-{repo_clean}")
    } else {
        sanitize_for_skill_name(folder)
    };

    let candidate = if base.ends_with(NAME_SUFFIX) {
        base
    } else {
        format!("{base}{NAME_SUFFIX}")
    };

    validate_skillset_name(&candidate).map_err(|e| {
        Error::msg(format!(
            "Could not infer valid canonical skillset name from URL: {e}; please specify a name explicitly"
        ))
    })?;

    Ok(candidate)
}

fn discover_skillset_members_in_boundary(
    boundary_dir: &Path,
) -> Result<Vec<(String, String)>, Error> {
    refuse_symlink(boundary_dir)?;
    if !boundary_dir.is_dir() {
        return Err(Error::msg(format!(
            "Skillset boundary is not a directory: {}",
            output::display_path(boundary_dir)
        )));
    }

    let mut entries: Vec<_> = fs::read_dir(boundary_dir)
        .map_err(|e| map_io(boundary_dir, e))?
        .collect::<Result<_, _>>()
        .map_err(|e| map_io(boundary_dir, e))?;
    entries.sort_by_key(|e| e.file_name());

    let mut members = Vec::new();
    for entry in entries {
        let path = entry.path();
        let name = path.file_name().and_then(|v| v.to_str()).unwrap_or("");
        if name.starts_with('.') {
            continue;
        }
        refuse_symlink(&path)?;
        if !path.is_dir() {
            continue;
        }
        let skill_file = path.join(ROUTER_FILE);
        if skill_file.exists() || skill_file.is_symlink() {
            let (skill, desc) = skills::read_skill_and_description(&path, true)?;
            members.push((skill.name, desc));
        }
    }

    if members.is_empty() {
        return Err(Error::msg(format!(
            "No member skills found in boundary: {}",
            output::display_path(boundary_dir)
        )));
    }

    members.sort_by(|a, b| a.0.cmp(&b.0));
    Ok(members)
}

fn ensure_catalog_definition(
    home: &Path,
    name: &str,
    candidate_meta: &SkillsetMeta,
) -> Result<(), Error> {
    let catalog_dir = home::by_skillset_path(home).join(name);
    let meta_path = catalog_dir.join("meta.json");
    if meta_path.exists() || meta_path.is_symlink() {
        refuse_symlink(&meta_path)?;
        let existing_meta: SkillsetMeta = read_json(&meta_path, "skillset catalog meta")?;
        if &existing_meta != candidate_meta {
            let mut diffs = Vec::new();
            if existing_meta.source != candidate_meta.source {
                diffs.push("source");
            }
            if existing_meta.revision != candidate_meta.revision {
                diffs.push("revision");
            }
            if existing_meta.source_root != candidate_meta.source_root {
                diffs.push("sourceRoot");
            }
            if existing_meta.members != candidate_meta.members {
                diffs.push("members");
            }
            let hint = if !diffs.contains(&"source") && !diffs.contains(&"sourceRoot") {
                format!(
                    "\n\nHint: To update this skillset to the latest upstream commit, run:\n  tink skillset update {name}"
                )
            } else {
                String::new()
            };
            return Err(Error::msg(format!(
                "Refusing to add {name}: catalog definition already exists with differing metadata ({}) at {}{hint}",
                diffs.join(", "),
                output::display_path(&meta_path)
            )));
        }
        return Ok(());
    }

    fs::create_dir_all(&catalog_dir).map_err(|e| map_io(&catalog_dir, e))?;
    let text = serde_json::to_string_pretty(candidate_meta)
        .map_err(|e| Error::msg(format!("serialize skillset catalog meta: {e}")))?;
    fs::write(&meta_path, format!("{text}\n")).map_err(|e| map_io(&meta_path, e))?;
    Ok(())
}

pub fn add_skillset(
    project_root: &Path,
    target: &str,
    custom_name: Option<&str>,
) -> Result<SkillsetAddOutcome, Error> {
    add_skillset_at(None, project_root, target, custom_name)
}

pub(crate) fn add_skillset_at(
    home: Option<&Path>,
    project_root: &Path,
    target: &str,
    custom_name: Option<&str>,
) -> Result<SkillsetAddOutcome, Error> {
    if target.starts_with("https://") {
        add_skillset_url_at(home, project_root, target, custom_name)
    } else {
        if custom_name.is_some() {
            return Err(Error::msg(
                "Unexpected argument: custom name is only supported when adding by URL",
            ));
        }
        add_skillset_name_at(home, project_root, target)
    }
}

fn add_skillset_url_at(
    home: Option<&Path>,
    project_root: &Path,
    target: &str,
    custom_name: Option<&str>,
) -> Result<SkillsetAddOutcome, Error> {
    let github = sources::parse_github_add_source(target)?;
    if let (Some(tree_ref), Some(skill_path)) =
        (github.tree_ref.as_deref(), github.skill_path.as_deref())
    {
        git::reject_ambiguous_tree_ref(&github.remote, tree_ref, skill_path)?;
    }

    let (owner, repo) = {
        let stripped = github
            .remote
            .url
            .strip_prefix("https://github.com/")
            .ok_or_else(|| Error::msg("Expected GitHub remote URL"))?
            .trim_end_matches(".git");
        let (o, r) = stripped
            .split_once('/')
            .ok_or_else(|| Error::msg("Expected owner/repo in remote URL"))?;
        (o.to_string(), r.to_string())
    };

    let boundary_str = github.skill_path.clone().unwrap_or_else(|| ".".to_string());
    let name = derive_skillset_name(&owner, &repo, &boundary_str, custom_name)?;

    let (_clone, repository, revision) =
        git::checkout_ref(&github.remote, github.tree_ref.as_deref())?;
    validate_revision(&revision)?;

    let boundary_dir = if boundary_str == "." || boundary_str.is_empty() {
        repository.clone()
    } else {
        canonicalize_beneath(&repository, Path::new(&boundary_str))?
    };

    let members = discover_skillset_members_in_boundary(&boundary_dir)?;
    let member_names: Vec<String> = members.iter().map(|(n, _)| n.clone()).collect();
    validate_members(&member_names)?;

    let source_root = if boundary_str.is_empty() {
        ".".to_string()
    } else {
        boundary_str
    };

    let candidate_meta = SkillsetMeta {
        source: github.remote.url.clone(),
        revision: revision.clone(),
        source_root,
        members: member_names,
    };

    preflight_library_target(home, &name)?;
    let (resolved_home, _) = home::ensure_inventory_root(home)?;
    ensure_catalog_definition(&resolved_home, &name, &candidate_meta)?;

    init::ensure_project_layout_at(home, project_root)?;
    let target_dir = home::project_skills_path(project_root).join(&name);
    if target_dir.exists() || target_dir.is_symlink() {
        refuse_symlink(&target_dir)?;
        if !target_dir.is_dir() {
            return Err(Error::msg(format!(
                "Refusing to overwrite non-directory skillset: {}",
                output::display_path(&target_dir)
            )));
        }
        let receipt = read_owned_receipt(&target_dir, "installed skillset receipt")?;
        if receipt.digest_version != DIGEST_VERSION {
            return Err(Error::msg(format!(
                "Skillset receipt uses a legacy digest; run `tink skillset refresh {name}` to migrate it"
            )));
        }
        if validate_installed_tree(&target_dir, &receipt).is_err() {
            return Err(Error::msg(format!(
                "Refusing to add {name}: local modifications are present; remove it first to discard them"
            )));
        }
        if receipt_meta(&receipt) != candidate_meta {
            return Err(Error::msg(format!(
                "Skillset catalog changed for {name}; run `tink skillset refresh {name}`"
            )));
        }
        let library_write = sync_library_from_project(home, &target_dir)?;
        return Ok(SkillsetAddOutcome {
            name,
            created: false,
            library_write,
            member_count: candidate_meta.members.len(),
        });
    }

    let (installed, created) = install_from_checkout(
        &repository,
        &candidate_meta,
        &home::project_skills_path(project_root),
        &name,
    )?;
    let library_write = sync_library_from_project(home, &installed)?;
    Ok(SkillsetAddOutcome {
        name,
        created,
        library_write,
        member_count: candidate_meta.members.len(),
    })
}

fn add_skillset_name_at(
    home: Option<&Path>,
    project_root: &Path,
    name: &str,
) -> Result<SkillsetAddOutcome, Error> {
    let canonical = canonicalize_skillset_name(name)?;
    let name = canonical.as_str();
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
        return Ok(SkillsetAddOutcome {
            name: name.to_string(),
            created: false,
            library_write,
            member_count: meta.members.len(),
        });
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
    Ok(SkillsetAddOutcome {
        name: name.to_string(),
        created,
        library_write,
        member_count: meta.members.len(),
    })
}

pub fn refresh_skillset(project_root: &Path, name: &str) -> Result<bool, Error> {
    refresh_skillset_at(None, project_root, name)
}

pub(crate) fn refresh_skillset_at(
    home: Option<&Path>,
    project_root: &Path,
    name: &str,
) -> Result<bool, Error> {
    let canonical = canonicalize_skillset_name(name)?;
    let name = canonical.as_str();
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

#[derive(Debug, Clone)]
pub struct SkillsetUpdateOutcome {
    pub name: String,
    pub updated: bool,
    pub old_revision: String,
    pub new_revision: String,
    pub members: usize,
}

pub fn update_skillset(
    project_root: &Path,
    name: Option<&str>,
) -> Result<Vec<SkillsetUpdateOutcome>, Error> {
    update_skillset_at(None, project_root, name)
}

pub(crate) fn update_skillset_at(
    home: Option<&Path>,
    project_root: &Path,
    name: Option<&str>,
) -> Result<Vec<SkillsetUpdateOutcome>, Error> {
    if let Some(name) = name {
        let outcome = update_single_skillset_at(home, project_root, name)?;
        Ok(vec![outcome])
    } else {
        let installed = list_installed(project_root)?;
        if installed.is_empty() {
            return Ok(Vec::new());
        }
        // Mutations stay fail-closed: any divergent tree blocks update-all.
        if let Some(error) = installed.iter().find_map(|item| item.error.clone()) {
            return Err(Error::msg(error));
        }
        let mut outcomes = Vec::with_capacity(installed.len());
        for item in installed {
            let outcome = update_single_skillset_at(home, project_root, &item.name)?;
            outcomes.push(outcome);
        }
        Ok(outcomes)
    }
}

pub(crate) fn update_single_skillset_at(
    home: Option<&Path>,
    project_root: &Path,
    name: &str,
) -> Result<SkillsetUpdateOutcome, Error> {
    let canonical = canonicalize_skillset_name(name)?;
    let name = canonical.as_str();
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
            "Refusing to update {name}: local modifications are present"
        )));
    }
    preflight_library_target(home, name)?;

    let remote = validate_meta(&meta)?;
    let (_clone, repository, tip) = git::checkout(&remote)?;
    if tip == meta.revision {
        sync_library_from_project(home, &target)?;
        return Ok(SkillsetUpdateOutcome {
            name: name.to_string(),
            updated: false,
            old_revision: meta.revision.clone(),
            new_revision: tip,
            members: meta.members.len(),
        });
    }

    validate_revision(&tip)?;
    let boundary_str = &meta.source_root;
    let boundary_dir = if boundary_str == "." || boundary_str.is_empty() {
        repository.clone()
    } else {
        canonicalize_beneath(&repository, Path::new(boundary_str))?
    };

    let members = discover_skillset_members_in_boundary(&boundary_dir)?;
    if members.is_empty() {
        return Err(Error::msg(format!(
            "Refusing to update {name}: no valid member skills found in boundary `{boundary_str}` at upstream revision {tip}"
        )));
    }
    let member_names: Vec<String> = members.iter().map(|(n, _)| n.clone()).collect();
    validate_members(&member_names)?;

    let new_meta = SkillsetMeta {
        source: meta.source.clone(),
        revision: tip.clone(),
        source_root: meta.source_root.clone(),
        members: member_names,
    };

    let installed = replace_from_checkout(&repository, &new_meta, &skills_root, name)?;

    let (resolved_home, _) = home::ensure_inventory_root(home)?;
    let catalog_path = resolved_home
        .join("catalog")
        .join("by-skillset")
        .join(name)
        .join("meta.json");
    let text = serde_json::to_string_pretty(&new_meta)
        .map_err(|e| Error::msg(format!("serialize skillset catalog meta: {e}")))?;
    fs::write(&catalog_path, format!("{text}\n")).map_err(|e| map_io(&catalog_path, e))?;

    sync_library_from_project(home, &installed)?;

    Ok(SkillsetUpdateOutcome {
        name: name.to_string(),
        updated: true,
        old_revision: meta.revision,
        new_revision: tip,
        members: new_meta.members.len(),
    })
}

pub fn remove_skillset(project_root: &Path, name: &str) -> Result<PathBuf, Error> {
    let canonical = canonicalize_skillset_name(name)?;
    let name = canonical.as_str();
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
///
/// Returns the receipt member count on success.
pub fn validate_installed(path: &Path) -> Result<usize, Error> {
    read_installed(path).map(|installed| installed.receipt.members.len())
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
    let existing_router = if replace {
        read_router_overlay(&library.join(name))?
    } else {
        None
    };
    let staging = tempfile::Builder::new()
        .prefix(".tink-skillset-library-")
        .tempdir_in(library)
        .map_err(|e| Error::msg(format!("skillset library staging: {e}")))?;
    let staged = staging.path().join(name);
    skills::copy_skill_tree(project, &staged, &[".DS_Store"])?;
    // Project router wins; otherwise keep the library router across member sync.
    if read_router_overlay(&staged)?.is_none() {
        restore_router_overlay(&staged, existing_router)?;
    }
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
    if skills::skill_contents_equal_except(project, &target, &[ROUTER_FILE, ".DS_Store"])? {
        match (read_router_overlay(project)?, read_router_overlay(&target)?) {
            (Some(bytes), library_router) if library_router.as_ref() != Some(&bytes) => {
                write_router_overlay(&target, &bytes)?;
                return Ok(LibraryWrite::Repaired);
            }
            _ => return Ok(LibraryWrite::Unchanged),
        }
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
            skillsets.push(list_skillset_entry(&path));
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
        let digest = skills::tree_digest_legacy(&installed, DIGEST_ROOT_IGNORE).unwrap();
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
            "skills/../other",
            "/skills",
            "skills//common",
            "skills\\common",
            "./skills",
            "skills/.",
        ] {
            assert!(normalized_source_root(value).is_err(), "{value}");
        }
        assert_eq!(
            normalized_source_root("skills/common").unwrap(),
            PathBuf::from("skills/common")
        );
        assert_eq!(normalized_source_root(".").unwrap(), PathBuf::from("."));
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

    #[test]
    fn classify_entry_sorts_listing_entries() {
        let root = tempfile::tempdir().unwrap();

        assert_eq!(
            classify_entry(&root.path().join("README.md")),
            EntryClass::Ignored
        );
        assert_eq!(
            classify_entry(&root.path().join(".hidden")),
            EntryClass::Ignored
        );

        let file = root.path().join("notes.txt");
        fs::write(&file, "notes").unwrap();
        assert_eq!(classify_entry(&file), EntryClass::Unexpected);

        let plain = root.path().join("demo-skill");
        fs::create_dir(&plain).unwrap();
        assert_eq!(classify_entry(&plain), EntryClass::Standalone);

        let link = root.path().join("linked-skill");
        std::os::unix::fs::symlink(&plain, &link).unwrap();
        assert_eq!(classify_entry(&link), EntryClass::Unexpected);

        let owned = root.path().join("demo-skillset");
        fs::create_dir(&owned).unwrap();
        fs::write(owned.join(RECEIPT_FILE), "receipt").unwrap();
        assert_eq!(classify_entry(&owned), EntryClass::Skillset);

        fs::remove_file(owned.join(RECEIPT_FILE)).unwrap();
        std::os::unix::fs::symlink(owned.join("missing"), owned.join(RECEIPT_FILE)).unwrap();
        assert_eq!(classify_entry(&owned), EntryClass::Skillset);
    }

    #[test]
    fn canonicalize_skillset_name_appends_suffix_when_omitted() {
        assert_eq!(
            canonicalize_skillset_name("my-tools").unwrap(),
            "my-tools-skillset"
        );
        assert_eq!(
            canonicalize_skillset_name("my-tools-skillset").unwrap(),
            "my-tools-skillset"
        );
        assert!(canonicalize_skillset_name("bad name").is_err());
        assert!(canonicalize_skillset_name("").is_err());
    }

    #[test]
    fn list_installed_keeps_healthy_skillset_when_sibling_mismatches() {
        let temp = tempfile::tempdir().unwrap();
        let project = temp.path();
        let skills = project.join(".agents/skills");
        fs::create_dir_all(&skills).unwrap();

        let write_tree = |name: &str, member: &str, body: &str| {
            let root = skills.join(name);
            let member_dir = root.join(member);
            fs::create_dir_all(&member_dir).unwrap();
            fs::write(
                member_dir.join("SKILL.md"),
                format!(
                    "---\nname: {member}\ndescription: Fixture for {member}.\n---\n\n{body}\n"
                ),
            )
            .unwrap();
            let digest = skills::tree_digest(&root, DIGEST_ROOT_IGNORE).unwrap();
            let receipt = SkillsetReceipt {
                source: "https://github.com/example/skills.git".into(),
                revision: "a".repeat(40),
                source_root: "skills".into(),
                members: vec![member.into()],
                digest_version: DIGEST_VERSION,
                digest,
            };
            let text = serde_json::to_string_pretty(&receipt).unwrap();
            fs::write(root.join(RECEIPT_FILE), format!("{text}\n")).unwrap();
            root
        };

        let _ok = write_tree("alpha-skillset", "alpha", "ok");
        let dirty = write_tree("zeta-skillset", "zeta", "clean");
        fs::write(
            dirty.join("zeta/SKILL.md"),
            "---\nname: zeta\ndescription: Fixture for zeta.\n---\n\ndrifted\n",
        )
        .unwrap();

        let listed = list_installed(project).unwrap();
        assert_eq!(listed.len(), 2);
        assert!(listed[0].error.is_none(), "{:?}", listed[0]);
        assert_eq!(listed[0].name, "alpha-skillset");
        assert!(
            listed[1].error.as_ref().unwrap().contains("digest mismatch"),
            "{:?}",
            listed[1]
        );
        let ok = listed.iter().filter(|item| item.error.is_none()).count();
        let ok_members: usize = listed
            .iter()
            .filter(|item| item.error.is_none())
            .map(|item| item.members.len())
            .sum();
        assert_eq!((ok, ok_members), (1, 1));
    }
}
