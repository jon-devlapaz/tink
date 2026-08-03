//! Embedded `manage-tink` skill shipped with the Tink binary.

use std::path::Path;

use crate::add::{self, AddOutcome};
use crate::error::Error;
use crate::paths::map_io;

const SKILL_MD: &str = include_str!("../skills/manage-tink/SKILL.md");
const OPENAI_YAML: &str = include_str!("../skills/manage-tink/agents/openai.yaml");

/// Stage the embedded skill and install it into the project via `add`.
///
/// Does not write skill trees under `~/.tink` (catalog records names only).
pub fn install_manage_tink(project_root: &Path) -> Result<AddOutcome, Error> {
    let staging = tempfile::Builder::new()
        .prefix(".tink-manage-tink-")
        .tempdir()
        .map_err(|e| Error::msg(format!("manage-tink staging: {e}")))?;
    let skill_root = staging.path().join("manage-tink");
    let agents = skill_root.join("agents");
    std::fs::create_dir_all(&agents).map_err(|e| map_io(&agents, e))?;
    std::fs::write(skill_root.join("SKILL.md"), SKILL_MD)
        .map_err(|e| map_io(&skill_root.join("SKILL.md"), e))?;
    std::fs::write(agents.join("openai.yaml"), OPENAI_YAML)
        .map_err(|e| map_io(&agents.join("openai.yaml"), e))?;
    add::add_skill(
        project_root,
        skill_root
            .to_str()
            .ok_or_else(|| Error::msg("manage-tink path is not UTF-8"))?,
        None,
    )
}
