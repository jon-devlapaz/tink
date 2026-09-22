//! Read-only inspection of GitHub source structure.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

use crate::error::Error;
use crate::git;
use crate::output;
use crate::skills;
use crate::sources::{
    RemoteSource, github_part_ok, github_tree_segment_ok, is_public_github_https, url_lite,
};
use serde::Serialize;

#[derive(Debug)]
struct ParsedUrl {
    remote: RemoteSource,
    requested_ref: Option<String>,
    boundary: PathBuf,
    boundary_display: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct DiscoveredSkill {
    pub name: String,
    pub path: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct InferredSkillset {
    pub name: Option<String>,
    pub source_root: String,
    pub member_names: Vec<String>,
    pub installable: bool,
    pub exclusion_reasons: Vec<String>,
    pub provenance: String,
}

impl InferredSkillset {
    pub fn member_count(&self) -> usize {
        self.member_names.len()
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct InspectionReport {
    pub repository: String,
    pub revision: String,
    pub boundary: String,
    pub skillsets: Vec<InferredSkillset>,
    pub skills: Vec<DiscoveredSkill>,
    pub diagnostics: Vec<String>,
}

fn inspection_boundary(checkout: &Path, relative: &Path) -> Result<PathBuf, Error> {
    let boundary = if relative.as_os_str().is_empty() {
        checkout
            .canonicalize()
            .map_err(|error| crate::paths::map_io(checkout, error))?
    } else {
        crate::paths::canonicalize_beneath(checkout, relative).map_err(|error| {
            Error::msg(format!(
                "Inspection boundary {} is invalid: {error}",
                output::display_path(relative)
            ))
        })?
    };
    if !boundary.is_dir() {
        return Err(Error::msg(format!(
            "Inspection boundary is not a directory: {}",
            output::display_path(relative)
        )));
    }
    Ok(boundary)
}

pub fn inspect(url: &str) -> Result<InspectionReport, Error> {
    let parsed = parse_url(url)?;
    reject_ambiguous_ref(&parsed)?;
    let (_temp, checkout, revision) =
        git::checkout_ref(&parsed.remote, parsed.requested_ref.as_deref())?;
    let boundary = inspection_boundary(&checkout, &parsed.boundary)?;

    let mut diagnostics = Vec::new();
    let mut discovered = Vec::new();
    let scan = skills::discover_recursive(&boundary)?;
    for invalid in scan.invalid {
        let relative_directory = relative_posix(&checkout, &invalid.path);
        let skill_file = invalid.path.join("SKILL.md");
        let relative_skill_file = format!("{relative_directory}/SKILL.md");
        let detail = invalid
            .detail
            .replace(skill_file.to_string_lossy().as_ref(), &relative_skill_file);
        diagnostics.push(format!(
            "invalid SKILL.md at {relative_directory}: {detail}"
        ));
    }
    for skill in scan.skills {
        let path = relative_posix(&checkout, &skill.path);
        let directory_name = skill
            .path
            .file_name()
            .and_then(|value| value.to_str())
            .unwrap_or("");
        if skill.name != directory_name {
            let directory_name = output::escape_untrusted(directory_name);
            diagnostics.push(format!(
                "skill name {} does not match directory {directory_name} at {path}",
                skill.name
            ));
        }
        discovered.push(DiscoveredSkill {
            name: skill.name,
            path,
        });
    }
    discovered.sort_by(|left, right| left.path.cmp(&right.path));
    diagnostics.extend(duplicate_diagnostics(&discovered));
    diagnostics.extend(overlap_diagnostics(&discovered));
    let skillsets = infer_skillsets(&checkout, &boundary, &discovered, &mut diagnostics)?;
    for skillset in &skillsets {
        if skillset.name.is_none() && skillset.source_root != "." {
            diagnostics.push(format!(
                "no valid canonical skillset name for {}",
                skillset.source_root
            ));
        }
    }

    Ok(InspectionReport {
        repository: parsed.remote.url,
        revision,
        boundary: parsed.boundary_display,
        skillsets,
        skills: discovered,
        diagnostics,
    })
}

fn reject_ambiguous_ref(parsed: &ParsedUrl) -> Result<(), Error> {
    let Some(requested_ref) = parsed.requested_ref.as_deref() else {
        return Ok(());
    };
    if parsed.boundary.as_os_str().is_empty() {
        return Ok(());
    }
    for segment in parsed.boundary.iter() {
        segment
            .to_str()
            .ok_or_else(|| Error::msg("Inspection URL contains non-UTF-8 path data"))?;
    }
    git::reject_ambiguous_tree_ref_for(
        &parsed.remote,
        requested_ref,
        &parsed.boundary_display,
        "Inspection URL",
    )
}

fn parse_url(value: &str) -> Result<ParsedUrl, Error> {
    let parsed = value
        .parse::<url_lite::Url>()
        .map_err(|_| Error::msg("Inspection URL must be a public GitHub HTTPS URL"))?;
    if !is_public_github_https(&parsed) {
        return Err(Error::msg(
            "Inspection URL must be a public GitHub HTTPS URL",
        ));
    }
    let parts: Vec<&str> = parsed
        .path
        .trim_matches('/')
        .split('/')
        .filter(|part| !part.is_empty())
        .collect();
    if parts.len() < 2 || parts[0].is_empty() || parts[1].is_empty() {
        return Err(Error::msg(
            "Inspection URL must identify a GitHub repository",
        ));
    }
    let owner = parts[0];
    let repository = parts[1].trim_end_matches(".git");
    if !github_part_ok(owner) || !github_part_ok(repository) {
        return Err(Error::msg(
            "Inspection URL has an invalid GitHub repository",
        ));
    }
    let remote = RemoteSource {
        display: value.to_string(),
        url: format!("https://github.com/{owner}/{repository}.git"),
    };
    if parts.len() == 2 {
        return Ok(ParsedUrl {
            remote,
            requested_ref: None,
            boundary: PathBuf::new(),
            boundary_display: ".".to_string(),
        });
    }
    if parts[2] != "tree" || parts.len() < 4 {
        return Err(Error::msg(
            "Inspection URL must use the GitHub /tree/<ref>/<path> form",
        ));
    }
    let requested_ref = parts[3];
    if requested_ref.is_empty() {
        return Err(Error::msg("Inspection URL is missing the Git ref"));
    }
    let boundary_parts = &parts[4..];
    let boundary = if boundary_parts.is_empty() {
        PathBuf::new()
    } else {
        let mut boundary = PathBuf::new();
        for part in boundary_parts {
            if !github_tree_segment_ok(part) {
                return Err(Error::msg(
                    "Inspection boundary must stay inside the repository",
                ));
            }
            boundary.push(part);
        }
        boundary
    };
    let boundary_display = if boundary.as_os_str().is_empty() {
        ".".to_string()
    } else {
        boundary.to_string_lossy().replace('\\', "/")
    };
    Ok(ParsedUrl {
        remote,
        requested_ref: Some(requested_ref.to_string()),
        boundary,
        boundary_display,
    })
}

fn relative_posix(root: &Path, path: &Path) -> String {
    let relative = path.strip_prefix(root).unwrap_or(path).to_string_lossy();
    #[cfg(windows)]
    let relative = relative.replace('\\', "/");
    #[cfg(not(windows))]
    let relative = relative.into_owned();
    output::escape_untrusted(&relative)
}

fn duplicate_diagnostics(skills: &[DiscoveredSkill]) -> Vec<String> {
    let mut by_name: BTreeMap<&str, Vec<&str>> = BTreeMap::new();
    for skill in skills {
        by_name.entry(&skill.name).or_default().push(&skill.path);
    }
    by_name
        .into_iter()
        .filter(|(_, paths)| paths.len() > 1)
        .map(|(name, paths)| format!("duplicate skill name: {name} ({})", paths.join(", ")))
        .collect()
}

fn overlap_diagnostics(skills: &[DiscoveredSkill]) -> Vec<String> {
    let mut diagnostics = Vec::new();
    for (index, ancestor) in skills.iter().enumerate() {
        for descendant in skills.iter().skip(index + 1) {
            if descendant.path.starts_with(&(ancestor.path.clone() + "/")) {
                diagnostics.push(format!(
                    "overlapping skill roots: {} and {}",
                    ancestor.path, descendant.path
                ));
            }
        }
    }
    diagnostics
}

fn is_fixture_peer(name: &str) -> bool {
    matches!(
        name,
        "deprecated"
            | "test"
            | "tests"
            | "fixture"
            | "fixtures"
            | "example"
            | "examples"
            | "docs"
            | "documentation"
            | "empty"
    )
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

fn skillset_name_for_directory(checkout: &Path, directory: &Path) -> Option<String> {
    if directory == checkout {
        return None;
    }
    let source_root = relative_posix(checkout, directory);
    let folder = directory
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or("");
    let base = if folder == "skills" {
        if source_root == "skills" {
            return None;
        }
        source_root
            .trim_end_matches("/skills")
            .rsplit('/')
            .next()
            .map(sanitize_for_skill_name)?
    } else if skills::valid_skill_name(folder) {
        sanitize_for_skill_name(folder)
    } else {
        return None;
    };
    if base.is_empty() {
        return None;
    }
    let candidate = if base.ends_with("-skillset") {
        base
    } else {
        format!("{base}-skillset")
    };
    skills::valid_skill_name(&candidate).then_some(candidate)
}

fn analyze_skillset_directory(
    checkout: &Path,
    directory: &Path,
) -> Result<InferredSkillset, Error> {
    let source_root = if directory == checkout {
        ".".to_string()
    } else {
        relative_posix(checkout, directory)
    };
    let folder = directory
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or("")
        .to_string();
    let mut exclusion_reasons = Vec::new();
    if is_fixture_peer(&folder) {
        exclusion_reasons.push(format!("fixture or documentation peer: {folder}"));
    }

    let mut member_names = Vec::new();
    let mut has_nested_category = false;
    for child in regular_children(directory)? {
        let member_dir = child
            .file_name()
            .and_then(|value| value.to_str())
            .unwrap_or("")
            .to_string();
        let skill_file = child.join("SKILL.md");
        if skill_file.is_file() {
            match skills::read_skill(&child, true) {
                Ok(skill) => {
                    if skill.name != member_dir {
                        exclusion_reasons.push(format!(
                            "member directory {member_dir} does not match skill name {}",
                            skill.name
                        ));
                    } else if let Some(descendant) = skills::find_descendant_skill_md(&child)? {
                        exclusion_reasons.push(format!(
                            "member {} contains nested SKILL.md at {}",
                            output::escape_untrusted(&member_dir),
                            output::escape_untrusted(&descendant.to_string_lossy())
                        ));
                    } else {
                        member_names.push(member_dir);
                    }
                }
                Err(error) => exclusion_reasons.push(format!(
                    "invalid member skill at {}: {error}",
                    output::escape_untrusted(&member_dir)
                )),
            }
            continue;
        }
        if regular_children(&child)?
            .iter()
            .any(|nested| nested.join("SKILL.md").is_file())
        {
            has_nested_category = true;
        }
    }
    member_names.sort();

    if member_names.is_empty() {
        exclusion_reasons.push("no immediate installable member directories".to_string());
    }
    if has_nested_category {
        exclusion_reasons.push(
            "nested category directories remain below sourceRoot; inspect a narrower tree URL"
                .to_string(),
        );
    }

    let name = skillset_name_for_directory(checkout, directory);
    if name.is_none() && source_root != "." {
        exclusion_reasons.push(format!(
            "no valid canonical skillset name for {source_root}"
        ));
    }

    let installable = name.is_some() && !member_names.is_empty() && exclusion_reasons.is_empty();
    Ok(InferredSkillset {
        name,
        source_root,
        member_names,
        installable,
        exclusion_reasons,
        provenance: "structural-inference".to_string(),
    })
}

fn infer_skillsets(
    checkout: &Path,
    boundary: &Path,
    skills: &[DiscoveredSkill],
    diagnostics: &mut Vec<String>,
) -> Result<Vec<InferredSkillset>, Error> {
    let boundary_prefix = relative_posix(checkout, boundary);
    if skills.iter().any(|skill| skill.path == boundary_prefix) {
        return Ok(Vec::new());
    }
    if skills.is_empty() {
        diagnostics.push("no valid skills found in this boundary".to_string());
        return Ok(Vec::new());
    }
    let direct: Vec<&DiscoveredSkill> = skills
        .iter()
        .filter(|skill| {
            Path::new(&skill.path)
                .parent()
                .map(|parent| parent == Path::new(&boundary_prefix))
                == Some(true)
        })
        .collect();
    if !direct.is_empty() {
        if boundary_prefix == "skills" {
            return Ok(Vec::new());
        }
        let has_nested_collection = regular_children(boundary)?.iter().any(|child| {
            let child_path = relative_posix(checkout, child);
            skills.iter().any(|skill| {
                skill.path.starts_with(&(child_path.clone() + "/")) && skill.path != child_path
            })
        });
        if has_nested_collection {
            diagnostics.push(
                "mixed skill layout: direct skills and nested skill collections coexist; inspect a narrower GitHub tree URL to select a skillset"
                    .to_string(),
            );
            return Ok(Vec::new());
        }
        return Ok(vec![analyze_skillset_directory(checkout, boundary)?]);
    }
    let mut children = regular_children(boundary)?;
    children.sort();
    let descendants: BTreeSet<PathBuf> = children
        .iter()
        .filter(|child| {
            let child_path = relative_posix(checkout, child);
            skills.iter().any(|skill| {
                skill.path == child_path || skill.path.starts_with(&(child_path.clone() + "/"))
            }) || child.join("SKILL.md").is_file()
        })
        .cloned()
        .collect();
    if descendants.len() >= 2 {
        return children
            .iter()
            .map(|child| analyze_skillset_directory(checkout, child))
            .collect();
    }
    if descendants.len() == 1 {
        let child = descendants
            .iter()
            .next()
            .expect("descendants length checked");
        return infer_skillsets(checkout, child, skills, diagnostics);
    }
    diagnostics.push("skillsets could not be inferred from this boundary".to_string());
    Ok(Vec::new())
}

fn regular_children(directory: &Path) -> Result<Vec<PathBuf>, Error> {
    let mut children = Vec::new();
    for entry in fs::read_dir(directory)
        .map_err(|error| Error::msg(format!("Could not inspect source: {error}")))?
    {
        let entry =
            entry.map_err(|error| Error::msg(format!("Could not inspect source: {error}")))?;
        let path = entry.path();
        let name = path
            .file_name()
            .and_then(|value| value.to_str())
            .unwrap_or("");
        let metadata = fs::symlink_metadata(&path)
            .map_err(|error| Error::msg(format!("Could not inspect source: {error}")))?;
        if !name.starts_with('.') && metadata.is_dir() && !metadata.file_type().is_symlink() {
            children.push(path);
        }
    }
    Ok(children)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[cfg(unix)]
    #[test]
    fn inspection_boundary_refuses_symlinked_ancestor() {
        let temp = tempfile::tempdir().unwrap();
        let checkout = temp.path().join("checkout");
        let outside = temp.path().join("outside");
        fs::create_dir_all(&checkout).unwrap();
        fs::create_dir_all(outside.join("skills")).unwrap();
        std::os::unix::fs::symlink(&outside, checkout.join("jump")).unwrap();

        let error = inspection_boundary(&checkout, Path::new("jump/skills"))
            .expect_err("ancestor symlink must be refused");

        assert!(error.to_string().contains("symlink"), "{error}");
    }

    #[test]
    fn analyze_skillset_marks_fixture_peer_non_installable() {
        let temp = tempfile::tempdir().unwrap();
        let checkout = temp.path();
        let deprecated = checkout.join("skills/deprecated");
        fs::create_dir_all(&deprecated).unwrap();
        fs::write(deprecated.join("README.md"), "empty\n").unwrap();

        let skillset = analyze_skillset_directory(checkout, &deprecated).unwrap();
        assert!(!skillset.installable);
        assert!(
            skillset
                .exclusion_reasons
                .iter()
                .any(|reason| reason.contains("fixture"))
        );
    }

    #[test]
    fn analyze_plugin_skills_directory_uses_creator_name() {
        let temp = tempfile::tempdir().unwrap();
        let checkout = temp.path();
        let root = checkout.join("plugins/acme/skills/alpha");
        fs::create_dir_all(&root).unwrap();
        fs::write(
            root.join("SKILL.md"),
            "---\nname: alpha\ndescription: Alpha skill.\n---\n",
        )
        .unwrap();
        let beta = checkout.join("plugins/acme/skills/beta");
        fs::create_dir_all(&beta).unwrap();
        fs::write(
            beta.join("SKILL.md"),
            "---\nname: beta\ndescription: Beta skill.\n---\n",
        )
        .unwrap();

        let skillset =
            analyze_skillset_directory(checkout, &checkout.join("plugins/acme/skills")).unwrap();
        assert!(skillset.installable);
        assert_eq!(skillset.name.as_deref(), Some("acme-skillset"));
        assert_eq!(skillset.source_root, "plugins/acme/skills");
        assert_eq!(skillset.member_names, vec!["alpha", "beta"]);
    }
}
