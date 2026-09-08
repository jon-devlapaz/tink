//! Pre-refresh snapshots and `tink skill rollback`.
//!
//! Each applied standalone refresh keeps exactly one generation: the
//! displaced tree plus a small meta file under
//! `<project>/.tink/rollbacks/<skill>/`. Rollback restores that tree
//! through the same atomic staged rename as refresh, then consumes the
//! snapshot, so a rollback is single-use by construction.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use serde_json::{Value, json};

use crate::catalog;
use crate::check;
use crate::error::Error;
use crate::library;
use crate::manifest;
use crate::paths::{map_io, mkdir_p, refuse_symlink};
use crate::provenance::{self, Provenance};
use crate::skills::{self, Skill};

const ROLLBACKS_DIR: &str = "rollbacks";
const META_FILE: &str = "meta.json";
const SNAPSHOT_DIR: &str = "tree";

/// Staged snapshot: tree copied before replace, meta committed after the
/// post-refresh digest is known. A snapshot without valid meta is ignored.
#[derive(Debug)]
pub(crate) struct PendingSnapshot {
    dir: PathBuf,
    name: String,
    prior_revision: String,
    prior_provenance: Provenance,
}

#[derive(Debug)]
struct RollbackSnapshot {
    dir: PathBuf,
    tree: PathBuf,
    post_digest: String,
    provenance: Provenance,
}

fn snapshot_dir(project_root: &Path, name: &str) -> PathBuf {
    project_root
        .join(manifest::DIRECTORY)
        .join(ROLLBACKS_DIR)
        .join(name)
}

/// Copy the about-to-be-replaced tree aside (receipt excluded; the full
/// prior provenance lands in meta instead). Replaces any older snapshot:
// one generation per skill, so storage stays bounded without a policy.
pub(crate) fn begin_snapshot(
    project_root: &Path,
    installed: &Skill,
) -> Result<PendingSnapshot, Error> {
    let prior = provenance::read(installed)?.ok_or_else(|| {
        Error::msg(format!(
            "Rollback needs recorded provenance: {}",
            installed.name
        ))
    })?;
    let prior_revision = prior.get("revision").cloned().ok_or_else(|| {
        Error::msg(format!(
            "Rollback needs a recorded revision: {}",
            installed.name
        ))
    })?;
    let dir = snapshot_dir(project_root, &installed.name);
    if dir.exists() {
        refuse_symlink(&dir)?;
        fs::remove_dir_all(&dir).map_err(|e| map_io(&dir, e))?;
    }
    let tree = dir.join(SNAPSHOT_DIR);
    mkdir_p(&tree)?;
    skills::copy_skill_tree(&installed.path, &tree, &[provenance::SIDECAR_FILE])?;
    Ok(PendingSnapshot {
        dir,
        name: installed.name.clone(),
        prior_revision,
        prior_provenance: prior,
    })
}

impl PendingSnapshot {
    pub(crate) fn commit(self, post_digest: &str) -> Result<(), Error> {
        let meta = json!({
            "name": self.name,
            "prior_revision": self.prior_revision,
            "post_digest": post_digest,
            "provenance": self.prior_provenance,
        });
        let body = format!(
            "{}\n",
            serde_json::to_string_pretty(&meta)
                .map_err(|e| { Error::msg(format!("Could not encode rollback meta: {e}")) })?
        );
        fs::write(self.dir.join(META_FILE), body).map_err(|e| map_io(&self.dir.join(META_FILE), e))
    }
}

fn read_snapshot(project_root: &Path, name: &str) -> Result<Option<RollbackSnapshot>, Error> {
    let dir = snapshot_dir(project_root, name);
    let meta_path = dir.join(META_FILE);
    let tree = dir.join(SNAPSHOT_DIR);
    if !meta_path.is_file() || !tree.is_dir() {
        return Ok(None);
    }
    let raw = fs::read_to_string(&meta_path).map_err(|e| map_io(&meta_path, e))?;
    let value: Value = serde_json::from_str(&raw)
        .map_err(|_| Error::msg(format!("Invalid rollback snapshot: {name}")))?;
    let post_digest = value
        .get("post_digest")
        .and_then(Value::as_str)
        .ok_or_else(|| Error::msg(format!("Invalid rollback snapshot: {name}")))?;
    let provenance = value
        .get("provenance")
        .and_then(Value::as_object)
        .ok_or_else(|| Error::msg(format!("Invalid rollback snapshot: {name}")))?
        .iter()
        .map(|(key, entry)| {
            entry
                .as_str()
                .map(|text| (key.clone(), text.to_string()))
                .ok_or_else(|| Error::msg(format!("Invalid rollback snapshot: {name}")))
        })
        .collect::<Result<BTreeMap<_, _>, _>>()?;
    Ok(Some(RollbackSnapshot {
        dir,
        tree,
        post_digest: post_digest.to_string(),
        provenance,
    }))
}

pub fn rollback_skill(root: &Path, name: &str) -> Result<(), Error> {
    rollback_skill_at(None, root, name)
}

pub(crate) fn rollback_skill_at(home: Option<&Path>, root: &Path, name: &str) -> Result<(), Error> {
    let Some(snapshot) = read_snapshot(root, name)? else {
        return Err(Error::msg(format!(
            "No rollback snapshot for skill: {name}"
        )));
    };
    let skills: BTreeMap<_, _> = check::load_project_skills(root)?
        .into_iter()
        .map(|skill| (skill.name.clone(), skill))
        .collect();
    let current = skills
        .get(name)
        .ok_or_else(|| Error::msg(format!("Installed skill not found: {name}")))?;
    if skills::tree_digest(&current.path, &[])? != snapshot.post_digest {
        return Err(Error::msg(format!(
            "Refusing rollback for {name}: project tree changed since refresh"
        )));
    }
    let destination_root = current
        .path
        .parent()
        .ok_or_else(|| Error::msg("skill has no parent"))?;
    let snapshot_skill = Skill {
        name: name.to_string(),
        path: snapshot.tree,
    };
    let restored =
        skills::replace_verified(&snapshot_skill, destination_root, &snapshot.provenance)?;
    library::sync_from_installed_at(
        home,
        &Skill {
            name: name.to_string(),
            path: restored,
        },
    )?;
    catalog::deposit_skill_at(home, root, name)?;
    refuse_symlink(&snapshot.dir)?;
    fs::remove_dir_all(&snapshot.dir).map_err(|e| map_io(&snapshot.dir, e))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn snapshot_meta_round_trips() {
        let temp = tempfile::tempdir().unwrap();
        let project = temp.path().join("app");
        let skill = temp.path().join("installed").join("alpha");
        fs::create_dir_all(&skill).unwrap();
        fs::write(skill.join("SKILL.md"), "body\n").unwrap();
        let installed = Skill {
            name: "alpha".to_string(),
            path: skill,
        };
        // No receipt yet: snapshot must refuse instead of recording garbage.
        begin_snapshot(&project, &installed).unwrap_err();

        let mut provenance = Provenance::new();
        provenance.insert(
            "source".into(),
            "https://github.com/example/alpha.git".into(),
        );
        provenance.insert(
            "revision".into(),
            "0123456789abcdef0123456789abcdef01234567".into(),
        );
        provenance.insert("path".into(), ".".into());
        provenance::write_file(&installed.path.join(provenance::SIDECAR_FILE), &provenance)
            .unwrap();
        let pending = begin_snapshot(&project, &installed).unwrap();
        // Receipt excluded from the tree copy; full provenance in meta.
        assert!(
            !pending
                .dir
                .join(SNAPSHOT_DIR)
                .join(provenance::SIDECAR_FILE)
                .exists()
        );
        pending.commit("digest-1").unwrap();

        let snapshot = read_snapshot(&project, "alpha").unwrap().unwrap();
        assert_eq!(snapshot.post_digest, "digest-1");
        assert_eq!(snapshot.provenance, provenance);
        assert!(read_snapshot(&project, "missing").unwrap().is_none());
    }
}
