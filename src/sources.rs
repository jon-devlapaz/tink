//! Source classification and remote parsing.

use std::path::{Component, Path, PathBuf};

use crate::error::Error;

pub const EMBEDDED_MANAGE_TINK: &str = "tink:embedded/manage-tink";

#[derive(Debug)]
pub enum AddSource {
    LocalPath(PathBuf),
    Github(RemoteSource),
    LibraryName(String),
}

#[derive(Debug, Clone)]
pub enum LockedSource {
    LocalPath {
        declared: String,
        project_root: PathBuf,
    },
    Github {
        remote: RemoteSource,
        revision: String,
        path: String,
    },
    EmbeddedManageTink,
}

impl LockedSource {
    pub fn declared(&self) -> &str {
        match self {
            Self::LocalPath { declared, .. } => declared,
            Self::Github { remote, .. } => &remote.url,
            Self::EmbeddedManageTink => EMBEDDED_MANAGE_TINK,
        }
    }

    pub fn revision(&self) -> Option<&str> {
        match self {
            Self::Github { revision, .. } => Some(revision),
            _ => None,
        }
    }

    pub fn source_path(&self) -> Option<&str> {
        match self {
            Self::Github { path, .. } => Some(path),
            _ => None,
        }
    }
}

#[derive(Debug, Clone)]
pub struct RemoteSource {
    pub display: String,
    pub url: String,
}

/// Classify ambiguous command-line input once, using the documented add precedence.
pub fn classify_add_input(value: &str) -> Result<AddSource, Error> {
    let path = Path::new(value);
    if path.exists() {
        return Ok(AddSource::LocalPath(path.to_path_buf()));
    }
    if looks_like_filesystem_path(value) {
        return Err(Error::msg(format!("Path does not exist: {value}")));
    }
    if looks_like_remote_source(value) {
        return Ok(AddSource::Github(parse_remote(value)?));
    }
    Ok(AddSource::LibraryName(value.to_string()))
}

/// Recover the source kind from lockfile structure, never from path existence.
pub fn classify_locked(
    project_root: &Path,
    name: &str,
    source: &str,
    revision: Option<&str>,
    source_path: Option<&str>,
) -> Result<LockedSource, Error> {
    if source == EMBEDDED_MANAGE_TINK {
        if name != "manage-tink" {
            return Err(Error::msg(format!(
                "Embedded source does not provide skill: {name}"
            )));
        }
        if revision.is_some() || source_path.is_some() {
            return Err(Error::msg(format!(
                "Embedded lock entry has remote fields: {name}"
            )));
        }
        return Ok(LockedSource::EmbeddedManageTink);
    }
    if source.starts_with("https://") {
        let remote = parse_remote(source)?;
        if remote.url != source {
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
        let source_path = source_path
            .ok_or_else(|| Error::msg(format!("Remote lock entry missing path: {name}")))?;
        validate_remote_path(source_path, name, "lockfile")?;
        return Ok(LockedSource::Github {
            remote,
            revision: revision.to_string(),
            path: source_path.to_string(),
        });
    }
    if revision.is_some() || source_path.is_some() {
        return Err(Error::msg(format!(
            "Local lock entry has remote fields: {name}"
        )));
    }
    normalize_project_path(Path::new(source), name)?;
    Ok(LockedSource::LocalPath {
        declared: source.to_string(),
        project_root: project_root.to_path_buf(),
    })
}

pub fn validate_manifest_source(
    name: &str,
    source: &str,
    source_path: Option<&str>,
) -> Result<(), Error> {
    if source == EMBEDDED_MANAGE_TINK {
        if name != "manage-tink" {
            return Err(Error::msg(format!(
                "Embedded source does not provide skill: {name}"
            )));
        }
        if source_path.is_some() {
            return Err(Error::msg(format!(
                "Embedded manifest skill has remote path: {name}"
            )));
        }
        return Ok(());
    }
    if source.starts_with("https://") {
        let remote = parse_remote(source)?;
        if remote.url != source {
            return Err(Error::msg(format!(
                "Manifest source must be canonical GitHub HTTPS URL: {name}"
            )));
        }
        if let Some(path) = source_path {
            validate_remote_path(path, name, "manifest")?;
        }
        return Ok(());
    }
    if source_path.is_some() {
        return Err(Error::msg(format!(
            "Local manifest skill has remote path: {name}"
        )));
    }
    normalize_project_path(Path::new(source), name).map(|_| ())
}

fn validate_remote_path(path: &str, name: &str, kind: &str) -> Result<(), Error> {
    if path.starts_with('/') || path.contains("..") || path.contains('\\') {
        return Err(Error::msg(format!(
            "Invalid source path for project {kind} skill: {name}"
        )));
    }
    Ok(())
}

pub fn normalize_project_path(path: &Path, name: &str) -> Result<String, Error> {
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

fn looks_like_remote_source(value: &str) -> bool {
    value.contains('/') || value.contains("://")
}

fn looks_like_filesystem_path(value: &str) -> bool {
    let path = Path::new(value);
    path.is_absolute()
        || value.starts_with("./")
        || value.starts_with("../")
        || value.starts_with("~/")
        || value.contains('\\')
}

fn github_part_ok(part: &str) -> bool {
    if part.is_empty() || part == "." || part == ".." {
        return false;
    }
    // Disallow leading/trailing dots (blocks `./foo` → owner `.`).
    if part.starts_with('.') || part.ends_with('.') {
        return false;
    }
    part.chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '.' || c == '-')
}

/// Parse `owner/repo` or `https://github.com/owner/repo[.git]`.
pub fn parse_remote(value: &str) -> Result<RemoteSource, Error> {
    if let Some((owner, repo)) = value.split_once('/')
        && !value.contains("://")
        && !value.contains('@')
        && github_part_ok(owner)
        && github_part_ok(repo.trim_end_matches(".git"))
        && !owner.is_empty()
        && value.matches('/').count() == 1
    {
        let repo = repo.trim_end_matches(".git");
        return Ok(RemoteSource {
            display: value.to_string(),
            url: format!("https://github.com/{owner}/{repo}.git"),
        });
    }

    let err = || Error::msg("Remote sources must be public GitHub HTTPS URLs or owner/repository");

    let url = value.parse::<url_lite::Url>().map_err(|_| err())?;
    if url.scheme != "https" || url.host.as_deref() != Some("github.com") {
        return Err(err());
    }
    if url.userinfo.is_some() || !url.query.is_empty() || url.fragment.is_some() {
        return Err(err());
    }
    let parts: Vec<&str> = url
        .path
        .trim_matches('/')
        .split('/')
        .filter(|p| !p.is_empty())
        .collect();
    if parts.len() != 2 {
        return Err(Error::msg(
            "Remote GitHub source must identify exactly one owner and repository",
        ));
    }
    let owner = parts[0];
    let repo = parts[1].trim_end_matches(".git");
    if !github_part_ok(owner) || !github_part_ok(repo) {
        return Err(err());
    }
    Ok(RemoteSource {
        display: value.to_string(),
        url: format!("https://github.com/{owner}/{repo}.git"),
    })
}

/// Minimal URL parse without pulling the `url` crate — only what we need.
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
            Some(i) => (&rest[..i], &rest[i..]),
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
            Some((p, f)) => (p, Some(f.to_string())),
            None => (path_and_more, None),
        };
        let (path, query) = match path_query.split_once('?') {
            Some((p, q)) => (p.to_string(), q.to_string()),
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
        fn from_str(s: &str) -> Result<Self, Self::Err> {
            parse(s)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_dot_owner_shorthand() {
        assert!(parse_remote("./relative-missing").is_err());
        assert!(parse_remote("../up").is_err());
    }

    #[test]
    fn accepts_owner_repo() {
        let remote = parse_remote("example/skills").unwrap();
        assert_eq!(remote.url, "https://github.com/example/skills.git");
    }

    #[test]
    fn classifies_add_input_variants() {
        let temp = tempfile::tempdir().unwrap();
        assert!(matches!(
            classify_add_input(temp.path().to_str().unwrap()).unwrap(),
            AddSource::LocalPath(_)
        ));
        assert!(classify_add_input("./definitely-missing").is_err());
        assert!(matches!(
            classify_add_input("example/skills").unwrap(),
            AddSource::Github(_)
        ));
        assert!(matches!(
            classify_add_input("https://github.com/example/skills").unwrap(),
            AddSource::Github(_)
        ));
        assert!(matches!(
            classify_add_input("reviewer").unwrap(),
            AddSource::LibraryName(name) if name == "reviewer"
        ));
        assert!(classify_add_input(EMBEDDED_MANAGE_TINK).is_err());
    }

    #[test]
    fn locked_local_source_does_not_become_github_when_missing() {
        let root = Path::new("/project");
        assert!(matches!(
            classify_locked(root, "local-skill", "owner/repo-shape", None, None).unwrap(),
            LockedSource::LocalPath { project_root, .. } if project_root == root
        ));
    }
}
