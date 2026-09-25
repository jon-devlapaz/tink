//! Read-only inspection of one standalone installed skill.

use std::path::Path;

use crate::error::Error;
use crate::home;
use crate::library;
use crate::output;
use crate::paths::refuse_symlink;
use crate::provenance;
use crate::skills;
use crate::style::CliStyle;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SkillKind {
    Embedded,
    StandaloneLocal,
    StandaloneRemote,
}

impl SkillKind {
    fn label(self) -> &'static str {
        match self {
            Self::Embedded => "embedded",
            Self::StandaloneLocal => "standalone (local)",
            Self::StandaloneRemote => "standalone (remote)",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SkillReadReport {
    pub name: String,
    pub description: String,
    pub path: String,
    pub kind: SkillKind,
    pub source: Option<String>,
    pub revision: Option<String>,
    pub source_path: Option<String>,
}

pub fn read_skill_report(
    project_root: &Path,
    name: &str,
    library: bool,
) -> Result<SkillReadReport, Error> {
    if !skills::valid_skill_name(name) {
        return Err(Error::msg(format!("Invalid skill name: {name}")));
    }
    if library {
        return read_library_skill(name);
    }
    read_project_skill(project_root, name)
}

pub fn print_report(report: &SkillReadReport, raw: bool, style: CliStyle) -> Result<(), Error> {
    if raw {
        return output::stdout_line(format_args!(
            "{}",
            output::escape_untrusted(&report.description)
        ));
    }

    output::stdout_line(format_args!("{}", style.skill(&report.name)))?;
    print_field(style, "Description:", &report.description)?;
    print_field(style, "Path:", &report.path)?;
    print_field(style, "Kind:", report.kind.label())?;
    match report.kind {
        SkillKind::StandaloneRemote => {
            print_field(
                style,
                "Source:",
                report.source.as_deref().unwrap_or_default(),
            )?;
            print_field(
                style,
                "Revision:",
                report.revision.as_deref().unwrap_or_default(),
            )?;
            print_field(
                style,
                "Source Path:",
                report.source_path.as_deref().unwrap_or_default(),
            )?;
        }
        SkillKind::Embedded | SkillKind::StandaloneLocal => {}
    }
    Ok(())
}

fn print_field(style: CliStyle, label: &str, value: &str) -> Result<(), Error> {
    output::stdout_line(format_args!(
        "  {label:<12} {}",
        style.accent(output::escape_untrusted(value))
    ))
}

fn read_library_skill(name: &str) -> Result<SkillReadReport, Error> {
    let skill = library::load_existing_at(None, name)?;
    skills::validate_skill_tree(&skill.path)?;
    let (_, description) = skills::read_skill_and_description(&skill.path, true)?;
    report_from_skill(&skill, description, None)
}

fn read_project_skill(project_root: &Path, name: &str) -> Result<SkillReadReport, Error> {
    let agents = home::project_agents_path(project_root);
    let skills_root = home::project_skills_path(project_root);
    refuse_symlink(&agents)?;
    refuse_symlink(&skills_root)?;
    if !skills_root.is_dir() {
        return Err(Error::msg("Missing .agents/skills"));
    }

    let target = skills_root.join(name);
    if !target.exists() && !target.is_symlink() {
        return Err(Error::msg(missing_project_skill(name)));
    }
    refuse_symlink(&target)?;
    if !target.is_dir() {
        return Err(Error::msg(format!(
            "Unexpected entry in .agents/skills: {name}"
        )));
    }
    if crate::skillsets::has_receipt_entry(&target) {
        return Err(Error::msg(
            "Skillset root detected; use `tink skillset list`",
        ));
    }

    let (skill, description) = skills::read_skill_and_description(&target, true)?;
    skills::validate_skill_tree(&skill.path)?;
    report_from_skill(&skill, description, Some(project_root))
}

fn missing_project_skill(name: &str) -> String {
    let hint = library::list_names(None)
        .ok()
        .is_some_and(|names| names.iter().any(|entry| entry == name));
    if hint {
        format!("Skill not found: {name} (present in library; use --library)")
    } else {
        format!("Skill not found: {name}")
    }
}

fn report_from_skill(
    skill: &skills::Skill,
    description: String,
    project_root: Option<&Path>,
) -> Result<SkillReadReport, Error> {
    let provenance = provenance::read(skill)?;
    let (kind, source, revision, source_path) = match provenance {
        Some(provenance) => (
            SkillKind::StandaloneRemote,
            Some(provenance["source"].clone()),
            Some(provenance["revision"].clone()),
            Some(provenance["path"].clone()),
        ),
        None if skill.name == "manage-tink" => (SkillKind::Embedded, None, None, None),
        None => (SkillKind::StandaloneLocal, None, None, None),
    };
    Ok(SkillReadReport {
        name: skill.name.clone(),
        description,
        path: displayed_skill_path(project_root, &skill.path),
        kind,
        source,
        revision,
        source_path,
    })
}

fn displayed_skill_path(project_root: Option<&Path>, skill_path: &Path) -> String {
    if let Some(root) = project_root
        && let Ok(relative) = skill_path.strip_prefix(root)
    {
        return relative.to_string_lossy().into_owned();
    }
    skill_path.to_string_lossy().into_owned()
}
