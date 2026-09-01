//! Skill library (`$TINK_HOME/skills/<name>/`).
//!
//! Rebuildable collection of skill trees from successful installs. Not an agent
//! discovery root — promote into a project with `tink skill add <name>`.

use std::fs;
use std::path::{Path, PathBuf};

use crate::error::Error;
use crate::home::{
    BY_PROJECT, ensure_inventory_root, existing_inventory_root, skills_library_path,
};
use crate::paths::{map_io, mkdir_p, refuse_symlink};
use crate::provenance::{self, Provenance};
use crate::skills::{self, PreflightOutcome, Skill};

/// Result of writing a skill into the library.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LibraryWrite {
    /// Library entry was missing; tree was created.
    Created,
    /// Library already matched the incoming tree (including receipt).
    Unchanged,
    /// Library diverged; replaced with the incoming tree.
    Repaired,
}

/// Result of a create-only library write (never repairs).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CreateOnlyWrite {
    /// Library entry was missing; tree was created.
    Created,
    /// Library already matched (exact, or body equal except receipt).
    Unchanged,
    /// Divergent or unreadable; no write. Optional reason for the caller.
    Skipped(Option<String>),
}

/// Result of explicitly publishing a project skill to the reusable library.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PromotionWrite {
    Created,
    Unchanged,
    Replaced,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PromotionOutcome {
    pub destination: PathBuf,
    pub digest: String,
    pub write: PromotionWrite,
}

const PROMOTION_IGNORED_ROOTS: &[&str] = &[".git", provenance::SIDECAR_FILE, ".tink-skillset.json"];

fn clear_path(target: &Path) -> Result<(), Error> {
    refuse_symlink(target)?;
    if target.is_dir() {
        fs::remove_dir_all(target).map_err(|e| map_io(target, e))?;
    } else if target.exists() {
        fs::remove_file(target).map_err(|e| map_io(target, e))?;
    }
    Ok(())
}

fn library_root(home: Option<&Path>) -> Result<PathBuf, Error> {
    let (home, _) = ensure_inventory_root(home)?;
    let root = skills_library_path(&home);
    mkdir_p(&root)?;
    Ok(root)
}

fn promotion_source(project_root: &Path, name: &str) -> Result<Skill, Error> {
    if !skills::valid_skill_name(name) {
        return Err(Error::msg(format!("Invalid skill name: {name}")));
    }
    let agents = crate::home::project_agents_path(project_root);
    let skills_root = crate::home::project_skills_path(project_root);
    refuse_symlink(&agents)?;
    refuse_symlink(&skills_root)?;
    if !skills_root.is_dir() {
        return Err(Error::msg("Missing .agents/skills"));
    }
    let path = skills_root.join(name);
    let skill = skills::read_skill(&path, true)?;
    crate::skillsets::ensure_standalone_source(&path, name)?;
    if path.join(".git").exists() || path.join(".git").is_symlink() {
        return Err(Error::msg(format!(
            "Refusing repository metadata in skill: {}",
            path.display()
        )));
    }
    skills::validate_skill_tree(&path)?;
    // Imported skills may be promoted, but their receipt must still be sound.
    // It is deliberately excluded from the reusable payload below.
    let _ = provenance::read(&skill)?;
    Ok(skill)
}

/// Publish a standalone project skill as a receipt-free library payload.
///
/// All source validation happens before the home is created or any staging
/// directory is made. A divergent destination is a conflict unless `replace`
/// is explicit.
pub fn promote(project_root: &Path, name: &str, replace: bool) -> Result<PromotionOutcome, Error> {
    promote_at(None, project_root, name, replace)
}

fn promote_at(
    home: Option<&Path>,
    project_root: &Path,
    name: &str,
    replace: bool,
) -> Result<PromotionOutcome, Error> {
    let skill = promotion_source(project_root, name)?;
    let digest = skills::tree_digest(&skill.path, PROMOTION_IGNORED_ROOTS)?;
    let library = library_root(home)?;
    let target = library.join(name);
    refuse_symlink(&target)?;

    if target.exists() {
        if !target.is_dir() {
            return Err(Error::msg(format!(
                "Refusing to replace non-directory library skill: {}",
                target.display()
            )));
        }
        crate::skillsets::ensure_standalone_source(&target, name)?;
        skills::read_skill(&target, true)?;
        skills::validate_skill_tree(&target)?;
        let destination_digest = skills::tree_digest(&target, PROMOTION_IGNORED_ROOTS)?;
        let has_receipt = target.join(provenance::SIDECAR_FILE).exists()
            || target.join(provenance::SIDECAR_FILE).is_symlink();
        if destination_digest == digest && !has_receipt {
            return Ok(PromotionOutcome {
                destination: target,
                digest,
                write: PromotionWrite::Unchanged,
            });
        }
        if !replace {
            return Err(Error::conflict(format!(
                "Library skill {name:?} differs from the project skill.\nSource digest: {digest}\nDestination digest: {destination_digest}\nRefusing to overwrite it; rerun with: tink skill promote {name} --replace"
            )));
        }
    }

    let staging = tempfile::Builder::new()
        .prefix(".tink-promote-")
        .tempdir_in(&library)
        .map_err(|e| Error::msg(format!("promotion staging dir: {e}")))?;
    let staged = staging.path().join(name);
    skills::copy_skill_tree(&skill.path, &staged, PROMOTION_IGNORED_ROOTS)?;
    skills::read_skill(&staged, true)?;
    skills::validate_skill_tree(&staged)?;
    if staged.join(provenance::SIDECAR_FILE).exists() || staged.join(".tink-skillset.json").exists()
    {
        return Err(Error::msg("Promotion staging retained a Tink receipt"));
    }
    if skills::tree_digest(&staged, PROMOTION_IGNORED_ROOTS)? != digest {
        return Err(Error::msg("Promotion staging digest does not match source"));
    }

    let write = if target.exists() {
        skills::publish_staged_tree(staging, staged, &target)?;
        PromotionWrite::Replaced
    } else {
        fs::rename(&staged, &target).map_err(|e| map_io(&target, e))?;
        PromotionWrite::Created
    };
    skills::read_skill(&target, true)?;
    skills::validate_skill_tree(&target)?;
    if target.join(provenance::SIDECAR_FILE).exists()
        || target.join(".tink-skillset.json").exists()
        || skills::tree_digest(&target, PROMOTION_IGNORED_ROOTS)? != digest
    {
        return Err(Error::msg(
            "Published library skill failed promotion verification",
        ));
    }
    Ok(PromotionOutcome {
        destination: target,
        digest,
        write,
    })
}

/// Validated standalone skill trees currently in the library.
/// Skips reserved, unreadable, and receipt-backed skillset roots.
fn iter_library_skills(library: &Path) -> Result<Vec<Skill>, Error> {
    if !library.exists() {
        return Ok(Vec::new());
    }
    refuse_symlink(library)?;
    if !library.is_dir() {
        return Err(Error::msg(format!(
            "Refusing to read non-directory library: {}",
            library.display()
        )));
    }

    let mut skills = Vec::new();
    for entry in fs::read_dir(library).map_err(|e| map_io(library, e))? {
        let entry = entry.map_err(|e| map_io(library, e))?;
        let path = entry.path();
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if name == BY_PROJECT || name.starts_with('.') || name == "README.md" {
            continue;
        }
        if path.is_symlink() || !path.is_dir() {
            continue;
        }
        if crate::skillsets::has_receipt_entry(&path) {
            continue;
        }
        if let Ok(skill) = skills::read_skill(&path, true) {
            skills::validate_skill_tree(&path)?;
            skills.push(skill);
        }
    }
    skills.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(skills)
}

/// Copy skill tree into `~/.tink/skills/<name>/`.
///
/// Identical → noop; missing → create; divergent → replace (caller should warn).
/// Project installs still refuse overwrites separately.
/// Receipt-backed skillset roots are never replaced by standalone skills.
pub(crate) fn deposit_at(
    home: Option<&Path>,
    skill: &Skill,
    provenance: Option<&Provenance>,
) -> Result<(PathBuf, LibraryWrite), Error> {
    crate::skillsets::ensure_standalone_source(&skill.path, &skill.name)?;
    let library = library_root(home)?;
    let target = library.join(&skill.name);
    if crate::skillsets::has_receipt_entry(&target) {
        return Err(Error::msg(format!(
            "Library entry is a skillset; refusing standalone skill collision: {}",
            skill.name
        )));
    }
    match skills::preflight_install(skill, &library, provenance)? {
        PreflightOutcome::Ready => {
            let (path, _) = skills::install_local(skill, &library, provenance)?;
            Ok((path, LibraryWrite::Created))
        }
        PreflightOutcome::Identical => Ok((library.join(&skill.name), LibraryWrite::Unchanged)),
        PreflightOutcome::ReceiptMismatch => {
            let (path, _) = skills::install_local(skill, &library, provenance)?;
            Ok((path, LibraryWrite::Repaired))
        }
        PreflightOutcome::Divergent => {
            clear_path(&library.join(&skill.name))?;
            let (path, _) = skills::install_local(skill, &library, provenance)?;
            Ok((path, LibraryWrite::Repaired))
        }
    }
}

/// Validate every existing library boundary a later [`deposit_at`] will touch.
/// Divergent ordinary standalone entries are acceptable because deposit repairs
/// them; symlinks, unsafe trees, and skillset ownership collisions are not.
pub(crate) fn preflight_deposit_at(
    home: Option<&Path>,
    skill: &Skill,
    provenance: Option<&Provenance>,
) -> Result<(), Error> {
    crate::skillsets::ensure_standalone_source(&skill.path, &skill.name)?;
    let library = library_root(home)?;
    let target = library.join(&skill.name);
    if crate::skillsets::has_receipt_entry(&target) {
        return Err(Error::msg(format!(
            "Library entry is a skillset; refusing standalone skill collision: {}",
            skill.name
        )));
    }
    let _ = skills::preflight_install(skill, &library, provenance)?;
    Ok(())
}

/// Copy skill tree into the library only when missing or identical.
///
/// Divergent trees are skipped (no repair). Unreadable/unsafe trees surface as
/// [`CreateOnlyWrite::Skipped`] with the error detail — same create-only
/// contract harvest used before this lived in `library`.
pub(crate) fn deposit_create_only_at(
    home: Option<&Path>,
    skill: &Skill,
) -> Result<(PathBuf, CreateOnlyWrite), Error> {
    if let Err(error) = crate::skillsets::ensure_standalone_source(&skill.path, &skill.name) {
        let home = match home {
            Some(home) => home.to_path_buf(),
            None => crate::home::resolve_home()?,
        };
        let target = skills_library_path(&home).join(&skill.name);
        return Ok((target, CreateOnlyWrite::Skipped(Some(error.to_string()))));
    }
    let library = library_root(home)?;
    let target = library.join(&skill.name);
    match skills::preflight_install(skill, &library, None) {
        Err(err) => Ok((target, CreateOnlyWrite::Skipped(Some(err.to_string())))),
        Ok(PreflightOutcome::Ready) => {
            let (path, _) = skills::install_local(skill, &library, None)?;
            Ok((path, CreateOnlyWrite::Created))
        }
        Ok(PreflightOutcome::Identical | PreflightOutcome::ReceiptMismatch) => {
            // Receipt-only drift: create-only never rewrites; body already matches.
            Ok((target, CreateOnlyWrite::Unchanged))
        }
        Ok(PreflightOutcome::Divergent) => Ok((
            target,
            CreateOnlyWrite::Skipped(Some("library differs; create-only".into())),
        )),
    }
}

/// When the library already holds the exact standalone tree we would install, return it.
/// Receipt-backed roots remain owned by the skillset lifecycle and are never cache hits.
pub(crate) fn matching_at(
    home: Option<&Path>,
    skill: &Skill,
    provenance: Option<&Provenance>,
) -> Result<Option<Skill>, Error> {
    crate::skillsets::ensure_standalone_source(&skill.path, &skill.name)?;
    let library = library_root(home)?;
    let target = library.join(&skill.name);
    if !target.is_dir() || crate::skillsets::has_receipt_entry(&target) {
        return Ok(None);
    }
    match skills::preflight_install(skill, &library, provenance)? {
        PreflightOutcome::Identical => Ok(Some(skills::read_skill(&target, true)?)),
        PreflightOutcome::Ready
        | PreflightOutcome::ReceiptMismatch
        | PreflightOutcome::Divergent => Ok(None),
    }
}

/// List standalone skill names present in the home library.
///
/// Creates nothing; empty when home or library root is missing. Receipt-backed
/// skillset roots are excluded.
pub fn list_names(home: Option<&Path>) -> Result<Vec<String>, Error> {
    let Some(home) = existing_inventory_root(home)? else {
        return Ok(Vec::new());
    };
    let library = skills_library_path(&home);
    Ok(iter_library_skills(&library)?
        .into_iter()
        .map(|skill| skill.name)
        .collect())
}

fn load_library_skill(library: &Path, name: &str) -> Result<Skill, Error> {
    let path = library.join(name);
    if path.is_symlink() {
        return Err(Error::msg(format!(
            "Refusing to follow symlink: {}",
            path.display()
        )));
    }
    if !path.is_dir() {
        return Err(Error::msg(format!("Library skill not found: {name}")));
    }
    if crate::skillsets::has_receipt_entry(&path) {
        return Err(Error::msg(format!(
            "Library entry is a skillset; use `tink skillset add {name}`"
        )));
    }
    skills::read_skill(&path, true)
}

/// Load one standalone skill from the library by directory name.
/// Receipt-backed roots remain owned by the skillset lifecycle.
pub(crate) fn load_at(home: Option<&Path>, name: &str) -> Result<Skill, Error> {
    let library = library_root(home)?;
    load_library_skill(&library, name)
}

/// Load one standalone library skill without creating home or library state.
pub(crate) fn load_existing_at(home: Option<&Path>, name: &str) -> Result<Skill, Error> {
    let Some(home) = existing_inventory_root(home)? else {
        return Err(Error::msg(format!("Library skill not found: {name}")));
    };
    let library = skills_library_path(&home);
    refuse_symlink(&library)?;
    if !library.exists() {
        return Err(Error::msg(format!("Library skill not found: {name}")));
    }
    if !library.is_dir() {
        return Err(Error::msg(format!(
            "Refusing to read non-directory library: {}",
            library.display()
        )));
    }
    load_library_skill(&library, name)
}

/// Find a library skill whose receipt matches this remote URL + revision tip.
pub(crate) fn for_remote_tip_at(
    home: Option<&Path>,
    source_url: &str,
    revision: &str,
    selected_name: Option<&str>,
) -> Result<Option<Skill>, Error> {
    let (home, _) = ensure_inventory_root(home)?;
    let library = skills_library_path(&home);
    if !library.is_dir() {
        return Ok(None);
    }

    let mut hits = Vec::new();
    for skill in iter_library_skills(&library)? {
        if let Some(want) = selected_name
            && skill.name != want
        {
            continue;
        }
        let Ok(Some(provenance)) = provenance::read(&skill) else {
            continue;
        };
        if provenance.get("source").map(String::as_str) == Some(source_url)
            && provenance.get("revision").map(String::as_str) == Some(revision)
        {
            hits.push(skill);
        }
    }
    match hits.len() {
        0 => Ok(None),
        1 => Ok(Some(hits.remove(0))),
        _ => {
            let commands = hits
                .iter()
                .map(|skill| format!("  tink skill add {source_url} --skill {}", skill.name))
                .collect::<Vec<_>>()
                .join("\n");
            Err(Error::msg(format!(
                "Library has multiple skills for this revision. Choose one:\n{commands}"
            )))
        }
    }
}

fn tracks_project(library_skill: &Path, project_skill: &Path) -> Result<bool, Error> {
    if skills::skill_contents_equal(library_skill, project_skill)? {
        return Ok(true);
    }
    // Allow a missing/different receipt when the skill body still matches.
    skills::skill_contents_equal_except(library_skill, project_skill, &[provenance::SIDECAR_FILE])
}

/// Before refreshing a project skill, ensure the library can accept `new`
/// (missing, already new, or still equal to the current project install).
pub(crate) fn preflight_refresh_at(
    home: Option<&Path>,
    project_installed: &Path,
    new_skill: &Skill,
    new_provenance: &Provenance,
) -> Result<(), Error> {
    crate::skillsets::ensure_standalone_source(&new_skill.path, &new_skill.name)?;
    let library = library_root(home)?;
    match skills::preflight_install(new_skill, &library, Some(new_provenance))? {
        PreflightOutcome::Ready
        | PreflightOutcome::Identical
        | PreflightOutcome::ReceiptMismatch => Ok(()),
        PreflightOutcome::Divergent => {
            let home_skill = library.join(&new_skill.name);
            if home_skill.is_dir() && tracks_project(&home_skill, project_installed)? {
                Ok(())
            } else {
                Err(Error::msg(format!(
                    "Refusing to refresh {}: library diverges",
                    new_skill.name
                )))
            }
        }
    }
}

/// Keep `$TINK_HOME/skills/<name>/` aligned with the installed project skill.
pub(crate) fn sync_from_installed_at(home: Option<&Path>, installed: &Skill) -> Result<(), Error> {
    deposit_at(home, installed, None).map(|_| ())
}

/// After a project refresh passed [`preflight_refresh_at`], write the new tree
/// into the library (create, noop, or repair — same rules as [`deposit_at`]).
pub(crate) fn deposit_refresh_at(
    home: Option<&Path>,
    new_skill: &Skill,
    new_provenance: &Provenance,
) -> Result<(), Error> {
    deposit_at(home, new_skill, Some(new_provenance)).map(|_| ())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    struct TempHome {
        home: PathBuf,
        root: PathBuf,
        _temp: TempDir,
    }

    impl TempHome {
        fn new() -> Self {
            let temp = TempDir::new().unwrap();
            let root = temp.path().to_path_buf();
            let home = root.join("tink-home");
            Self {
                home,
                root,
                _temp: temp,
            }
        }

        fn library_skill(&self, name: &str) -> PathBuf {
            skills_library_path(&self.home).join(name)
        }
    }

    fn write_skill(dir: &Path, name: &str, body: &str) -> Skill {
        fs::create_dir_all(dir).unwrap();
        let text = format!(
            "---\nname: {name}\ndescription: Unit fixture {name}.\n---\n\n# {name}\n\n{body}\n"
        );
        fs::write(dir.join("SKILL.md"), text).unwrap();
        skills::read_skill(dir, true).unwrap()
    }

    fn skill_md(path: &Path) -> String {
        fs::read_to_string(path.join("SKILL.md")).unwrap()
    }

    fn sample_provenance() -> Provenance {
        let mut provenance = Provenance::new();
        provenance.insert(
            "source".into(),
            "https://github.com/example/repo.git".into(),
        );
        provenance.insert(
            "revision".into(),
            "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa".into(),
        );
        provenance.insert("path".into(), "skills/demo-skill".into());
        provenance
    }

    #[test]
    fn create_only_missing_creates() {
        let home = TempHome::new();
        let src = home.root.join("src").join("demo-skill");
        let skill = write_skill(&src, "demo-skill", "fresh body");

        let (path, write) = deposit_create_only_at(Some(&home.home), &skill).unwrap();
        assert_eq!(write, CreateOnlyWrite::Created);
        assert_eq!(path, home.library_skill("demo-skill"));
        assert!(skill_md(&path).contains("fresh body"));
    }

    #[test]
    fn list_names_requires_owned_home_but_missing_home_is_empty() {
        let temp = TempDir::new().unwrap();
        let missing = temp.path().join("missing");
        assert!(list_names(Some(&missing)).unwrap().is_empty());

        let unmarked = temp.path().join("unmarked");
        fs::create_dir_all(unmarked.join("skills")).unwrap();
        assert!(
            list_names(Some(&unmarked))
                .unwrap_err()
                .to_string()
                .contains("Not a Tink home")
        );
    }

    #[test]
    fn load_existing_at_does_not_create_missing_home() {
        let temp = TempDir::new().unwrap();
        let missing = temp.path().join("missing");
        let err = load_existing_at(Some(&missing), "demo-skill").unwrap_err();
        assert!(
            err.to_string()
                .contains("Library skill not found: demo-skill"),
            "{err}"
        );
        assert!(!missing.exists());
    }

    #[test]
    fn list_names_skips_malformed_root_manifest() {
        let home = TempHome::new();
        ensure_inventory_root(Some(&home.home)).unwrap();
        let malformed = home.library_skill("broken-skill");
        fs::create_dir_all(&malformed).unwrap();
        fs::write(malformed.join("SKILL.md"), "not frontmatter\n").unwrap();

        assert!(list_names(Some(&home.home)).unwrap().is_empty());
    }

    #[cfg(unix)]
    #[test]
    fn list_names_refuses_valid_manifest_with_unsafe_nested_tree() {
        let home = TempHome::new();
        ensure_inventory_root(Some(&home.home)).unwrap();
        let unsafe_skill = home.library_skill("demo-skill");
        write_skill(&unsafe_skill, "demo-skill", "valid manifest, unsafe tree");
        std::os::unix::fs::symlink("/tmp", unsafe_skill.join("nested-link")).unwrap();

        let err = list_names(Some(&home.home)).unwrap_err();

        assert!(err.to_string().contains("symlink"), "{err}");
    }

    #[test]
    fn create_only_identical_unchanged() {
        let home = TempHome::new();
        let src = home.root.join("src").join("demo-skill");
        let skill = write_skill(&src, "demo-skill", "same body");

        assert_eq!(
            deposit_create_only_at(Some(&home.home), &skill).unwrap().1,
            CreateOnlyWrite::Created
        );
        let before = skill_md(&home.library_skill("demo-skill"));

        let (path, write) = deposit_create_only_at(Some(&home.home), &skill).unwrap();
        assert_eq!(write, CreateOnlyWrite::Unchanged);
        assert_eq!(skill_md(&path), before);
    }

    #[test]
    fn create_only_diverge_skips() {
        let home = TempHome::new();
        let first = home.root.join("first").join("demo-skill");
        let second = home.root.join("second").join("demo-skill");
        let original = write_skill(&first, "demo-skill", "original body");
        let incoming = write_skill(&second, "demo-skill", "incoming body");

        assert_eq!(
            deposit_create_only_at(Some(&home.home), &original)
                .unwrap()
                .1,
            CreateOnlyWrite::Created
        );
        let before = skill_md(&home.library_skill("demo-skill"));

        let (path, write) = deposit_create_only_at(Some(&home.home), &incoming).unwrap();
        assert!(
            matches!(write, CreateOnlyWrite::Skipped(Some(ref detail)) if detail.contains("create-only")),
            "{write:?}"
        );
        assert_eq!(skill_md(&path), before);
        assert!(before.contains("original body"));
    }

    #[test]
    fn deposit_diverge_repairs() {
        let home = TempHome::new();
        let first = home.root.join("first").join("demo-skill");
        let second = home.root.join("second").join("demo-skill");
        let original = write_skill(&first, "demo-skill", "original body");
        let incoming = write_skill(&second, "demo-skill", "incoming body");

        assert_eq!(
            deposit_at(Some(&home.home), &original, None).unwrap().1,
            LibraryWrite::Created
        );

        let (path, write) = deposit_at(Some(&home.home), &incoming, None).unwrap();
        assert_eq!(write, LibraryWrite::Repaired);
        assert!(skill_md(&path).contains("incoming body"));
        assert!(!skill_md(&path).contains("original body"));
    }

    #[test]
    fn create_only_receipt_only_diff_unchanged() {
        let home = TempHome::new();
        let src = home.root.join("src").join("demo-skill");
        let skill = write_skill(&src, "demo-skill", "shared body");
        let provenance = sample_provenance();

        assert_eq!(
            deposit_at(Some(&home.home), &skill, Some(&provenance))
                .unwrap()
                .1,
            LibraryWrite::Created
        );
        let library = home.library_skill("demo-skill");
        assert!(library.join(".tink-source.json").is_file());
        let before = skill_md(&library);

        // Incoming tree matches body but has no receipt — create-only must not rewrite.
        let (path, write) = deposit_create_only_at(Some(&home.home), &skill).unwrap();
        assert_eq!(write, CreateOnlyWrite::Unchanged);
        assert_eq!(skill_md(&path), before);
        assert!(path.join(".tink-source.json").is_file());
    }

    #[test]
    fn load_at_sees_only_the_given_home() {
        let home = TempHome::new();
        let other = TempHome::new();
        let src = home.root.join("src").join("demo-skill");
        let skill = write_skill(&src, "demo-skill", "isolated body");
        deposit_at(Some(&home.home), &skill, None).unwrap();

        let loaded = load_at(Some(&home.home), "demo-skill").unwrap();
        assert_eq!(loaded.name, "demo-skill");
        assert!(skill_md(&loaded.path).contains("isolated body"));

        let err = load_at(Some(&other.home), "demo-skill").unwrap_err();
        assert!(err.to_string().contains("Library skill not found"), "{err}");
    }
}
