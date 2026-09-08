//! Read-only staleness report (`tink skill outdated`).
//!
//! Unlike `refresh`, this never writes to the project or the home
//! inventory; clones used for tree comparison live in temp scratch.

use std::path::Path;

use crate::check;
use crate::error::Error;
use crate::git;
use crate::manage_tink;
use crate::provenance;
use crate::refresh;
use crate::skills::Skill;
use crate::sources;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OutdatedStatus {
    Current,
    Behind { tree_changed: bool },
    Local,
    Modified,
    Unknown,
}

#[derive(Debug, Clone)]
pub struct OutdatedRow {
    pub name: String,
    pub recorded: Option<String>,
    pub tip: Option<String>,
    pub status: OutdatedStatus,
    pub note: Option<String>,
}

pub fn outdated_all(root: &Path) -> Result<Vec<OutdatedRow>, Error> {
    outdated_all_at(None, root)
}

pub(crate) fn outdated_all_at(home: Option<&Path>, root: &Path) -> Result<Vec<OutdatedRow>, Error> {
    // Lenient load: a stale embedded manage-tink must report as behind,
    // not fail the whole command the way `check` does.
    let mut rows = Vec::new();
    for installed in check::load_project_skills_lenient(root)? {
        rows.push(outdated_skill(home, &installed)?);
    }
    rows.sort_by(|left, right| left.name.cmp(&right.name));
    Ok(rows)
}

fn outdated_skill(home: Option<&Path>, installed: &Skill) -> Result<OutdatedRow, Error> {
    let name = installed.name.clone();
    let Some(recorded_provenance) = provenance::read(installed)? else {
        if name == "manage-tink" {
            // Embedded reserved copy: freshness is content equality.
            return Ok(if manage_tink::is_current(installed)? {
                current(name, None, None)
            } else {
                OutdatedRow {
                    name,
                    recorded: None,
                    tip: None,
                    status: OutdatedStatus::Behind { tree_changed: true },
                    note: Some("embedded copy differs from this Tink binary".to_string()),
                }
            });
        }
        return Ok(OutdatedRow {
            name,
            recorded: None,
            tip: None,
            status: OutdatedStatus::Local,
            note: None,
        });
    };
    let recorded = recorded_provenance.get("revision").cloned();
    let remote = sources::parse_remote(&recorded_provenance["source"])?;
    let tip = match git::remote_head(&remote) {
        Ok(tip) => tip,
        Err(error) => {
            return Ok(OutdatedRow {
                name,
                recorded,
                tip: None,
                status: OutdatedStatus::Unknown,
                note: Some(error.to_string()),
            });
        }
    };
    if Some(tip.as_str()) == recorded.as_deref() {
        return Ok(current(name, recorded, Some(tip)));
    }
    match refresh::prepare_refresh(home, installed.clone()) {
        Ok(refresh::RefreshPlan::Unchanged { .. }) => Ok(current(name, recorded, Some(tip))),
        Ok(refresh::RefreshPlan::Local { .. }) => Ok(OutdatedRow {
            name,
            recorded,
            tip: Some(tip),
            status: OutdatedStatus::Local,
            note: None,
        }),
        Ok(refresh::RefreshPlan::Update { tree_changed, .. }) => Ok(OutdatedRow {
            name,
            recorded,
            tip: Some(tip),
            status: OutdatedStatus::Behind { tree_changed },
            note: None,
        }),
        // Single contained string match: refresh owns this wording, and a
        // wording change safely degrades `modified` to `unknown` with detail.
        Err(error) if error.to_string().contains("local modifications") => Ok(OutdatedRow {
            name,
            recorded,
            tip: Some(tip),
            status: OutdatedStatus::Modified,
            note: None,
        }),
        Err(error) => Ok(OutdatedRow {
            name,
            recorded,
            tip: Some(tip),
            status: OutdatedStatus::Unknown,
            note: Some(error.to_string()),
        }),
    }
}

fn current(name: String, recorded: Option<String>, tip: Option<String>) -> OutdatedRow {
    OutdatedRow {
        name,
        recorded,
        tip,
        status: OutdatedStatus::Current,
        note: None,
    }
}
