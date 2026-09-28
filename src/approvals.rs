//! User-scope approvals of library skill trees (`$TINK_HOME/approvals.json`).
//!
//! An approval pins a library skill's tree digest (the same digest `tink mount
//! --json` reports). Payload delivery refuses a skill whose current digest is
//! not the approved one. Tink records approvals itself when it writes a skill
//! into the library (approve-on-write); out-of-band edits need
//! `tink library approve`.

use std::collections::BTreeMap;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::error::Error;
use crate::paths::{self, map_io, refuse_symlink};

pub const FILE: &str = "approvals.json";
const LOCK_FILE: &str = ".approvals.lock";
const VERSION: u32 = 1;

#[derive(Debug, Serialize, Deserialize)]
struct Document {
    version: u32,
    #[serde(default)]
    skills: BTreeMap<String, String>,
}

fn file_path(home: &Path) -> PathBuf {
    home.join(FILE)
}

/// Approved `name -> "sha256:<hex>"` pairs. Missing file means none approved.
pub fn load(home: &Path) -> Result<BTreeMap<String, String>, Error> {
    let path = file_path(home);
    refuse_symlink(&path)?;
    let text = match fs::read_to_string(&path) {
        Ok(text) => text,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(BTreeMap::new()),
        Err(error) => return Err(map_io(&path, error)),
    };
    let document: Document = serde_json::from_str(&text).map_err(|error| {
        Error::msg(format!(
            "{} is not valid approvals JSON ({error}); left unchanged",
            crate::output::display_path(&path)
        ))
    })?;
    if document.version != VERSION {
        return Err(Error::msg(format!(
            "Unsupported approvals version {} in {}",
            document.version,
            crate::output::display_path(&path)
        )));
    }
    Ok(document.skills)
}

/// Record approvals under an exclusive lock and publish the file atomically.
pub fn record(home: &Path, entries: &[(String, String)]) -> Result<(), Error> {
    if entries.is_empty() {
        return Ok(());
    }
    let lock_path = home.join(LOCK_FILE);
    refuse_symlink(&lock_path)?;
    let lock = fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .write(true)
        .open(&lock_path)
        .map_err(|e| map_io(&lock_path, e))?;
    lock.lock().map_err(|e| map_io(&lock_path, e))?;

    let mut skills = load(home)?;
    for (name, digest) in entries {
        skills.insert(name.clone(), digest.clone());
    }
    let body = serde_json::to_string_pretty(&Document {
        version: VERSION,
        skills,
    })
    .map_err(|e| Error::msg(format!("approvals serialize failed: {e}")))?;
    let path = file_path(home);
    let mut tmp = tempfile::NamedTempFile::new_in(home).map_err(|e| paths::map_io(home, e))?;
    tmp.write_all(body.as_bytes())
        .and_then(|()| tmp.write_all(b"\n"))
        .and_then(|()| tmp.as_file().sync_all())
        .map_err(|e| map_io(&path, e))?;
    tmp.persist(&path).map_err(|e| map_io(&path, e.error))?;
    Ok(())
}
