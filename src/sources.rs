//! Remote source parsing (public GitHub only).

use crate::error::Error;

#[derive(Debug, Clone)]
pub struct RemoteSource {
    pub display: String,
    pub url: String,
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
    if let Some((owner, repo)) = value.split_once('/') {
        if !value.contains("://")
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
    }

    let err = || {
        Error::msg("Remote sources must be public GitHub HTTPS URLs or owner/repository")
    };

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
}
