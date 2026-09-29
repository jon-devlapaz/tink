//! `tink use`: compile a skillset's `required` members into persistent rules.
//!
//! The pin at `$TINK_HOME/skillsets/<name>.json` names the members that carry
//! discipline rules (`"required": [...]`). Each one passes the same trust checks
//! as `tink mount --json --payload` (no symlinks, identity, approved digest)
//! before a one-line rule per skill is written into a managed block of
//! AGENTS.md, and optionally into a snapshot directory with a lock file.
//! tink knows skillsets, not what a caller calls a "phase".

use std::collections::BTreeSet;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use serde::Serialize;
use sha2::{Digest, Sha256};

use crate::error::Error;
use crate::home;
use crate::mount::{self, MountRefusal, VerifiedSkill};
use crate::skills;
use crate::skillsets;

pub const CONTRACT_VERSION: u32 = 1;
pub const DEFAULT_MAX_BYTES: usize = 8192;

const HEADER: &str = "Discipline rules for this phase (compiled by tink; do not edit by hand):\n";
const MARKER_PREFIX: &str = "<!-- tink:rules";
const BEGIN_PREFIX: &str = "<!-- tink:rules begin skillset=";
const BEGIN_SUFFIX: &str = " -->";
const END_LINE: &str = "<!-- tink:rules end -->";
const RULES_FILE: &str = "rules.md";
const LOCK_FILE: &str = "skills.lock.json";
const DESCRIPTION_CAP: usize = 400;

pub struct Options {
    pub skillset: String,
    pub agents_md: Option<PathBuf>,
    pub snapshot: Option<PathBuf>,
    pub max_bytes: usize,
    pub check: bool,
}

#[derive(Debug, Serialize)]
pub struct Report {
    pub contract_version: u32,
    pub skillset: String,
    pub skills: Vec<String>,
    pub bytes: usize,
    pub rules_digest: String,
    pub agents_md: String,
    pub snapshot: Option<String>,
}

pub enum Failure {
    /// Trust or configuration refusal (exit 2, or 1 for plain I/O errors).
    Refused(MountRefusal),
    /// `--check` found drift (exit 1); one reason per mismatch.
    Mismatch(Vec<String>),
}

impl Failure {
    pub fn code(&self) -> &str {
        match self {
            Self::Refused(refusal) => refusal.code,
            Self::Mismatch(_) => "check_failed",
        }
    }

    pub fn message(&self) -> String {
        match self {
            Self::Refused(refusal) => refusal.message.clone(),
            Self::Mismatch(reasons) => reasons.join("; "),
        }
    }

    pub fn exit_code(&self) -> u8 {
        match self {
            Self::Refused(refusal) if refusal.code != "error" => 2,
            _ => 1,
        }
    }

    pub fn lines(&self) -> Vec<String> {
        match self {
            Self::Refused(refusal) => {
                vec![format!("tink use: {} [{}]", refusal.message, refusal.code)]
            }
            Self::Mismatch(reasons) => reasons.clone(),
        }
    }
}

impl From<Error> for Failure {
    fn from(error: Error) -> Self {
        Self::Refused(error.into())
    }
}

impl From<MountRefusal> for Failure {
    fn from(refusal: MountRefusal) -> Self {
        Self::Refused(refusal)
    }
}

fn refuse(code: &'static str, message: impl Into<String>) -> Failure {
    Failure::Refused(MountRefusal::new(code, message))
}

fn sha_hex(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

struct Compiled {
    name: String,
    digest: String,
    line: String,
    unapproved: Option<MountRefusal>,
}

fn collapse(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn trim_chars(text: String, cap: usize) -> String {
    if text.chars().count() <= cap {
        return text;
    }
    let mut trimmed: String = text.chars().take(cap - 3).collect();
    trimmed.push_str("...");
    trimmed
}

fn rule_for(skill: &VerifiedSkill) -> Result<String, Failure> {
    let text = skill.skill_md_text().unwrap_or_default();
    let explicit = skills::frontmatter_field(&text, "rule")
        .map(|value| collapse(&value))
        .filter(|value| !value.is_empty());
    let rule = match explicit {
        Some(rule) => rule,
        None => {
            let description = skills::frontmatter_field(&text, "description")
                .map(|value| collapse(&value))
                .unwrap_or_default();
            trim_chars(description, DESCRIPTION_CAP)
        }
    };
    if rule.is_empty() {
        return Err(refuse(
            "invalid_rule",
            format!(
                "Skill '{}' has neither a `rule:` nor a `description:` in its frontmatter",
                skill.name
            ),
        ));
    }
    if rule.contains("<!--") || rule.contains("-->") {
        return Err(refuse(
            "invalid_rule",
            format!(
                "Skill '{}' rule contains a comment delimiter; refusing to compile it",
                skill.name
            ),
        ));
    }
    Ok(rule)
}

fn validate_required(
    name: &str,
    pin_path: &Path,
    meta: &skillsets::SkillsetMeta,
) -> Result<(), Failure> {
    if meta.required.is_empty() {
        return Err(refuse(
            "required_missing",
            format!(
                "Skillset pin {name} has no `required` members; add a \"required\": [\"member\", ...] list to {}",
                crate::output::display_path(pin_path)
            ),
        ));
    }
    let members: BTreeSet<&String> = meta.members.iter().collect();
    let mut seen = BTreeSet::new();
    for member in &meta.required {
        if !seen.insert(member) {
            return Err(refuse(
                "required_invalid",
                format!("Duplicate `required` member '{member}' in pin {name}"),
            ));
        }
        if !members.contains(member) {
            return Err(refuse(
                "required_invalid",
                format!("`required` member '{member}' is not a member of {name}"),
            ));
        }
    }
    Ok(())
}

fn compile_skills(
    home_root: &Path,
    meta: &skillsets::SkillsetMeta,
    check: bool,
) -> Result<Vec<Compiled>, Failure> {
    let library_root = home::skills_library_path(home_root);
    let mut compiled = Vec::new();
    for name in &meta.required {
        let skill = mount::verify_library_skill(&library_root, name)?;
        if skill.skill_md_text().is_none() {
            return Err(refuse(
                "invalid_skill",
                format!("Skill '{name}' has no regular SKILL.md"),
            ));
        }
        let unapproved = match mount::require_approved(home_root, &skill) {
            Ok(()) => None,
            Err(refusal) if check => Some(refusal),
            Err(refusal) => return Err(refusal.into()),
        };
        let rule = rule_for(&skill)?;
        compiled.push(Compiled {
            name: name.clone(),
            digest: skill.tree_digest(),
            line: format!(
                "- {name}: {rule} (full: .tink/.active/{name}/SKILL.md; run: tink mount {name})\n"
            ),
            unapproved,
        });
    }
    Ok(compiled)
}

struct Located {
    start: usize,
    end: usize,
}

fn malformed(detail: &str) -> Failure {
    refuse(
        "malformed_markers",
        format!(
            "AGENTS.md has malformed `tink:rules` markers ({detail}); fix or remove them by hand"
        ),
    )
}

fn valid_begin(line: &str) -> bool {
    let Some(rest) = line.strip_prefix(BEGIN_PREFIX) else {
        return false;
    };
    let Some(rest) = rest.strip_suffix(BEGIN_SUFFIX) else {
        return false;
    };
    let Some((name, digest)) = rest.split_once(" digest=") else {
        return false;
    };
    !name.is_empty()
        && name
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
        && digest.len() == 64
        && digest.chars().all(|c| matches!(c, '0'..='9' | 'a'..='f'))
}

fn locate_block(text: &str) -> Result<Option<Located>, Failure> {
    let mut begin: Option<usize> = None;
    let mut found: Option<Located> = None;
    let mut offset = 0;
    for raw in text.split_inclusive('\n') {
        let line = raw.trim_end_matches(['\n', '\r']);
        if line.starts_with(MARKER_PREFIX) {
            if found.is_some() {
                return Err(malformed("more than one block"));
            }
            if line == END_LINE {
                match begin.take() {
                    Some(start) => {
                        found = Some(Located {
                            start,
                            end: offset + raw.len(),
                        });
                    }
                    None => return Err(malformed("end marker without begin")),
                }
            } else if valid_begin(line) {
                if begin.is_some() {
                    return Err(malformed("nested begin marker"));
                }
                begin = Some(offset);
            } else {
                return Err(malformed("unrecognised marker line"));
            }
        }
        offset += raw.len();
    }
    if begin.is_some() {
        return Err(malformed("begin marker without end"));
    }
    Ok(found)
}

fn atomic_write(path: &Path, bytes: &[u8], mode_from: Option<&Path>) -> Result<(), Error> {
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    let mut tmp =
        tempfile::NamedTempFile::new_in(parent).map_err(|e| crate::paths::map_io(parent, e))?;
    tmp.write_all(bytes)
        .and_then(|()| tmp.as_file().sync_all())
        .map_err(|e| crate::paths::map_io(path, e))?;
    if let Some(source) = mode_from
        && let Ok(meta) = fs::metadata(source)
    {
        let _ = fs::set_permissions(tmp.path(), meta.permissions());
    }
    tmp.persist(path)
        .map_err(|e| crate::paths::map_io(path, e.error))?;
    Ok(())
}

#[derive(Serialize)]
struct LockSkill<'a> {
    name: &'a str,
    digest: &'a str,
    source: Option<&'a str>,
    revision: Option<&'a str>,
}

#[derive(Serialize)]
struct Lock<'a> {
    schema: u32,
    skillset: &'a str,
    rules_digest: &'a str,
    skills: Vec<LockSkill<'a>>,
}

fn absolute(cwd: &Path, path: &Path) -> PathBuf {
    if path.is_absolute() {
        path.to_path_buf()
    } else {
        cwd.join(path)
    }
}

pub fn run(cwd: &Path, options: &Options) -> Result<Report, Failure> {
    let canonical = skillsets::canonicalize_skillset_name(&options.skillset)
        .map_err(|e| refuse("invalid_name", e.to_string()))?;
    let home_root = home::resolve_home()?;
    let pin_path = home::skillset_pin_path(&home_root, &canonical);
    if !pin_path.exists() && !pin_path.is_symlink() {
        return Err(refuse(
            "skillset_not_found",
            format!(
                "Skillset pin {canonical} not found: {}",
                crate::output::display_path(&pin_path)
            ),
        ));
    }
    let meta = skillsets::read_skillset_pin(Some(&home_root), &canonical)
        .map_err(|e| refuse("invalid_pin", e.to_string()))?;
    validate_required(&canonical, &pin_path, &meta)?;
    let compiled = compile_skills(&home_root, &meta, options.check)?;

    let mut body = String::from(HEADER);
    for skill in &compiled {
        body.push_str(&skill.line);
    }
    if body.len() > options.max_bytes {
        let mut sizes: Vec<(&str, usize)> = compiled
            .iter()
            .map(|skill| (skill.name.as_str(), skill.line.len()))
            .collect();
        sizes.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(b.0)));
        let largest: Vec<String> = sizes
            .iter()
            .take(3)
            .map(|(name, size)| format!("{name} ({size} bytes)"))
            .collect();
        return Err(refuse(
            "over_cap",
            format!(
                "Compiled rules for {canonical} are {} bytes, over the {}-byte cap; largest: {}. Trim `required` or pass --max-bytes",
                body.len(),
                options.max_bytes,
                largest.join(", ")
            ),
        ));
    }
    let body_digest = sha_hex(body.as_bytes());
    let rules_digest = format!("sha256:{body_digest}");
    let block =
        format!("{BEGIN_PREFIX}{canonical} digest={body_digest}{BEGIN_SUFFIX}\n{body}{END_LINE}\n");

    // AGENTS.md: must already exist, must not be a link.
    let agents_path = absolute(
        cwd,
        options
            .agents_md
            .as_deref()
            .unwrap_or(Path::new("AGENTS.md")),
    );
    if agents_path.is_symlink() {
        return Err(refuse(
            "agents_md_symlink",
            format!(
                "Refusing symlinked AGENTS.md: {}",
                crate::output::display_path(&agents_path)
            ),
        ));
    }
    if !agents_path.is_file() {
        return Err(refuse(
            "agents_md_missing",
            format!(
                "AGENTS.md not found: {} (tink use never creates it)",
                crate::output::display_path(&agents_path)
            ),
        ));
    }
    let original = fs::read(&agents_path).map_err(|e| crate::paths::map_io(&agents_path, e))?;
    let original = String::from_utf8(original).map_err(|_| {
        refuse(
            "agents_md_invalid",
            format!(
                "AGENTS.md is not UTF-8 text: {}",
                crate::output::display_path(&agents_path)
            ),
        )
    })?;
    let located = locate_block(&original)?;

    // Snapshot directory: validated before anything is written.
    let snapshot_dir = options.snapshot.as_ref().map(|dir| absolute(cwd, dir));
    if let Some(dir) = &snapshot_dir {
        if dir.is_symlink() {
            return Err(refuse(
                "snapshot_symlink",
                format!(
                    "Refusing symlinked snapshot directory: {}",
                    crate::output::display_path(dir)
                ),
            ));
        }
        if dir.exists() && !dir.is_dir() {
            return Err(refuse(
                "snapshot_invalid",
                format!(
                    "Snapshot path is not a directory: {}",
                    crate::output::display_path(dir)
                ),
            ));
        }
        for file in [RULES_FILE, LOCK_FILE] {
            if dir.join(file).is_symlink() {
                return Err(refuse(
                    "snapshot_symlink",
                    format!(
                        "Refusing symlinked snapshot file: {}",
                        crate::output::display_path(&dir.join(file))
                    ),
                ));
            }
        }
    }
    let lock_text = {
        let lock = Lock {
            schema: 1,
            skillset: &canonical,
            rules_digest: &rules_digest,
            skills: compiled
                .iter()
                .map(|skill| LockSkill {
                    name: &skill.name,
                    digest: &skill.digest,
                    source: Some(meta.source.as_str()),
                    revision: Some(meta.revision.as_str()),
                })
                .collect(),
        };
        let mut text = serde_json::to_string_pretty(&lock)
            .map_err(|e| Error::msg(format!("serialize lock: {e}")))?;
        text.push('\n');
        text
    };

    let report = Report {
        contract_version: CONTRACT_VERSION,
        skillset: canonical.clone(),
        skills: compiled.iter().map(|skill| skill.name.clone()).collect(),
        bytes: body.len(),
        rules_digest: rules_digest.clone(),
        agents_md: agents_path.to_string_lossy().into_owned(),
        snapshot: snapshot_dir
            .as_ref()
            .map(|dir| dir.to_string_lossy().into_owned()),
    };

    if options.check {
        let mut reasons = Vec::new();
        let rewrite_fix = format!("run `tink use {}` to rewrite it", options.skillset);
        match &located {
            None => reasons.push(format!(
                "missing block: no tink:rules block in {}; {rewrite_fix}",
                crate::output::display_path(&agents_path)
            )),
            Some(found) if original[found.start..found.end] != block => reasons.push(format!(
                "block differs: {} does not match the compiled rules; {rewrite_fix}",
                crate::output::display_path(&agents_path)
            )),
            Some(_) => {}
        }
        for skill in &compiled {
            if let Some(refusal) = &skill.unapproved {
                reasons.push(format!(
                    "unapproved {} ({}); review, then run `tink library approve {}`",
                    skill.name, refusal.code, skill.name
                ));
            }
        }
        if let Some(dir) = &snapshot_dir {
            let existing_lock = fs::read_to_string(dir.join(LOCK_FILE)).ok();
            if let Some(text) = &existing_lock
                && let Ok(value) = serde_json::from_str::<serde_json::Value>(text)
            {
                for skill in &compiled {
                    let locked = value["skills"]
                        .as_array()
                        .and_then(|list| {
                            list.iter()
                                .find(|entry| entry["name"] == skill.name.as_str())
                        })
                        .and_then(|entry| entry["digest"].as_str());
                    if locked.is_some_and(|digest| digest != skill.digest) {
                        reasons.push(format!(
                            "lock digest drift for {}; review, then run `tink library approve {}`",
                            skill.name, skill.name
                        ));
                    }
                }
            }
            for (file, want) in [(RULES_FILE, body.as_str()), (LOCK_FILE, lock_text.as_str())] {
                let path = dir.join(file);
                match fs::read(&path) {
                    Err(_) => reasons.push(format!(
                        "snapshot missing: {}; {rewrite_fix}",
                        crate::output::display_path(&path)
                    )),
                    Ok(bytes) if bytes != want.as_bytes() => reasons.push(format!(
                        "snapshot differs: {}; {rewrite_fix}",
                        crate::output::display_path(&path)
                    )),
                    Ok(_) => {}
                }
            }
        }
        return if reasons.is_empty() {
            Ok(report)
        } else {
            Err(Failure::Mismatch(reasons))
        };
    }

    let updated = match &located {
        Some(found) => format!(
            "{}{block}{}",
            &original[..found.start],
            &original[found.end..]
        ),
        None => {
            let mut text = original.clone();
            if !text.is_empty() {
                if !text.ends_with('\n') {
                    text.push('\n');
                }
                text.push('\n');
            }
            text.push_str(&block);
            text
        }
    };
    if let Some(dir) = &snapshot_dir {
        fs::create_dir_all(dir).map_err(|e| crate::paths::map_io(dir, e))?;
        atomic_write(&dir.join(LOCK_FILE), lock_text.as_bytes(), None)?;
        atomic_write(&dir.join(RULES_FILE), body.as_bytes(), None)?;
    }
    if updated != original {
        atomic_write(&agents_path, updated.as_bytes(), Some(&agents_path))?;
    }
    Ok(report)
}
