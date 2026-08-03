//! Offline project skill validation (`tink check`).

use std::fs;
use std::path::Path;

use crate::error::Error;
use crate::paths::{map_io, refuse_symlink};
use crate::skills::{self, Skill};
use crate::sources;

/// Validate project skills. No writes. No network for local skills; provenance
/// shape is checked without fetching.
pub fn check_project(root: &Path) -> Result<Vec<Skill>, Error> {
    let agents = root.join(".agents");
    let skills_root = agents.join("skills");
    refuse_symlink(&agents)?;
    refuse_symlink(&skills_root)?;
    if !skills_root.is_dir() {
        return Err(Error::msg("Missing .agents/skills"));
    }

    let mut entries: Vec<_> = fs::read_dir(&skills_root)
        .map_err(|e| map_io(&skills_root, e))?
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .collect();
    entries.sort();

    let mut skills = Vec::new();
    for path in entries {
        let name = path
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("");
        if name == "README.md" || name.starts_with('.') {
            continue;
        }
        if path.is_symlink() || !path.is_dir() {
            return Err(Error::msg(format!(
                "Unexpected entry in .agents/skills: {name}"
            )));
        }
        let skill = skills::read_skill(&path, true)?;
        read_provenance(&skill)?;
        skills.push(skill);
    }
    Ok(skills)
}

pub fn read_provenance(skill: &Skill) -> Result<Option<skills::Provenance>, Error> {
    let sidecar = skill.path.join(".tink-source.json");
    if !sidecar.exists() && !sidecar.is_symlink() {
        return Ok(None);
    }
    refuse_symlink(&sidecar)?;
    if !sidecar.is_file() {
        return Err(Error::msg(format!(
            "Provenance must be a regular file: {}",
            sidecar.display()
        )));
    }
    let text = fs::read_to_string(&sidecar).map_err(|e| map_io(&sidecar, e))?;
    let value: serde_json::Value =
        serde_json::from_str(&text).map_err(|e| Error::msg(format!("Invalid provenance JSON: {e}")))?;
    let obj = value
        .as_object()
        .ok_or_else(|| Error::msg("Provenance must contain exactly source, revision, and path"))?;
    let keys: std::collections::BTreeSet<&str> = obj.keys().map(|s| s.as_str()).collect();
    let expected: std::collections::BTreeSet<&str> =
        ["source", "revision", "path"].into_iter().collect();
    if keys != expected {
        return Err(Error::msg(
            "Provenance must contain exactly source, revision, and path",
        ));
    }
    let mut provenance = skills::Provenance::new();
    for key in ["source", "revision", "path"] {
        let Some(serde_json::Value::String(s)) = obj.get(key) else {
            return Err(Error::msg(format!(
                "Provenance fields must be non-empty strings: {}",
                sidecar.display()
            )));
        };
        if s.is_empty() {
            return Err(Error::msg(format!(
                "Provenance fields must be non-empty strings: {}",
                sidecar.display()
            )));
        }
        provenance.insert(key.into(), s.clone());
    }
    let source = sources::parse_remote(&provenance["source"])?;
    if source.url != provenance["source"] {
        return Err(Error::msg(format!(
            "Provenance source must be a canonical GitHub HTTPS URL: {}",
            sidecar.display()
        )));
    }
    let revision = &provenance["revision"];
    if !(revision.len() == 40 || revision.len() == 64)
        || !revision.chars().all(|c| c.is_ascii_hexdigit())
    {
        return Err(Error::msg(format!(
            "Provenance revision must be a full Git object ID: {}",
            sidecar.display()
        )));
    }
    let path = &provenance["path"];
    if path.starts_with('/') || path.contains("..") || path.contains('\\') {
        return Err(Error::msg(format!(
            "Provenance path must be normalized and relative: {}",
            sidecar.display()
        )));
    }
    Ok(Some(provenance))
}
