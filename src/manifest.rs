//! Project-owned skill intent (`.tink/skills.toml`) and resolved pins
//! (`.tink/skills.lock`).

use std::collections::BTreeMap;
use std::fs;
use std::io::Write;
use std::path::{Component, Path, PathBuf};

use serde::Deserialize;
use sha2::{Digest, Sha256};

use crate::error::Error;
use crate::paths::{map_io, refuse_symlink};
use crate::provenance;
use crate::skills;
use crate::sources;

pub const DIRECTORY: &str = ".tink";
pub const MANIFEST_FILE: &str = "skills.toml";
pub const LOCK_FILE: &str = "skills.lock";

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Manifest {
    pub version: u32,
    pub skills: Vec<ManifestSkill>,
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
pub struct Lockfile {
    pub version: u32,
    pub skills: Vec<LockedSkill>,
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

fn load_lock(root: &Path) -> Result<Lockfile, Error> {
    let file = lock_path(root);
    let lock: Lockfile = read_toml(root, &file, "project lockfile")?;
    validate_lock(&lock)?;
    Ok(lock)
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

fn validate_version(version: u32) -> Result<(), Error> {
    if version != 1 {
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

fn normalize_project_path(path: &Path, name: &str) -> Result<String, Error> {
    if path.as_os_str().is_empty() || path.to_string_lossy().contains('\\') {
        return Err(Error::msg(format!(
            "Project source must be a non-empty relative path: {name}"
        )));
    }
    let mut normalized = PathBuf::new();
    for component in path.components() {
        match component {
            Component::Normal(part) => normalized.push(part),
            Component::CurDir => {}
            Component::ParentDir | Component::RootDir | Component::Prefix(_) => {
                return Err(Error::msg(format!(
                    "Project source must stay inside project: {name}"
                )));
            }
        }
    }
    if normalized.as_os_str().is_empty() {
        return Err(Error::msg(format!(
            "Project source must be a non-empty relative path: {name}"
        )));
    }
    Ok(normalized.to_string_lossy().replace('\\', "/"))
}

fn validate_source(
    source: &str,
    path: Option<&str>,
    revision: Option<&str>,
    name: &str,
) -> Result<(), Error> {
    if source.starts_with("https://") {
        let parsed = sources::parse_remote(source)?;
        if parsed.url != source {
            return Err(Error::msg(format!(
                "Manifest source must be canonical GitHub HTTPS URL: {name}"
            )));
        }
        let revision = revision
            .ok_or_else(|| Error::msg(format!("Remote lock entry missing revision: {name}")))?;
        if !(revision.len() == 40 || revision.len() == 64)
            || !revision.chars().all(|c| c.is_ascii_hexdigit())
        {
            return Err(Error::msg(format!(
                "Invalid revision for project lockfile skill: {name}"
            )));
        }
        let source_path =
            path.ok_or_else(|| Error::msg(format!("Remote lock entry missing path: {name}")))?;
        if source_path.starts_with('/') || source_path.contains("..") || source_path.contains('\\')
        {
            return Err(Error::msg(format!(
                "Invalid source path for project lockfile skill: {name}"
            )));
        }
    } else {
        if revision.is_some() || path.is_some() {
            return Err(Error::msg(format!(
                "Local lock entry has remote fields: {name}"
            )));
        }
        normalize_project_path(Path::new(source), name)?;
    }
    Ok(())
}

fn validate_manifest(manifest: &Manifest) -> Result<(), Error> {
    validate_version(manifest.version)?;
    let mut names = BTreeMap::new();
    for skill in &manifest.skills {
        validate_name(&skill.name, "project manifest", &mut names)?;
        if skill.source.starts_with("https://") {
            let parsed = sources::parse_remote(&skill.source)?;
            if parsed.url != skill.source {
                return Err(Error::msg(format!(
                    "Manifest source must be canonical GitHub HTTPS URL: {}",
                    skill.name
                )));
            }
            if let Some(path) = &skill.path {
                if path.starts_with('/') || path.contains("..") || path.contains('\\') {
                    return Err(Error::msg(format!(
                        "Invalid source path for project manifest skill: {}",
                        skill.name
                    )));
                }
            }
        } else if skill.path.is_some() {
            return Err(Error::msg(format!(
                "Local manifest skill has remote path: {}",
                skill.name
            )));
        }
    }
    Ok(())
}

fn validate_lock(lock: &Lockfile) -> Result<(), Error> {
    validate_version(lock.version)?;
    let mut names = BTreeMap::new();
    for skill in &lock.skills {
        validate_name(&skill.name, "project lockfile", &mut names)?;
        if skill.sha256.len() != 64 || !skill.sha256.chars().all(|c| c.is_ascii_hexdigit()) {
            return Err(Error::msg(format!(
                "Invalid SHA-256 for project lockfile skill: {}",
                skill.name
            )));
        }
        validate_source(
            &skill.source,
            skill.path.as_deref(),
            skill.revision.as_deref(),
            &skill.name,
        )?;
    }
    Ok(())
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
                "tink:embedded/manage-tink".to_string()
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
            let source = normalize_project_path(&source_path, &skill.name)?;
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
    let manifest = entries
        .iter()
        .map(|entry| format_manifest_entry(entry))
        .collect::<Vec<_>>()
        .join("\n");
    let lock = entries
        .iter()
        .map(|entry| format_lock_entry(entry))
        .collect::<Vec<_>>()
        .join("\n");
    write_atomic(
        root,
        &format!("version = 1\n\n{manifest}"),
        &format!("version = 1\n\n{lock}"),
    )?;
    Ok(entries.len())
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
        match previous_manifest {
            Some(previous) => {
                let _ = fs::write(&manifest_path, previous);
            }
            None => {
                let _ = fs::remove_file(&manifest_path);
            }
        }
        return Err(map_io(&lock_path, error));
    }
    Ok(())
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
    for (name, declaration) in &declarations {
        let pin = pins[name];
        if pin.source != declaration.source || pin.path != declaration.path {
            return Err(Error::msg(format!(
                "Project lockfile does not match manifest: {name}"
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
        match (&pin.revision, receipt) {
            (Some(revision), Some(receipt))
                if receipt.get("source") == Some(&pin.source)
                    && receipt.get("revision") == Some(revision)
                    && receipt.get("path") == pin.path.as_ref() => {}
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
    Ok(manifest.skills.len())
}

fn tree_sha256(root: &Path) -> Result<String, Error> {
    fn walk(root: &Path, dir: &Path, digest: &mut Sha256) -> Result<(), Error> {
        let mut entries = fs::read_dir(dir)
            .map_err(|e| map_io(dir, e))?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| map_io(dir, e))?;
        entries.sort_by_key(|entry| entry.file_name());
        for entry in entries {
            let path = entry.path();
            let relative = path
                .strip_prefix(root)
                .map_err(|_| Error::msg("Manifest tree path escaped root"))?
                .to_string_lossy()
                .replace('\\', "/");
            if relative == ".tink-source.json" {
                continue;
            }
            refuse_symlink(&path)?;
            let file_type = entry.file_type().map_err(|e| map_io(&path, e))?;
            digest.update(relative.as_bytes());
            digest.update([0]);
            if file_type.is_dir() {
                digest.update([b'd']);
                walk(root, &path, digest)?;
            } else if file_type.is_file() {
                digest.update([b'f']);
                digest.update(fs::read(&path).map_err(|e| map_io(&path, e))?);
            } else {
                return Err(Error::msg(format!(
                    "Refusing special file in manifest skill: {}",
                    path.display()
                )));
            }
            digest.update([0]);
        }
        Ok(())
    }
    let mut digest = Sha256::new();
    walk(root, root, &mut digest)?;
    Ok(format!("{:x}", digest.finalize()))
}

#[cfg(test)]
mod tests {
    use super::*;
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
}
