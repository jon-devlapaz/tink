//! Promotion of explicitly added skills out of tink-route's ephemeral ledger.
//!
//! `tink-route -i` records skills it installs in `.tink/ephemeral.json` so
//! `tink-route --prune` can remove them. A deliberate `tink skill add` means the
//! user wants to keep the skill, so its name is dropped from that ledger.
//! tink-route marks its own installs with `TINK_ROUTE_INSTALL=1`; those leave
//! the ledger alone.

use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::path::Path;

use crate::error::Error;
use crate::paths;

const LEDGER: &str = ".tink/ephemeral.json";
/// Same lock file tink-route takes (`flock`) around its ledger writes.
const LOCK: &str = ".tink/ephemeral.lock";

/// Best-effort: a bad ledger warns on stderr and never fails the add.
pub(crate) fn promote_after_add(project_root: &Path, skill: &str) {
    if std::env::var_os("TINK_ROUTE_INSTALL").is_some_and(|v| v == "1") {
        return;
    }
    if !project_root.join(LEDGER).is_file() {
        return;
    }
    if let Err(err) = promote(project_root, skill) {
        eprintln!("warning: could not update {LEDGER}: {err}");
    }
}

fn promote(project_root: &Path, skill: &str) -> Result<(), Error> {
    let lock_path = project_root.join(LOCK);
    let lock: File = OpenOptions::new()
        .create(true)
        .truncate(false)
        .write(true)
        .open(&lock_path)
        .map_err(|e| paths::map_io(&lock_path, e))?;
    lock.lock().map_err(|e| paths::map_io(&lock_path, e))?;

    let ledger_path = project_root.join(LEDGER);
    let text = match fs::read_to_string(&ledger_path) {
        Ok(text) => text,
        // Removed while waiting for the lock: nothing to promote.
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(e) => return Err(paths::map_io(&ledger_path, e)),
    };
    let mut doc: serde_json::Value = serde_json::from_str(&text)
        .map_err(|e| Error::msg(format!("ledger is not valid JSON ({e}); left unchanged")))?;
    let skills = doc
        .get_mut("skills")
        .and_then(serde_json::Value::as_array_mut)
        .ok_or_else(|| Error::msg("ledger has no 'skills' list; left unchanged"))?;
    let before = skills.len();
    skills.retain(|s| s.as_str() != Some(skill));
    if skills.len() == before {
        return Ok(());
    }

    let body = serde_json::to_string_pretty(&doc)
        .map_err(|e| Error::msg(format!("ledger serialize failed: {e}")))?;
    let dir = ledger_path.parent().unwrap_or(project_root);
    let mut tmp = tempfile::NamedTempFile::new_in(dir).map_err(|e| paths::map_io(dir, e))?;
    tmp.write_all(body.as_bytes())
        .and_then(|()| tmp.write_all(b"\n"))
        .and_then(|()| tmp.as_file().sync_all())
        .map_err(|e| paths::map_io(&ledger_path, e))?;
    tmp.persist(&ledger_path)
        .map_err(|e| paths::map_io(&ledger_path, e.error))?;
    Ok(())
}
