//! Project-owned skill intent (`.tink/skills.toml`) and resolved pins
//! (`.tink/skills.lock`).

use std::collections::BTreeMap;
use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};

use crate::error::Error;
use crate::output;
use crate::paths::{map_io, refuse_symlink};
use crate::provenance;
use crate::skills;
use crate::sources;
use serde::Deserialize;

pub const DIRECTORY: &str = ".tink";
pub const MANIFEST_FILE: &str = "skills.toml";
pub const LOCK_FILE: &str = "skills.lock";
const MANIFEST_VERSION: u32 = 1;
const LOCK_VERSION: u32 = 2;

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Manifest {
    pub version: u32,
    pub skills: Vec<ManifestSkill>,
    #[serde(default)]
    pub skillsets: Vec<ManifestSkillset>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ManifestSkill {
    pub name: String,
    pub source: String,
    pub path: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ManifestSkillset {
    pub name: String,
    pub source: String,
    #[serde(rename = "sourceRoot")]
    pub source_root: String,
    pub members: Vec<String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Lockfile {
    pub version: u32,
    pub skills: Vec<LockedSkill>,
    #[serde(default)]
    pub skillsets: Vec<LockedSkillset>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LockedSkill {
    pub name: String,
    pub source: String,
    pub revision: Option<String>,
    pub path: Option<String>,
    pub sha256: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LockedSkillset {
    pub name: String,
    pub source: String,
    pub revision: String,
    #[serde(rename = "sourceRoot")]
    pub source_root: String,
    pub members: Vec<String>,
    pub sha256: String,
}

#[derive(Debug)]
struct ResolvedLockfile {
    skills: Vec<ResolvedLockedSkill>,
    skillsets: Vec<ResolvedLockedSkillset>,
}

#[derive(Debug)]
struct ResolvedLockedSkillset {
    name: String,
    meta: crate::skillsets::SkillsetMeta,
    sha256: String,
}

#[derive(Debug)]
struct ResolvedLockedSkill {
    name: String,
    source: sources::LockedSource,
    sha256: String,
}

pub fn path(root: &Path) -> PathBuf {
    root.join(DIRECTORY).join(MANIFEST_FILE)
}

fn lock_path(root: &Path) -> PathBuf {
    root.join(DIRECTORY).join(LOCK_FILE)
}

pub fn load(root: &Path) -> Result<Manifest, Error> {
    let file = path(root);
    let manifest: Manifest = read_toml(root, &file, "project manifest")?;
    validate_manifest(&manifest)?;
    Ok(manifest)
}

fn load_lock(root: &Path) -> Result<ResolvedLockfile, Error> {
    let file = lock_path(root);
    let lock: Lockfile = read_toml(root, &file, "project lockfile")?;
    resolve_lock(root, lock)
}

fn read_toml<T: for<'de> Deserialize<'de>>(
    root: &Path,
    file: &Path,
    label: &str,
) -> Result<T, Error> {
    refuse_symlink(&root.join(DIRECTORY))?;
    refuse_symlink(file)?;
    if !file.is_file() {
        return Err(Error::msg(format!("Missing {label}: {}", file.display())));
    }
    let text = fs::read_to_string(file).map_err(|e| map_io(file, e))?;
    toml::from_str(&text).map_err(|e| Error::msg(format!("Invalid {label}: {e}")))
}

fn validate_manifest_version(version: u32) -> Result<(), Error> {
    if version != MANIFEST_VERSION {
        return Err(Error::msg(format!(
            "Unsupported project manifest version: {version}"
        )));
    }
    Ok(())
}

fn validate_name(name: &str, kind: &str, names: &mut BTreeMap<String, ()>) -> Result<(), Error> {
    if !skills::valid_skill_name(name) {
        return Err(Error::msg(format!("Invalid skill name in {kind}: {name}")));
    }
    if names.insert(name.to_string(), ()).is_some() {
        return Err(Error::msg(format!("Duplicate skill in {kind}: {name}")));
    }
    Ok(())
}

fn validate_manifest_skillset(
    entry: &ManifestSkillset,
    names: &mut BTreeMap<String, ()>,
) -> Result<(), Error> {
    crate::skillsets::canonicalize_skillset_name(&entry.name)?;
    validate_name(&entry.name, "project manifest skillset", names)?;
    if entry.source.is_empty() {
        return Err(Error::msg(format!(
            "Invalid skillset source in project manifest: {}",
            entry.name
        )));
    }
    if entry.source_root.is_empty()
        || entry.source_root.starts_with('/')
        || entry.source_root.contains('\\')
        || entry.source_root.contains("..")
    {
        return Err(Error::msg(format!(
            "Invalid skillset sourceRoot in project manifest: {}",
            entry.name
        )));
    }
    if entry.members.is_empty() {
        return Err(Error::msg(format!(
            "Skillset members must not be empty in project manifest: {}",
            entry.name
        )));
    }
    let mut member_names = BTreeMap::new();
    for member in &entry.members {
        validate_name(
            member,
            "project manifest skillset member",
            &mut member_names,
        )?;
    }
    Ok(())
}

fn validate_manifest(manifest: &Manifest) -> Result<(), Error> {
    validate_manifest_version(manifest.version)?;
    let mut names = BTreeMap::new();
    for skill in &manifest.skills {
        validate_name(&skill.name, "project manifest", &mut names)?;
        sources::validate_manifest_source(&skill.name, &skill.source, skill.path.as_deref())?;
    }
    let mut skillset_names = BTreeMap::new();
    for skillset in &manifest.skillsets {
        validate_manifest_skillset(skillset, &mut skillset_names)?;
    }
    Ok(())
}

fn resolve_lock(root: &Path, lock: Lockfile) -> Result<ResolvedLockfile, Error> {
    if lock.version == 1 {
        return Err(Error::msg(
            "Project lockfile version 1 uses a legacy digest; run `tink skill lock` to rewrite version 2",
        ));
    }
    if lock.version != LOCK_VERSION {
        return Err(Error::msg(format!(
            "Unsupported project lockfile version: {}",
            lock.version
        )));
    }
    let mut names = BTreeMap::new();
    let mut skills = Vec::with_capacity(lock.skills.len());
    for skill in lock.skills {
        validate_name(&skill.name, "project lockfile", &mut names)?;
        if skill.sha256.len() != 64 || !skill.sha256.chars().all(|c| c.is_ascii_hexdigit()) {
            return Err(Error::msg(format!(
                "Invalid SHA-256 for project lockfile skill: {}",
                skill.name
            )));
        }
        let source = sources::classify_locked(
            root,
            &skill.name,
            &skill.source,
            skill.revision.as_deref(),
            skill.path.as_deref(),
        )?;
        skills.push(ResolvedLockedSkill {
            name: skill.name,
            source,
            sha256: skill.sha256,
        });
    }
    let mut skillset_names = BTreeMap::new();
    let mut skillsets = Vec::with_capacity(lock.skillsets.len());
    for skillset in lock.skillsets {
        validate_name(
            &skillset.name,
            "project lockfile skillset",
            &mut skillset_names,
        )?;
        crate::skillsets::canonicalize_skillset_name(&skillset.name)?;
        if skillset.sha256.len() != 64 || !skillset.sha256.chars().all(|c| c.is_ascii_hexdigit()) {
            return Err(Error::msg(format!(
                "Invalid SHA-256 for project lockfile skillset: {}",
                skillset.name
            )));
        }
        let meta = crate::skillsets::SkillsetMeta {
            source: skillset.source.clone(),
            revision: skillset.revision.clone(),
            source_root: skillset.source_root.clone(),
            members: skillset.members.clone(),
        };
        validate_manifest_skillset(
            &ManifestSkillset {
                name: skillset.name.clone(),
                source: meta.source.clone(),
                source_root: meta.source_root.clone(),
                members: meta.members.clone(),
            },
            &mut BTreeMap::new(),
        )?;
        skillsets.push(ResolvedLockedSkillset {
            name: skillset.name,
            meta,
            sha256: skillset.sha256,
        });
    }
    Ok(ResolvedLockfile { skills, skillsets })
}

pub fn lock(root: &Path, source_args: &[String]) -> Result<usize, Error> {
    let installed = crate::check::load_project_skills(root)?;
    let mut local_sources = BTreeMap::new();
    for value in source_args {
        let (name, source) = value.split_once('=').ok_or_else(|| {
            Error::msg(format!(
                "Invalid --source mapping (expected NAME=PATH): {value}"
            ))
        })?;
        if local_sources
            .insert(name.to_string(), source.to_string())
            .is_some()
        {
            return Err(Error::msg(format!("Duplicate --source mapping: {name}")));
        }
    }

    let mut entries = Vec::new();
    for skill in &installed {
        let remote = provenance::read(skill)?;
        let (source, revision, path) = if let Some(receipt) = remote {
            (
                receipt["source"].clone(),
                Some(receipt["revision"].clone()),
                Some(receipt["path"].clone()),
            )
        } else {
            let source = if skill.name == "manage-tink" {
                // Embedded by `tink init`; it has no user-facing source path.
                sources::EMBEDDED_MANAGE_TINK.to_string()
            } else {
                local_sources.remove(&skill.name).ok_or_else(|| {
                    Error::msg(format!(
                        "Local skill needs --source {}=PATH to write the manifest",
                        skill.name
                    ))
                })?
            };
            let source_path = Path::new(&source);
            let source_path = if source_path.is_absolute() {
                source_path
                    .strip_prefix(root)
                    .map_err(|_| {
                        Error::msg(format!(
                            "Local source must be inside project: {}",
                            skill.name
                        ))
                    })?
                    .to_path_buf()
            } else {
                source_path.to_path_buf()
            };
            let source = sources::normalize_project_path(&source_path, &skill.name)?;
            (source, None, None)
        };
        let sha256 = tree_sha256(&skill.path)?;
        entries.push(LockedSkill {
            name: skill.name.clone(),
            source,
            revision,
            path,
            sha256,
        });
    }
    if let Some((name, _)) = local_sources.into_iter().next() {
        return Err(Error::msg(format!(
            "--source names an uninstalled skill: {name}"
        )));
    }
    entries.sort_by(|a, b| a.name.cmp(&b.name));

    let mut skillset_entries = Vec::new();
    for (name, meta, sha256) in crate::skillsets::list_lockable_skillsets(root)? {
        skillset_entries.push(LockedSkillset {
            name,
            source: meta.source,
            revision: meta.revision,
            source_root: meta.source_root,
            members: meta.members,
            sha256,
        });
    }
    skillset_entries.sort_by(|a, b| a.name.cmp(&b.name));

    let manifest = format_manifest_document(&entries, &skillset_entries);
    let lock = format_lock_document(&entries, &skillset_entries);
    write_atomic(root, &manifest, &lock)?;
    Ok(entries.len() + skillset_entries.len())
}

fn format_manifest_document(skills: &[LockedSkill], skillsets: &[LockedSkillset]) -> String {
    let mut text = format!("version = {MANIFEST_VERSION}\n\n");
    if skills.is_empty() {
        text.push_str("skills = []\n");
    } else {
        text.push_str(
            &skills
                .iter()
                .map(format_manifest_entry)
                .collect::<Vec<_>>()
                .join("\n"),
        );
    }
    if !skillsets.is_empty() {
        text.push('\n');
        text.push_str(
            &skillsets
                .iter()
                .map(format_manifest_skillset_entry)
                .collect::<Vec<_>>()
                .join("\n"),
        );
    }
    text
}

fn format_lock_document(skills: &[LockedSkill], skillsets: &[LockedSkillset]) -> String {
    let mut text = format!("version = {LOCK_VERSION}\n\n");
    if skills.is_empty() {
        text.push_str("skills = []\n");
    } else {
        text.push_str(
            &skills
                .iter()
                .map(format_lock_entry)
                .collect::<Vec<_>>()
                .join("\n"),
        );
    }
    if !skillsets.is_empty() {
        text.push('\n');
        text.push_str(
            &skillsets
                .iter()
                .map(format_lock_skillset_entry)
                .collect::<Vec<_>>()
                .join("\n"),
        );
    }
    text
}

fn format_manifest_entry(entry: &LockedSkill) -> String {
    let mut text = format!(
        "[[skills]]\nname = {}\nsource = {}\n",
        quote(&entry.name),
        quote(&entry.source)
    );
    if let Some(path) = &entry.path {
        text.push_str(&format!("path = {}\n", quote(path)));
    }
    text
}

fn format_manifest_skillset_entry(entry: &LockedSkillset) -> String {
    format!(
        "[[skillsets]]\nname = {}\nsource = {}\nsourceRoot = {}\nmembers = {}\n",
        quote(&entry.name),
        quote(&entry.source),
        quote(&entry.source_root),
        quote_members(&entry.members),
    )
}

fn format_lock_skillset_entry(entry: &LockedSkillset) -> String {
    format!(
        "[[skillsets]]\nname = {}\nsource = {}\nrevision = {}\nsourceRoot = {}\nmembers = {}\nsha256 = {}\n",
        quote(&entry.name),
        quote(&entry.source),
        quote(&entry.revision),
        quote(&entry.source_root),
        quote_members(&entry.members),
        quote(&entry.sha256),
    )
}

fn quote_members(members: &[String]) -> String {
    let items: Vec<String> = members.iter().map(|member| quote(member)).collect();
    format!("[{}]", items.join(", "))
}

fn format_lock_entry(entry: &LockedSkill) -> String {
    let mut text = format!(
        "[[skills]]\nname = {}\nsource = {}\n",
        quote(&entry.name),
        quote(&entry.source)
    );
    if let Some(revision) = &entry.revision {
        text.push_str(&format!("revision = {}\n", quote(revision)));
    }
    if let Some(path) = &entry.path {
        text.push_str(&format!("path = {}\n", quote(path)));
    }
    text.push_str(&format!("sha256 = {}\n", quote(&entry.sha256)));
    text
}

fn quote(value: &str) -> String {
    toml::Value::String(value.to_string()).to_string()
}

fn write_atomic(root: &Path, manifest: &str, lock: &str) -> Result<(), Error> {
    let directory = root.join(DIRECTORY);
    refuse_symlink(&directory)?;
    if !directory.exists() {
        fs::create_dir_all(&directory).map_err(|e| map_io(&directory, e))?;
    }
    if !directory.is_dir() {
        return Err(Error::msg(format!(
            "Refusing non-directory manifest root: {}",
            directory.display()
        )));
    }
    let manifest_path = directory.join(MANIFEST_FILE);
    let lock_path = directory.join(LOCK_FILE);
    refuse_symlink(&manifest_path)?;
    refuse_symlink(&lock_path)?;
    let previous_manifest = if manifest_path.is_file() {
        Some(fs::read(&manifest_path).map_err(|e| map_io(&manifest_path, e))?)
    } else {
        None
    };
    let mut previous_backup = if let Some(previous) = &previous_manifest {
        let mut backup = tempfile::Builder::new()
            .prefix(".skills-manifest-backup-")
            .tempfile_in(&directory)
            .map_err(|e| map_io(&directory, e))?;
        backup
            .write_all(previous)
            .map_err(|e| map_io(backup.path(), e))?;
        Some(backup)
    } else {
        None
    };
    let mut manifest_temp = tempfile::Builder::new()
        .prefix(".skills-toml-")
        .tempfile_in(&directory)
        .map_err(|e| map_io(&directory, e))?;
    let mut lock_temp = tempfile::Builder::new()
        .prefix(".skills-lock-")
        .tempfile_in(&directory)
        .map_err(|e| map_io(&directory, e))?;
    manifest_temp
        .write_all(manifest.as_bytes())
        .map_err(|e| map_io(manifest_temp.path(), e))?;
    lock_temp
        .write_all(lock.as_bytes())
        .map_err(|e| map_io(lock_temp.path(), e))?;
    let manifest_temp_path = manifest_temp.path().to_path_buf();
    let lock_temp_path = lock_temp.path().to_path_buf();
    fs::rename(&manifest_temp_path, &manifest_path).map_err(|e| map_io(&manifest_path, e))?;
    if let Err(error) = fs::rename(&lock_temp_path, &lock_path) {
        // Roll the first rename back so the pair remains consistent.
        let outcome = match previous_backup.as_ref() {
            Some(backup) => {
                let backup_path = backup.path().to_path_buf();
                crate::paths::restore_or_orphan(
                    || fs::rename(&backup_path, &manifest_path),
                    &backup_path,
                    &directory,
                    &manifest_path,
                    || {
                        previous_backup
                            .take()
                            .expect("backup retained for orphan fallback")
                            .keep()
                            .map(|(_, path)| path)
                            .unwrap_or_else(|_| manifest_path.clone())
                    },
                )
            }
            None => match fs::remove_file(&manifest_path) {
                Ok(()) => crate::paths::RestoreOrOrphan::Restored,
                Err(rollback_error) => crate::paths::RestoreOrOrphan::Retained {
                    recovery: manifest_path.clone(),
                    orphan_path: manifest_path.clone(),
                    rollback_error,
                    orphan_error: io::Error::other("no manifest backup to orphan"),
                },
            },
        };
        match outcome {
            crate::paths::RestoreOrOrphan::Restored => return Err(map_io(&lock_path, error)),
            crate::paths::RestoreOrOrphan::Orphaned {
                recovery,
                rollback_error,
            }
            | crate::paths::RestoreOrOrphan::Retained {
                recovery,
                rollback_error,
                ..
            } => {
                return Err(Error::msg(format!(
                    "could not publish {} ({error}); manifest rollback failed ({rollback_error}); recovery backup: {}",
                    output::display_path(&lock_path),
                    output::display_path(&recovery)
                )));
            }
        }
    }
    Ok(())
}

pub fn sync(root: &Path) -> Result<usize, Error> {
    sync_at(root, None)
}

fn sync_at(root: &Path, home: Option<&Path>) -> Result<usize, Error> {
    let manifest = load(root)?;
    let lock = load_lock(root)?;
    let declarations: BTreeMap<_, _> = manifest.skills.iter().map(|s| (&s.name, s)).collect();
    let pins: BTreeMap<_, _> = lock.skills.iter().map(|s| (&s.name, s)).collect();
    if declarations.len() != pins.len() || declarations.keys().any(|name| !pins.contains_key(name))
    {
        return Err(Error::msg(
            "Project manifest and lockfile skill sets differ",
        ));
    }
    let skillset_declarations: BTreeMap<_, _> = manifest
        .skillsets
        .iter()
        .map(|entry| (&entry.name, entry))
        .collect();
    let skillset_pins: BTreeMap<_, _> = lock
        .skillsets
        .iter()
        .map(|entry| (&entry.name, entry))
        .collect();
    if skillset_declarations.len() != skillset_pins.len()
        || skillset_declarations
            .keys()
            .any(|name| !skillset_pins.contains_key(name))
    {
        return Err(Error::msg(
            "Project manifest and lockfile skillset sets differ",
        ));
    }

    // Resolve exact source bytes, validate pins, and protect every existing
    // project destination before the first project/library write.
    let agents_root = crate::home::project_agents_path(root);
    let destination_root = crate::home::project_skills_path(root);
    crate::paths::require_directory(&agents_root)?;
    crate::paths::require_directory(&destination_root)?;
    crate::paths::require_file(&destination_root.join("README.md"))?;
    let mut prepared = Vec::with_capacity(declarations.len());
    for (name, declaration) in &declarations {
        let pin = pins[name];
        if pin.source.declared() != declaration.source
            || pin.source.source_path() != declaration.path.as_deref()
        {
            return Err(Error::msg(format!(
                "Project lockfile does not match manifest: {name}"
            )));
        }
        let candidate = crate::add::prepare_locked_skill(name, pin.source.clone())?;
        if tree_sha256(&candidate.skill().path)? != pin.sha256.to_ascii_lowercase() {
            return Err(Error::msg(format!("Skill content hash mismatch: {name}")));
        }
        skills::preflight_install(candidate.skill(), &destination_root, candidate.provenance())?
            .require_compatible(name, &destination_root)?;
        prepared.push(candidate);
    }

    let mut prepared_skillsets = Vec::with_capacity(skillset_declarations.len());
    for (name, declaration) in &skillset_declarations {
        let pin = skillset_pins[name];
        if pin.meta.source != declaration.source
            || pin.meta.source_root != declaration.source_root
            || pin.meta.members != declaration.members
        {
            return Err(Error::msg(format!(
                "Project lockfile does not match manifest skillset: {name}"
            )));
        }
        validate_locked_skillset_pin(name, &pin.meta, &pin.sha256)?;
        prepared_skillsets.push(((*name).clone(), pin.meta.clone()));
    }

    if destination_root.is_dir() {
        for installed in crate::check::load_project_skills(root)? {
            if !declarations.contains_key(&installed.name) {
                return Err(Error::msg(format!(
                    "Installed skill is not declared in manifest: {}",
                    installed.name
                )));
            }
        }
        for (name, _, _) in crate::skillsets::list_lockable_skillsets(root)? {
            if !skillset_declarations.contains_key(&name) {
                return Err(Error::msg(format!(
                    "Installed skillset is not declared in manifest: {name}"
                )));
            }
        }
    }

    preflight_locked_namespace(root, &prepared, &prepared_skillsets)?;

    // Library refusals are predictable and must be discovered before
    // publishing the first prepared project skill. Operational failures
    // such as ENOSPC can still interrupt sequential publication and are
    // recoverable by rerunning sync.
    for candidate in &prepared {
        crate::library::preflight_deposit_at(home, candidate.skill(), candidate.provenance())?;
    }
    for (name, _) in &prepared_skillsets {
        crate::skillsets::preflight_library_target(home, name)?;
    }

    for candidate in prepared {
        candidate.publish_at(root, home)?;
    }
    for (name, meta) in &prepared_skillsets {
        crate::skillsets::sync_locked_skillset_at(home, root, name, meta)?;
    }
    verify(root)
}

pub fn verify(root: &Path) -> Result<usize, Error> {
    let manifest = load(root)?;
    let lock = load_lock(root)?;
    let declarations: BTreeMap<_, _> = manifest.skills.iter().map(|s| (&s.name, s)).collect();
    let pins: BTreeMap<_, _> = lock.skills.iter().map(|s| (&s.name, s)).collect();
    if declarations.len() != pins.len() || declarations.keys().any(|name| !pins.contains_key(name))
    {
        return Err(Error::msg(
            "Project manifest and lockfile skill sets differ",
        ));
    }
    let skillset_declarations: BTreeMap<_, _> = manifest
        .skillsets
        .iter()
        .map(|entry| (&entry.name, entry))
        .collect();
    let skillset_pins: BTreeMap<_, _> = lock
        .skillsets
        .iter()
        .map(|entry| (&entry.name, entry))
        .collect();
    if skillset_declarations.len() != skillset_pins.len()
        || skillset_declarations
            .keys()
            .any(|name| !skillset_pins.contains_key(name))
    {
        return Err(Error::msg(
            "Project manifest and lockfile skillset sets differ",
        ));
    }
    for (name, declaration) in &declarations {
        let pin = pins[name];
        if pin.source.declared() != declaration.source
            || pin.source.source_path() != declaration.path.as_deref()
        {
            return Err(Error::msg(format!(
                "Project lockfile does not match manifest: {name}"
            )));
        }
    }
    for (name, declaration) in &skillset_declarations {
        let pin = skillset_pins[name];
        if pin.meta.source != declaration.source
            || pin.meta.source_root != declaration.source_root
            || pin.meta.members != declaration.members
        {
            return Err(Error::msg(format!(
                "Project lockfile does not match manifest skillset: {name}"
            )));
        }
    }
    let installed = crate::check::load_project_skills(root)?;
    let installed: BTreeMap<_, _> = installed.into_iter().map(|s| (s.name.clone(), s)).collect();
    for (name, pin) in pins {
        let skill = installed
            .get(name)
            .ok_or_else(|| Error::msg(format!("Manifest skill is not installed: {name}")))?;
        if tree_sha256(&skill.path)? != pin.sha256.to_ascii_lowercase() {
            return Err(Error::msg(format!("Skill content hash mismatch: {name}")));
        }
        let receipt = provenance::read(skill)?;
        match (pin.source.revision(), receipt) {
            (Some(revision), Some(receipt))
                if receipt.get("source").map(String::as_str) == Some(pin.source.declared())
                    && receipt.get("revision").map(String::as_str) == Some(revision)
                    && receipt.get("path").map(String::as_str) == pin.source.source_path() => {}
            (None, None) => {}
            (Some(_), _) => {
                return Err(Error::msg(format!(
                    "Skill receipt does not match lockfile: {name}"
                )));
            }
            (None, Some(_)) => {
                return Err(Error::msg(format!(
                    "Unexpected remote receipt for local skill: {name}"
                )));
            }
        }
    }
    for name in installed.keys() {
        if !declarations.contains_key(name) {
            return Err(Error::msg(format!(
                "Installed skill is not declared in manifest: {name}"
            )));
        }
    }
    let installed_skillsets = crate::skillsets::list_lockable_skillsets(root)?;
    let installed_skillsets: BTreeMap<_, _> = installed_skillsets
        .into_iter()
        .map(|(name, meta, digest)| (name, (meta, digest)))
        .collect();
    for (name, pin) in skillset_pins {
        let (meta, digest) = installed_skillsets
            .get(name)
            .ok_or_else(|| Error::msg(format!("Manifest skillset is not installed: {name}")))?;
        if meta.source != pin.meta.source
            || meta.revision != pin.meta.revision
            || meta.source_root != pin.meta.source_root
            || meta.members != pin.meta.members
        {
            return Err(Error::msg(format!(
                "Skillset receipt does not match lockfile: {name}"
            )));
        }
        if !digest.eq_ignore_ascii_case(&pin.sha256) {
            return Err(Error::msg(format!(
                "Skillset content hash mismatch: {name}"
            )));
        }
    }
    for name in installed_skillsets.keys() {
        if !skillset_declarations.contains_key(name) {
            return Err(Error::msg(format!(
                "Installed skillset is not declared in manifest: {name}"
            )));
        }
    }
    Ok(manifest.skills.len() + manifest.skillsets.len())
}

fn validate_locked_skillset_pin(
    name: &str,
    meta: &crate::skillsets::SkillsetMeta,
    sha256: &str,
) -> Result<(), Error> {
    let remote = crate::sources::RemoteSource {
        display: meta.source.clone(),
        url: meta.source.clone(),
    };
    let (_clone, repository, tip) = crate::git::checkout(&remote)?;
    let (_old_checkout, checkout) = if tip == meta.revision {
        (None, repository)
    } else {
        let (temp, checkout) = crate::git::checkout_revision(&repository, &meta.revision)?;
        (Some(temp), checkout)
    };
    let digest = crate::skillsets::locked_digest_at_checkout(&checkout, meta, name)?;
    if digest != sha256.to_ascii_lowercase() {
        return Err(Error::msg(format!(
            "Skillset content hash mismatch: {name}"
        )));
    }
    Ok(())
}

fn preflight_locked_namespace(
    root: &Path,
    prepared: &[crate::add::PreparedLockedSkill],
    prepared_skillsets: &[(String, crate::skillsets::SkillsetMeta)],
) -> Result<(), Error> {
    let mut owners = BTreeMap::<String, crate::active_skills::SkillOwner>::new();
    for candidate in prepared {
        let name = candidate.skill().name.clone();
        let owner = crate::active_skills::SkillOwner::Standalone {
            path: crate::home::project_skills_path(root).join(&name),
        };
        if let Some(existing) = owners.insert(name.clone(), owner.clone()) {
            return Err(crate::active_skills::conflict_between(
                &name, &existing, &owner,
            ));
        }
    }
    for (skillset_name, meta) in prepared_skillsets {
        let member_pairs = crate::skillsets::member_skill_names_for_meta(meta, skillset_name)?;
        for (skill_name, member_dir) in member_pairs {
            let owner = crate::active_skills::SkillOwner::Member {
                skillset: skillset_name.clone(),
                member_dir,
            };
            if let Some(existing) = owners.insert(skill_name.clone(), owner.clone()) {
                return Err(crate::active_skills::conflict_between(
                    &skill_name,
                    &existing,
                    &owner,
                ));
            }
        }
    }
    let index = crate::active_skills::ActiveSkillIndex::build(root)?;
    for (name, owner) in owners {
        let exclude = match &owner {
            crate::active_skills::SkillOwner::Member { skillset, .. }
                if prepared_skillsets
                    .iter()
                    .any(|(candidate, _)| candidate == skillset) =>
            {
                Some(skillset.as_str())
            }
            _ => None,
        };
        index.ensure_available(&name, &owner, exclude)?;
    }
    Ok(())
}

fn tree_sha256(root: &Path) -> Result<String, Error> {
    skills::tree_digest(root, &[provenance::SIDECAR_FILE])
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::home::skills_library_path;

    fn write_skill(path: &Path, name: &str) {
        fs::create_dir_all(path).unwrap();
        fs::write(
            path.join("SKILL.md"),
            format!("---\nname: {name}\ndescription: Manifest sync fixture.\n---\n\n# {name}\n"),
        )
        .unwrap();
    }

    #[test]
    fn receipt_is_excluded_from_hash() {
        let temp = tempfile::tempdir().unwrap();
        let skill = temp.path().join("skill");
        fs::create_dir_all(&skill).unwrap();
        fs::write(skill.join("SKILL.md"), "body").unwrap();
        let before = tree_sha256(&skill).unwrap();
        fs::write(skill.join(".tink-source.json"), "receipt").unwrap();
        assert_eq!(before, tree_sha256(&skill).unwrap());
    }

    #[test]
    fn tree_hash_uses_unambiguous_entry_framing() {
        let temp = tempfile::tempdir().unwrap();
        let one_file = temp.path().join("one-file");
        let two_files = temp.path().join("two-files");
        fs::create_dir_all(&one_file).unwrap();
        fs::create_dir_all(&two_files).unwrap();
        fs::write(one_file.join("SKILL.md"), b"BASE\0z\0fPAYLOAD").unwrap();
        fs::write(two_files.join("SKILL.md"), b"BASE").unwrap();
        fs::write(two_files.join("z"), b"PAYLOAD").unwrap();

        assert_ne!(
            tree_sha256(&one_file).unwrap(),
            tree_sha256(&two_files).unwrap(),
            "distinct entry framing must not collide"
        );
    }

    #[cfg(unix)]
    #[test]
    fn tree_hash_pins_regular_file_mode() {
        use std::os::unix::fs::PermissionsExt;

        let temp = tempfile::tempdir().unwrap();
        let skill = temp.path().join("skill");
        fs::create_dir_all(&skill).unwrap();
        let script = skill.join("run.sh");
        fs::write(&script, b"#!/bin/sh\n").unwrap();
        fs::set_permissions(&script, fs::Permissions::from_mode(0o755)).unwrap();
        let executable = tree_sha256(&skill).unwrap();

        fs::set_permissions(&script, fs::Permissions::from_mode(0o644)).unwrap();

        assert_ne!(tree_sha256(&skill).unwrap(), executable);
    }

    #[test]
    fn sync_preflights_every_project_destination_before_publishing() {
        let temp = tempfile::tempdir().unwrap();
        let project = temp.path().join("project");
        let home = temp.path().join("home");
        let alpha = project.join("sources/alpha");
        let beta = project.join("sources/beta");
        write_skill(&alpha, "alpha");
        write_skill(&beta, "beta");
        let alpha_hash = tree_sha256(&alpha).unwrap();
        let beta_hash = tree_sha256(&beta).unwrap();
        fs::create_dir_all(project.join(DIRECTORY)).unwrap();
        fs::write(
            path(&project),
            "version = 1\n\n[[skills]]\nname = \"alpha\"\nsource = \"sources/alpha\"\n\n[[skills]]\nname = \"beta\"\nsource = \"sources/beta\"\n",
        )
        .unwrap();
        fs::write(
            lock_path(&project),
            format!(
                "version = 2\n\n[[skills]]\nname = \"alpha\"\nsource = \"sources/alpha\"\nsha256 = \"{alpha_hash}\"\n\n[[skills]]\nname = \"beta\"\nsource = \"sources/beta\"\nsha256 = \"{beta_hash}\"\n"
            ),
        )
        .unwrap();
        let installed_beta = crate::home::project_skills_path(&project).join("beta");
        write_skill(&installed_beta, "beta");
        fs::write(installed_beta.join("local-change.txt"), "keep me\n").unwrap();
        let before = fs::read(installed_beta.join("SKILL.md")).unwrap();
        let error = sync_at(&project, Some(&home)).unwrap_err();

        assert!(
            error.to_string().contains("Refusing to overwrite"),
            "{error}"
        );
        assert!(
            !crate::home::project_skills_path(&project)
                .join("alpha")
                .exists()
        );
        assert_eq!(fs::read(installed_beta.join("SKILL.md")).unwrap(), before);
        assert!(!skills_library_path(&home).exists());
    }

    #[test]
    fn write_atomic_restores_manifest_when_lock_publish_fails() {
        let temp = tempfile::tempdir().unwrap();
        let project = temp.path();
        let directory = project.join(DIRECTORY);
        fs::create_dir_all(&directory).unwrap();
        let manifest_path = directory.join(MANIFEST_FILE);
        let lock_path = directory.join(LOCK_FILE);
        fs::write(&manifest_path, "version = 1\n\nskills = []\n").unwrap();
        fs::write(&lock_path, "version = 2\n\nskills = []\n").unwrap();
        fs::remove_file(&lock_path).unwrap();
        fs::create_dir_all(&lock_path).unwrap();

        let error = write_atomic(
            project,
            "version = 1\n\n[[skills]]\nname = \"alpha\"\nsource = \"sources/alpha\"\n",
            "version = 2\n\n[[skills]]\nname = \"alpha\"\nsource = \"sources/alpha\"\nsha256 = \"0000000000000000000000000000000000000000000000000000000000000000\"\n",
        )
        .unwrap_err();

        assert!(
            error.to_string().contains("skills.lock"),
            "expected lock publish failure: {error}"
        );
        assert!(
            !error.to_string().contains("recovery backup"),
            "manifest rollback should succeed without retaining a recovery backup: {error}"
        );
        assert!(
            orphan_files_in(&directory).is_empty(),
            "successful rollback must not leave orphan files: {error}"
        );
        assert_eq!(
            fs::read_to_string(&manifest_path).unwrap(),
            "version = 1\n\nskills = []\n"
        );
    }

    fn orphan_files_in(root: &Path) -> Vec<PathBuf> {
        fs::read_dir(root)
            .into_iter()
            .flatten()
            .flatten()
            .filter(|entry| {
                entry
                    .file_name()
                    .to_str()
                    .is_some_and(|name| name.starts_with(".tink-orphan-"))
            })
            .map(|entry| entry.path())
            .collect()
    }
}
