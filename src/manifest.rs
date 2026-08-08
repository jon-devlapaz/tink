//! Project-owned skill manifest (`.tink/skills.toml`).

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use serde::Deserialize;
use sha2::{Digest, Sha256};

use crate::error::Error;
use crate::paths::{map_io, refuse_symlink};
use crate::skills;

pub const DIRECTORY: &str = ".tink";
pub const FILE: &str = "skills.toml";

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Manifest {
    pub version: u32,
    #[serde(rename = "skills")]
    pub skills: Vec<ManifestSkill>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ManifestSkill {
    pub name: String,
    pub source: String,
    pub revision: Option<String>,
    pub path: Option<String>,
    pub sha256: String,
}

pub fn path(root: &Path) -> PathBuf {
    root.join(DIRECTORY).join(FILE)
}

pub fn load(root: &Path) -> Result<Manifest, Error> {
    let file = path(root);
    refuse_symlink(&root.join(DIRECTORY))?;
    refuse_symlink(&file)?;
    if !file.is_file() {
        return Err(Error::msg(format!(
            "Missing project manifest: {}",
            file.display()
        )));
    }
    let text = fs::read_to_string(&file).map_err(|e| map_io(&file, e))?;
    let manifest: Manifest =
        toml::from_str(&text).map_err(|e| Error::msg(format!("Invalid project manifest: {e}")))?;
    validate(&manifest)?;
    Ok(manifest)
}

fn validate(manifest: &Manifest) -> Result<(), Error> {
    if manifest.version != 1 {
        return Err(Error::msg(format!(
            "Unsupported project manifest version: {}",
            manifest.version
        )));
    }
    let mut names = BTreeMap::new();
    for skill in &manifest.skills {
        if !skills::valid_skill_name(&skill.name) {
            return Err(Error::msg(format!(
                "Invalid skill name in project manifest: {}",
                skill.name
            )));
        }
        if names.insert(&skill.name, ()).is_some() {
            return Err(Error::msg(format!(
                "Duplicate skill in project manifest: {}",
                skill.name
            )));
        }
        if skill.sha256.len() != 64 || !skill.sha256.chars().all(|c| c.is_ascii_hexdigit()) {
            return Err(Error::msg(format!(
                "Invalid SHA-256 for manifest skill: {}",
                skill.name
            )));
        }
        if skill.source.starts_with("https://") {
            let revision = skill.revision.as_deref().ok_or_else(|| {
                Error::msg(format!(
                    "Remote manifest skill missing revision: {}",
                    skill.name
                ))
            })?;
            if revision.len() != 40 || !revision.chars().all(|c| c.is_ascii_hexdigit()) {
                return Err(Error::msg(format!(
                    "Invalid revision for manifest skill: {}",
                    skill.name
                )));
            }
            let source_path = skill.path.as_deref().ok_or_else(|| {
                Error::msg(format!(
                    "Remote manifest skill missing path: {}",
                    skill.name
                ))
            })?;
            if source_path.starts_with('/')
                || source_path.contains("..")
                || source_path.contains('\\')
            {
                return Err(Error::msg(format!(
                    "Invalid source path for manifest skill: {}",
                    skill.name
                )));
            }
        } else {
            if skill.revision.is_some() || skill.path.is_some() {
                return Err(Error::msg(format!(
                    "Local manifest skill has remote fields: {}",
                    skill.name
                )));
            }
            let source = Path::new(&skill.source);
            if source.is_absolute() || skill.source.starts_with("../") || skill.source == ".." {
                return Err(Error::msg(format!(
                    "Manifest source must be project-relative: {}",
                    skill.name
                )));
            }
        }
    }
    Ok(())
}

pub fn verify(root: &Path) -> Result<usize, Error> {
    let manifest = load(root)?;
    let installed = crate::check::load_project_skills(root)?;
    let installed: BTreeMap<_, _> = installed.into_iter().map(|s| (s.name.clone(), s)).collect();
    for declared in &manifest.skills {
        let skill = installed.get(&declared.name).ok_or_else(|| {
            Error::msg(format!(
                "Manifest skill is not installed: {}",
                declared.name
            ))
        })?;
        let actual = tree_sha256(&skill.path)?;
        if actual != declared.sha256.to_ascii_lowercase() {
            return Err(Error::msg(format!(
                "Skill content hash mismatch: {}",
                declared.name
            )));
        }
    }
    for name in installed.keys() {
        if !manifest.skills.iter().any(|skill| &skill.name == name) {
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
    fn validates_manifest_and_ignores_receipt_in_hash() {
        let temp = tempfile::tempdir().unwrap();
        let skill = temp.path().join("skill");
        fs::create_dir_all(&skill).unwrap();
        fs::write(skill.join("SKILL.md"), "body").unwrap();
        let before = tree_sha256(&skill).unwrap();
        fs::write(skill.join(".tink-source.json"), "receipt").unwrap();
        assert_eq!(before, tree_sha256(&skill).unwrap());
    }

    #[test]
    fn rejects_duplicate_names() {
        let mut manifest = Manifest {
            version: 1,
            skills: vec![
                ManifestSkill {
                    name: "same".into(),
                    source: "./a".into(),
                    revision: None,
                    path: None,
                    sha256: "0".repeat(64),
                },
                ManifestSkill {
                    name: "same".into(),
                    source: "./b".into(),
                    revision: None,
                    path: None,
                    sha256: "0".repeat(64),
                },
            ],
        };
        assert!(validate(&manifest).is_err());
        manifest.skills.pop();
        assert!(validate(&manifest).is_ok());
    }
}
