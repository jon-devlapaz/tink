//! `.tink-source.json` receipt: read, validate, and stable serialize.

use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

use crate::error::Error;
use crate::paths::{map_io, refuse_symlink};
use crate::skills::Skill;
use crate::sources;

pub type Provenance = BTreeMap<String, String>;

/// Stable key order (`source`, `revision`, `path`) so preflight byte-compares
/// of `.tink-source.json` stay deterministic.
pub fn to_bytes(provenance: &Provenance) -> Result<Vec<u8>, Error> {
    for key in ["source", "revision", "path"] {
        if !provenance.contains_key(key) {
            return Err(Error::msg(format!(
                "provenance missing required field: {key}"
            )));
        }
    }
    if provenance.len() != 3 {
        return Err(Error::msg(
            "provenance must contain exactly source, revision, and path",
        ));
    }
    let body = format!(
        "{{\n  \"source\": {},\n  \"revision\": {},\n  \"path\": {}\n}}\n",
        serde_json::to_string(&provenance["source"]).map_err(|e| Error::msg(e.to_string()))?,
        serde_json::to_string(&provenance["revision"]).map_err(|e| Error::msg(e.to_string()))?,
        serde_json::to_string(&provenance["path"]).map_err(|e| Error::msg(e.to_string()))?,
    );
    Ok(body.into_bytes())
}

/// Load and validate a skill's `.tink-source.json`, if present.
pub fn read(skill: &Skill) -> Result<Option<Provenance>, Error> {
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
    let mut provenance = Provenance::new();
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

/// Write receipt bytes to `path` (caller chooses location).
pub fn write_file(path: &Path, provenance: &Provenance) -> Result<(), Error> {
    fs::write(path, to_bytes(provenance)?).map_err(|e| map_io(path, e))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn to_bytes_stable_key_order() {
        let mut provenance = Provenance::new();
        provenance.insert("path".into(), "skills/remote-skill".into());
        provenance.insert(
            "revision".into(),
            "abc".repeat(10).chars().take(40).collect(),
        );
        provenance.insert(
            "source".into(),
            "https://github.com/example/skills.git".into(),
        );
        let bytes = to_bytes(&provenance).unwrap();
        let text = String::from_utf8(bytes).unwrap();
        let source_at = text.find("\"source\"").unwrap();
        let revision_at = text.find("\"revision\"").unwrap();
        let path_at = text.find("\"path\"").unwrap();
        assert!(source_at < revision_at && revision_at < path_at, "{text}");
    }
}
