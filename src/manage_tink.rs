//! Embedded `manage-tink` skill shipped with the Tink binary.

use std::path::Path;

use crate::add;
use crate::error::Error;
use crate::paths::map_io;

const SKILL_MD: &str = include_str!("../skills/manage-tink/SKILL.md");
const OPENAI_YAML: &str = include_str!("../skills/manage-tink/agents/openai.yaml");
const COMMANDS_MD: &str = include_str!("../skills/manage-tink/references/commands.md");

/// Stage the embedded skill and install it into the project via `add`.
///
/// Uses the normal add path (project install + home stash + name catalog).
/// Returns the installed skill name.
pub fn install_manage_tink(project_root: &Path) -> Result<String, Error> {
    let staging = tempfile::Builder::new()
        .prefix(".tink-manage-tink-")
        .tempdir()
        .map_err(|e| Error::msg(format!("manage-tink staging: {e}")))?;
    let skill_root = staging.path().join("manage-tink");
    let agents = skill_root.join("agents");
    let references = skill_root.join("references");
    std::fs::create_dir_all(&agents).map_err(|e| map_io(&agents, e))?;
    std::fs::create_dir_all(&references).map_err(|e| map_io(&references, e))?;
    std::fs::write(skill_root.join("SKILL.md"), SKILL_MD)
        .map_err(|e| map_io(&skill_root.join("SKILL.md"), e))?;
    std::fs::write(agents.join("openai.yaml"), OPENAI_YAML)
        .map_err(|e| map_io(&agents.join("openai.yaml"), e))?;
    std::fs::write(references.join("commands.md"), COMMANDS_MD)
        .map_err(|e| map_io(&references.join("commands.md"), e))?;
    let outcome = add::add_skill(
        project_root,
        skill_root
            .to_str()
            .ok_or_else(|| Error::msg("manage-tink path is not UTF-8"))?,
        None,
    )?;
    Ok(outcome.name)
}
