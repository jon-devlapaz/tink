//! Read-only inspection of GitHub source structure.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

use crate::error::Error;
use crate::git;
use crate::output;
use crate::skills;
use crate::sources::RemoteSource;

#[derive(Debug)]
struct ParsedUrl {
    remote: RemoteSource,
    requested_ref: Option<String>,
    boundary: PathBuf,
    boundary_display: String,
}

#[derive(Debug, Clone)]
pub struct DiscoveredSkill {
    pub name: String,
    pub path: String,
}

#[derive(Debug, Clone)]
pub struct InferredSkillset {
    pub name: Option<String>,
    pub path: String,
    pub members: usize,
}

#[derive(Debug, Clone)]
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
        if skillset.name.is_none() && skillset.path != "." {
            diagnostics.push(format!(
                "no valid canonical skillset name for {}",
                skillset.path
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

    let remote_refs = git::remote_ref_names(&parsed.remote)?;
    let mut candidate = requested_ref.to_string();
    for segment in parsed.boundary.iter() {
        let segment = segment
            .to_str()
            .ok_or_else(|| Error::msg("Inspection URL contains non-UTF-8 path data"))?;
        candidate.push('/');
        candidate.push_str(segment);
        if remote_refs.contains(&candidate) {
            return Err(Error::msg(format!(
                "Inspection URL is ambiguous because Git ref `{candidate}` contains `/`; use a ref without `/`"
            )));
        }
    }
    Ok(())
}

fn parse_url(value: &str) -> Result<ParsedUrl, Error> {
    let parsed = value
        .parse::<url_lite::Url>()
        .map_err(|_| Error::msg("Inspection URL must be a public GitHub HTTPS URL"))?;
    if parsed.scheme != "https"
        || parsed.host.as_deref() != Some("github.com")
        || parsed.userinfo.is_some()
        || !parsed.query.is_empty()
        || parsed.fragment.is_some()
    {
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
    if !valid_part(owner) || !valid_part(repository) {
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
            if *part == "." || *part == ".." || part.contains('\\') {
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

fn valid_part(part: &str) -> bool {
    !part.is_empty()
        && !part.starts_with('.')
        && !part.ends_with('.')
        && part
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || "_.-".contains(character))
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
        if boundary.file_name().and_then(|name| name.to_str()) == Some("skills") {
            return Ok(Vec::new());
        }
        let has_nested_collection = regular_children(boundary)?.iter().any(|child| {
            let child_path = relative_posix(checkout, child);
            skills
                .iter()
                .any(|skill| skill.path.starts_with(&(child_path.clone() + "/")))
        });
        if has_nested_collection {
            diagnostics.push(
                "mixed skill layout: direct skills and nested skill collections coexist; inspect a narrower GitHub tree URL to select a skillset"
                    .to_string(),
            );
            return Ok(Vec::new());
        }
        return Ok(vec![make_skillset(checkout, boundary, skills)]);
    }
    let mut children = regular_children(boundary)?;
    children.sort();
    let descendants: BTreeSet<PathBuf> = children
        .iter()
        .filter(|child| {
            let child_path = relative_posix(checkout, child);
            skills.iter().any(|skill| {
                skill.path == child_path || skill.path.starts_with(&(child_path.clone() + "/"))
            })
        })
        .cloned()
        .collect();
    if descendants.len() >= 2 {
        return Ok(children
            .iter()
            .map(|child| {
                let child_relative = relative_posix(checkout, child);
                make_skillset(checkout, child, &skills_for_prefix(skills, &child_relative))
            })
            .collect());
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

fn skills_for_prefix(skills: &[DiscoveredSkill], prefix: &str) -> Vec<DiscoveredSkill> {
    skills
        .iter()
        .filter(|skill| skill.path == prefix || skill.path.starts_with(&(prefix.to_string() + "/")))
        .cloned()
        .collect()
}

fn make_skillset(
    checkout: &Path,
    directory: &Path,
    members: &[DiscoveredSkill],
) -> InferredSkillset {
    if directory == checkout {
        return InferredSkillset {
            name: None,
            path: ".".to_string(),
            members: members.len(),
        };
    }
    let path = relative_posix(checkout, directory);
    let folder = directory
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or("")
        .to_string();
    let name = if skills::valid_skill_name(&folder) {
        let candidate = if folder.ends_with("-skillset") {
            folder.clone()
        } else {
            format!("{folder}-skillset")
        };
        skills::valid_skill_name(&candidate).then_some(candidate)
    } else {
        None
    };
    InferredSkillset {
        name,
        path,
        members: members.len(),
    }
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
}

mod url_lite {
    #[derive(Debug)]
    pub struct Url {
        pub scheme: String,
        pub host: Option<String>,
        pub userinfo: Option<String>,
        pub path: String,
        pub query: String,
        pub fragment: Option<String>,
    }

    pub fn parse(input: &str) -> Result<Url, ()> {
        let (scheme, rest) = input.split_once("://").ok_or(())?;
        let (authority, path_and_more) = match rest.find('/') {
            Some(index) => (&rest[..index], &rest[index..]),
            None => (rest, "/"),
        };
        let (userinfo, host) = if let Some((user, host)) = authority.split_once('@') {
            (Some(user.to_string()), host)
        } else {
            (None, authority)
        };
        if host.is_empty() {
            return Err(());
        }
        let (path_query, fragment) = match path_and_more.split_once('#') {
            Some((path, fragment)) => (path, Some(fragment.to_string())),
            None => (path_and_more, None),
        };
        let (path, query) = match path_query.split_once('?') {
            Some((path, query)) => (path.to_string(), query.to_string()),
            None => (path_query.to_string(), String::new()),
        };
        Ok(Url {
            scheme: scheme.to_string(),
            host: Some(host.to_string()),
            userinfo,
            path,
            query,
            fragment,
        })
    }

    impl std::str::FromStr for Url {
        type Err = ();
        fn from_str(value: &str) -> Result<Self, Self::Err> {
            parse(value)
        }
    }
}
