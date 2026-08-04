//! Replace the running tink binary from the latest GitHub Release.

use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use tempfile::TempDir;

use crate::error::Error;
use crate::style::CliStyle;

pub const DEFAULT_RELEASES_API: &str =
    "https://api.github.com/repos/jon-devlapaz/tink/releases/latest";

/// Host target triple for the binary we are running.
pub fn host_target() -> &'static str {
    if cfg!(all(target_os = "macos", target_arch = "aarch64")) {
        "aarch64-apple-darwin"
    } else if cfg!(all(target_os = "macos", target_arch = "x86_64")) {
        "x86_64-apple-darwin"
    } else if cfg!(all(target_os = "linux", target_arch = "aarch64")) {
        "aarch64-unknown-linux-gnu"
    } else if cfg!(all(target_os = "linux", target_arch = "x86_64")) {
        "x86_64-unknown-linux-gnu"
    } else {
        "unsupported"
    }
}

/// Asset name for a release version (without leading `v`) and target triple.
pub fn asset_name(version: &str, target: &str) -> String {
    format!("tink-{version}-{target}.tar.gz")
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReleaseAsset {
    pub tag_name: String,
    pub version: String,
    pub download_url: String,
}

/// Pick the asset for `target` from a GitHub Releases API latest-release JSON body.
pub fn select_release_asset(json: &str, target: &str) -> Result<ReleaseAsset, Error> {
    let value: serde_json::Value =
        serde_json::from_str(json).map_err(|e| Error::msg(format!("release metadata: {e}")))?;
    let tag_name = value
        .get("tag_name")
        .and_then(|v| v.as_str())
        .ok_or_else(|| Error::msg("release metadata missing tag_name"))?
        .to_string();
    let version = tag_name
        .strip_prefix('v')
        .unwrap_or(tag_name.as_str())
        .to_string();
    let want = asset_name(&version, target);
    let assets = value
        .get("assets")
        .and_then(|v| v.as_array())
        .ok_or_else(|| Error::msg("release metadata missing assets"))?;
    for asset in assets {
        let name = asset.get("name").and_then(|v| v.as_str()).unwrap_or("");
        if name == want {
            let download_url = asset
                .get("browser_download_url")
                .and_then(|v| v.as_str())
                .ok_or_else(|| Error::msg(format!("asset {want} missing browser_download_url")))?
                .to_string();
            return Ok(ReleaseAsset {
                tag_name,
                version,
                download_url,
            });
        }
    }
    let names: Vec<&str> = assets
        .iter()
        .filter_map(|a| a.get("name").and_then(|v| v.as_str()))
        .collect();
    Err(Error::msg(format!(
        "no release asset named {want} (have: {})",
        names.join(", ")
    )))
}

fn releases_api_url() -> String {
    env::var("TINK_RELEASES_API").unwrap_or_else(|_| DEFAULT_RELEASES_API.to_string())
}

fn require_tool(name: &str) -> Result<(), Error> {
    match Command::new(name).arg("--version").output() {
        Ok(_) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            Err(Error::msg(format!("{name} is required for tink update")))
        }
        Err(e) => Err(Error::msg(format!("{name}: {e}"))),
    }
}

fn curl_to_file(url: &str, dest: &Path) -> Result<(), Error> {
    let output = Command::new("curl")
        .args(["-fsSL", url, "-o"])
        .arg(dest)
        .output()
        .map_err(|e| {
            if e.kind() == std::io::ErrorKind::NotFound {
                Error::msg("curl is required for tink update")
            } else {
                Error::msg(format!("curl: {e}"))
            }
        })?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let detail = stderr.lines().last().unwrap_or("curl failed").trim();
        return Err(Error::msg(format!("could not download {url}: {detail}")));
    }
    Ok(())
}

fn extract_tink_binary(archive: &Path, into: &Path) -> Result<PathBuf, Error> {
    let status = Command::new("tar")
        .args(["-xzf"])
        .arg(archive)
        .arg("-C")
        .arg(into)
        .status()
        .map_err(|e| {
            if e.kind() == std::io::ErrorKind::NotFound {
                Error::msg("tar is required for tink update")
            } else {
                Error::msg(format!("tar: {e}"))
            }
        })?;
    if !status.success() {
        return Err(Error::msg("failed to extract release archive"));
    }
    let candidate = into.join("tink");
    if candidate.is_file() {
        return Ok(candidate);
    }
    // Tolerate a single nested directory containing tink.
    let entries = fs::read_dir(into).map_err(|e| Error::msg(format!("extract dir: {e}")))?;
    for entry in entries.flatten() {
        let path = entry.path().join("tink");
        if path.is_file() {
            return Ok(path);
        }
    }
    Err(Error::msg("release archive did not contain a tink binary"))
}

fn replace_binary(current: &Path, new_bin: &Path) -> Result<(), Error> {
    let parent = current
        .parent()
        .ok_or_else(|| Error::msg(format!("cannot update {}: no parent directory", current.display())))?;
    let staging = parent.join(format!(
        ".tink-update-{}",
        std::process::id()
    ));
    fs::copy(new_bin, &staging).map_err(|e| {
        Error::msg(format!(
            "cannot write {} ({}). Re-run install.sh or fix permissions.",
            staging.display(),
            e
        ))
    })?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&staging, fs::Permissions::from_mode(0o755)).map_err(|e| {
            Error::msg(format!("chmod {}: {e}", staging.display()))
        })?;
    }
    fs::rename(&staging, current).map_err(|e| {
        let _ = fs::remove_file(&staging);
        Error::msg(format!(
            "cannot replace {} ({}). Re-run install.sh or fix permissions.",
            current.display(),
            e
        ))
    })?;
    Ok(())
}

/// Fetch the latest release and replace the running binary when newer.
pub fn update_binary() -> Result<UpdateReport, Error> {
    let target = host_target();
    if target == "unsupported" {
        return Err(Error::msg(
            "tink update does not support this platform yet",
        ));
    }
    require_tool("curl")?;
    require_tool("tar")?;

    let api = releases_api_url();
    let temp = TempDir::new().map_err(|e| Error::msg(format!("temp dir: {e}")))?;
    let meta_path = temp.path().join("release.json");
    curl_to_file(&api, &meta_path)?;
    let json = fs::read_to_string(&meta_path).map_err(|e| Error::msg(format!("read metadata: {e}")))?;
    let asset = select_release_asset(&json, target)?;

    let current_version = env!("CARGO_PKG_VERSION");
    if asset.version == current_version {
        return Ok(UpdateReport::AlreadyLatest {
            version: asset.version,
            path: env::current_exe().unwrap_or_else(|_| PathBuf::from("tink")),
        });
    }

    let archive = temp.path().join("tink.tgz");
    curl_to_file(&asset.download_url, &archive)?;
    let extracted = extract_tink_binary(&archive, temp.path())?;
    let current = env::current_exe().map_err(|e| Error::msg(format!("current exe: {e}")))?;
    // Resolve symlinks so we replace the real binary (e.g. cargo install shim).
    let current = fs::canonicalize(&current).unwrap_or(current);
    replace_binary(&current, &extracted)?;

    Ok(UpdateReport::Updated {
        from: current_version.to_string(),
        to: asset.version,
        path: current,
    })
}

#[derive(Debug)]
pub enum UpdateReport {
    AlreadyLatest { version: String, path: PathBuf },
    Updated { from: String, to: String, path: PathBuf },
}

pub fn print_report(report: &UpdateReport) {
    let style = CliStyle::auto_stdout();
    match report {
        UpdateReport::AlreadyLatest { version, path } => {
            println!(
                "{} {} ({})",
                style.muted("Up to date"),
                style.accent(format!("v{version}")),
                style.muted(path.display())
            );
        }
        UpdateReport::Updated { from, to, path } => {
            println!(
                "{} {} → {}",
                style.success("Updated"),
                style.muted(format!("v{from}")),
                style.accent(format!("v{to}"))
            );
            println!("{}", style.muted(format!("Installed to {}", path.display())));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn asset_name_matches_release_layout() {
        assert_eq!(
            asset_name("0.2.0", "aarch64-apple-darwin"),
            "tink-0.2.0-aarch64-apple-darwin.tar.gz"
        );
    }

    #[test]
    fn select_release_asset_picks_matching_target() {
        let json = r#"{
          "tag_name": "v0.2.0",
          "assets": [
            {
              "name": "tink-0.2.0-x86_64-unknown-linux-gnu.tar.gz",
              "browser_download_url": "https://example.test/linux.tgz"
            },
            {
              "name": "tink-0.2.0-aarch64-apple-darwin.tar.gz",
              "browser_download_url": "https://example.test/mac.tgz"
            }
          ]
        }"#;
        let asset = select_release_asset(json, "aarch64-apple-darwin").unwrap();
        assert_eq!(asset.version, "0.2.0");
        assert_eq!(asset.tag_name, "v0.2.0");
        assert_eq!(asset.download_url, "https://example.test/mac.tgz");
    }

    #[test]
    fn select_release_asset_errors_when_missing() {
        let json = r#"{
          "tag_name": "v0.2.0",
          "assets": [
            {
              "name": "tink-0.2.0-x86_64-unknown-linux-gnu.tar.gz",
              "browser_download_url": "https://example.test/linux.tgz"
            }
          ]
        }"#;
        let err = select_release_asset(json, "aarch64-apple-darwin").unwrap_err();
        assert!(err.to_string().contains("no release asset"));
    }
}
