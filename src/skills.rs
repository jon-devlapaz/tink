//! Skill discovery, validation, and install.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use crate::error::Error;
use crate::paths::{map_io, refuse_symlink};

#[derive(Debug, Clone)]
#[allow(dead_code)] // description validated at read; used by future listing.
pub struct Skill {
    pub name: String,
    pub description: String,
    pub path: PathBuf,
}

pub type Provenance = BTreeMap<String, String>;

fn valid_skill_name(name: &str) -> bool {
    if name.is_empty() || name.len() > 64 {
        return false;
    }
    let mut parts = name.split('-');
    let Some(first) = parts.next() else {
        return false;
    };
    if first.is_empty() || !first.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit()) {
        return false;
    }
    for part in parts {
        if part.is_empty() || !part.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit()) {
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
        return Err(Error::msg(format!("Invalid skill name in {}", skill_file.display())));
    }
    if require_directory_name {
        let dir_name = path
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("");
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
        description,
        path: path.to_path_buf(),
    })
}

pub fn discover(source: &Path) -> Result<Vec<Skill>, Error> {
    let source = source
        .canonicalize()
        .map_err(|e| map_io(source, e))?;
    if source.join("SKILL.md").exists() {
        return Ok(vec![read_skill(&source, false)?]);
    }
    let skills_root = source.join("skills");
    refuse_symlink(&skills_root)?;
    if !skills_root.is_dir() {
        return Err(Error::msg(format!("No skill found at {}", source.display())));
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
        return Err(Error::msg(format!("No skill found at {}", source.display())));
    }
    Ok(skills)
}

fn reject_unsafe_entries(source: &Path) -> Result<(), Error> {
    fn walk(dir: &Path, is_root: bool) -> Result<(), Error> {
        for entry in fs::read_dir(dir).map_err(|e| map_io(dir, e))? {
            let entry = entry.map_err(|e| map_io(dir, e))?;
            let path = entry.path();
            let name = entry.file_name();
            if is_root && name == *".git" {
                continue;
            }
            if path.is_symlink() {
                return Err(Error::msg(format!("Refusing to copy symlink: {}", path.display())));
            }
            let ft = entry.file_type().map_err(|e| map_io(&path, e))?;
            if ft.is_dir() {
                walk(&path, false)?;
            } else if !ft.is_file() {
                return Err(Error::msg(format!(
                    "Refusing to copy special file: {}",
                    path.display()
                )));
            }
        }
        Ok(())
    }
    walk(source, true)
}

#[derive(Debug, PartialEq, Eq)]
enum EntryKind {
    Dir,
    File(Vec<u8>),
}

fn tree_contents(root: &Path) -> Result<Option<BTreeMap<String, EntryKind>>, Error> {
    let mut contents = BTreeMap::new();
    fn walk(
        root: &Path,
        dir: &Path,
        contents: &mut BTreeMap<String, EntryKind>,
    ) -> Result<bool, Error> {
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
                return Ok(false);
            }
            let ft = entry.file_type().map_err(|e| map_io(&path, e))?;
            if ft.is_dir() {
                contents.insert(relative, EntryKind::Dir);
                if !walk(root, &path, contents)? {
                    return Ok(false);
                }
            } else if ft.is_file() {
                let bytes = fs::read(&path).map_err(|e| map_io(&path, e))?;
                contents.insert(relative, EntryKind::File(bytes));
            } else {
                return Ok(false);
            }
        }
        Ok(true)
    }
    if !walk(root, root, &mut contents)? {
        return Ok(None);
    }
    Ok(Some(contents))
}

pub fn skill_contents_equal(left: &Path, right: &Path) -> Result<bool, Error> {
    Ok(tree_contents(left)? == tree_contents(right)?)
}

pub fn copy_skill_tree(
    source: &Path,
    destination: &Path,
    root_ignore: &[&str],
) -> Result<(), Error> {
    reject_unsafe_entries(source)?;
    fn copy_dir(source: &Path, destination: &Path, root: &Path, root_ignore: &[&str]) -> Result<(), Error> {
        fs::create_dir_all(destination).map_err(|e| map_io(destination, e))?;
        for entry in fs::read_dir(source).map_err(|e| map_io(source, e))? {
            let entry = entry.map_err(|e| map_io(source, e))?;
            let name = entry.file_name();
            let name_str = name.to_string_lossy();
            if source == root && root_ignore.iter().any(|i| *i == name_str) {
                continue;
            }
            let from = entry.path();
            let to = destination.join(&name);
            if from.is_symlink() {
                return Err(Error::msg(format!("Refusing to copy symlink: {}", from.display())));
            }
            let ft = entry.file_type().map_err(|e| map_io(&from, e))?;
            if ft.is_dir() {
                copy_dir(&from, &to, root, root_ignore)?;
            } else if ft.is_file() {
                fs::copy(&from, &to).map_err(|e| map_io(&from, e))?;
            } else {
                return Err(Error::msg(format!(
                    "Refusing to copy special file: {}",
                    from.display()
                )));
            }
        }
        Ok(())
    }
    copy_dir(source, destination, source, root_ignore)
}

fn provenance_bytes(provenance: &Provenance) -> Result<Vec<u8>, Error> {
    // Stable key order (`source`, `revision`, `path`) so preflight byte-compares
    // of `.tink-source.json` stay deterministic.
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

/// Returns `true` if install should proceed; `false` if identical noop.
pub fn preflight_install(
    skill: &Skill,
    destination_root: &Path,
    provenance: Option<&Provenance>,
) -> Result<bool, Error> {
    reject_unsafe_entries(&skill.path)?;
    let mut expected = tree_contents(&skill.path)?
        .ok_or_else(|| Error::msg(format!("Skill contains unsupported entries: {}", skill.path.display())))?;
    if let Some(provenance) = provenance {
        if expected.contains_key(".tink-source.json") {
            return Err(Error::msg(format!(
                "Remote skill already contains reserved .tink-source.json: {}",
                skill.path.display()
            )));
        }
        expected.insert(
            ".tink-source.json".into(),
            EntryKind::File(provenance_bytes(provenance)?),
        );
    }
    let target = destination_root.join(&skill.name);
    if !target.exists() && !target.is_symlink() {
        return Ok(true);
    }
    if target.is_dir() {
        if let Some(existing) = tree_contents(&target)? {
            if existing == expected {
                return Ok(false);
            }
        }
    }
    Err(Error::msg(format!(
        "Refusing to overwrite existing skill: {}",
        target.display()
    )))
}

/// Install skill; returns `(installed_path, created)`.
pub fn install_local(
    skill: &Skill,
    destination_root: &Path,
    provenance: Option<&Provenance>,
) -> Result<(PathBuf, bool), Error> {
    let created = preflight_install(skill, destination_root, provenance)?;
    let target = destination_root.join(&skill.name);
    if !created {
        return Ok((target, false));
    }
    let staging = tempfile::Builder::new()
        .prefix(".tink-stage-")
        .tempdir_in(destination_root)
        .map_err(|e| Error::msg(format!("staging dir: {e}")))?;
    let staged = staging.path().join(&skill.name);
    copy_skill_tree(&skill.path, &staged, &[".git"])?;
    if let Some(provenance) = provenance {
        let receipt = staged.join(".tink-source.json");
        fs::write(&receipt, provenance_bytes(provenance)?).map_err(|e| map_io(&receipt, e))?;
    }
    fs::rename(&staged, &target).map_err(|e| map_io(&target, e))?;
    Ok((target, true))
}

/// Replace an existing imported skill after dirty-tree preflight elsewhere.
pub fn replace_verified(
    skill: &Skill,
    destination_root: &Path,
    provenance: &Provenance,
) -> Result<PathBuf, Error> {
    reject_unsafe_entries(&skill.path)?;
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
    if staged.join(".tink-source.json").exists() {
        return Err(Error::msg(format!(
            "Remote skill contains reserved .tink-source.json: {}",
            skill.path.display()
        )));
    }
    fs::write(
        staged.join(".tink-source.json"),
        provenance_bytes(provenance)?,
    )
    .map_err(|e| map_io(&staged, e))?;
    fs::rename(&target, &backup).map_err(|e| map_io(&target, e))?;
    if let Err(err) = fs::rename(&staged, &target) {
        let _ = fs::rename(&backup, &target);
        return Err(map_io(&target, err));
    }
    let _ = fs::remove_dir_all(&backup);
    Ok(target)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn provenance_bytes_stable_key_order() {
        let mut provenance = Provenance::new();
        // Insert in alphabetical order to prove serialization ignores map order.
        provenance.insert("path".into(), "skills/remote-skill".into());
        provenance.insert("revision".into(), "abc".repeat(10).chars().take(40).collect());
        provenance.insert(
            "source".into(),
            "https://github.com/example/skills.git".into(),
        );
        let bytes = provenance_bytes(&provenance).unwrap();
        let text = String::from_utf8(bytes).unwrap();
        let source_at = text.find("\"source\"").unwrap();
        let revision_at = text.find("\"revision\"").unwrap();
        let path_at = text.find("\"path\"").unwrap();
        assert!(source_at < revision_at && revision_at < path_at, "{text}");
    }
}
