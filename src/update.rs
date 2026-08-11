//! Replace the running tink binary from a newer verified GitHub Release.

use std::cmp::Ordering;
use std::env;
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
#[cfg(test)]
use std::thread;
use std::time::Duration;
#[cfg(test)]
use std::time::Instant;

use sha2::{Digest, Sha256};
use tempfile::{Builder, TempDir};

use crate::error::Error;
use crate::output;
use crate::style::CliStyle;

pub const DEFAULT_RELEASES_API: &str =
    "https://api.github.com/repos/jon-devlapaz/tink/releases/latest";

const CURL_CONNECT_TIMEOUT_SECONDS: &str = "5";
/// Bound for tiny JSON metadata (releases API).
const CURL_METADATA_MAX_TIME_SECONDS: &str = "30";
/// Bound for release archive download (can be multi-MB on slow links).
const CURL_ASSET_MAX_TIME_SECONDS: &str = "300";
const CURL_RETRY_COUNT: &str = "2";
const CURL_RETRY_DELAY_SECONDS: &str = "1";
const TOOL_TIMEOUT: Duration = Duration::from_secs(5);
const TAR_TIMEOUT: Duration = Duration::from_secs(30);
const CANDIDATE_TIMEOUT: Duration = Duration::from_secs(5);

#[derive(Debug, Clone, Copy)]
enum CurlBudget {
    Metadata,
    Asset,
}

impl CurlBudget {
    fn max_time_seconds(self) -> &'static str {
        match self {
            CurlBudget::Metadata => CURL_METADATA_MAX_TIME_SECONDS,
            CurlBudget::Asset => CURL_ASSET_MAX_TIME_SECONDS,
        }
    }
}

/// Full curl argv for a download (transport flags wired in one place).
fn curl_command_args<'a>(budget: CurlBudget, url: &'a str, dest: &'a Path) -> Vec<&'a str> {
    let protocol = if url.starts_with("file://") {
        "=file"
    } else {
        "=https"
    };
    vec![
        "-fsSL",
        "--proto",
        protocol,
        "--proto-redir",
        protocol,
        "--connect-timeout",
        CURL_CONNECT_TIMEOUT_SECONDS,
        "--max-time",
        budget.max_time_seconds(),
        "--retry",
        CURL_RETRY_COUNT,
        "--retry-delay",
        CURL_RETRY_DELAY_SECONDS,
        url,
        "-o",
        dest.to_str().unwrap_or(""),
    ]
}

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
    pub sha256: [u8; 32],
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum PrereleaseIdentifier {
    Numeric(String),
    Text(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct SemVersion {
    core: [u64; 3],
    prerelease: Option<Vec<PrereleaseIdentifier>>,
}

impl SemVersion {
    fn parse(value: &str) -> Result<Self, Error> {
        let (without_build, build) = value
            .split_once('+')
            .map_or((value, None), |(version, build)| (version, Some(build)));
        if value.matches('+').count() > 1
            || build.is_some_and(|build| !valid_identifiers(build, false))
        {
            return Err(Error::msg(format!("invalid semantic version: {value}")));
        }
        let (core, prerelease) = without_build
            .split_once('-')
            .map_or((without_build, None), |(core, pre)| (core, Some(pre)));
        let core_parts: Vec<&str> = core.split('.').collect();
        if core_parts.len() != 3 {
            return Err(Error::msg(format!("invalid semantic version: {value}")));
        }
        let mut parsed_core = [0_u64; 3];
        for (index, part) in core_parts.iter().enumerate() {
            if !valid_numeric_identifier(part) {
                return Err(Error::msg(format!("invalid semantic version: {value}")));
            }
            parsed_core[index] = part
                .parse()
                .map_err(|_| Error::msg(format!("semantic version component overflow: {value}")))?;
        }
        let prerelease = match prerelease {
            Some(pre) if valid_identifiers(pre, true) => Some(
                pre.split('.')
                    .map(|identifier| {
                        if identifier.bytes().all(|byte| byte.is_ascii_digit()) {
                            PrereleaseIdentifier::Numeric(identifier.to_string())
                        } else {
                            PrereleaseIdentifier::Text(identifier.to_string())
                        }
                    })
                    .collect(),
            ),
            Some(_) => return Err(Error::msg(format!("invalid semantic version: {value}"))),
            None => None,
        };
        Ok(Self {
            core: parsed_core,
            prerelease,
        })
    }
}

fn valid_numeric_identifier(value: &str) -> bool {
    !value.is_empty()
        && value.bytes().all(|byte| byte.is_ascii_digit())
        && (value == "0" || !value.starts_with('0'))
}

fn valid_identifiers(value: &str, reject_numeric_leading_zero: bool) -> bool {
    !value.is_empty()
        && value.split('.').all(|identifier| {
            !identifier.is_empty()
                && identifier
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
                && (!reject_numeric_leading_zero
                    || !identifier.bytes().all(|byte| byte.is_ascii_digit())
                    || valid_numeric_identifier(identifier))
        })
}

impl Ord for SemVersion {
    fn cmp(&self, other: &Self) -> Ordering {
        self.core
            .cmp(&other.core)
            .then_with(|| match (&self.prerelease, &other.prerelease) {
                (None, None) => Ordering::Equal,
                (None, Some(_)) => Ordering::Greater,
                (Some(_), None) => Ordering::Less,
                (Some(left), Some(right)) => {
                    for (left, right) in left.iter().zip(right) {
                        let ordering = match (left, right) {
                            (
                                PrereleaseIdentifier::Numeric(left),
                                PrereleaseIdentifier::Numeric(right),
                            ) => left.len().cmp(&right.len()).then_with(|| left.cmp(right)),
                            (PrereleaseIdentifier::Numeric(_), PrereleaseIdentifier::Text(_)) => {
                                Ordering::Less
                            }
                            (PrereleaseIdentifier::Text(_), PrereleaseIdentifier::Numeric(_)) => {
                                Ordering::Greater
                            }
                            (
                                PrereleaseIdentifier::Text(left),
                                PrereleaseIdentifier::Text(right),
                            ) => left.cmp(right),
                        };
                        if ordering != Ordering::Equal {
                            return ordering;
                        }
                    }
                    left.len().cmp(&right.len())
                }
            })
    }
}

impl PartialOrd for SemVersion {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

fn parse_sha256(value: &str) -> Result<[u8; 32], Error> {
    let hex = value
        .strip_prefix("sha256:")
        .ok_or_else(|| Error::msg("release asset digest must use sha256:<hex>"))?;
    if hex.len() != 64
        || !hex
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return Err(Error::msg(
            "release asset SHA-256 digest must be 64 lowercase hexadecimal characters",
        ));
    }
    let mut digest = [0_u8; 32];
    for (index, byte) in digest.iter_mut().enumerate() {
        *byte = u8::from_str_radix(&hex[index * 2..index * 2 + 2], 16)
            .map_err(|_| Error::msg("release asset has invalid SHA-256 digest"))?;
    }
    Ok(digest)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ReleaseUrlMode {
    Https,
    File,
}

fn validate_release_url(url: &str, allow_file: bool) -> Result<ReleaseUrlMode, Error> {
    if url.is_empty()
        || url.contains(['?', '#', '\\'])
        || url
            .bytes()
            .any(|byte| byte.is_ascii_control() || byte.is_ascii_whitespace())
    {
        return Err(Error::msg("release download URL is not allowed"));
    }
    if let Some(rest) = url.strip_prefix("https://") {
        let authority = rest.split('/').next().unwrap_or("");
        if authority.is_empty() || authority.contains('@') {
            return Err(Error::msg("release download URL is not allowed"));
        }
        return Ok(ReleaseUrlMode::Https);
    }
    if allow_file
        && url
            .strip_prefix("file://")
            .is_some_and(|path| path.starts_with('/'))
    {
        return Ok(ReleaseUrlMode::File);
    }
    Err(Error::msg("release download URL is not allowed"))
}

fn validate_asset_url(url: &str, api_mode: ReleaseUrlMode) -> Result<(), Error> {
    let asset_mode = validate_release_url(url, true)
        .map_err(|_| Error::msg("release asset has invalid download URL"))?;
    if asset_mode == ReleaseUrlMode::File && api_mode != ReleaseUrlMode::File {
        return Err(Error::msg(
            "file release assets require an explicit file TINK_RELEASES_API override",
        ));
    }
    Ok(())
}

fn run_bounded(command: &mut Command, timeout: Duration, context: &str) -> Result<Output, Error> {
    let missing = format!("{context} is required for tink update");
    crate::process::run_bounded(command, timeout, context, Some(&missing))
}

fn release_version(tag_name: &str) -> Result<String, Error> {
    let version = tag_name.strip_prefix('v').unwrap_or(tag_name);
    SemVersion::parse(version).map_err(|_| Error::msg("release metadata has invalid tag_name"))?;
    Ok(version.to_string())
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
    let version = release_version(&tag_name)?;
    let want = asset_name(&version, target);
    let assets = value
        .get("assets")
        .and_then(|v| v.as_array())
        .ok_or_else(|| Error::msg("release metadata missing assets"))?;
    let matches: Vec<&serde_json::Value> = assets
        .iter()
        .filter(|asset| asset.get("name").and_then(|v| v.as_str()) == Some(want.as_str()))
        .collect();
    if matches.len() > 1 {
        return Err(Error::msg(format!(
            "expected exactly one release asset named {want}"
        )));
    }
    if let Some(asset) = matches.first() {
        let download_url = asset
            .get("browser_download_url")
            .and_then(|v| v.as_str())
            .ok_or_else(|| Error::msg(format!("asset {want} missing browser_download_url")))?
            .to_string();
        validate_release_url(&download_url, true)
            .map_err(|_| Error::msg(format!("asset {want} has invalid download URL")))?;
        let digest = asset
            .get("digest")
            .and_then(|value| value.as_str())
            .ok_or_else(|| Error::msg(format!("asset {want} missing digest")))?;
        return Ok(ReleaseAsset {
            tag_name,
            version,
            download_url,
            sha256: parse_sha256(digest)?,
        });
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

fn releases_api_url() -> Result<(String, ReleaseUrlMode), Error> {
    match env::var("TINK_RELEASES_API") {
        Ok(url) => {
            let mode = validate_release_url(&url, true)
                .map_err(|_| Error::msg("TINK_RELEASES_API is not an allowed release URL"))?;
            Ok((url, mode))
        }
        Err(_) => Ok((DEFAULT_RELEASES_API.to_string(), ReleaseUrlMode::Https)),
    }
}

fn require_tool(name: &str) -> Result<(), Error> {
    let output = run_bounded(Command::new(name).arg("--version"), TOOL_TIMEOUT, name)?;
    if output.status.success() {
        Ok(())
    } else {
        Err(Error::msg(format!("{name} is unavailable for tink update")))
    }
}

fn curl_to_file(budget: CurlBudget, url: &str, dest: &Path, context: &str) -> Result<(), Error> {
    if dest.to_str().is_none() {
        return Err(Error::msg("non-utf8 download path"));
    }
    let timeout = Duration::from_secs(budget.max_time_seconds().parse::<u64>().unwrap_or(300) + 5);
    let output = run_bounded(
        Command::new("curl").args(curl_command_args(budget, url, dest)),
        timeout,
        "curl",
    )?;
    if !output.status.success() {
        return Err(Error::msg(format!("could not download {context}")));
    }
    Ok(())
}

fn sha256_file(path: &Path) -> Result<[u8; 32], Error> {
    let mut file = fs::File::open(path)
        .map_err(|error| Error::msg(format!("read {}: {error}", path.display())))?;
    let mut hasher = Sha256::new();
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        let count = file
            .read(&mut buffer)
            .map_err(|error| Error::msg(format!("read {}: {error}", path.display())))?;
        if count == 0 {
            break;
        }
        hasher.update(&buffer[..count]);
    }
    Ok(hasher.finalize().into())
}

fn verify_archive_digest(path: &Path, expected: &[u8; 32]) -> Result<(), Error> {
    if sha256_file(path)? != *expected {
        return Err(Error::msg("release archive SHA-256 digest mismatch"));
    }
    Ok(())
}

fn tar_output(archive: &Path, verbose: bool) -> Result<std::process::Output, Error> {
    let list_flag = if verbose { "-tvzf" } else { "-tzf" };
    run_bounded(
        Command::new("tar").arg(list_flag).arg(archive),
        TAR_TIMEOUT,
        "tar archive inspection",
    )
}

fn validate_archive_inventory(archive: &Path) -> Result<(), Error> {
    let listing = tar_output(archive, false)?;
    if !listing.status.success() {
        return Err(Error::msg("failed to inspect release archive"));
    }
    let entries = String::from_utf8(listing.stdout)
        .map_err(|_| Error::msg("release archive has non-UTF-8 entry names"))?;
    let entries: Vec<&str> = entries.lines().collect();
    if entries != ["tink"] {
        return Err(Error::msg(
            "release archive must contain exactly one top-level tink file",
        ));
    }

    let verbose = tar_output(archive, true)?;
    if !verbose.status.success() {
        return Err(Error::msg("failed to inspect release archive entry type"));
    }
    let details = String::from_utf8(verbose.stdout)
        .map_err(|_| Error::msg("release archive has invalid entry metadata"))?;
    let rows: Vec<&str> = details.lines().collect();
    if rows.len() != 1 || !rows[0].trim_start().starts_with('-') {
        return Err(Error::msg(
            "release archive tink entry must be a regular file",
        ));
    }
    Ok(())
}

fn extract_tink_binary(archive: &Path, into: &Path) -> Result<PathBuf, Error> {
    validate_archive_inventory(archive)?;
    let output = run_bounded(
        Command::new("tar")
            .args(["-xzf"])
            .arg(archive)
            .arg("-C")
            .arg(into),
        TAR_TIMEOUT,
        "tar archive extraction",
    )?;
    if !output.status.success() {
        return Err(Error::msg("failed to extract release archive"));
    }
    let candidate = into.join("tink");
    let metadata = fs::symlink_metadata(&candidate)
        .map_err(|_| Error::msg("release archive did not contain a tink binary"))?;
    if metadata.file_type().is_file() {
        return Ok(candidate);
    }
    Err(Error::msg("release archive did not contain a tink binary"))
}

fn probe_candidate(candidate: &Path, expected_version: &str) -> Result<(), Error> {
    let result = run_bounded(
        Command::new(candidate).arg("--version"),
        CANDIDATE_TIMEOUT,
        "release candidate probe",
    )?;
    let expected = format!("tink {expected_version}\n");
    if !result.status.success() || result.stdout != expected.as_bytes() {
        return Err(Error::msg(format!(
            "release candidate failed version probe for tink {expected_version}"
        )));
    }
    Ok(())
}

fn replace_binary(current: &Path, new_bin: &Path, expected_version: &str) -> Result<(), Error> {
    let parent = current.parent().ok_or_else(|| {
        Error::msg(format!(
            "cannot update {}: no parent directory",
            current.display()
        ))
    })?;
    let staging = Builder::new()
        .prefix(".tink-update-")
        .tempfile_in(parent)
        .map_err(|error| {
            Error::msg(format!(
                "cannot stage update beside {} ({error})",
                current.display()
            ))
        })?;
    let backup = Builder::new()
        .prefix(".tink-backup-")
        .tempfile_in(parent)
        .map_err(|error| {
            Error::msg(format!(
                "cannot preserve {} before update ({error})",
                current.display()
            ))
        })?;
    fs::copy(current, backup.path()).map_err(|error| {
        Error::msg(format!(
            "cannot preserve {} before update ({error})",
            current.display()
        ))
    })?;
    fs::copy(new_bin, staging.path()).map_err(|e| {
        Error::msg(format!(
            "cannot write {} ({}). Re-run install.sh or fix permissions.",
            staging.path().display(),
            e
        ))
    })?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(staging.path(), fs::Permissions::from_mode(0o755))
            .map_err(|e| Error::msg(format!("chmod {}: {e}", staging.path().display())))?;
    }
    probe_candidate(staging.path(), expected_version)?;
    let published = staging.persist(current).map_err(|error| {
        Error::msg(format!(
            "cannot replace {} ({}). Re-run install.sh or fix permissions.",
            current.display(),
            error.error
        ))
    })?;
    drop(published);
    if let Err(probe_error) = probe_candidate(current, expected_version) {
        return match backup.persist(current) {
            Ok(restored) => {
                drop(restored);
                Err(Error::msg(format!(
                    "published tink failed exact version verification and was rolled back: {probe_error}"
                )))
            }
            Err(restore_error) => {
                let cause = restore_error.error;
                let recovery = restore_error.file.into_temp_path().keep().ok();
                let recovery = recovery.as_deref().map_or_else(
                    || "unavailable".to_string(),
                    |path| path.display().to_string(),
                );
                Err(Error::msg(format!(
                    "published tink failed exact version verification ({probe_error}); rollback failed ({cause}); recovery backup: {recovery}"
                )))
            }
        };
    }
    Ok(())
}

fn validate_current_binary_target(current: &Path) -> Result<(), Error> {
    let metadata = fs::symlink_metadata(current).map_err(|error| {
        Error::msg(format!(
            "cannot inspect current binary {}: {error}",
            current.display()
        ))
    })?;
    if !metadata.file_type().is_file() {
        return Err(Error::msg(format!(
            "refusing to replace non-file current binary {}",
            current.display()
        )));
    }
    Ok(())
}

fn current_binary_path() -> Result<PathBuf, Error> {
    let current = env::current_exe().map_err(|e| Error::msg(format!("current exe: {e}")))?;
    // Resolve a launcher symlink once, then validate and atomically replace its
    // regular-file target without following any later destination changes.
    let current = fs::canonicalize(&current).unwrap_or(current);
    validate_current_binary_target(&current)?;
    Ok(current)
}

fn verify_and_replace(
    current: &Path,
    archive: &Path,
    expected_digest: &[u8; 32],
    expected_version: &str,
    extract_dir: &Path,
) -> Result<(), Error> {
    verify_archive_digest(archive, expected_digest)?;
    let extracted = extract_tink_binary(archive, extract_dir)?;
    probe_candidate(&extracted, expected_version)?;
    replace_binary(current, &extracted, expected_version)
}

/// Fetch the latest release and replace the running binary when newer.
pub fn update_binary() -> Result<UpdateReport, Error> {
    let target = host_target();
    if target == "unsupported" {
        return Err(Error::msg("tink update does not support this platform yet"));
    }
    require_tool("curl")?;
    require_tool("tar")?;

    let (api, api_mode) = releases_api_url()?;
    let temp = TempDir::new().map_err(|e| Error::msg(format!("temp dir: {e}")))?;
    let meta_path = temp.path().join("release.json");
    curl_to_file(CurlBudget::Metadata, &api, &meta_path, "release metadata")?;
    let json =
        fs::read_to_string(&meta_path).map_err(|e| Error::msg(format!("read metadata: {e}")))?;
    let asset = select_release_asset(&json, target)?;
    validate_asset_url(&asset.download_url, api_mode)?;

    let current_version = env!("CARGO_PKG_VERSION");
    match SemVersion::parse(&asset.version)?.cmp(&SemVersion::parse(current_version)?) {
        Ordering::Equal => {
            return Ok(UpdateReport::AlreadyLatest {
                version: asset.version,
                path: env::current_exe().unwrap_or_else(|_| PathBuf::from("tink")),
            });
        }
        Ordering::Less => {
            return Err(Error::msg(format!(
                "refusing to downgrade tink from v{current_version} to v{}",
                asset.version
            )));
        }
        Ordering::Greater => {}
    }

    let archive = temp.path().join("tink.tgz");
    curl_to_file(
        CurlBudget::Asset,
        &asset.download_url,
        &archive,
        "release asset",
    )?;
    let current = current_binary_path()?;
    let extract_dir = temp.path().join("extract");
    fs::create_dir(&extract_dir).map_err(|e| Error::msg(format!("extract dir: {e}")))?;
    verify_and_replace(
        &current,
        &archive,
        &asset.sha256,
        &asset.version,
        &extract_dir,
    )?;

    Ok(UpdateReport::Updated {
        from: current_version.to_string(),
        to: asset.version,
        path: current,
    })
}

#[derive(Debug)]
pub enum UpdateReport {
    AlreadyLatest {
        version: String,
        path: PathBuf,
    },
    Updated {
        from: String,
        to: String,
        path: PathBuf,
    },
}

pub fn print_report(report: &UpdateReport) -> Result<(), Error> {
    let style = CliStyle::auto_stdout();
    match report {
        UpdateReport::AlreadyLatest { version, path } => {
            output::stdout_line(format_args!(
                "{} {} ({})",
                style.muted("Up to date"),
                style.accent(format!("v{version}")),
                style.muted(path.display())
            ))?;
        }
        UpdateReport::Updated { from, to, path } => {
            output::stdout_line(format_args!(
                "{} {} → {}",
                style.success("Updated"),
                style.muted(format!("v{from}")),
                style.accent(format!("v{to}"))
            ))?;
            output::stdout_line(format_args!(
                "{}",
                style.muted(format!("Installed to {}", path.display()))
            ))?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn curl_command_args_wire_timeouts_retries_and_dest() {
        let dest = Path::new("/tmp/tink-release.json");
        assert_eq!(
            curl_command_args(CurlBudget::Metadata, "https://example.test/meta", dest),
            [
                "-fsSL",
                "--proto",
                "=https",
                "--proto-redir",
                "=https",
                "--connect-timeout",
                CURL_CONNECT_TIMEOUT_SECONDS,
                "--max-time",
                CURL_METADATA_MAX_TIME_SECONDS,
                "--retry",
                CURL_RETRY_COUNT,
                "--retry-delay",
                CURL_RETRY_DELAY_SECONDS,
                "https://example.test/meta",
                "-o",
                "/tmp/tink-release.json",
            ]
        );
        let archive = Path::new("/tmp/tink.tgz");
        let asset_args =
            curl_command_args(CurlBudget::Asset, "https://example.test/tink.tgz", archive);
        let max_time_idx = asset_args
            .iter()
            .position(|a| *a == "--max-time")
            .expect("--max-time present");
        assert_eq!(asset_args[max_time_idx + 1], CURL_ASSET_MAX_TIME_SECONDS);
        assert_ne!(CURL_METADATA_MAX_TIME_SECONDS, CURL_ASSET_MAX_TIME_SECONDS);
        let file_args = curl_command_args(
            CurlBudget::Asset,
            "file:///tmp/tink.tgz",
            Path::new("/tmp/download.tgz"),
        );
        assert!(
            file_args
                .windows(2)
                .any(|args| args == ["--proto", "=file"])
        );
    }

    #[test]
    fn file_asset_requires_explicit_file_api_mode() {
        assert!(validate_asset_url("file:///tmp/tink.tgz", ReleaseUrlMode::Https).is_err());
        assert!(validate_asset_url("file:///tmp/tink.tgz", ReleaseUrlMode::File).is_ok());
        assert!(validate_asset_url("https://example.test/tink.tgz", ReleaseUrlMode::Https).is_ok());
    }

    #[cfg(unix)]
    #[test]
    fn bounded_process_kills_and_reaps_timeout() {
        let started = Instant::now();
        let err = run_bounded(
            Command::new("sh").args(["-c", "exec sleep 2"]),
            Duration::from_millis(30),
            "timeout fixture",
        )
        .unwrap_err();
        assert!(err.to_string().contains("timed out"));
        assert!(started.elapsed() < Duration::from_secs(1));
    }

    #[cfg(unix)]
    #[test]
    fn bounded_process_times_out_when_descendant_holds_output_pipes() {
        let temp = tempfile::tempdir().unwrap();
        let marker = temp.path().join("descendant-survived");
        let started = Instant::now();
        let err = run_bounded(
            Command::new("sh")
                .args(["-c", "(sleep 0.2; : > \"$TINK_TIMEOUT_MARKER\") & exit 0"])
                .env("TINK_TIMEOUT_MARKER", &marker),
            Duration::from_millis(30),
            "descendant pipe fixture",
        )
        .unwrap_err();

        assert!(err.to_string().contains("timed out"));
        assert!(started.elapsed() < Duration::from_secs(1));
        thread::sleep(Duration::from_millis(300));
        assert!(!marker.exists(), "timed-out descendant was left running");
    }

    #[cfg(unix)]
    #[test]
    fn bounded_process_cleans_descendants_after_direct_child_succeeds() {
        let temp = tempfile::tempdir().unwrap();
        let marker = temp.path().join("descendant-survived");
        let output = run_bounded(
            Command::new("sh")
                .args([
                    "-c",
                    "(sleep 0.2; : > \"$TINK_TIMEOUT_MARKER\") >/dev/null 2>&1 & printf done",
                ])
                .env("TINK_TIMEOUT_MARKER", &marker),
            Duration::from_secs(1),
            "successful descendant fixture",
        )
        .unwrap();

        assert!(output.status.success());
        assert_eq!(output.stdout, b"done");
        thread::sleep(Duration::from_millis(300));
        assert!(
            !marker.exists(),
            "successful child left a descendant running"
        );
    }

    #[test]
    fn asset_name_matches_release_layout() {
        assert_eq!(
            asset_name("0.2.0", "aarch64-apple-darwin"),
            "tink-0.2.0-aarch64-apple-darwin.tar.gz"
        );
    }

    #[test]
    fn semantic_version_comparison_is_numeric_and_prerelease_aware() {
        assert!(SemVersion::parse("0.3.10").unwrap() > SemVersion::parse("0.3.9").unwrap());
        assert!(SemVersion::parse("1.0.0").unwrap() > SemVersion::parse("1.0.0-rc.1").unwrap());
        assert!(
            SemVersion::parse("1.0.0-rc.10").unwrap() > SemVersion::parse("1.0.0-rc.2").unwrap()
        );
        assert!(
            SemVersion::parse("1.0.0-100000000000000000000").unwrap()
                > SemVersion::parse("1.0.0-99999999999999999999").unwrap()
        );
        assert_eq!(
            SemVersion::parse("1.0.0+build.2").unwrap(),
            SemVersion::parse("1.0.0+build.1").unwrap()
        );
    }

    #[test]
    fn semantic_version_parser_rejects_ambiguous_versions() {
        for version in ["1.2", "01.2.3", "1.2.3-01", "1.2.3+", "1.2.3/evil"] {
            assert!(SemVersion::parse(version).is_err(), "accepted {version}");
        }
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
              "browser_download_url": "https://example.test/mac.tgz",
              "digest": "sha256:0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef"
            }
          ]
        }"#;
        let asset = select_release_asset(json, "aarch64-apple-darwin").unwrap();
        assert_eq!(asset.version, "0.2.0");
        assert_eq!(asset.tag_name, "v0.2.0");
        assert_eq!(asset.download_url, "https://example.test/mac.tgz");
        assert_eq!(
            asset.sha256,
            parse_sha256("sha256:0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef")
                .unwrap()
        );
    }

    #[test]
    fn select_release_asset_rejects_missing_digest() {
        let json = r#"{
          "tag_name": "v0.2.0",
          "assets": [
            {
              "name": "tink-0.2.0-aarch64-apple-darwin.tar.gz",
              "browser_download_url": "https://example.test/mac.tgz"
            }
          ]
        }"#;
        let err = select_release_asset(json, "aarch64-apple-darwin").unwrap_err();
        assert!(err.to_string().contains("digest"));
    }

    #[test]
    fn select_release_asset_rejects_malformed_sha256_digest() {
        let json = r#"{
          "tag_name": "v0.2.0",
          "assets": [
            {
              "name": "tink-0.2.0-aarch64-apple-darwin.tar.gz",
              "browser_download_url": "https://example.test/mac.tgz",
              "digest": "sha256:not-a-digest"
            }
          ]
        }"#;
        let err = select_release_asset(json, "aarch64-apple-darwin").unwrap_err();
        assert!(err.to_string().contains("SHA-256"));
    }

    #[test]
    fn select_release_asset_rejects_duplicate_exact_names() {
        let json = r#"{
          "tag_name": "v0.2.0",
          "assets": [
            {
              "name": "tink-0.2.0-aarch64-apple-darwin.tar.gz",
              "browser_download_url": "https://example.test/first.tgz",
              "digest": "sha256:0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef"
            },
            {
              "name": "tink-0.2.0-aarch64-apple-darwin.tar.gz",
              "browser_download_url": "https://example.test/second.tgz",
              "digest": "sha256:abcdef0123456789abcdef0123456789abcdef0123456789abcdef0123456789"
            }
          ]
        }"#;

        let err = select_release_asset(json, "aarch64-apple-darwin").unwrap_err();

        assert!(err.to_string().contains("exactly one"));
    }

    #[cfg(unix)]
    #[test]
    fn replace_binary_rolls_back_when_published_probe_fails() {
        use std::os::unix::fs::PermissionsExt;

        let temp = tempfile::tempdir().unwrap();
        let current = temp.path().join("installed-tink");
        let candidate = temp.path().join("candidate-tink");
        let original = b"#!/bin/sh\nprintf 'tink 0.3.14\\n'\n";
        fs::write(&current, original).unwrap();
        fs::set_permissions(&current, fs::Permissions::from_mode(0o755)).unwrap();
        fs::write(
            &candidate,
            b"#!/bin/sh\ncase \"$0\" in\n  */installed-tink) exit 7 ;;\nesac\nprintf 'tink 99.0.0\\n'\n",
        )
        .unwrap();
        fs::set_permissions(&candidate, fs::Permissions::from_mode(0o755)).unwrap();

        let err = replace_binary(&current, &candidate, "99.0.0").unwrap_err();

        assert!(err.to_string().contains("published"));
        assert_eq!(fs::read(&current).unwrap(), original);
        assert_eq!(
            fs::metadata(&current).unwrap().permissions().mode() & 0o777,
            0o755
        );
    }

    #[test]
    fn select_release_asset_rejects_urls_with_credentials_query_or_fragment() {
        for url in [
            "https://user@example.test/tink.tgz",
            "https://example.test/tink.tgz?token=secret",
            "https://example.test/tink.tgz#asset",
        ] {
            let json = format!(
                r#"{{
                  "tag_name": "v0.2.0",
                  "assets": [{{
                    "name": "tink-0.2.0-aarch64-apple-darwin.tar.gz",
                    "browser_download_url": "{url}",
                    "digest": "sha256:0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef"
                  }}]
                }}"#
            );
            let err = select_release_asset(&json, "aarch64-apple-darwin").unwrap_err();
            assert!(
                err.to_string().contains("download URL"),
                "accepted or leaked {url}: {err}"
            );
            assert!(!err.to_string().contains(url));
        }
    }

    #[test]
    fn verify_archive_digest_rejects_mismatch() {
        let temp = tempfile::tempdir().unwrap();
        let archive = temp.path().join("tink.tgz");
        fs::write(&archive, b"downloaded archive bytes").unwrap();
        let expected: [u8; 32] = Sha256::digest(b"different archive bytes").into();

        let err = verify_archive_digest(&archive, &expected).unwrap_err();

        assert!(err.to_string().contains("digest mismatch"));
    }

    #[test]
    fn validate_archive_inventory_rejects_extra_entries() {
        let temp = tempfile::tempdir().unwrap();
        let stage = temp.path().join("stage");
        fs::create_dir(&stage).unwrap();
        fs::write(stage.join("tink"), b"candidate").unwrap();
        fs::write(stage.join("extra"), b"unexpected").unwrap();
        let archive = temp.path().join("tink.tgz");
        let status = Command::new("tar")
            .args(["-czf"])
            .arg(&archive)
            .arg("-C")
            .arg(&stage)
            .args(["tink", "extra"])
            .status()
            .unwrap();
        assert!(status.success());

        let err = validate_archive_inventory(&archive).unwrap_err();

        assert!(err.to_string().contains("exactly one"));
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
