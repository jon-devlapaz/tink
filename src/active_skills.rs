//! Project-wide active skill-name index.
//!
//! Each skill name may be owned by at most one standalone tree or one skillset
//! member. Mutation commands preflight against this index; `skill check` reports
//! collisions already present on disk.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use crate::error::Error;
use crate::output;
use crate::skills;
use crate::skillsets::{self, EntryClass};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum SkillOwner {
    Standalone {
        path: PathBuf,
    },
    Member {
        skillset: String,
        member_dir: String,
    },
}

#[derive(Debug, Default)]
pub(crate) struct ActiveSkillIndex {
    owners: BTreeMap<String, SkillOwner>,
}

impl ActiveSkillIndex {
    pub(crate) fn build(project_root: &Path) -> Result<Self, Error> {
        let skills_root = crate::home::project_skills_path(project_root);
        if !skills_root.is_dir() {
            return Ok(Self::default());
        }
        let mut index = Self::default();
        let mut entries: Vec<_> = std::fs::read_dir(&skills_root)
            .map_err(|e| crate::paths::map_io(&skills_root, e))?
            .filter_map(|entry| entry.ok())
            .map(|entry| entry.path())
            .collect();
        entries.sort();
        for path in entries {
            match skillsets::classify_entry(&path) {
                EntryClass::Ignored | EntryClass::Unexpected => {}
                EntryClass::Standalone => {
                    let skill = skills::read_skill(&path, true)?;
                    index
                        .owners
                        .insert(skill.name, SkillOwner::Standalone { path });
                }
                EntryClass::Skillset => {
                    let skillset_name = path
                        .file_name()
                        .and_then(|value| value.to_str())
                        .unwrap_or("")
                        .to_string();
                    let receipt =
                        skillsets::read_owned_receipt(&path, "installed skillset receipt")?;
                    for member_dir in receipt.members {
                        let member_path = path.join(&member_dir);
                        let skill = skills::read_skill(&member_path, true)?;
                        index.owners.insert(
                            skill.name,
                            SkillOwner::Member {
                                skillset: skillset_name.clone(),
                                member_dir,
                            },
                        );
                    }
                }
            }
        }
        Ok(index)
    }

    pub(crate) fn ensure_available(
        &self,
        name: &str,
        proposed: &SkillOwner,
        exclude_skillset: Option<&str>,
    ) -> Result<(), Error> {
        let Some(existing) = self.owners.get(name) else {
            return Ok(());
        };
        if existing == proposed {
            return Ok(());
        }
        if let (SkillOwner::Member { skillset, .. }, Some(excluded)) = (existing, exclude_skillset)
            && skillset == excluded
            && matches!(proposed, SkillOwner::Member { skillset: proposed_set, .. } if proposed_set == excluded)
        {
            return Ok(());
        }
        Err(conflict_error(name, existing, proposed))
    }

    pub(crate) fn ensure_members_available(
        &self,
        members: &[(String, String)],
        skillset_name: &str,
        exclude_skillset: Option<&str>,
    ) -> Result<(), Error> {
        for (skill_name, member_dir) in members {
            self.ensure_available(
                skill_name,
                &SkillOwner::Member {
                    skillset: skillset_name.to_string(),
                    member_dir: member_dir.clone(),
                },
                exclude_skillset,
            )?;
        }
        Ok(())
    }

    pub(crate) fn collect_conflicts(project_root: &Path) -> Result<Vec<String>, Error> {
        let skills_root = crate::home::project_skills_path(project_root);
        if !skills_root.is_dir() {
            return Ok(Vec::new());
        }
        let mut seen: BTreeMap<String, SkillOwner> = BTreeMap::new();
        let mut conflicts = Vec::new();
        let mut entries: Vec<_> = std::fs::read_dir(&skills_root)
            .map_err(|e| crate::paths::map_io(&skills_root, e))?
            .filter_map(|entry| entry.ok())
            .map(|entry| entry.path())
            .collect();
        entries.sort();
        for path in entries {
            match skillsets::classify_entry(&path) {
                EntryClass::Ignored | EntryClass::Unexpected => {}
                EntryClass::Standalone => {
                    let skill = match skills::read_skill(&path, true) {
                        Ok(skill) => skill,
                        Err(_) => continue,
                    };
                    record_owner(
                        &mut seen,
                        &mut conflicts,
                        skill.name,
                        SkillOwner::Standalone { path },
                    );
                }
                EntryClass::Skillset => {
                    let skillset_name = path
                        .file_name()
                        .and_then(|value| value.to_str())
                        .unwrap_or("")
                        .to_string();
                    let receipt =
                        match skillsets::read_owned_receipt(&path, "installed skillset receipt") {
                            Ok(receipt) => receipt,
                            Err(_) => continue,
                        };
                    for member_dir in receipt.members {
                        let member_path = path.join(&member_dir);
                        let skill = match skills::read_skill(&member_path, true) {
                            Ok(skill) => skill,
                            Err(_) => continue,
                        };
                        record_owner(
                            &mut seen,
                            &mut conflicts,
                            skill.name,
                            SkillOwner::Member {
                                skillset: skillset_name.clone(),
                                member_dir,
                            },
                        );
                    }
                }
            }
        }
        Ok(conflicts)
    }
}

fn record_owner(
    seen: &mut BTreeMap<String, SkillOwner>,
    conflicts: &mut Vec<String>,
    name: String,
    owner: SkillOwner,
) {
    if let Some(existing) = seen.get(&name) {
        let message = conflict_error(&name, existing, &owner).to_string();
        if !conflicts.iter().any(|entry| entry == &message) {
            conflicts.push(message);
        }
    } else {
        seen.insert(name, owner);
    }
}

pub(crate) fn conflict_between(name: &str, existing: &SkillOwner, proposed: &SkillOwner) -> Error {
    conflict_error(name, existing, proposed)
}

fn conflict_error(name: &str, existing: &SkillOwner, proposed: &SkillOwner) -> Error {
    Error::msg(format!(
        "Active skill name conflict for {name}: {} vs {}",
        describe_owner(existing),
        describe_owner(proposed)
    ))
}

fn describe_owner(owner: &SkillOwner) -> String {
    match owner {
        SkillOwner::Standalone { path } => {
            format!("standalone skill at {}", output::display_path(path))
        }
        SkillOwner::Member {
            skillset,
            member_dir,
        } => format!("member {member_dir} in skillset {skillset}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn write_standalone(skills_root: &Path, name: &str) {
        let path = skills_root.join(name);
        fs::create_dir_all(&path).unwrap();
        fs::write(
            path.join("SKILL.md"),
            format!("---\nname: {name}\ndescription: Fixture.\n---\n"),
        )
        .unwrap();
    }

    fn write_skillset(skills_root: &Path, skillset: &str, members: &[&str]) {
        let root = skills_root.join(skillset);
        for member in members {
            let member_path = root.join(member);
            fs::create_dir_all(&member_path).unwrap();
            fs::write(
                member_path.join("SKILL.md"),
                format!("---\nname: {member}\ndescription: Fixture.\n---\n"),
            )
            .unwrap();
        }
        let digest =
            skills::tree_digest(&root, &[".tink-skillset.json", "SKILL.md", ".DS_Store"]).unwrap();
        let receipt = serde_json::json!({
            "source": "https://github.com/example/skills.git",
            "revision": "a".repeat(40),
            "sourceRoot": "skills",
            "members": members,
            "digestVersion": 2,
            "digest": digest,
        });
        fs::write(
            root.join(".tink-skillset.json"),
            format!("{}\n", serde_json::to_string_pretty(&receipt).unwrap()),
        )
        .unwrap();
        fs::write(
            root.join("SKILL.md"),
            format!("---\nname: {skillset}\ndescription: Router.\n---\n"),
        )
        .unwrap();
    }

    #[test]
    fn index_detects_standalone_and_member_names() {
        let temp = tempfile::tempdir().unwrap();
        let project = temp.path();
        let skills_root = project.join(".agents/skills");
        fs::create_dir_all(&skills_root).unwrap();
        write_standalone(&skills_root, "solo");
        write_skillset(&skills_root, "group-skillset", &["alpha"]);

        let index = ActiveSkillIndex::build(project).unwrap();
        assert!(index.owners.contains_key("solo"));
        assert!(index.owners.contains_key("alpha"));
    }

    #[test]
    fn collect_conflicts_reports_both_owners() {
        let temp = tempfile::tempdir().unwrap();
        let project = temp.path();
        let skills_root = project.join(".agents/skills");
        fs::create_dir_all(&skills_root).unwrap();
        write_standalone(&skills_root, "alpha");
        write_skillset(&skills_root, "group-skillset", &["alpha"]);

        let conflicts = ActiveSkillIndex::collect_conflicts(project).unwrap();
        assert_eq!(conflicts.len(), 1);
        assert!(conflicts[0].contains("standalone skill"));
        assert!(conflicts[0].contains("group-skillset"));
    }
}
