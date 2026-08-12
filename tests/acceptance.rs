//! Acceptance rows from ACCEPTANCE.md. These must fail until each row is implemented.

use assert_cmd::Command;
use assert_cmd::cargo::cargo_bin_cmd;
use predicates::prelude::*;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command as StdCommand;
use tempfile::TempDir;

struct Workspace {
    _temp: TempDir,
    root: PathBuf,
    inventory: PathBuf,
}

impl Workspace {
    fn new() -> Self {
        let temp = TempDir::new().expect("tempdir");
        let root = temp.path().to_path_buf();
        let inventory = root.join("inventory");
        Self {
            _temp: temp,
            root,
            inventory,
        }
    }

    fn project(&self, name: &str) -> PathBuf {
        let path = self.root.join(name);
        fs::create_dir_all(&path).expect("project dir");
        path
    }

    fn cmd(&self, cwd: &Path) -> Command {
        let mut cmd = cargo_bin_cmd!("tink");
        cmd.current_dir(cwd);
        cmd.env("TINK_HOME", &self.inventory);
        cmd
    }

    fn initialize_inventory(&self) {
        let bootstrap = self.root.join("inventory-bootstrap");
        fs::create_dir_all(&bootstrap).expect("inventory bootstrap project");
        self.cmd(&bootstrap)
            .args(["init", "--no-zen", "--no-tink-skills", "--no-manage-tink"])
            .assert()
            .success();
    }

    fn skill_path(project: &Path, name: &str) -> PathBuf {
        project.join(".agents").join("skills").join(name)
    }

    fn catalog_meta(&self, project_name: &str) -> PathBuf {
        let project = self
            .root
            .join(project_name)
            .canonicalize()
            .expect("canonical project");
        let by_project = self.inventory.join("catalog").join("by-project");
        if let Ok(entries) = fs::read_dir(&by_project) {
            for entry in entries.flatten() {
                let meta = entry.path().join("meta.json");
                let Ok(raw) = fs::read_to_string(&meta) else {
                    continue;
                };
                let Ok(value) = serde_json::from_str::<serde_json::Value>(&raw) else {
                    continue;
                };
                let Some(root) = value.get("root").and_then(serde_json::Value::as_str) else {
                    continue;
                };
                if Path::new(root).canonicalize().ok().as_deref() == Some(project.as_path()) {
                    return meta;
                }
            }
        }
        by_project
            .join(format!(".missing-{project_name}"))
            .join("meta.json")
    }

    fn library_skill(&self, skill: &str) -> PathBuf {
        self.inventory.join("skills").join(skill)
    }

    fn library_skillset(&self, skillset: &str) -> PathBuf {
        self.inventory.join("skills").join(skillset)
    }

    fn skillset_meta(&self, name: &str) -> PathBuf {
        self.inventory
            .join("catalog")
            .join("by-skillset")
            .join(name)
            .join("meta.json")
    }

    fn assert_cataloged(&self, project_name: &str, skill: &str) {
        let meta = self.catalog_meta(project_name);
        let raw = fs::read_to_string(&meta)
            .unwrap_or_else(|_| panic!("missing catalog for {project_name}"));
        assert!(
            raw.contains(&format!("\"{skill}\"")),
            "expected {skill} in catalog: {raw}"
        );
        assert!(
            self.library_skill(skill).join("SKILL.md").is_file(),
            "expected library at skills/{skill}"
        );
        assert!(
            !meta
                .parent()
                .expect("catalog project directory")
                .join(skill)
                .exists(),
            "must not copy skill trees into catalog/by-project"
        );
    }
}

fn write_skill(path: &Path, name: &str, body: &str) {
    fs::create_dir_all(path).expect("skill dir");
    let text = format!(
        "---\nname: {name}\ndescription: Valid skill fixture named {name}.\n---\n\n# {name}\n\n{body}\n"
    );
    fs::write(path.join("SKILL.md"), text).expect("SKILL.md");
}

fn write_skillset_meta(
    path: &Path,
    source: &str,
    revision: &str,
    source_root: &str,
    members: &[&str],
) {
    fs::create_dir_all(path.parent().expect("skillset catalog parent")).expect("catalog dir");
    let meta = serde_json::json!({
        "source": source,
        "revision": revision,
        "sourceRoot": source_root,
        "members": members,
    });
    fs::write(
        path,
        format!("{}\n", serde_json::to_string_pretty(&meta).unwrap()),
    )
    .expect("skillset meta");
}

fn git(cwd: &Path, args: &[&str]) {
    let status = StdCommand::new("git")
        .args(args)
        .current_dir(cwd)
        .env("GIT_AUTHOR_NAME", "Tink Test")
        .env("GIT_AUTHOR_EMAIL", "tink@example.test")
        .env("GIT_COMMITTER_NAME", "Tink Test")
        .env("GIT_COMMITTER_EMAIL", "tink@example.test")
        .status()
        .expect("git");
    assert!(status.success(), "git {args:?} failed");
}

fn init_repo(path: &Path) {
    fs::create_dir_all(path).expect("repo");
    git(path, &["init", "-q"]);
}

fn commit_all(path: &Path, message: &str) -> String {
    git(path, &["add", "."]);
    git(path, &["commit", "-qm", message]);
    let output = StdCommand::new("git")
        .args(["rev-parse", "HEAD"])
        .current_dir(path)
        .output()
        .expect("rev-parse");
    assert!(output.status.success());
    String::from_utf8(output.stdout)
        .expect("utf8")
        .trim()
        .to_string()
}

fn github_redirect(local_repo: &Path, public_url: &str) -> Vec<(String, String)> {
    let file_url = format!(
        "file://{}",
        local_repo.canonicalize().expect("canon").display()
    );
    vec![
        ("GIT_CONFIG_COUNT".into(), "1".into()),
        (
            "GIT_CONFIG_KEY_0".into(),
            format!("url.{file_url}.insteadOf"),
        ),
        ("GIT_CONFIG_VALUE_0".into(), public_url.into()),
        ("GIT_TERMINAL_PROMPT".into(), "0".into()),
    ]
}

// --- I*: init ---

#[test]
fn i1_init_creates_agents_skills() {
    let ws = Workspace::new();
    let project = ws.project("app");
    ws.cmd(&project).arg("init").assert().success();
    let skills = project.join(".agents").join("skills");
    assert!(skills.is_dir());
    assert!(!project.join(".agents").is_symlink());
    assert!(!skills.is_symlink());
}

#[test]
fn i2_init_refuses_agents_symlink() {
    let ws = Workspace::new();
    let project = ws.project("app");
    let real = ws.root.join("real-agents");
    fs::create_dir_all(&real).unwrap();
    std::os::unix::fs::symlink(&real, project.join(".agents")).unwrap();
    ws.cmd(&project)
        .arg("init")
        .assert()
        .failure()
        .stderr(predicate::str::contains("symlink").or(predicate::str::contains("Symlink")));
}

#[test]
fn i3_init_does_not_write_product_bundles() {
    let ws = Workspace::new();
    let project = ws.project("app");
    ws.cmd(&project)
        .args(["init", "--no-zen", "--no-tink-skills"])
        .assert()
        .success();
    assert!(!project.join("AGENTS.md").exists());
    assert!(!project.join("ZEN.md").exists());
    assert!(!project.join(".github").exists());
}

#[test]
fn i4_init_ensures_inventory_root() {
    let ws = Workspace::new();
    let project = ws.project("app");
    ws.cmd(&project).arg("init").assert().success();
    assert!(ws.inventory.is_dir());
    assert!(ws.inventory.join("layout.json").is_file());
    let layout = fs::read_to_string(ws.inventory.join("layout.json")).unwrap();
    assert!(layout.contains("tink-skill-inventory"));
    assert!(ws.inventory.join("catalog").join("by-project").is_dir());
    assert!(ws.inventory.join("skills").is_dir());
    assert!(!ws.inventory.join("skills").join("by-project").exists());
}

#[test]
fn i5_init_with_zen_writes_agents_reference() {
    let ws = Workspace::new();
    let project = ws.project("app");
    ws.cmd(&project)
        .args(["init", "--with-zen", "--no-tink-skills"])
        .assert()
        .success();
    assert!(project.join("ZEN.md").is_file());
    let agents = fs::read_to_string(project.join("AGENTS.md")).unwrap();
    assert!(agents.contains("[ZEN.md](ZEN.md)"));
    ws.cmd(&project).args(["skill", "check"]).assert().success();
}

#[test]
fn i6_init_installs_manage_tink_and_catalogs_name() {
    let ws = Workspace::new();
    let project = ws.project("app");
    ws.cmd(&project).arg("init").assert().success();
    let skill = Workspace::skill_path(&project, "manage-tink");
    assert!(skill.join("SKILL.md").is_file());
    assert!(skill.join("references").join("commands.md").is_file());
    ws.assert_cataloged("app", "manage-tink");
}

#[test]
fn i7_init_no_manage_tink_skips_embedded_skill() {
    let ws = Workspace::new();
    let project = ws.project("app");
    ws.cmd(&project)
        .args(["init", "--no-manage-tink"])
        .assert()
        .success();
    assert!(!Workspace::skill_path(&project, "manage-tink").exists());
}

#[test]
fn i8_relative_tink_home_resolves_absolute_not_nested() {
    // I8: relative TINK_HOME is absolutized against cwd; must not nest under project.
    let root = tempfile::TempDir::new().unwrap();
    let root = root
        .path()
        .canonicalize()
        .unwrap_or_else(|_| root.path().to_path_buf());
    let project = root.join("app");
    fs::create_dir_all(&project).unwrap();
    let expected_home = root.join("tink-home");

    Command::cargo_bin("tink")
        .unwrap()
        .current_dir(&project)
        .env("TINK_HOME", "../tink-home")
        .env("HOME", root.join("unused-unix-home"))
        .args(["init", "--no-zen", "--no-tink-skills", "--no-manage-tink"])
        .assert()
        .success()
        .stdout(predicate::str::contains(
            expected_home.display().to_string(),
        ));

    assert!(
        expected_home.join("layout.json").is_file(),
        "home should live at absolutized sibling"
    );
    assert!(
        !project.join("tink-home").exists(),
        "home must not nest under project cwd"
    );
}

#[test]
fn i9_init_rerun_is_idempotent() {
    let ws = Workspace::new();
    let project = ws.project("app");
    let args = ["init", "--no-zen", "--no-tink-skills"];

    ws.cmd(&project).args(args).assert().success();

    let contract_files = [
        project.join(".agents").join("skills").join("README.md"),
        Workspace::skill_path(&project, "manage-tink").join("SKILL.md"),
        ws.catalog_meta("app"),
        ws.library_skill("manage-tink").join("SKILL.md"),
        ws.inventory.join("layout.json"),
    ];
    let before: Vec<Vec<u8>> = contract_files
        .iter()
        .map(|path| fs::read(path).unwrap_or_else(|_| panic!("missing {}", path.display())))
        .collect();

    ws.cmd(&project).args(args).assert().success().stdout(
        predicate::str::contains("Ready")
            .and(predicate::str::contains("Already present manage-tink")),
    );

    let after: Vec<Vec<u8>> = contract_files
        .iter()
        .map(|path| fs::read(path).unwrap_or_else(|_| panic!("missing {}", path.display())))
        .collect();
    assert_eq!(after, before, "re-running init changed contract files");
}

#[test]
fn i10_init_bundle_failure_is_resumable() {
    let ws = Workspace::new();
    let project = ws.project("app");
    let remote = ws.root.join("tink-skills");
    init_repo(&remote);
    write_skill(
        &remote.join("skills").join("skill-scout"),
        "skill-scout",
        "Scout for skills.",
    );
    commit_all(&remote, "add skill-scout");

    let redirect = github_redirect(&remote, "https://github.com/jon-devlapaz/tink-skills.git");
    let args = ["init", "--no-zen", "--with-tink-skills"];

    let mut first = ws.cmd(&project);
    first.args(args);
    for (key, value) in &redirect {
        first.env(key, value);
    }
    first
        .assert()
        .failure()
        .stderr(predicate::str::contains("skill-eval-loop"));

    ws.assert_cataloged("app", "manage-tink");
    ws.assert_cataloged("app", "skill-scout");
    assert!(!Workspace::skill_path(&project, "skill-eval-loop").exists());
    assert!(!ws.library_skill("skill-eval-loop").exists());
    ws.cmd(&project).args(["skill", "check"]).assert().success();
    let manage_before =
        fs::read(Workspace::skill_path(&project, "manage-tink").join("SKILL.md")).unwrap();
    let scout_before =
        fs::read(Workspace::skill_path(&project, "skill-scout").join("SKILL.md")).unwrap();

    write_skill(
        &remote.join("skills").join("skill-eval-loop"),
        "skill-eval-loop",
        "Evaluate skills.",
    );
    commit_all(&remote, "add skill-eval-loop");

    let mut second = ws.cmd(&project);
    second.args(args);
    for (key, value) in &redirect {
        second.env(key, value);
    }
    second
        .assert()
        .success()
        .stdout(predicate::str::contains("Added skill-eval-loop"));

    ws.assert_cataloged("app", "manage-tink");
    ws.assert_cataloged("app", "skill-scout");
    ws.assert_cataloged("app", "skill-eval-loop");
    assert_eq!(
        fs::read(Workspace::skill_path(&project, "manage-tink").join("SKILL.md")).unwrap(),
        manage_before,
        "resuming init changed manage-tink"
    );
    assert_eq!(
        fs::read(Workspace::skill_path(&project, "skill-scout").join("SKILL.md")).unwrap(),
        scout_before,
        "resuming init changed skill-scout"
    );
}

#[test]
fn i11_init_refuses_unrelated_existing_tink_home_without_writes() {
    let root = TempDir::new().unwrap();
    let project = root.path().join("project");
    fs::create_dir_all(&project).unwrap();
    let readme = project.join("README.md");
    fs::write(&readme, "# Important project\n\nKeep this content.\n").unwrap();
    let before = fs::read(&readme).unwrap();

    Command::cargo_bin("tink")
        .unwrap()
        .current_dir(&project)
        .env("TINK_HOME", ".")
        .args(["init", "--no-zen", "--no-tink-skills", "--no-manage-tink"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("Tink home").and(predicate::str::contains("non-empty")));

    assert_eq!(fs::read(&readme).unwrap(), before);
    assert_eq!(fs::read_dir(&project).unwrap().count(), 1);
    assert!(!project.join(".agents").exists());
}

#[test]
fn i12_init_resumes_marker_only_partial_inventory() {
    let ws = Workspace::new();
    let project = ws.project("app");
    fs::create_dir_all(&ws.inventory).unwrap();
    fs::write(
        ws.inventory.join("layout.json"),
        "{\n  \"kind\": \"tink-skill-inventory\"\n}\n",
    )
    .unwrap();

    ws.cmd(&project)
        .args(["init", "--no-zen", "--no-tink-skills", "--no-manage-tink"])
        .assert()
        .success();

    assert!(ws.inventory.join("skills").is_dir());
    assert!(ws.inventory.join("catalog").join("by-project").is_dir());
    assert!(ws.inventory.join("catalog").join("by-skillset").is_dir());
    assert!(project.join(".agents").join("skills").is_dir());
}

// --- A*: local add ---

#[test]
fn a1_add_local_skill_installs() {
    let ws = Workspace::new();
    let project = ws.project("app");
    ws.cmd(&project).arg("init").assert().success();
    let source = ws.root.join("demo-skill");
    write_skill(&source, "demo-skill", "Do the work.");
    ws.cmd(&project)
        .args(["skill", "add", source.to_str().unwrap()])
        .assert()
        .success();
    let installed = Workspace::skill_path(&project, "demo-skill");
    assert!(installed.join("SKILL.md").is_file());
    ws.assert_cataloged("app", "demo-skill");
}

#[test]
fn a2_add_identical_is_noop() {
    let ws = Workspace::new();
    let project = ws.project("app");
    ws.cmd(&project).arg("init").assert().success();
    let source = ws.root.join("demo-skill");
    write_skill(&source, "demo-skill", "Do the work.");
    ws.cmd(&project)
        .args(["skill", "add", source.to_str().unwrap()])
        .assert()
        .success();
    let first = fs::read(Workspace::skill_path(&project, "demo-skill").join("SKILL.md")).unwrap();
    ws.cmd(&project)
        .args(["skill", "add", source.to_str().unwrap()])
        .assert()
        .success();
    let second = fs::read(Workspace::skill_path(&project, "demo-skill").join("SKILL.md")).unwrap();
    assert_eq!(first, second);
}

#[test]
fn a3_add_refuses_overwrite_when_diverged() {
    let ws = Workspace::new();
    let project = ws.project("app");
    ws.cmd(&project).arg("init").assert().success();
    let source = ws.root.join("demo-skill");
    write_skill(&source, "demo-skill", "original");
    ws.cmd(&project)
        .args(["skill", "add", source.to_str().unwrap()])
        .assert()
        .success();
    write_skill(&source, "demo-skill", "changed");
    ws.cmd(&project)
        .args(["skill", "add", source.to_str().unwrap()])
        .assert()
        .failure()
        .stderr(predicate::str::contains("Refusing to overwrite"));
    let body =
        fs::read_to_string(Workspace::skill_path(&project, "demo-skill").join("SKILL.md")).unwrap();
    assert!(body.contains("original"));
    assert!(!body.contains("changed"));
}

#[test]
fn a3b_add_local_repair_sidecar_only_divergence() {
    let ws = Workspace::new();
    let project = ws.project("app");
    ws.cmd(&project).arg("init").assert().success();
    let source = ws.root.join("demo-skill");
    write_skill(&source, "demo-skill", "content");
    ws.cmd(&project)
        .args(["skill", "add", source.to_str().unwrap()])
        .assert()
        .success();

    let installed = Workspace::skill_path(&project, "demo-skill");
    fs::write(installed.join(".tink-source.json"), "{\"bad\": true}\n").expect("stale sidecar");

    ws.cmd(&project)
        .args(["skill", "add", source.to_str().unwrap()])
        .assert()
        .success();
    assert!(!installed.join(".tink-source.json").exists());
}

#[test]
fn a4_add_refuses_symlink_in_skill_tree() {
    let ws = Workspace::new();
    let project = ws.project("app");
    ws.cmd(&project).arg("init").assert().success();
    let source = ws.root.join("linked-skill");
    write_skill(&source, "linked-skill", "body");
    std::os::unix::fs::symlink(source.join("SKILL.md"), source.join("link")).unwrap();
    ws.cmd(&project)
        .args(["skill", "add", source.to_str().unwrap()])
        .assert()
        .failure()
        .stderr(predicate::str::contains("symlink").or(predicate::str::contains("Symlink")));
}

#[test]
fn a5_add_multi_skill_requires_skill_flag() {
    let ws = Workspace::new();
    let project = ws.project("app");
    ws.cmd(&project).arg("init").assert().success();
    let repo = ws.root.join("bundle");
    write_skill(&repo.join("skills").join("alpha"), "alpha", "a");
    write_skill(&repo.join("skills").join("beta"), "beta", "b");
    ws.cmd(&project)
        .args(["skill", "add", repo.to_str().unwrap()])
        .assert()
        .failure()
        .stderr(
            predicate::str::contains("alpha")
                .and(predicate::str::contains("beta"))
                .and(predicate::str::contains("--skill").or(predicate::str::contains("skill"))),
        );
}

#[test]
fn a7_add_refuses_reserved_by_project_name() {
    let ws = Workspace::new();
    let project = ws.project("app");
    ws.cmd(&project)
        .args(["init", "--no-zen", "--no-tink-skills", "--no-manage-tink"])
        .assert()
        .success();
    let source = ws.root.join("by-project");
    write_skill(&source, "by-project", "reserved path");
    ws.cmd(&project)
        .args(["skill", "add", source.to_str().unwrap()])
        .assert()
        .failure()
        .stderr(
            predicate::str::contains("Invalid skill name")
                .or(predicate::str::contains("by-project")),
        );
    assert!(!Workspace::skill_path(&project, "by-project").exists());
    assert!(!ws.library_skill("by-project").exists());
}

#[test]
fn a6_add_repairs_divergent_library_and_installs_project() {
    let ws = Workspace::new();
    let app = ws.project("app");
    let other = ws.project("other");
    ws.cmd(&app)
        .args(["init", "--no-zen", "--no-tink-skills", "--no-manage-tink"])
        .assert()
        .success();
    ws.cmd(&other)
        .args(["init", "--no-zen", "--no-tink-skills", "--no-manage-tink"])
        .assert()
        .success();

    let first = ws.root.join("demo-a");
    let second = ws.root.join("demo-b");
    write_skill(&first, "demo-skill", "from app");
    write_skill(&second, "demo-skill", "from other");

    ws.cmd(&app)
        .args(["skill", "add", first.to_str().unwrap()])
        .assert()
        .success();
    assert!(ws.library_skill("demo-skill").join("SKILL.md").is_file());

    ws.cmd(&other)
        .args(["skill", "add", second.to_str().unwrap()])
        .assert()
        .success()
        .stderr(predicate::str::contains("Updated home copy of"));
    assert!(
        Workspace::skill_path(&other, "demo-skill")
            .join("SKILL.md")
            .is_file()
    );
    let project =
        fs::read_to_string(Workspace::skill_path(&other, "demo-skill").join("SKILL.md")).unwrap();
    assert!(project.contains("from other"));
    let archived = fs::read_to_string(ws.library_skill("demo-skill").join("SKILL.md")).unwrap();
    assert!(archived.contains("from other"));
    assert!(!archived.contains("from app"));
}

#[cfg(unix)]
#[test]
fn a6b_closed_warning_pipe_does_not_interrupt_completed_add() {
    use std::os::fd::OwnedFd;
    use std::os::unix::net::UnixStream;
    use std::process::Stdio;

    let ws = Workspace::new();
    let app = ws.project("app");
    let other = ws.project("other");
    for project in [&app, &other] {
        ws.cmd(project)
            .args(["init", "--no-zen", "--no-tink-skills", "--no-manage-tink"])
            .assert()
            .success();
    }
    let first = ws.root.join("demo-a");
    let second = ws.root.join("demo-b");
    write_skill(&first, "demo-skill", "from app");
    write_skill(&second, "demo-skill", "from other");
    ws.cmd(&app)
        .args(["skill", "add", first.to_str().unwrap()])
        .assert()
        .success();

    let (read_end, write_end) = UnixStream::pair().expect("stderr pipe");
    drop(read_end);
    let write_end: OwnedFd = write_end.into();
    let status = StdCommand::new(assert_cmd::cargo::cargo_bin!("tink"))
        .current_dir(&other)
        .env("TINK_HOME", &ws.inventory)
        .args(["skill", "add", second.to_str().unwrap()])
        .stdout(Stdio::null())
        .stderr(Stdio::from(write_end))
        .status()
        .expect("add with closed warning pipe");

    assert_eq!(status.code(), Some(0));
    let project =
        fs::read_to_string(Workspace::skill_path(&other, "demo-skill").join("SKILL.md")).unwrap();
    let archived = fs::read_to_string(ws.library_skill("demo-skill").join("SKILL.md")).unwrap();
    assert!(project.contains("from other"));
    assert!(archived.contains("from other"));
}

#[test]
fn a8_add_uses_library_when_remote_tip_matches() {
    let ws = Workspace::new();
    let app = ws.project("app");
    let other = ws.project("other");
    ws.cmd(&app)
        .args(["init", "--no-zen", "--no-tink-skills", "--no-manage-tink"])
        .assert()
        .success();
    ws.cmd(&other)
        .args(["init", "--no-zen", "--no-tink-skills", "--no-manage-tink"])
        .assert()
        .success();

    let remote = ws.root.join("root-skill-repo");
    init_repo(&remote);
    write_skill(&remote, "root-skill", "cached");
    commit_all(&remote, "v1");
    let public = "https://github.com/example/root-skill.git";
    let redirect = github_redirect(&remote, public);

    let mut first = ws.cmd(&app);
    first.args(["skill", "add", "example/root-skill"]);
    for (k, v) in &redirect {
        first.env(k, v);
    }
    first.assert().success();

    let mut second = ws.cmd(&other);
    second.args(["skill", "add", "example/root-skill"]);
    for (k, v) in &redirect {
        second.env(k, v);
    }
    second
        .assert()
        .success()
        .stdout(predicate::str::contains("from library"));
    assert!(
        Workspace::skill_path(&other, "root-skill")
            .join(".tink-source.json")
            .is_file()
    );
    let receipt =
        fs::read_to_string(Workspace::skill_path(&other, "root-skill").join(".tink-source.json"))
            .unwrap();
    assert!(
        receipt.contains("\"path\": \".\"") || receipt.contains("\"path\":\".\""),
        "{receipt}"
    );
}

#[test]
fn a13_add_uses_library_for_non_root_skill_without_cloning() {
    let ws = Workspace::new();
    let first_project = ws.project("first");
    let second_project = ws.project("second");
    for project in [&first_project, &second_project] {
        ws.cmd(project)
            .args(["init", "--no-zen", "--no-tink-skills", "--no-manage-tink"])
            .assert()
            .success();
    }

    let remote = ws.root.join("non-root-cache-repo");
    init_repo(&remote);
    write_skill(
        &remote.join("skills/non-root-skill"),
        "non-root-skill",
        "cached non-root skill",
    );
    commit_all(&remote, "non-root skill");
    let public = "https://github.com/example/non-root-cache.git";
    let redirect = github_redirect(&remote, public);

    let mut first_add = ws.cmd(&first_project);
    first_add.args(["skill", "add", "example/non-root-cache"]);
    first_add.envs(redirect.clone());
    first_add.assert().success();

    let tree_output = StdCommand::new("git")
        .args(["rev-parse", "HEAD^{tree}"])
        .current_dir(&remote)
        .output()
        .unwrap();
    assert!(tree_output.status.success());
    let tree = String::from_utf8(tree_output.stdout).unwrap();
    let tree = tree.trim();
    let tree_object = remote
        .join(".git/objects")
        .join(&tree[..2])
        .join(&tree[2..]);
    assert!(tree_object.is_file(), "missing loose tree object {tree}");
    fs::remove_file(tree_object).unwrap();
    let mut second_add = ws.cmd(&second_project);
    second_add.args(["skill", "add", "example/non-root-cache"]);
    second_add.envs(redirect);
    second_add
        .assert()
        .success()
        .stdout(predicate::str::contains("from library"));
    assert!(
        Workspace::skill_path(&second_project, "non-root-skill")
            .join("SKILL.md")
            .is_file()
    );
}

#[test]
fn a9_add_catalog_failure_is_resumable() {
    let ws = Workspace::new();
    let project = ws.project("app");
    ws.cmd(&project)
        .args(["init", "--no-zen", "--no-tink-skills"])
        .assert()
        .success();

    let source = ws.root.join("demo-skill");
    write_skill(&source, "demo-skill", "Do the work.");
    let args = ["skill", "add", source.to_str().unwrap()];
    let catalog = ws.catalog_meta("app");
    let catalog_before = fs::read(&catalog).unwrap();
    let malformed_catalog = b"{not-json}\n";
    fs::write(&catalog, malformed_catalog).unwrap();

    ws.cmd(&project)
        .args(args)
        .assert()
        .failure()
        .stderr(predicate::str::contains("Invalid catalog meta"));

    let installed_root = Workspace::skill_path(&project, "demo-skill");
    let library_root = ws.library_skill("demo-skill");
    let installed = installed_root.join("SKILL.md");
    let library = library_root.join("SKILL.md");
    assert!(installed.is_file());
    assert!(library.is_file());
    assert_eq!(fs::read(&catalog).unwrap(), malformed_catalog);
    ws.cmd(&project).args(["skill", "check"]).assert().success();
    let source_before = fs::read(source.join("SKILL.md")).unwrap();
    let installed_before = fs::read(&installed).unwrap();
    let library_before = fs::read(&library).unwrap();
    assert_eq!(installed_before, source_before);
    assert_eq!(library_before, source_before);
    assert_eq!(fs::read_dir(&installed_root).unwrap().count(), 1);
    assert_eq!(fs::read_dir(&library_root).unwrap().count(), 1);

    fs::write(&catalog, catalog_before).unwrap();
    ws.cmd(&project)
        .args(args)
        .assert()
        .success()
        .stdout(predicate::str::contains("Unchanged demo-skill"));

    ws.assert_cataloged("app", "manage-tink");
    ws.assert_cataloged("app", "demo-skill");
    let meta: serde_json::Value =
        serde_json::from_slice(&fs::read(&catalog).unwrap()).expect("valid catalog meta");
    let skills = meta["skills"].as_array().expect("catalog skills");
    assert_eq!(skills.len(), 2);
    assert!(skills.contains(&serde_json::json!("demo-skill")));
    assert!(skills.contains(&serde_json::json!("manage-tink")));
    assert_eq!(fs::read(installed).unwrap(), installed_before);
    assert_eq!(fs::read(library).unwrap(), library_before);
    assert_eq!(fs::read_dir(installed_root).unwrap().count(), 1);
    assert_eq!(fs::read_dir(library_root).unwrap().count(), 1);
    ws.cmd(&project).args(["skill", "check"]).assert().success();
}

#[test]
fn a10_add_refuses_symlinked_skill_roots() {
    let ws = Workspace::new();
    let project = ws.project("app");
    ws.cmd(&project)
        .args(["init", "--no-zen", "--no-tink-skills", "--no-manage-tink"])
        .assert()
        .success();

    let outside = ws.root.join("outside");
    write_skill(&outside, "linked-skill", "Outside the declared tree.");
    let bundle = ws.root.join("bundle");
    fs::create_dir_all(bundle.join("skills")).unwrap();
    std::os::unix::fs::symlink(&outside, bundle.join("skills").join("linked-skill")).unwrap();

    ws.cmd(&project)
        .args([
            "skill",
            "add",
            bundle.to_str().unwrap(),
            "--skill",
            "linked-skill",
        ])
        .assert()
        .failure()
        .stderr(predicate::str::contains("symlink"));
    assert!(!Workspace::skill_path(&project, "linked-skill").exists());
    assert!(!ws.library_skill("linked-skill").exists());

    let direct = ws.root.join("direct-skill");
    write_skill(&direct, "direct-skill", "Direct source target.");
    let direct_link = ws.root.join("direct-link");
    std::os::unix::fs::symlink(&direct, &direct_link).unwrap();
    ws.cmd(&project)
        .args(["skill", "add", direct_link.to_str().unwrap()])
        .assert()
        .failure()
        .stderr(predicate::str::contains("symlink"));
    assert!(!Workspace::skill_path(&project, "direct-skill").exists());
    assert!(!ws.library_skill("direct-skill").exists());
    assert!(!ws.catalog_meta("app").exists());
}

#[test]
fn a11_add_refuses_matching_project_target_symlink() {
    let ws = Workspace::new();
    let project = ws.project("app");
    ws.cmd(&project)
        .args(["init", "--no-zen", "--no-tink-skills", "--no-manage-tink"])
        .assert()
        .success();

    let source = ws.root.join("source");
    let external = ws.root.join("external");
    write_skill(&source, "demo-skill", "Same bytes.");
    write_skill(&external, "demo-skill", "Same bytes.");
    let target = Workspace::skill_path(&project, "demo-skill");
    std::os::unix::fs::symlink(&external, &target).unwrap();

    ws.cmd(&project)
        .args(["skill", "add", source.to_str().unwrap()])
        .assert()
        .failure()
        .stderr(predicate::str::contains("symlink"));
    assert!(target.is_symlink());
    assert!(!ws.library_skill("demo-skill").exists());
    assert!(!ws.catalog_meta("app").exists());
}

#[test]
fn a12_add_refuses_unrelated_existing_tink_home() {
    let root = TempDir::new().unwrap();
    let project = root.path().join("project");
    let source = root.path().join("source");
    fs::create_dir_all(&project).unwrap();
    write_skill(&source, "demo-skill", "Safe source.");
    let readme = project.join("README.md");
    fs::write(&readme, "# Important project\n\nKeep this content.\n").unwrap();
    let before = fs::read(&readme).unwrap();

    Command::cargo_bin("tink")
        .unwrap()
        .current_dir(&project)
        .env("TINK_HOME", ".")
        .args(["skill", "add", source.to_str().unwrap()])
        .assert()
        .failure()
        .stderr(predicate::str::contains("Tink home").and(predicate::str::contains("non-empty")));

    assert_eq!(fs::read(&readme).unwrap(), before);
    assert!(!project.join(".agents").exists());
    assert!(!project.join("layout.json").exists());
    assert!(!project.join("catalog").exists());
    assert!(!project.join("skills").exists());
}

#[cfg(unix)]
#[test]
fn a14_add_preserves_executable_permissions_in_project_and_library() {
    use std::os::unix::fs::PermissionsExt;

    let ws = Workspace::new();
    let project = ws.project("app");
    ws.cmd(&project)
        .args(["init", "--no-zen", "--no-tink-skills", "--no-manage-tink"])
        .assert()
        .success();
    let source = ws.root.join("executable-skill");
    write_skill(&source, "executable-skill", "mode fixture");
    let script = source.join("run.sh");
    fs::write(&script, "#!/bin/sh\nexit 0\n").unwrap();
    fs::set_permissions(&script, fs::Permissions::from_mode(0o755)).unwrap();

    ws.cmd(&project)
        .args(["skill", "add", source.to_str().unwrap()])
        .assert()
        .success();

    for copied in [
        Workspace::skill_path(&project, "executable-skill").join("run.sh"),
        ws.library_skill("executable-skill").join("run.sh"),
    ] {
        assert_eq!(
            fs::metadata(copied).unwrap().permissions().mode() & 0o777,
            0o755
        );
    }
}

#[cfg(unix)]
#[test]
fn a15_add_preserves_distinct_non_utf8_unix_filenames() {
    use std::ffi::OsString;
    use std::os::unix::ffi::OsStringExt;

    let ws = Workspace::new();
    let project = ws.project("app");
    ws.cmd(&project)
        .args(["init", "--no-zen", "--no-tink-skills", "--no-manage-tink"])
        .assert()
        .success();
    let source = ws.root.join("opaque-name-skill");
    write_skill(&source, "opaque-name-skill", "raw filename fixture");
    let names = [
        OsString::from_vec(vec![0x80]),
        OsString::from_vec(vec![0x81]),
    ];
    for (name, body) in names
        .iter()
        .zip([b"first".as_slice(), b"second".as_slice()])
    {
        if let Err(error) = fs::write(source.join(name), body) {
            #[cfg(target_os = "macos")]
            {
                assert_eq!(error.raw_os_error(), Some(92), "unexpected fixture error");
                assert!(!Workspace::skill_path(&project, "opaque-name-skill").exists());
                return;
            }
            #[cfg(not(target_os = "macos"))]
            panic!("filesystem rejected non-UTF-8 fixture: {error}");
        }
    }

    ws.cmd(&project)
        .args(["skill", "add", source.to_str().unwrap()])
        .assert()
        .success();

    for root in [
        Workspace::skill_path(&project, "opaque-name-skill"),
        ws.library_skill("opaque-name-skill"),
    ] {
        assert_eq!(fs::read(root.join(&names[0])).unwrap(), b"first");
        assert_eq!(fs::read(root.join(&names[1])).unwrap(), b"second");
    }
}

#[cfg(unix)]
#[test]
fn a16_standalone_add_refuses_receipt_owned_sources_before_any_mutation() {
    for dangling_receipt in [false, true] {
        let ws = Workspace::new();
        let project = ws.project("app");
        let source = ws.root.join("common-skillset");
        write_skill(
            &source,
            "common-skillset",
            "receipt ownership boundary fixture",
        );
        let receipt = source.join(".tink-skillset.json");
        if dangling_receipt {
            std::os::unix::fs::symlink(source.join("missing-receipt"), &receipt).unwrap();
        } else {
            fs::write(&receipt, "{}\n").unwrap();
        }

        ws.cmd(&project)
            .args(["skill", "add", source.to_str().unwrap()])
            .assert()
            .failure()
            .stdout(predicate::str::is_empty())
            .stderr(predicate::str::contains(
                "tink skillset add common-skillset",
            ));

        assert!(!project.join(".agents").exists());
        assert!(!ws.inventory.exists());
    }
}

// --- K*: skillsets ---

#[test]
fn k1_skillset_add_installs_explicit_members_and_checks_digest() {
    let ws = Workspace::new();
    let project = ws.project("app");
    ws.cmd(&project)
        .args(["init", "--no-zen", "--no-tink-skills", "--no-manage-tink"])
        .assert()
        .success();

    let repository = ws.root.join("skillset-repo");
    init_repo(&repository);
    write_skill(
        &repository.join("bundles/common/alpha"),
        "alpha",
        "first member",
    );
    write_skill(
        &repository.join("bundles/common/beta"),
        "beta",
        "second member",
    );
    let revision = commit_all(&repository, "skillset");
    let source = "https://github.com/example/skillsets.git";
    write_skillset_meta(
        &ws.skillset_meta("common-skillset"),
        source,
        &revision,
        "bundles/common",
        &["alpha", "beta"],
    );

    let redirect = github_redirect(&repository, source);
    ws.cmd(&project)
        .args(["skillset", "add", "common-skillset"])
        .envs(redirect.clone())
        .assert()
        .success();

    let installed = Workspace::skill_path(&project, "common-skillset");
    let library = ws.library_skillset("common-skillset");
    assert!(installed.join("alpha/SKILL.md").is_file());
    assert!(installed.join("beta/SKILL.md").is_file());
    assert!(installed.join(".tink-skillset.json").is_file());
    assert!(library.join("alpha/SKILL.md").is_file());
    assert!(library.join("beta/SKILL.md").is_file());
    assert!(library.join(".tink-skillset.json").is_file());
    let receipt: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(installed.join(".tink-skillset.json")).unwrap())
            .unwrap();
    assert_eq!(receipt["digestVersion"], 2);

    ws.cmd(&project)
        .args(["skillset", "add", "common-skillset"])
        .envs(redirect.clone())
        .assert()
        .success()
        .stdout(predicate::str::contains("Unchanged"));
    ws.cmd(&project).args(["skill", "check"]).assert().success();
    ws.cmd(&project)
        .args(["skillset", "list"])
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "common-skillset (2 skills)\n  alpha\n  beta\n",
        ));
    ws.cmd(&project)
        .args(["skillset", "list", "--library"])
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "common-skillset (2 skills)\n  alpha\n  beta\n",
        ));

    fs::write(
        library.join("alpha/SKILL.md"),
        "---\nname: alpha\ndescription: library drift\n---\n",
    )
    .unwrap();
    ws.cmd(&project)
        .args(["skillset", "add", "common-skillset"])
        .envs(redirect.clone())
        .assert()
        .success();
    assert_eq!(
        fs::read(installed.join("alpha/SKILL.md")).unwrap(),
        fs::read(library.join("alpha/SKILL.md")).unwrap(),
        "library must conform to the validated project tree"
    );

    fs::write(
        installed.join("alpha/SKILL.md"),
        "---\nname: alpha\ndescription: changed\n---\n",
    )
    .unwrap();
    let library_before_failed_add = fs::read(library.join("alpha/SKILL.md")).unwrap();
    ws.cmd(&project)
        .args(["skillset", "add", "common-skillset"])
        .envs(redirect)
        .assert()
        .failure()
        .stderr(predicate::str::contains("local modifications are present"));
    assert_eq!(
        fs::read(library.join("alpha/SKILL.md")).unwrap(),
        library_before_failed_add,
        "project drift must block library mutation"
    );
    ws.cmd(&project)
        .args(["skill", "check"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("digest mismatch"));

    ws.cmd(&project)
        .args(["skill", "remove", "common-skillset"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("skillset remove"));
    ws.cmd(&project)
        .args(["skillset", "remove", "common-skillset"])
        .assert()
        .success();
    assert!(!installed.exists());
    assert!(library.is_dir());
    assert!(ws.skillset_meta("common-skillset").is_file());
}

#[test]
fn k4_skillset_commands_require_canonical_suffix() {
    let ws = Workspace::new();
    let project = ws.project("app");
    ws.cmd(&project)
        .args(["skillset", "add", "common"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("must end in -skillset"));
    assert!(!Workspace::skill_path(&project, "common").exists());
}

#[test]
fn k5_skillset_add_refuses_unowned_library_collision() {
    let ws = Workspace::new();
    let project = ws.project("app");
    ws.initialize_inventory();
    let name = "common-skillset";
    write_skill(
        &ws.library_skill(name),
        name,
        "ordinary library skill with colliding name",
    );
    write_skillset_meta(
        &ws.skillset_meta(name),
        "https://github.com/example/skillsets.git",
        &"a".repeat(40),
        "bundles/common",
        &["alpha"],
    );

    ws.cmd(&project)
        .args(["skillset", "add", name])
        .assert()
        .failure()
        .stderr(predicate::str::contains("Library name collision"));
    assert!(!Workspace::skill_path(&project, name).exists());
}

#[test]
fn k6_skillset_remove_requires_a_valid_owned_receipt() {
    let ws = Workspace::new();
    let project = ws.project("app");
    let target = Workspace::skill_path(&project, "victim-skillset");
    fs::create_dir_all(&target).unwrap();
    fs::write(target.join(".tink-skillset.json"), "not a receipt\n").unwrap();
    fs::write(target.join("keep.txt"), "valuable user data\n").unwrap();

    ws.cmd(&project)
        .args(["skillset", "remove", "victim-skillset"])
        .assert()
        .failure()
        .stderr(predicate::str::contains(
            "Invalid installed skillset receipt",
        ));
    assert_eq!(
        fs::read_to_string(target.join("keep.txt")).unwrap(),
        "valuable user data\n"
    );

    let missing_receipt = Workspace::skill_path(&project, "unowned-skillset");
    fs::create_dir_all(&missing_receipt).unwrap();
    fs::write(missing_receipt.join("keep.txt"), "also valuable\n").unwrap();
    ws.cmd(&project)
        .args(["skillset", "remove", "unowned-skillset"])
        .assert()
        .failure()
        .stderr(predicate::str::contains(
            "Missing installed skillset receipt",
        ));
    assert!(missing_receipt.join("keep.txt").is_file());
}

#[test]
fn k7_skillset_setup_failures_are_actionable_and_non_mutating() {
    let ws = Workspace::new();
    let project = ws.project("app");

    ws.cmd(&project)
        .args(["skillset", "list"])
        .assert()
        .failure()
        .stderr(
            predicate::str::contains("Not a Tink project")
                .and(predicate::str::contains("tink init")),
        );
    ws.cmd(&project)
        .args(["skillset", "add", "missing-skillset"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("Missing skillset catalog meta"));
    assert!(!project.join(".agents").exists());
}

#[test]
fn k8_skillset_readd_is_offline_when_project_is_unchanged() {
    let ws = Workspace::new();
    let project = ws.project("app");
    ws.initialize_inventory();
    let repository = ws.root.join("skillset-repo");
    init_repo(&repository);
    write_skill(&repository.join("bundles/common/alpha"), "alpha", "member");
    let revision = commit_all(&repository, "skillset");
    let source = "https://github.com/example/skillsets.git";
    write_skillset_meta(
        &ws.skillset_meta("common-skillset"),
        source,
        &revision,
        "bundles/common",
        &["alpha"],
    );
    ws.cmd(&project)
        .args(["skillset", "add", "common-skillset"])
        .envs(github_redirect(&repository, source))
        .assert()
        .success();
    fs::remove_dir_all(ws.library_skillset("common-skillset")).unwrap();

    ws.cmd(&project)
        .args(["skillset", "add", "common-skillset"])
        .env("GIT_CONFIG_COUNT", "1")
        .env(
            "GIT_CONFIG_KEY_0",
            "url.file:///definitely-missing-tink-repo/.insteadOf",
        )
        .env("GIT_CONFIG_VALUE_0", source)
        .assert()
        .success()
        .stdout(predicate::str::contains("Unchanged common-skillset"));
    assert!(
        ws.library_skillset("common-skillset")
            .join("alpha/SKILL.md")
            .is_file()
    );
}

#[test]
fn k9_skill_status_reports_grouped_members_honestly() {
    let ws = Workspace::new();
    let project = ws.project("app");
    ws.initialize_inventory();
    let repository = ws.root.join("skillset-repo");
    init_repo(&repository);
    write_skill(&repository.join("bundles/common/alpha"), "alpha", "first");
    write_skill(&repository.join("bundles/common/beta"), "beta", "second");
    let revision = commit_all(&repository, "skillset");
    let source = "https://github.com/example/skillsets.git";
    write_skillset_meta(
        &ws.skillset_meta("common-skillset"),
        source,
        &revision,
        "bundles/common",
        &["alpha", "beta"],
    );
    ws.cmd(&project)
        .args(["skillset", "add", "common-skillset"])
        .envs(github_redirect(&repository, source))
        .assert()
        .success();

    ws.cmd(&project)
        .args(["skill", "check"])
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "OK 0 skill(s), 1 skillset(s), 2 member skill(s)",
        ));
    ws.cmd(&project)
        .args(["skill", "list"])
        .assert()
        .success()
        .stdout(
            predicate::str::contains("no standalone skills")
                .and(predicate::str::contains("tink skillset list")),
        );
}

#[test]
fn k10_skillset_refresh_updates_clean_tree_and_refuses_local_edits() {
    let ws = Workspace::new();
    let project = ws.project("app");
    ws.initialize_inventory();
    let repository = ws.root.join("skillset-repo");
    init_repo(&repository);
    let source = "https://github.com/example/skillsets.git";
    let member = repository.join("bundles/common/alpha");
    write_skill(&member, "alpha", "version one");
    let first_revision = commit_all(&repository, "first");
    write_skillset_meta(
        &ws.skillset_meta("common-skillset"),
        source,
        &first_revision,
        "bundles/common",
        &["alpha"],
    );
    let redirect = github_redirect(&repository, source);
    ws.cmd(&project)
        .args(["skillset", "add", "common-skillset"])
        .envs(redirect.clone())
        .assert()
        .success();

    write_skill(&member, "alpha", "version two");
    let second_revision = commit_all(&repository, "second");
    write_skillset_meta(
        &ws.skillset_meta("common-skillset"),
        source,
        &second_revision,
        "bundles/common",
        &["alpha"],
    );
    ws.cmd(&project)
        .args(["skillset", "refresh", "common-skillset"])
        .envs(redirect.clone())
        .assert()
        .success()
        .stdout(predicate::str::contains("Refreshed common-skillset"));
    let installed = Workspace::skill_path(&project, "common-skillset");
    assert!(
        fs::read_to_string(installed.join("alpha/SKILL.md"))
            .unwrap()
            .contains("version two")
    );
    assert_eq!(
        fs::read(installed.join("alpha/SKILL.md")).unwrap(),
        fs::read(
            ws.library_skillset("common-skillset")
                .join("alpha/SKILL.md")
        )
        .unwrap()
    );

    fs::write(
        installed.join("alpha/SKILL.md"),
        "---\nname: alpha\ndescription: local edit\n---\n",
    )
    .unwrap();
    ws.cmd(&project)
        .args(["skillset", "refresh", "common-skillset"])
        .envs(redirect)
        .assert()
        .failure()
        .stderr(predicate::str::contains("local modifications are present"));
}

#[test]
fn k11_skillset_rejects_member_directory_name_mismatch() {
    let ws = Workspace::new();
    let project = ws.project("app");
    ws.initialize_inventory();
    let repository = ws.root.join("skillset-repo");
    init_repo(&repository);
    write_skill(
        &repository.join("bundles/common/alpha"),
        "different-name",
        "mismatched member",
    );
    let revision = commit_all(&repository, "mismatched member");
    let source = "https://github.com/example/skillsets.git";
    write_skillset_meta(
        &ws.skillset_meta("common-skillset"),
        source,
        &revision,
        "bundles/common",
        &["alpha"],
    );

    ws.cmd(&project)
        .args(["skillset", "add", "common-skillset"])
        .envs(github_redirect(&repository, source))
        .assert()
        .failure()
        .stderr(
            predicate::str::contains("different-name")
                .and(predicate::str::contains("must match directory"))
                .and(predicate::str::contains("alpha")),
        );
    assert!(!Workspace::skill_path(&project, "common-skillset").exists());
    assert!(!ws.library_skillset("common-skillset").exists());
}

// --- R*: remote add ---

#[test]
fn r1_add_github_writes_receipt() {
    let ws = Workspace::new();
    let project = ws.project("app");
    ws.cmd(&project).arg("init").assert().success();

    let remote = ws.root.join("remote-repo");
    init_repo(&remote);
    write_skill(
        &remote.join("skills").join("remote-skill"),
        "remote-skill",
        "from remote",
    );
    let revision = commit_all(&remote, "add skill");
    let public = "https://github.com/example/skills.git";
    let mut cmd = ws.cmd(&project);
    cmd.args(["skill", "add", "example/skills", "--skill", "remote-skill"]);
    for (k, v) in github_redirect(&remote, public) {
        cmd.env(k, v);
    }
    cmd.assert().success();

    let receipt_path = Workspace::skill_path(&project, "remote-skill").join(".tink-source.json");
    let receipt = fs::read_to_string(receipt_path).unwrap();
    assert!(receipt.contains(public) || receipt.contains("https://github.com/example/skills.git"));
    assert!(receipt.contains(&revision));
    assert!(receipt.contains("skills/remote-skill"));
}

#[test]
fn r2_add_rejects_non_github_remote() {
    let ws = Workspace::new();
    let project = ws.project("app");
    ws.cmd(&project).arg("init").assert().success();
    ws.cmd(&project)
        .args(["skill", "add", "https://gitlab.com/example/skills.git"])
        .assert()
        .failure()
        .stderr(
            predicate::str::contains("GitHub")
                .or(predicate::str::contains("github"))
                .or(predicate::str::contains("public")),
        );
}

#[test]
fn r12_add_rejects_embedded_lock_source() {
    let ws = Workspace::new();
    let project = ws.project("app");
    ws.cmd(&project).arg("init").assert().success();
    ws.cmd(&project)
        .args(["skill", "add", "tink:embedded/manage-tink"])
        .assert()
        .failure()
        .stderr(predicate::str::contains(
            "Remote sources must be public GitHub HTTPS URLs or owner/repository",
        ));
}

#[test]
fn r3_add_dot_slash_missing_is_path_not_github() {
    let ws = Workspace::new();
    let project = ws.project("app");
    ws.cmd(&project).arg("init").assert().success();
    ws.cmd(&project)
        .args(["skill", "add", "./relative-missing"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("Path does not exist"))
        .stderr(predicate::str::contains("github.com").not());
}

#[test]
fn r4_add_absolute_missing_is_path_not_remote_shape() {
    let ws = Workspace::new();
    let project = ws.project("app");
    ws.cmd(&project).arg("init").assert().success();
    let missing = ws.root.join("does-not-exist-skill");
    ws.cmd(&project)
        .args(["skill", "add", missing.to_str().unwrap()])
        .assert()
        .failure()
        .stderr(predicate::str::contains("Path does not exist"));
}

#[test]
fn r5_add_root_level_skill_writes_dot_path_check_and_refresh() {
    let ws = Workspace::new();
    let project = ws.project("app");
    ws.cmd(&project)
        .args(["init", "--no-zen", "--no-manage-tink"])
        .assert()
        .success();

    let remote = ws.root.join("root-skill-repo");
    init_repo(&remote);
    write_skill(&remote, "root-skill", "v1");
    commit_all(&remote, "v1");
    let public = "https://github.com/example/root-skill.git";
    let redirect = github_redirect(&remote, public);

    let mut add = ws.cmd(&project);
    add.args(["skill", "add", "example/root-skill"]);
    for (k, v) in &redirect {
        add.env(k, v);
    }
    add.assert().success();

    let receipt =
        fs::read_to_string(Workspace::skill_path(&project, "root-skill").join(".tink-source.json"))
            .unwrap();
    assert!(
        receipt.contains("\"path\": \".\"") || receipt.contains("\"path\":\".\""),
        "expected path \".\": {receipt}"
    );
    assert!(!receipt.contains("\"path\": \"\""));

    ws.cmd(&project)
        .args(["skill", "check"])
        .assert()
        .success()
        .stdout(predicate::str::contains("OK"));

    write_skill(&remote, "root-skill", "v2");
    let new_rev = commit_all(&remote, "v2");
    let mut refresh = ws.cmd(&project);
    refresh.args(["skill", "refresh", "root-skill"]);
    for (k, v) in &redirect {
        refresh.env(k, v);
    }
    refresh.assert().success();

    let skill_md =
        fs::read_to_string(Workspace::skill_path(&project, "root-skill").join("SKILL.md")).unwrap();
    assert!(skill_md.contains("v2"), "{skill_md}");
    let receipt =
        fs::read_to_string(Workspace::skill_path(&project, "root-skill").join(".tink-source.json"))
            .unwrap();
    assert!(receipt.contains(&new_rev));
    assert!(receipt.contains("\"path\": \".\"") || receipt.contains("\"path\":\".\""));
}

#[test]
fn r6_add_finds_unique_nested_remote_skill_by_name() {
    let ws = Workspace::new();
    let project = ws.project("app");
    ws.cmd(&project)
        .args(["init", "--no-zen", "--no-tink-skills", "--no-manage-tink"])
        .assert()
        .success();

    let remote = ws.root.join("nested-skills-repo");
    init_repo(&remote);
    write_skill(
        &remote.join("packages/shared/skills/nested-skill"),
        "nested-skill",
        "selected nested skill",
    );
    write_skill(
        &remote.join("packages/other/skills/unrelated-skill"),
        "unrelated-skill",
        "unrelated nested skill",
    );
    let revision = commit_all(&remote, "nested skills");
    let public = "https://github.com/example/nested-skills.git";

    let mut add = ws.cmd(&project);
    add.args([
        "skill",
        "add",
        "example/nested-skills",
        "--skill",
        "nested-skill",
    ]);
    add.envs(github_redirect(&remote, public));
    add.assert().success();

    let installed = Workspace::skill_path(&project, "nested-skill");
    assert!(
        fs::read_to_string(installed.join("SKILL.md"))
            .unwrap()
            .contains("selected nested skill")
    );
    let receipt: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(installed.join(".tink-source.json")).unwrap())
            .unwrap();
    assert_eq!(receipt["source"], public);
    assert_eq!(receipt["revision"], revision);
    assert_eq!(receipt["path"], "packages/shared/skills/nested-skill");
    ws.assert_cataloged("app", "nested-skill");
    ws.cmd(&project).args(["skill", "check"]).assert().success();
}

#[test]
fn r7_add_refuses_ambiguous_nested_skill_name_without_writes() {
    let ws = Workspace::new();
    let project = ws.project("app");
    ws.cmd(&project)
        .args(["init", "--no-zen", "--no-tink-skills", "--no-manage-tink"])
        .assert()
        .success();

    let remote = ws.root.join("duplicate-skills-repo");
    init_repo(&remote);
    write_skill(
        &remote.join("packages/one/skills/same-skill"),
        "same-skill",
        "first duplicate",
    );
    write_skill(
        &remote.join("packages/two/skills/same-skill"),
        "same-skill",
        "second duplicate",
    );
    commit_all(&remote, "duplicate skills");
    let public = "https://github.com/example/duplicate-skills.git";

    let mut add = ws.cmd(&project);
    add.args([
        "skill",
        "add",
        "example/duplicate-skills",
        "--skill",
        "same-skill",
    ]);
    add.envs(github_redirect(&remote, public));
    add.assert().failure().stderr(
        predicate::str::contains("multiple skills")
            .and(predicate::str::contains("packages/one/skills/same-skill"))
            .and(predicate::str::contains("packages/two/skills/same-skill")),
    );

    assert!(!Workspace::skill_path(&project, "same-skill").exists());
    assert!(!ws.library_skill("same-skill").exists());
    let catalog = fs::read_to_string(ws.catalog_meta("app")).unwrap_or_default();
    assert!(!catalog.contains("same-skill"), "{catalog}");
}

#[test]
fn r8_add_selects_nested_remote_skill_by_repository_path_and_refreshes() {
    let ws = Workspace::new();
    let project = ws.project("app");
    ws.cmd(&project)
        .args(["init", "--no-zen", "--no-tink-skills", "--no-manage-tink"])
        .assert()
        .success();

    let remote = ws.root.join("path-selected-skills-repo");
    init_repo(&remote);
    let selected_path = "packages/shared-skills/skills/git-master";
    write_skill(
        &remote.join(selected_path),
        "git-master",
        "shared implementation v1",
    );
    write_skill(
        &remote.join("packages/generated/skills/git-master"),
        "git-master",
        "generated duplicate",
    );
    let revision = commit_all(&remote, "duplicate git-master skills");
    let public = "https://github.com/example/path-selected-skills.git";
    let redirect = github_redirect(&remote, public);

    let mut add = ws.cmd(&project);
    add.args([
        "skill",
        "add",
        "example/path-selected-skills",
        "--skill",
        selected_path,
    ]);
    add.envs(redirect.clone());
    add.assert().success();

    let installed = Workspace::skill_path(&project, "git-master");
    let receipt: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(installed.join(".tink-source.json")).unwrap())
            .unwrap();
    assert_eq!(receipt["source"], public);
    assert_eq!(receipt["revision"], revision);
    assert_eq!(receipt["path"], selected_path);
    assert!(
        fs::read_to_string(installed.join("SKILL.md"))
            .unwrap()
            .contains("shared implementation v1")
    );

    write_skill(
        &remote.join(selected_path),
        "git-master",
        "shared implementation v2",
    );
    commit_all(&remote, "update selected git-master");
    let mut refresh = ws.cmd(&project);
    refresh.args(["skill", "refresh", "git-master"]);
    refresh.envs(redirect);
    refresh.assert().success();
    assert!(
        fs::read_to_string(installed.join("SKILL.md"))
            .unwrap()
            .contains("shared implementation v2")
    );
}

#[test]
fn r9_cached_duplicate_does_not_bypass_remote_name_ambiguity() {
    let ws = Workspace::new();
    let first_project = ws.project("first");
    let second_project = ws.project("second");
    for project in [&first_project, &second_project] {
        ws.cmd(project)
            .args(["init", "--no-zen", "--no-tink-skills", "--no-manage-tink"])
            .assert()
            .success();
    }

    let remote = ws.root.join("cached-duplicate-skills-repo");
    init_repo(&remote);
    let selected_path = "packages/shared/skills/same-skill";
    write_skill(
        &remote.join(selected_path),
        "same-skill",
        "cached duplicate",
    );
    write_skill(
        &remote.join("packages/other/skills/same-skill"),
        "same-skill",
        "other duplicate",
    );
    commit_all(&remote, "cached duplicate skills");
    let public = "https://github.com/example/cached-duplicate-skills.git";
    let redirect = github_redirect(&remote, public);

    let mut first_add = ws.cmd(&first_project);
    first_add.args([
        "skill",
        "add",
        "example/cached-duplicate-skills",
        "--skill",
        selected_path,
    ]);
    first_add.envs(redirect.clone());
    first_add.assert().success();
    assert!(ws.library_skill("same-skill").join("SKILL.md").is_file());

    let mut second_add = ws.cmd(&second_project);
    second_add.args([
        "skill",
        "add",
        "example/cached-duplicate-skills",
        "--skill",
        "same-skill",
    ]);
    second_add.envs(redirect);
    second_add.assert().failure().stderr(
        predicate::str::contains("multiple skills")
            .and(predicate::str::contains(selected_path))
            .and(predicate::str::contains("packages/other/skills/same-skill")),
    );
    assert!(!Workspace::skill_path(&second_project, "same-skill").exists());
}

#[test]
fn r10_mismatched_directory_name_does_not_create_false_name_ambiguity() {
    let ws = Workspace::new();
    let project = ws.project("app");
    ws.cmd(&project)
        .args(["init", "--no-zen", "--no-tink-skills", "--no-manage-tink"])
        .assert()
        .success();

    let remote = ws.root.join("mismatched-name-repo");
    init_repo(&remote);
    write_skill(
        &remote.join("packages/valid/skills/target-skill"),
        "target-skill",
        "installable target",
    );
    write_skill(
        &remote.join("packages/invalid/skills/wrong-directory"),
        "target-skill",
        "mismatched directory",
    );
    commit_all(&remote, "valid and mismatched targets");
    let public = "https://github.com/example/mismatched-name.git";

    let mut add = ws.cmd(&project);
    add.args([
        "skill",
        "add",
        "example/mismatched-name",
        "--skill",
        "target-skill",
    ]);
    add.envs(github_redirect(&remote, public));
    add.assert().success();
    assert!(
        fs::read_to_string(Workspace::skill_path(&project, "target-skill").join("SKILL.md"))
            .unwrap()
            .contains("installable target")
    );
}

#[test]
fn r11_dot_path_selects_root_skill_when_name_is_ambiguous() {
    let ws = Workspace::new();
    let project = ws.project("app");
    ws.cmd(&project)
        .args(["init", "--no-zen", "--no-tink-skills", "--no-manage-tink"])
        .assert()
        .success();

    let remote = ws.root.join("root-duplicate-repo");
    init_repo(&remote);
    write_skill(&remote, "same-skill", "root implementation");
    let nested_path = "packages/nested/skills/same-skill";
    write_skill(
        &remote.join(nested_path),
        "same-skill",
        "nested implementation",
    );
    commit_all(&remote, "root and nested duplicate");
    let public = "https://github.com/example/root-duplicate.git";
    let redirect = github_redirect(&remote, public);

    let mut ambiguous = ws.cmd(&project);
    ambiguous.args([
        "skill",
        "add",
        "example/root-duplicate",
        "--skill",
        "same-skill",
    ]);
    ambiguous.envs(redirect.clone());
    ambiguous
        .assert()
        .failure()
        .stderr(predicate::str::contains("--skill .").and(predicate::str::contains(nested_path)));

    let mut add_root = ws.cmd(&project);
    add_root.args(["skill", "add", "example/root-duplicate", "--skill", "."]);
    add_root.envs(redirect);
    add_root.assert().success();

    let installed = Workspace::skill_path(&project, "same-skill");
    assert!(
        fs::read_to_string(installed.join("SKILL.md"))
            .unwrap()
            .contains("root implementation")
    );
    let receipt: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(installed.join(".tink-source.json")).unwrap())
            .unwrap();
    assert_eq!(receipt["path"], ".");
}

#[test]
fn v3_main_help_works_when_current_directory_is_unavailable_commands_fail() {
    let ws = Workspace::new();
    let bad_dir = ws.root.join("vanish-now");
    fs::create_dir_all(&bad_dir).unwrap();

    let bin = std::env::var_os("CARGO_BIN_EXE_tink").expect("CARGO_BIN_EXE_tink missing");

    // --help must not depend on a live workdir (clap exit after parse succeeds).
    let help = StdCommand::new("sh")
        .arg("-c")
        .arg("cd \"$1\" && rm -rf \"$1\" && \"$2\" --help")
        .arg("tink-main-current-dir-help")
        .arg(bad_dir.to_string_lossy().to_string())
        .arg(&bin)
        .output()
        .expect("run help shim");
    assert!(
        help.status.success(),
        "tink --help should succeed without cwd; status={:?} stderr={}",
        help.status,
        String::from_utf8_lossy(&help.stderr)
    );
    let help_out = String::from_utf8_lossy(&help.stdout);
    assert!(
        help_out.contains("Usage") || help_out.contains("Usage:"),
        "expected help output: {help_out}"
    );

    // Commands that need a project path still fail closed when cwd is gone.
    let bad_dir2 = ws.root.join("vanish-cmd");
    fs::create_dir_all(&bad_dir2).unwrap();
    let cmd = StdCommand::new("sh")
        .arg("-c")
        .arg("cd \"$1\" && rm -rf \"$1\" && \"$2\" skill check")
        .arg("tink-main-current-dir-cmd")
        .arg(bad_dir2.to_string_lossy().to_string())
        .arg(&bin)
        .output()
        .expect("run skill check shim");
    assert!(!cmd.status.success(), "skill check should fail without cwd");
    let stderr = String::from_utf8_lossy(&cmd.stderr);
    assert!(
        stderr.contains("Failed to resolve current directory"),
        "stderr did not include current directory failure: {stderr}"
    );
    assert_eq!(cmd.status.code().unwrap(), 1);
}

// --- C*: check ---

#[test]
fn c1_check_passes_valid_project() {
    let ws = Workspace::new();
    let project = ws.project("app");
    ws.cmd(&project).arg("init").assert().success();
    let source = ws.root.join("demo-skill");
    write_skill(&source, "demo-skill", "ok");
    ws.cmd(&project)
        .args(["skill", "add", source.to_str().unwrap()])
        .assert()
        .success();
    ws.cmd(&project).args(["skill", "check"]).assert().success();
}

#[test]
fn c2_check_fails_without_skills_dir() {
    let ws = Workspace::new();
    let project = ws.project("app");
    ws.cmd(&project)
        .args(["skill", "check"])
        .assert()
        .failure()
        .stderr(predicate::str::contains(".agents/skills"));
}

#[test]
fn c3_check_refuses_agents_symlink() {
    let ws = Workspace::new();
    let project = ws.project("app");
    let real = ws.root.join("real-agents");
    fs::create_dir_all(real.join("skills")).unwrap();
    std::os::unix::fs::symlink(&real, project.join(".agents")).unwrap();
    ws.cmd(&project)
        .args(["skill", "check"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("symlink").or(predicate::str::contains("Symlink")));
}

#[test]
fn c5_check_fails_with_corrupt_installed_skill() {
    let ws = Workspace::new();
    let project = ws.project("app");
    ws.cmd(&project).arg("init").assert().success();

    let source = ws.root.join("demo-skill");
    write_skill(&source, "demo-skill", "ok");
    ws.cmd(&project)
        .args(["skill", "add", source.to_str().unwrap()])
        .assert()
        .success();

    let installed = Workspace::skill_path(&project, "demo-skill");
    let corrupt = r#"---
name: mismatch
description: corrupted metadata
---

# mismatch
"#;
    std::fs::write(installed.join("SKILL.md"), corrupt).expect("corrupt skill");

    ws.cmd(&project)
        .args(["skill", "check"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("Skill name"));
}

#[test]
fn c6_check_rejects_skill_without_yaml_frontmatter() {
    let ws = Workspace::new();
    let project = ws.project("app");
    ws.cmd(&project).arg("init").assert().success();

    let installed = Workspace::skill_path(&project, "broken-skill");
    fs::create_dir_all(&installed).expect("broken skill dir");
    fs::write(installed.join("SKILL.md"), "# Missing frontmatter\n")
        .expect("write malformed SKILL.md");

    ws.cmd(&project)
        .args(["skill", "check"])
        .assert()
        .failure()
        .stderr(predicate::str::contains(
            "SKILL.md must start with YAML frontmatter",
        ));
}

#[test]
fn c7_check_rejects_unclosed_skill_frontmatter() {
    let ws = Workspace::new();
    let project = ws.project("app");
    ws.cmd(&project).arg("init").assert().success();

    let installed = Workspace::skill_path(&project, "broken-skill");
    fs::create_dir_all(&installed).expect("broken skill dir");
    fs::write(
        installed.join("SKILL.md"),
        "---\nname: broken-skill\ndescription: missing closing marker\n",
    )
    .expect("write malformed SKILL.md");

    ws.cmd(&project)
        .args(["skill", "check"])
        .assert()
        .failure()
        .stderr(predicate::str::contains(
            "SKILL.md frontmatter is not closed",
        ));
}

#[test]
fn c8_check_rejects_stale_embedded_manage_tink() {
    let ws = Workspace::new();
    let project = ws.project("app");
    ws.cmd(&project).arg("init").assert().success();

    fs::write(
        Workspace::skill_path(&project, "manage-tink").join("references/commands.md"),
        "stale embedded contents\n",
    )
    .expect("replace embedded commands reference");

    ws.cmd(&project)
        .args(["skill", "check"])
        .assert()
        .failure()
        .stderr(
            predicate::str::contains("manage-tink differs from this Tink binary")
                .and(predicate::str::contains("tink skill refresh manage-tink")),
        );

    ws.cmd(&project)
        .args(["skill", "lock"])
        .assert()
        .failure()
        .stderr(predicate::str::contains(
            "manage-tink differs from this Tink binary",
        ));
    assert!(!project.join(".tink/skills.toml").exists());
    assert!(!project.join(".tink/skills.lock").exists());
}

// --- M*: project manifest ---

#[test]
fn m1_skill_verify_accepts_empty_manifest_for_empty_project() {
    let ws = Workspace::new();
    let project = ws.project("app");
    ws.cmd(&project)
        .args(["init", "--no-zen", "--no-tink-skills", "--no-manage-tink"])
        .assert()
        .success();
    fs::create_dir_all(project.join(".tink")).unwrap();
    fs::write(
        project.join(".tink").join("skills.toml"),
        "version = 1\nskills = []\n",
    )
    .unwrap();
    fs::write(
        project.join(".tink").join("skills.lock"),
        "version = 2\nskills = []\n",
    )
    .unwrap();
    ws.cmd(&project)
        .args(["skill", "verify"])
        .assert()
        .success()
        .stdout(predicate::str::contains("OK 0 manifest skill(s)"));
}

#[test]
fn m2_skill_lock_generates_manifest_and_lockfile() {
    let ws = Workspace::new();
    let project = ws.project("app");
    ws.cmd(&project)
        .args(["init", "--no-zen", "--no-tink-skills", "--no-manage-tink"])
        .assert()
        .success();
    let source = project.join("fixture").join("reviewer");
    write_skill(&source, "reviewer", "body");
    ws.cmd(&project)
        .args(["skill", "add", source.to_str().unwrap()])
        .assert()
        .success();
    ws.cmd(&project)
        .args(["skill", "lock", "--source", "reviewer=fixture/reviewer"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Wrote 1 manifest skill(s)"));
    assert!(project.join(".tink").join("skills.toml").is_file());
    assert!(project.join(".tink").join("skills.lock").is_file());
    ws.cmd(&project)
        .args(["skill", "verify"])
        .assert()
        .success();
}

#[test]
fn m3_skill_sync_restores_missing_local_skill() {
    let ws = Workspace::new();
    let project = ws.project("app");
    ws.cmd(&project)
        .args(["init", "--no-zen", "--no-tink-skills", "--no-manage-tink"])
        .assert()
        .success();
    let source = project.join("fixture").join("reviewer");
    write_skill(&source, "reviewer", "body");
    ws.cmd(&project)
        .args(["skill", "add", source.to_str().unwrap()])
        .assert()
        .success();
    ws.cmd(&project)
        .args(["skill", "lock", "--source", "reviewer=fixture/reviewer"])
        .assert()
        .success();
    fs::remove_dir_all(Workspace::skill_path(&project, "reviewer")).unwrap();
    ws.cmd(&project)
        .args(["skill", "sync"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Synced 1 manifest skill(s)"));
    ws.cmd(&project)
        .args(["skill", "verify"])
        .assert()
        .success();
}

#[test]
fn m4_skill_sync_restores_embedded_manage_tink() {
    let ws = Workspace::new();
    let project = ws.project("app");
    ws.cmd(&project)
        .args(["init", "--no-zen", "--no-tink-skills", "--with-manage-tink"])
        .assert()
        .success();
    ws.cmd(&project).args(["skill", "lock"]).assert().success();
    ws.cmd(&project)
        .args(["skill", "verify"])
        .assert()
        .success();
    fs::remove_dir_all(Workspace::skill_path(&project, "manage-tink")).unwrap();
    ws.cmd(&project)
        .args(["skill", "sync"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Synced 1 manifest skill(s)"));
    ws.cmd(&project)
        .args(["skill", "verify"])
        .assert()
        .success();
}

#[test]
fn m5_skill_sync_keeps_missing_local_source_local() {
    let ws = Workspace::new();
    let project = ws.project("app");
    ws.cmd(&project)
        .args(["init", "--no-zen", "--no-tink-skills", "--no-manage-tink"])
        .assert()
        .success();
    let source = project.join("owner").join("repo-shape");
    write_skill(&source, "local-skill", "body");
    ws.cmd(&project)
        .args(["skill", "add", source.to_str().unwrap()])
        .assert()
        .success();
    ws.cmd(&project)
        .args(["skill", "lock", "--source", "local-skill=owner/repo-shape"])
        .assert()
        .success();
    fs::remove_dir_all(Workspace::skill_path(&project, "local-skill")).unwrap();
    fs::remove_dir_all(project.join("owner")).unwrap();
    ws.cmd(&project)
        .args(["skill", "sync"])
        .assert()
        .failure()
        .stderr(
            predicate::str::contains("Path does not exist")
                .and(predicate::str::contains("github.com").not()),
        );
}

#[test]
fn m6_skill_verify_requires_manifest() {
    let ws = Workspace::new();
    let project = ws.project("app");
    ws.cmd(&project)
        .args(["init", "--no-zen", "--no-tink-skills", "--no-manage-tink"])
        .assert()
        .success();
    ws.cmd(&project)
        .args(["skill", "verify"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("Missing project manifest"));
}

#[test]
fn m7_skill_sync_rejects_late_bad_hash_before_any_publication() {
    let ws = Workspace::new();
    let project = ws.project("app");
    ws.cmd(&project)
        .args(["init", "--no-zen", "--no-tink-skills", "--no-manage-tink"])
        .assert()
        .success();
    for name in ["alpha", "beta"] {
        let source = project.join("sources").join(name);
        write_skill(&source, name, "manifest preflight fixture");
        ws.cmd(&project)
            .args(["skill", "add", source.to_str().unwrap()])
            .assert()
            .success();
    }
    ws.cmd(&project)
        .args([
            "skill",
            "lock",
            "--source",
            "alpha=sources/alpha",
            "--source",
            "beta=sources/beta",
        ])
        .assert()
        .success();
    for name in ["alpha", "beta"] {
        ws.cmd(&project)
            .args(["skill", "remove", name])
            .assert()
            .success();
        fs::remove_dir_all(ws.library_skill(name)).unwrap();
    }
    let lock_path = project.join(".tink/skills.lock");
    let mut lock = fs::read_to_string(&lock_path).unwrap();
    let hash_start = lock.rfind("sha256 = \"").unwrap() + "sha256 = \"".len();
    lock.replace_range(hash_start..hash_start + 64, &"0".repeat(64));
    fs::write(&lock_path, lock).unwrap();

    ws.cmd(&project)
        .args(["skill", "sync"])
        .assert()
        .failure()
        .stderr(predicate::str::contains(
            "Skill content hash mismatch: beta",
        ));

    assert!(!Workspace::skill_path(&project, "alpha").exists());
    assert!(!ws.library_skill("alpha").exists());
    assert!(!ws.catalog_meta("app").exists());
}

#[cfg(unix)]
#[test]
fn m8_skill_sync_preflights_late_library_refusal_before_publication() {
    let ws = Workspace::new();
    let project = ws.project("app");
    ws.cmd(&project)
        .args(["init", "--no-zen", "--no-tink-skills", "--no-manage-tink"])
        .assert()
        .success();
    for name in ["alpha", "beta"] {
        let source = project.join("sources").join(name);
        write_skill(&source, name, "library preflight fixture");
        ws.cmd(&project)
            .args(["skill", "add", source.to_str().unwrap()])
            .assert()
            .success();
    }
    ws.cmd(&project)
        .args([
            "skill",
            "lock",
            "--source",
            "alpha=sources/alpha",
            "--source",
            "beta=sources/beta",
        ])
        .assert()
        .success();
    for name in ["alpha", "beta"] {
        ws.cmd(&project)
            .args(["skill", "remove", name])
            .assert()
            .success();
        fs::remove_dir_all(ws.library_skill(name)).unwrap();
    }
    let outside = ws.root.join("outside-library-target");
    fs::create_dir_all(&outside).unwrap();
    std::os::unix::fs::symlink(&outside, ws.library_skill("beta")).unwrap();

    ws.cmd(&project)
        .args(["skill", "sync"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("symlink"));

    assert!(!Workspace::skill_path(&project, "alpha").exists());
    assert!(!ws.library_skill("alpha").exists());
    assert!(!ws.catalog_meta("app").exists());
}

#[test]
fn m9_legacy_lock_requires_relock_and_migrates_to_v2() {
    let ws = Workspace::new();
    let project = ws.project("app");
    ws.cmd(&project)
        .args(["init", "--no-zen", "--no-tink-skills", "--no-manage-tink"])
        .assert()
        .success();
    fs::create_dir_all(project.join(".tink")).unwrap();
    fs::write(
        project.join(".tink/skills.toml"),
        "version = 1\nskills = []\n",
    )
    .unwrap();
    fs::write(
        project.join(".tink/skills.lock"),
        "version = 1\nskills = []\n",
    )
    .unwrap();

    ws.cmd(&project)
        .args(["skill", "verify"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("run `tink skill lock`"));
    ws.cmd(&project).args(["skill", "lock"]).assert().success();

    assert!(
        fs::read_to_string(project.join(".tink/skills.lock"))
            .unwrap()
            .starts_with("version = 2\n")
    );
    ws.cmd(&project)
        .args(["skill", "verify"])
        .assert()
        .success();
}

// --- L*: list ---

#[test]
fn l1_skill_list_after_init_includes_manage_tink() {
    let ws = Workspace::new();
    let project = ws.project("app");
    ws.cmd(&project).arg("init").assert().success();
    ws.cmd(&project)
        .args(["skill", "list"])
        .assert()
        .success()
        .stdout(predicate::str::contains("manage-tink"));
}

#[test]
fn l14_library_list_top_level_matches_legacy_alias() {
    let ws = Workspace::new();
    let project = ws.project("app");
    ws.cmd(&project).arg("init").assert().success();

    let canonical = ws
        .cmd(&project)
        .args(["library", "list"])
        .output()
        .expect("run library list");
    let legacy = ws
        .cmd(&project)
        .args(["skill", "list", "--library"])
        .output()
        .expect("run skill list --library");

    assert!(canonical.status.success());
    assert!(legacy.status.success());
    assert_eq!(canonical.stdout, legacy.stdout);
    assert_eq!(canonical.stderr, legacy.stderr);
}

#[test]
fn l2_skill_list_fails_without_skills_dir() {
    let ws = Workspace::new();
    let project = ws.project("app");
    ws.cmd(&project)
        .args(["skill", "list"])
        .assert()
        .failure()
        .stderr(predicate::str::contains(".agents/skills"));
}

#[test]
fn l3_skill_list_catalog_prints_tsv() {
    let ws = Workspace::new();
    let project = ws.project("app");
    ws.cmd(&project).arg("init").assert().success();
    let source = ws.root.join("demo-skill");
    write_skill(&source, "demo-skill", "cataloged");
    ws.cmd(&project)
        .args(["skill", "add", source.to_str().unwrap()])
        .assert()
        .success();
    let root = project.canonicalize().unwrap();
    let root_s = root.to_str().unwrap();
    ws.cmd(&project)
        .args(["skill", "list", "--catalog"])
        .assert()
        .success()
        .stdout(
            predicate::str::starts_with("project\troot\tskill\n")
                .and(predicate::str::contains("app\t"))
                .and(predicate::str::contains(root_s))
                .and(predicate::str::contains("demo-skill"))
                .and(predicate::str::contains("manage-tink")),
        );
}

#[test]
fn l11_hidden_project_remains_visible_in_catalog_listing() {
    let ws = Workspace::new();
    let project = ws.project(".app");
    ws.cmd(&project).arg("init").assert().success();
    let canonical = project.canonicalize().unwrap();

    ws.cmd(&project)
        .args(["skill", "list", "--catalog"])
        .assert()
        .success()
        .stdout(predicate::str::contains(format!(
            ".app\t{}\tmanage-tink",
            canonical.display()
        )));
}

#[test]
fn l12_catalog_listing_escapes_row_delimiters_without_changing_columns() {
    let ws = Workspace::new();
    let project = ws.project("project\trow\nnext\u{1b}[31m");
    ws.cmd(&project).arg("init").assert().success();
    let canonical = project.canonicalize().unwrap();
    let escaped_root = canonical
        .to_string_lossy()
        .replace('\\', "\\\\")
        .replace('\t', "\\t")
        .replace('\r', "\\r")
        .replace('\n', "\\n")
        .replace('\u{1b}', "\\x1b");

    let output = ws
        .cmd(&project)
        .args(["skill", "list", "--catalog"])
        .output()
        .expect("run skill list --catalog");
    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    let lines: Vec<_> = stdout.lines().collect();
    assert_eq!(
        lines.len(),
        2,
        "catalog output must remain one row per skill"
    );
    assert_eq!(lines[0], "project\troot\tskill");
    let fields: Vec<_> = lines[1].split('\t').collect();
    assert_eq!(
        fields,
        [
            "project\\trow\\nnext\\x1b[31m",
            &escaped_root,
            "manage-tink"
        ]
    );
}

#[test]
fn l13_empty_catalog_listing_is_header_only_tsv() {
    let ws = Workspace::new();
    let project = ws.project("app");
    ws.initialize_inventory();

    ws.cmd(&project)
        .args(["skill", "list", "--catalog"])
        .assert()
        .success()
        .stdout("project\troot\tskill\n");
}

#[test]
fn l6_skill_list_catalog_skips_malformed_meta_entries() {
    let ws = Workspace::new();
    let project = ws.project("app");
    ws.initialize_inventory();

    let by_project = ws.inventory.join("catalog").join("by-project");
    let good = by_project.join("good-project");
    let malformed = by_project.join("bad-project");
    fs::create_dir_all(&good).unwrap();
    fs::create_dir_all(&malformed).unwrap();
    fs::write(
        good.join("meta.json"),
        "{\"name\":\"good-project\",\"root\":\"/tmp/example\",\"skills\":[\"demo-skill\"]}",
    )
    .unwrap();
    fs::write(malformed.join("meta.json"), "{not-json}").unwrap();

    let output = ws
        .cmd(&project)
        .args(["skill", "list", "--catalog"])
        .output()
        .expect("run skill list --catalog");
    assert!(
        output.status.success(),
        "skill list --catalog should ignore malformed entries"
    );
    let out = String::from_utf8_lossy(&output.stdout);
    assert!(out.starts_with("project	root	skill\n"));
    assert!(out.contains("good-project	/tmp/example	demo-skill"));
    assert!(out.contains("good-project"));
    assert!(out.contains("/tmp/example"));
    assert!(!out.contains("bad-project"));
}

#[test]
fn l5_skill_list_rejects_removed_stash_and_home_flags() {
    let ws = Workspace::new();
    let project = ws.project("app");
    ws.cmd(&project).arg("init").assert().success();
    for flag in ["--stash", "--home"] {
        ws.cmd(&project)
            .args(["skill", "list", flag])
            .assert()
            .failure()
            .stderr(
                predicate::str::contains(flag).or(predicate::str::contains("unexpected argument")),
            );
    }
}

#[test]
fn l4_skill_list_warns_on_zen_without_agents_still_lists() {
    let ws = Workspace::new();
    let project = ws.project("app");
    ws.cmd(&project)
        .args(["init", "--no-zen", "--no-manage-tink"])
        .assert()
        .success();
    let source = ws.root.join("demo-skill");
    write_skill(&source, "demo-skill", "listed despite zen warning");
    ws.cmd(&project)
        .args(["skill", "add", source.to_str().unwrap()])
        .assert()
        .success();
    fs::write(project.join("ZEN.md"), "# Zen\n").unwrap();

    ws.cmd(&project)
        .args(["skill", "list"])
        .assert()
        .success()
        .stdout(predicate::str::contains("demo-skill"))
        .stderr(predicate::str::contains(
            "ZEN.md is not referenced by a regular AGENTS.md",
        ));
    ws.cmd(&project)
        .args(["skill", "check"])
        .assert()
        .failure()
        .stderr(predicate::str::contains(
            "ZEN.md is not referenced by a regular AGENTS.md",
        ));
}

#[test]
fn l7_skill_list_and_check_reject_nested_symlink_drift() {
    let ws = Workspace::new();
    let project = ws.project("app");
    ws.cmd(&project)
        .args(["init", "--no-zen", "--no-tink-skills", "--no-manage-tink"])
        .assert()
        .success();
    let source = ws.root.join("demo-skill");
    write_skill(&source, "demo-skill", "safe before local drift");
    ws.cmd(&project)
        .args(["skill", "add", source.to_str().unwrap()])
        .assert()
        .success();

    let outside = ws.root.join("outside.txt");
    fs::write(&outside, "outside\n").unwrap();
    let link = Workspace::skill_path(&project, "demo-skill").join("escape");
    std::os::unix::fs::symlink(&outside, &link).unwrap();

    for command in [["skill", "list"], ["skill", "check"]] {
        ws.cmd(&project)
            .args(command)
            .assert()
            .failure()
            .stderr(predicate::str::contains("symlink"));
    }
    assert!(link.is_symlink());
    assert_eq!(fs::read_to_string(&outside).unwrap(), "outside\n");
}

#[test]
fn l8_skill_list_refuses_unmarked_existing_home_without_writes() {
    let root = TempDir::new().unwrap();
    let project = root.path().join("project");
    let home = root.path().join("unrelated-home");
    fs::create_dir_all(&project).unwrap();
    fs::create_dir_all(&home).unwrap();
    let readme = home.join("README.md");
    fs::write(&readme, "# Important unrelated directory\n").unwrap();
    let before = fs::read(&readme).unwrap();

    for mode in ["--library", "--catalog"] {
        Command::cargo_bin("tink")
            .unwrap()
            .current_dir(&project)
            .env("TINK_HOME", &home)
            .args(["skill", "list", mode])
            .assert()
            .failure()
            .stderr(predicate::str::contains("Tink home"));
    }

    assert_eq!(fs::read(&readme).unwrap(), before);
    assert_eq!(fs::read_dir(&home).unwrap().count(), 1);
}

#[test]
fn l9_skill_list_refuses_symlinked_home_owner_directories() {
    for (owner, mode) in [("skills", "--library"), ("catalog", "--catalog")] {
        let ws = Workspace::new();
        let project = ws.project("app");
        ws.initialize_inventory();
        let owned = ws.inventory.join(owner);
        let outside = ws.root.join(format!("outside-{owner}"));
        fs::rename(&owned, &outside).unwrap();
        std::os::unix::fs::symlink(&outside, &owned).unwrap();

        ws.cmd(&project)
            .args(["skill", "list", mode])
            .assert()
            .failure()
            .stderr(predicate::str::contains("symlink"));
        assert!(owned.is_symlink());
        assert!(outside.is_dir());
    }
}

#[test]
fn l10_catalog_distinguishes_projects_with_the_same_basename() {
    let ws = Workspace::new();
    let first = ws.root.join("first/app");
    let second = ws.root.join("second/app");
    fs::create_dir_all(&first).unwrap();
    fs::create_dir_all(&second).unwrap();
    for project in [&first, &second] {
        ws.cmd(project)
            .args(["init", "--no-zen", "--no-tink-skills", "--no-manage-tink"])
            .assert()
            .success();
    }
    let alpha = ws.root.join("alpha");
    let beta = ws.root.join("beta");
    write_skill(&alpha, "alpha", "first project");
    write_skill(&beta, "beta", "second project");
    ws.cmd(&first)
        .args(["skill", "add", alpha.to_str().unwrap()])
        .assert()
        .success();
    ws.cmd(&second)
        .args(["skill", "add", beta.to_str().unwrap()])
        .assert()
        .success();

    let output = ws
        .cmd(&first)
        .args(["skill", "list", "--catalog"])
        .output()
        .unwrap();
    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains(&format!(
        "app\t{}\talpha",
        first.canonicalize().unwrap().display()
    )));
    assert!(stdout.contains(&format!(
        "app\t{}\tbeta",
        second.canonicalize().unwrap().display()
    )));
    assert_eq!(
        fs::read_dir(ws.inventory.join("catalog/by-project"))
            .unwrap()
            .count(),
        2
    );
}

// --- H*: library ---

#[test]
fn h1_skill_list_library_includes_archived_names() {
    let ws = Workspace::new();
    let project = ws.project("app");
    ws.cmd(&project)
        .args(["init", "--no-zen", "--no-tink-skills", "--no-manage-tink"])
        .assert()
        .success();
    let source = ws.root.join("demo-skill");
    write_skill(&source, "demo-skill", "stashed");
    ws.cmd(&project)
        .args(["skill", "add", source.to_str().unwrap()])
        .assert()
        .success();
    ws.cmd(&project)
        .args(["skill", "list", "--library"])
        .assert()
        .success()
        .stdout(predicate::str::contains("demo-skill"));
}

#[test]
fn h10_skill_list_library_refuses_nested_symlink() {
    let ws = Workspace::new();
    let project = ws.project("app");
    ws.initialize_inventory();
    let skill = ws.library_skill("unsafe-skill");
    write_skill(
        &skill,
        "unsafe-skill",
        "valid manifest with unsafe nested content",
    );
    let outside = ws.root.join("outside-library.txt");
    fs::write(&outside, "outside stays unchanged\n").unwrap();
    let link = skill.join("escape");
    std::os::unix::fs::symlink(&outside, &link).unwrap();
    let before = fs::read(&outside).unwrap();

    ws.cmd(&project)
        .args(["skill", "list", "--library"])
        .assert()
        .failure()
        .stdout(predicate::str::contains("unsafe-skill").not())
        .stderr(predicate::str::contains("symlink"));

    assert!(link.is_symlink());
    assert_eq!(fs::read(&outside).unwrap(), before);
}

#[test]
fn h2_skill_add_library_installs_into_project() {
    let ws = Workspace::new();
    let donor = ws.project("donor");
    let app = ws.project("app");
    ws.cmd(&donor)
        .args(["init", "--no-zen", "--no-tink-skills", "--no-manage-tink"])
        .assert()
        .success();
    let source = ws.root.join("stash-skill");
    write_skill(&source, "stash-skill", "from stash");
    ws.cmd(&donor)
        .args(["skill", "add", source.to_str().unwrap()])
        .assert()
        .success();
    assert!(ws.library_skill("stash-skill").join("SKILL.md").is_file());

    ws.cmd(&app)
        .args(["init", "--no-zen", "--no-tink-skills", "--no-manage-tink"])
        .assert()
        .success();
    ws.cmd(&app)
        .args(["skill", "add", "stash-skill"])
        .assert()
        .success()
        .stdout(
            predicate::str::contains("stash-skill").and(predicate::str::contains("from library")),
        );
    assert!(
        Workspace::skill_path(&app, "stash-skill")
            .join("SKILL.md")
            .is_file()
    );
    ws.assert_cataloged("app", "stash-skill");
}

#[test]
fn h3_skill_add_library_missing_refuses_without_github() {
    let ws = Workspace::new();
    let project = ws.project("app");
    ws.cmd(&project)
        .args(["init", "--no-zen", "--no-tink-skills", "--no-manage-tink"])
        .assert()
        .success();
    ws.cmd(&project)
        .args(["skill", "add", "no-such-skill"])
        .assert()
        .failure()
        .stderr(
            predicate::str::contains("Library skill not found")
                .or(predicate::str::contains("not found")),
        );
}

#[test]
fn h4_skill_add_library_refuses_overwrite_when_diverged() {
    let ws = Workspace::new();
    let donor = ws.project("donor");
    let app = ws.project("app");
    ws.cmd(&donor)
        .args(["init", "--no-zen", "--no-tink-skills", "--no-manage-tink"])
        .assert()
        .success();
    let source = ws.root.join("demo-skill");
    write_skill(&source, "demo-skill", "stash body");
    ws.cmd(&donor)
        .args(["skill", "add", source.to_str().unwrap()])
        .assert()
        .success();

    ws.cmd(&app)
        .args(["init", "--no-zen", "--no-tink-skills", "--no-manage-tink"])
        .assert()
        .success();
    write_skill(
        &Workspace::skill_path(&app, "demo-skill"),
        "demo-skill",
        "project local body",
    );
    ws.cmd(&app)
        .args(["skill", "add", "demo-skill"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("Refusing to overwrite"));
    let body =
        fs::read_to_string(Workspace::skill_path(&app, "demo-skill").join("SKILL.md")).unwrap();
    assert!(body.contains("project local body"));
}

#[test]
fn h5_skill_harvest_copies_harness_skills_into_library() {
    let ws = Workspace::new();
    let home = ws.root.join("home");
    let project = ws.project("app");
    write_skill(
        &home.join(".agents").join("skills").join("agents-skill"),
        "agents-skill",
        "from agents",
    );
    write_skill(
        &home.join(".claude").join("skills").join("claude-skill"),
        "claude-skill",
        "from claude",
    );
    // Nested under codex (recursive).
    write_skill(
        &home
            .join(".codex")
            .join("skills")
            .join("workflows")
            .join("nested-skill"),
        "nested-skill",
        "from codex nest",
    );

    ws.cmd(&project)
        .env("HOME", &home)
        .args(["skill", "harvest"])
        .assert()
        .success()
        .stdout(
            predicate::str::contains("Harvested")
                .and(predicate::str::contains("agents-skill"))
                .and(predicate::str::contains("claude-skill"))
                .and(predicate::str::contains("nested-skill")),
        );

    assert!(ws.library_skill("agents-skill").join("SKILL.md").is_file());
    assert!(ws.library_skill("claude-skill").join("SKILL.md").is_file());
    assert!(ws.library_skill("nested-skill").join("SKILL.md").is_file());
    assert!(
        !project.join(".agents").exists(),
        "harvest must not create project .agents"
    );
}

#[test]
fn h6_skill_harvest_identical_is_unchanged() {
    let ws = Workspace::new();
    let home = ws.root.join("home");
    let project = ws.project("app");
    let skill = home.join(".agents").join("skills").join("same-skill");
    write_skill(&skill, "same-skill", "body");
    ws.cmd(&project)
        .env("HOME", &home)
        .args(["skill", "harvest"])
        .assert()
        .success();
    ws.cmd(&project)
        .env("HOME", &home)
        .args(["skill", "harvest"])
        .assert()
        .success()
        .stdout(
            predicate::str::contains("already present")
                .and(predicate::str::contains("0 harvested"))
                .and(predicate::str::contains("same-skill").not()),
        );
}

#[test]
fn h7_skill_harvest_divergent_skips_without_repair() {
    let ws = Workspace::new();
    let home = ws.root.join("home");
    let project = ws.project("app");
    ws.initialize_inventory();
    write_skill(
        &home.join(".agents").join("skills").join("demo-skill"),
        "demo-skill",
        "harness body",
    );
    // Pre-seed divergent library.
    write_skill(
        ws.library_skill("demo-skill").as_path(),
        "demo-skill",
        "stash body",
    );
    let before = fs::read_to_string(ws.library_skill("demo-skill").join("SKILL.md")).unwrap();

    ws.cmd(&project)
        .env("HOME", &home)
        .args(["skill", "harvest"])
        .assert()
        .success()
        .stderr(predicate::str::contains("Skipped").and(predicate::str::contains("demo-skill")));

    let after = fs::read_to_string(ws.library_skill("demo-skill").join("SKILL.md")).unwrap();
    assert_eq!(
        before, after,
        "create-only must not repair divergent library"
    );
    assert!(after.contains("stash body"));
}

#[test]
fn h8_skill_harvest_skips_tink_home_and_unsafe_trees() {
    let ws = Workspace::new();
    let home = ws.root.join("home");
    ws.initialize_inventory();
    // Project lives under TINK_HOME but outside skills/ — must still harvest.
    let project = ws.inventory.join("workspace");
    fs::create_dir_all(&project).unwrap();
    write_skill(
        &home.join(".agents").join("skills").join("good-skill"),
        "good-skill",
        "ok",
    );
    write_skill(
        &project
            .join(".agents")
            .join("skills")
            .join("nested-home-skill"),
        "nested-home-skill",
        "under tink home workspace",
    );
    // Unsafe: symlink inside skill tree.
    let bad = home.join(".claude").join("skills").join("bad-skill");
    write_skill(&bad, "bad-skill", "has link");
    std::os::unix::fs::symlink("/tmp", bad.join("link-out")).unwrap();

    ws.cmd(&project)
        .env("HOME", &home)
        .args(["init", "--no-zen", "--no-tink-skills", "--no-manage-tink"])
        .assert()
        .success();
    write_skill(
        ws.library_skill("from-home").as_path(),
        "from-home",
        "stash resident",
    );
    // Harness entry that realpaths into the library — must be skipped as a source.
    fs::create_dir_all(home.join(".cursor").join("skills")).unwrap();
    std::os::unix::fs::symlink(
        ws.library_skill("from-home"),
        home.join(".cursor").join("skills").join("from-home"),
    )
    .unwrap();

    ws.cmd(&project)
        .env("HOME", &home)
        .args(["skill", "harvest"])
        .assert()
        .success()
        .stdout(
            predicate::str::contains("good-skill")
                .and(predicate::str::contains("nested-home-skill")),
        )
        .stderr(predicate::str::contains("bad-skill").and(predicate::str::contains("from-home")));

    assert!(ws.library_skill("good-skill").join("SKILL.md").is_file());
    assert!(
        ws.library_skill("nested-home-skill")
            .join("SKILL.md")
            .is_file()
    );
    assert!(!ws.library_skill("bad-skill").exists());
    let home_body = fs::read_to_string(ws.library_skill("from-home").join("SKILL.md")).unwrap();
    assert!(home_body.contains("stash resident"));
}

#[cfg(unix)]
#[test]
fn h14_harvest_skips_receipt_owned_sources_without_library_publication() {
    for dangling_receipt in [false, true] {
        let ws = Workspace::new();
        let home = ws.root.join("home");
        let project = ws.project("app");
        let source = home.join(".agents").join("skills").join("common-skillset");
        write_skill(
            &source,
            "common-skillset",
            "receipt ownership boundary fixture",
        );
        let receipt = source.join(".tink-skillset.json");
        if dangling_receipt {
            std::os::unix::fs::symlink(source.join("missing-receipt"), &receipt).unwrap();
        } else {
            fs::write(&receipt, "{}\n").unwrap();
        }

        ws.cmd(&project)
            .env("HOME", &home)
            .args(["skill", "harvest"])
            .assert()
            .success()
            .stdout(predicate::str::contains("0 harvested"))
            .stderr(
                predicate::str::contains("Skipped")
                    .and(predicate::str::contains("common-skillset"))
                    .and(predicate::str::contains(
                        "tink skillset add common-skillset",
                    )),
            );

        assert!(!ws.library_skill("common-skillset").exists());
        assert!(!project.join(".agents").exists());
    }
}

#[test]
fn h9_completion_offers_current_library_matches_without_creating_home() {
    let ws = Workspace::new();
    let project = ws.project("app");

    ws.cmd(&project)
        .env("COMPLETE", "zsh")
        .env("_CLAP_COMPLETE_INDEX", "3")
        .env("_CLAP_IFS", "\n")
        .args(["--", "tink", "skill", "add", "de"])
        .assert()
        .success()
        .stdout(predicate::str::is_empty());
    assert!(
        !ws.inventory.exists(),
        "completion must not create the Tink home"
    );

    write_skill(
        project.join("demo-local").as_path(),
        "demo-local",
        "local path fixture",
    );
    ws.cmd(&project)
        .env("COMPLETE", "zsh")
        .env("_CLAP_COMPLETE_INDEX", "3")
        .env("_CLAP_IFS", "\n")
        .args(["--", "tink", "skill", "add", "./de"])
        .assert()
        .success()
        .stdout(predicate::str::contains("./demo-local/"));

    ws.cmd(&project)
        .env("COMPLETE", "zsh")
        .env("_CLAP_COMPLETE_INDEX", "1")
        .env("_CLAP_IFS", "\n")
        .args(["--", "tink", "sk"])
        .assert()
        .success()
        .stdout(predicate::str::contains("skill"));

    ws.cmd(&project)
        .env("COMPLETE", "zsh")
        .env("_CLAP_COMPLETE_INDEX", "4")
        .env("_CLAP_IFS", "\n")
        .args(["--", "tink", "skill", "add", "demo-local", "--s"])
        .assert()
        .success()
        .stdout(predicate::str::contains("--skill"));

    ws.initialize_inventory();
    write_skill(
        ws.library_skill("demo-skill").as_path(),
        "demo-skill",
        "completion fixture",
    );
    write_skill(
        ws.library_skill("other-skill").as_path(),
        "other-skill",
        "non-matching fixture",
    );

    ws.cmd(&project)
        .env("COMPLETE", "zsh")
        .env("_CLAP_COMPLETE_INDEX", "3")
        .env("_CLAP_IFS", "\n")
        .args(["--", "tink", "skill", "add", "de"])
        .assert()
        .success()
        .stdout(
            predicate::str::contains("demo-skill")
                .and(predicate::str::contains("other-skill").not()),
        );
}

#[test]
fn h11_receipt_bearing_library_root_is_not_a_standalone_skill() {
    let ws = Workspace::new();
    let project = ws.project("app");
    ws.cmd(&project)
        .args(["init", "--no-zen", "--no-tink-skills", "--no-manage-tink"])
        .assert()
        .success();

    let root = ws.library_skill("bundle-skillset");
    write_skill(
        &root,
        "bundle-skillset",
        "root skill must not override skillset ownership",
    );
    fs::write(root.join(".tink-skillset.json"), "{}\n").unwrap();

    ws.cmd(&project)
        .args(["skill", "list", "--library"])
        .assert()
        .success()
        .stdout(predicate::str::contains("bundle-skillset").not());

    ws.cmd(&project)
        .args(["skill", "add", "bundle-skillset"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("Library entry is a skillset").and(
            predicate::str::contains("tink skillset add bundle-skillset"),
        ));
    assert!(!Workspace::skill_path(&project, "bundle-skillset").exists());
}

#[test]
fn h12_standalone_add_preserves_receipt_bearing_library_root() {
    let ws = Workspace::new();
    let project = ws.project("app");
    ws.cmd(&project)
        .args(["init", "--no-zen", "--no-tink-skills", "--no-manage-tink"])
        .assert()
        .success();

    let library_root = ws.library_skill("bundle-skillset");
    write_skill(&library_root, "bundle-skillset", "managed root");
    fs::write(library_root.join(".tink-skillset.json"), "owned receipt\n").unwrap();
    fs::write(library_root.join("keep.txt"), "preserve me\n").unwrap();
    let before_skill = fs::read(library_root.join("SKILL.md")).unwrap();
    let before_receipt = fs::read(library_root.join(".tink-skillset.json")).unwrap();
    let before_keep = fs::read(library_root.join("keep.txt")).unwrap();

    let source = ws.root.join("incoming-skill");
    write_skill(&source, "bundle-skillset", "standalone collision");
    ws.cmd(&project)
        .args(["skill", "add", source.to_str().unwrap()])
        .assert()
        .failure()
        .stderr(predicate::str::contains("Library entry is a skillset").and(
            predicate::str::contains("refusing standalone skill collision"),
        ));

    assert_eq!(
        fs::read(library_root.join("SKILL.md")).unwrap(),
        before_skill
    );
    assert_eq!(
        fs::read(library_root.join(".tink-skillset.json")).unwrap(),
        before_receipt
    );
    assert_eq!(
        fs::read(library_root.join("keep.txt")).unwrap(),
        before_keep
    );
    assert!(!Workspace::skill_path(&project, "bundle-skillset").exists());
}

#[test]
fn h13_exact_cache_match_does_not_publish_skillset_as_standalone() {
    let ws = Workspace::new();
    let project = ws.project("app");
    ws.cmd(&project)
        .args(["init", "--no-zen", "--no-tink-skills", "--no-manage-tink"])
        .assert()
        .success();

    let library_root = ws.library_skill("bundle-skillset");
    write_skill(&library_root, "bundle-skillset", "identical managed root");
    fs::write(library_root.join(".tink-skillset.json"), "owned receipt\n").unwrap();
    let before_skill = fs::read(library_root.join("SKILL.md")).unwrap();
    let before_receipt = fs::read(library_root.join(".tink-skillset.json")).unwrap();

    let source = ws.root.join("identical-source");
    write_skill(&source, "bundle-skillset", "identical managed root");
    fs::write(source.join(".tink-skillset.json"), "owned receipt\n").unwrap();

    ws.cmd(&project)
        .args(["skill", "add", source.to_str().unwrap()])
        .assert()
        .failure()
        .stderr(
            predicate::str::contains("Source is owned by a skillset").and(
                predicate::str::contains("tink skillset add bundle-skillset"),
            ),
        );

    assert_eq!(
        fs::read(library_root.join("SKILL.md")).unwrap(),
        before_skill
    );
    assert_eq!(
        fs::read(library_root.join(".tink-skillset.json")).unwrap(),
        before_receipt
    );
    assert!(!Workspace::skill_path(&project, "bundle-skillset").exists());
}

// --- V*: CLI surface (skill nest) ---

#[test]
fn v1_skill_add_installs_local_skill() {
    let ws = Workspace::new();
    let project = ws.project("app");
    ws.cmd(&project)
        .args(["init", "--no-manage-tink"])
        .assert()
        .success();
    let source = ws.root.join("demo-skill");
    write_skill(&source, "demo-skill", "via skill add");
    ws.cmd(&project)
        .args(["skill", "add", source.to_str().unwrap()])
        .assert()
        .success();
    assert!(
        Workspace::skill_path(&project, "demo-skill")
            .join("SKILL.md")
            .is_file()
    );
}

#[test]
fn v2_skill_check_passes_valid_project() {
    let ws = Workspace::new();
    let project = ws.project("app");
    ws.cmd(&project).arg("init").assert().success();
    ws.cmd(&project)
        .args(["skill", "check"])
        .assert()
        .success()
        .stdout(predicate::str::contains("OK"));
}

// --- P*: refresh ---

#[test]
fn p1_refresh_updates_clean_import() {
    let ws = Workspace::new();
    let project = ws.project("app");
    ws.cmd(&project).arg("init").assert().success();

    let remote = ws.root.join("remote-repo");
    init_repo(&remote);
    write_skill(
        &remote.join("skills").join("remote-skill"),
        "remote-skill",
        "v1",
    );
    commit_all(&remote, "v1");
    let public = "https://github.com/example/skills.git";
    let redirect = github_redirect(&remote, public);

    let mut add = ws.cmd(&project);
    add.args(["skill", "add", "example/skills", "--skill", "remote-skill"]);
    for (k, v) in &redirect {
        add.env(k, v);
    }
    add.assert().success();

    write_skill(
        &remote.join("skills").join("remote-skill"),
        "remote-skill",
        "v2",
    );
    let new_rev = commit_all(&remote, "v2");

    let mut refresh = ws.cmd(&project);
    refresh.args(["skill", "refresh", "remote-skill"]);
    for (k, v) in &redirect {
        refresh.env(k, v);
    }
    refresh.assert().success();

    let skill_md =
        fs::read_to_string(Workspace::skill_path(&project, "remote-skill").join("SKILL.md"))
            .unwrap();
    assert!(skill_md.contains("v2"));
    let receipt = fs::read_to_string(
        Workspace::skill_path(&project, "remote-skill").join(".tink-source.json"),
    )
    .unwrap();
    assert!(receipt.contains(&new_rev));
}

#[test]
fn p2_refresh_refuses_local_modifications() {
    let ws = Workspace::new();
    let project = ws.project("app");
    ws.cmd(&project).arg("init").assert().success();

    let remote = ws.root.join("remote-repo");
    init_repo(&remote);
    write_skill(
        &remote.join("skills").join("remote-skill"),
        "remote-skill",
        "v1",
    );
    commit_all(&remote, "v1");
    let public = "https://github.com/example/skills.git";
    let redirect = github_redirect(&remote, public);

    let mut add = ws.cmd(&project);
    add.args(["skill", "add", "example/skills", "--skill", "remote-skill"]);
    for (k, v) in &redirect {
        add.env(k, v);
    }
    add.assert().success();

    let skill_md = Workspace::skill_path(&project, "remote-skill").join("SKILL.md");
    let mut body = fs::read_to_string(&skill_md).unwrap();
    body.push_str("\ndirty\n");
    fs::write(&skill_md, body).unwrap();

    let mut refresh = ws.cmd(&project);
    refresh.args(["skill", "refresh", "remote-skill"]);
    for (k, v) in &redirect {
        refresh.env(k, v);
    }
    refresh
        .assert()
        .failure()
        .stderr(predicate::str::contains("local modifications"));
}

#[test]
fn p3_refresh_backfills_missing_library_on_noop() {
    let ws = Workspace::new();
    let project = ws.project("app");
    ws.cmd(&project).arg("init").assert().success();

    let remote = ws.root.join("remote-repo");
    init_repo(&remote);
    write_skill(
        &remote.join("skills").join("remote-skill"),
        "remote-skill",
        "v1",
    );
    commit_all(&remote, "v1");
    let public = "https://github.com/example/skills.git";
    let redirect = github_redirect(&remote, public);

    let mut add = ws.cmd(&project);
    add.args(["skill", "add", "example/skills", "--skill", "remote-skill"]);
    for (k, v) in &redirect {
        add.env(k, v);
    }
    add.assert().success();
    fs::remove_dir_all(ws.library_skill("remote-skill")).unwrap();

    let mut refresh = ws.cmd(&project);
    refresh.args(["skill", "refresh", "remote-skill"]);
    for (k, v) in &redirect {
        refresh.env(k, v);
    }
    refresh.assert().success();
    assert!(ws.library_skill("remote-skill").join("SKILL.md").is_file());
}

#[test]
fn p4_refresh_allows_archive_missing_only_receipt() {
    let ws = Workspace::new();
    let project = ws.project("app");
    ws.cmd(&project).arg("init").assert().success();

    let remote = ws.root.join("remote-repo");
    init_repo(&remote);
    write_skill(
        &remote.join("skills").join("remote-skill"),
        "remote-skill",
        "v1",
    );
    commit_all(&remote, "v1");
    let public = "https://github.com/example/skills.git";
    let redirect = github_redirect(&remote, public);

    let mut add = ws.cmd(&project);
    add.args(["skill", "add", "example/skills", "--skill", "remote-skill"]);
    for (k, v) in &redirect {
        add.env(k, v);
    }
    add.assert().success();
    fs::remove_file(ws.library_skill("remote-skill").join(".tink-source.json")).unwrap();

    write_skill(
        &remote.join("skills").join("remote-skill"),
        "remote-skill",
        "v2",
    );
    commit_all(&remote, "v2");

    let mut refresh = ws.cmd(&project);
    refresh.args(["skill", "refresh", "remote-skill"]);
    for (k, v) in &redirect {
        refresh.env(k, v);
    }
    refresh.assert().success();
    assert!(
        fs::read_to_string(ws.library_skill("remote-skill").join("SKILL.md"))
            .unwrap()
            .contains("v2")
    );
}

#[test]
fn p5_refresh_refuses_divergent_library() {
    let ws = Workspace::new();
    let project = ws.project("app");
    ws.cmd(&project).arg("init").assert().success();

    let remote = ws.root.join("remote-repo");
    init_repo(&remote);
    write_skill(
        &remote.join("skills").join("remote-skill"),
        "remote-skill",
        "v1",
    );
    commit_all(&remote, "v1");
    let public = "https://github.com/example/skills.git";
    let redirect = github_redirect(&remote, public);

    let mut add = ws.cmd(&project);
    add.args(["skill", "add", "example/skills", "--skill", "remote-skill"]);
    for (k, v) in &redirect {
        add.env(k, v);
    }
    add.assert().success();
    write_skill(
        ws.library_skill("remote-skill").as_path(),
        "remote-skill",
        "other",
    );

    write_skill(
        &remote.join("skills").join("remote-skill"),
        "remote-skill",
        "v2",
    );
    commit_all(&remote, "v2");

    let before =
        fs::read_to_string(Workspace::skill_path(&project, "remote-skill").join("SKILL.md"))
            .unwrap();
    let mut refresh = ws.cmd(&project);
    refresh.args(["skill", "refresh", "remote-skill"]);
    for (k, v) in &redirect {
        refresh.env(k, v);
    }
    refresh
        .assert()
        .failure()
        .stderr(predicate::str::contains("library diverges"));
    let after =
        fs::read_to_string(Workspace::skill_path(&project, "remote-skill").join("SKILL.md"))
            .unwrap();
    assert_eq!(before, after);
}

#[test]
fn p7_refresh_repairs_stale_archive_when_project_already_new() {
    let ws = Workspace::new();
    let project = ws.project("app");
    ws.cmd(&project).arg("init").assert().success();

    let remote = ws.root.join("remote-repo");
    init_repo(&remote);
    write_skill(
        &remote.join("skills").join("remote-skill"),
        "remote-skill",
        "v1",
    );
    commit_all(&remote, "v1");
    let public = "https://github.com/example/skills.git";
    let redirect = github_redirect(&remote, public);

    let mut add = ws.cmd(&project);
    add.args(["skill", "add", "example/skills", "--skill", "remote-skill"]);
    for (k, v) in &redirect {
        add.env(k, v);
    }
    add.assert().success();

    write_skill(
        &remote.join("skills").join("remote-skill"),
        "remote-skill",
        "v2",
    );
    commit_all(&remote, "v2");

    let mut refresh = ws.cmd(&project);
    refresh.args(["skill", "refresh", "remote-skill"]);
    for (k, v) in &redirect {
        refresh.env(k, v);
    }
    refresh.assert().success();

    // Simulate failed library deposit: project at v2, library rolled back to v1.
    write_skill(
        ws.library_skill("remote-skill").as_path(),
        "remote-skill",
        "v1",
    );

    let mut repair = ws.cmd(&project);
    repair.args(["skill", "refresh", "remote-skill"]);
    for (k, v) in &redirect {
        repair.env(k, v);
    }
    repair.assert().success();
    assert!(
        fs::read_to_string(ws.library_skill("remote-skill").join("SKILL.md"))
            .unwrap()
            .contains("v2"),
        "noop refresh must repair stale library from project"
    );
}

#[test]
fn p6_refresh_identical_tree_new_revision_bumps_receipts() {
    let ws = Workspace::new();
    let project = ws.project("app");
    ws.cmd(&project).arg("init").assert().success();

    let remote = ws.root.join("remote-repo");
    init_repo(&remote);
    write_skill(
        &remote.join("skills").join("remote-skill"),
        "remote-skill",
        "v1",
    );
    commit_all(&remote, "v1");
    let public = "https://github.com/example/skills.git";
    let redirect = github_redirect(&remote, public);

    let mut add = ws.cmd(&project);
    add.args(["skill", "add", "example/skills", "--skill", "remote-skill"]);
    for (k, v) in &redirect {
        add.env(k, v);
    }
    add.assert().success();

    let new_rev = {
        git(&remote, &["commit", "--allow-empty", "-qm", "empty"]);
        let output = StdCommand::new("git")
            .args(["rev-parse", "HEAD"])
            .current_dir(&remote)
            .output()
            .unwrap();
        String::from_utf8(output.stdout).unwrap().trim().to_string()
    };

    let mut refresh = ws.cmd(&project);
    refresh.args(["skill", "refresh", "remote-skill"]);
    for (k, v) in &redirect {
        refresh.env(k, v);
    }
    refresh.assert().success();

    let project_receipt = fs::read_to_string(
        Workspace::skill_path(&project, "remote-skill").join(".tink-source.json"),
    )
    .unwrap();
    assert!(project_receipt.contains(&new_rev));
    let archive_receipt =
        fs::read_to_string(ws.library_skill("remote-skill").join(".tink-source.json")).unwrap();
    assert!(archive_receipt.contains(&new_rev));
}

#[test]
fn p8_refresh_all_preflights_before_updating_any_skill() {
    let ws = Workspace::new();
    let project = ws.project("app");
    ws.cmd(&project).arg("init").assert().success();

    let remote = ws.root.join("remote-repo");
    init_repo(&remote);
    write_skill(&remote.join("skills/aaa-clean"), "aaa-clean", "v1");
    write_skill(&remote.join("skills/zzz-dirty"), "zzz-dirty", "v1");
    commit_all(&remote, "v1");
    let public = "https://github.com/example/skills.git";
    let redirect = github_redirect(&remote, public);

    for name in ["aaa-clean", "zzz-dirty"] {
        let mut add = ws.cmd(&project);
        add.args(["skill", "add", "example/skills", "--skill", name]);
        for (k, v) in &redirect {
            add.env(k, v);
        }
        add.assert().success();
    }

    write_skill(&remote.join("skills/aaa-clean"), "aaa-clean", "v2");
    write_skill(&remote.join("skills/zzz-dirty"), "zzz-dirty", "v2");
    commit_all(&remote, "v2");

    let dirty = Workspace::skill_path(&project, "zzz-dirty").join("SKILL.md");
    let mut body = fs::read_to_string(&dirty).unwrap();
    body.push_str("\nlocal edit\n");
    fs::write(&dirty, body).unwrap();

    let mut refresh = ws.cmd(&project);
    refresh.args(["skill", "refresh"]);
    for (k, v) in &redirect {
        refresh.env(k, v);
    }
    refresh
        .assert()
        .failure()
        .stderr(predicate::str::contains("local modifications"));

    let clean =
        fs::read_to_string(Workspace::skill_path(&project, "aaa-clean").join("SKILL.md")).unwrap();
    assert!(clean.contains("v1"), "earlier skill must remain at v1");
    assert!(
        !clean.contains("v2"),
        "refresh-all must not partially apply"
    );
    assert!(
        fs::read_to_string(dirty).unwrap().contains("local edit"),
        "dirty skill must remain untouched"
    );
}

#[test]
fn p9_refresh_manage_tink_installs_missing_embedded_copy() {
    let ws = Workspace::new();
    let project = ws.project("app");
    ws.cmd(&project)
        .args(["init", "--no-zen", "--no-tink-skills", "--no-manage-tink"])
        .assert()
        .success();

    ws.cmd(&project)
        .args(["skill", "refresh", "manage-tink"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Installed manage-tink"));

    ws.cmd(&project).args(["skill", "check"]).assert().success();
    ws.assert_cataloged("app", "manage-tink");
}

#[test]
fn p10_refresh_manage_tink_reports_current_copy_unchanged() {
    let ws = Workspace::new();
    let project = ws.project("app");
    ws.cmd(&project).arg("init").assert().success();

    let installed = Workspace::skill_path(&project, "manage-tink");
    let before = fs::read(installed.join("SKILL.md")).expect("installed SKILL.md");

    ws.cmd(&project)
        .args(["skill", "refresh", "manage-tink"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Unchanged manage-tink"));

    assert_eq!(
        fs::read(installed.join("SKILL.md")).expect("refreshed SKILL.md"),
        before
    );
    ws.cmd(&project).args(["skill", "check"]).assert().success();
    ws.assert_cataloged("app", "manage-tink");
}

#[test]
fn p11_refresh_manage_tink_replaces_differing_reserved_copy() {
    let ws = Workspace::new();
    let project = ws.project("app");
    ws.cmd(&project).arg("init").assert().success();

    let installed = Workspace::skill_path(&project, "manage-tink");
    let commands = installed.join("references/commands.md");
    let embedded = fs::read(&commands).expect("embedded commands reference");
    fs::write(&commands, "local edit to reserved embedded contents\n")
        .expect("modify embedded commands reference");

    ws.cmd(&project)
        .args(["skill", "refresh", "manage-tink"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Refreshed manage-tink"));

    assert_eq!(fs::read(&commands).expect("refreshed commands"), embedded);
    assert_eq!(
        fs::read(
            ws.library_skill("manage-tink")
                .join("references/commands.md")
        )
        .expect("library commands"),
        embedded
    );
    ws.cmd(&project).args(["skill", "check"]).assert().success();
    ws.assert_cataloged("app", "manage-tink");
}

#[test]
fn p12_refresh_manage_tink_refuses_remote_provenance_collision() {
    let ws = Workspace::new();
    let project = ws.project("app");
    ws.cmd(&project)
        .args(["init", "--no-zen", "--no-tink-skills", "--no-manage-tink"])
        .assert()
        .success();

    let remote = ws.root.join("remote-manage-tink");
    init_repo(&remote);
    write_skill(&remote, "manage-tink", "remote user-owned contents");
    commit_all(&remote, "remote manage-tink");
    let public = "https://github.com/example/remote-manage-tink.git";
    let mut add = ws.cmd(&project);
    add.args(["skill", "add", "example/remote-manage-tink"]);
    add.envs(github_redirect(&remote, public));
    add.assert().success();

    let installed = Workspace::skill_path(&project, "manage-tink");
    let before = fs::read(installed.join("SKILL.md")).expect("remote SKILL.md");
    ws.cmd(&project)
        .args(["skill", "refresh", "manage-tink"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("remote provenance"));
    assert_eq!(
        fs::read(installed.join("SKILL.md")).expect("preserved remote SKILL.md"),
        before
    );
}

// --- D*: destroy ---

#[test]
fn d1_destroy_yes_removes_agents_and_preserves_guidance() {
    let ws = Workspace::new();
    let project = ws.project("app");
    ws.cmd(&project)
        .args(["init", "--with-zen", "--no-tink-skills"])
        .assert()
        .success();
    let source = ws.root.join("extra-skill");
    write_skill(&source, "extra-skill", "also cataloged");
    ws.cmd(&project)
        .args(["skill", "add", source.to_str().unwrap()])
        .assert()
        .success();
    assert!(project.join(".agents").is_dir());
    assert!(project.join("ZEN.md").is_file());
    assert!(project.join("AGENTS.md").is_file());
    let zen_before = fs::read(project.join("ZEN.md")).unwrap();
    let agents_before = fs::read(project.join("AGENTS.md")).unwrap();
    assert!(ws.inventory.join("layout.json").is_file());
    ws.assert_cataloged("app", "manage-tink");
    ws.assert_cataloged("app", "extra-skill");

    ws.cmd(&project)
        .args(["destroy", "--yes"])
        .assert()
        .success();

    assert!(!project.join(".agents").exists());
    assert_eq!(fs::read(project.join("ZEN.md")).unwrap(), zen_before);
    assert_eq!(fs::read(project.join("AGENTS.md")).unwrap(), agents_before);
    assert!(ws.inventory.join("layout.json").is_file());
    assert!(
        ws.library_skill("manage-tink").join("SKILL.md").is_file(),
        "library must remain after destroy"
    );
    assert!(
        ws.library_skill("extra-skill").join("SKILL.md").is_file(),
        "library must remain after destroy"
    );
    ws.cmd(&project)
        .args(["skill", "list", "--catalog"])
        .assert()
        .success()
        .stdout(
            predicate::str::contains("manage-tink")
                .not()
                .and(predicate::str::contains("extra-skill").not()),
        );
}

#[test]
fn d2_destroy_without_yes_refuses_non_tty() {
    let ws = Workspace::new();
    let project = ws.project("app");
    ws.cmd(&project).arg("init").assert().success();
    ws.cmd(&project)
        .arg("destroy")
        .assert()
        .failure()
        .stderr(predicate::str::contains("confirmation").or(predicate::str::contains("--yes")));
    assert!(project.join(".agents").is_dir());
}

#[test]
fn d3_destroy_refuses_agents_symlink() {
    let ws = Workspace::new();
    let project = ws.project("app");
    let real = ws.root.join("real-agents");
    fs::create_dir_all(real.join("skills")).unwrap();
    std::os::unix::fs::symlink(&real, project.join(".agents")).unwrap();
    ws.cmd(&project)
        .args(["destroy", "--yes"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("symlink").or(predicate::str::contains("Symlink")));
    assert!(project.join(".agents").is_symlink());
}

#[test]
fn d4_destroy_refuses_symlinked_catalog_without_external_or_project_writes() {
    let ws = Workspace::new();
    let project = ws.project("app");
    ws.cmd(&project).arg("init").assert().success();
    let catalog = ws.inventory.join("catalog");
    let catalog_meta = ws.catalog_meta("app");
    let catalog_meta_relative = catalog_meta.strip_prefix(&catalog).unwrap().to_path_buf();
    let external = ws.root.join("external-catalog-destroy");
    fs::rename(&catalog, &external).unwrap();
    std::os::unix::fs::symlink(&external, &catalog).unwrap();
    let external_meta = external.join(catalog_meta_relative);
    let catalog_before = fs::read(&external_meta).unwrap();
    let project_before =
        fs::read(Workspace::skill_path(&project, "manage-tink").join("SKILL.md")).unwrap();

    ws.cmd(&project)
        .args(["destroy", "--yes"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("symlink"));

    assert!(catalog.is_symlink());
    assert_eq!(fs::read(&external_meta).unwrap(), catalog_before);
    assert_eq!(
        fs::read(Workspace::skill_path(&project, "manage-tink").join("SKILL.md")).unwrap(),
        project_before
    );
    assert!(project.join(".agents").is_dir());
}

#[test]
fn d5_destroy_preserves_unrelated_agents_siblings() {
    let ws = Workspace::new();
    let project = ws.project("app");
    ws.cmd(&project)
        .args(["init", "--no-zen", "--no-tink-skills"])
        .assert()
        .success();
    let foreign = project.join(".agents/foreign-config.json");
    fs::write(&foreign, b"keep\n").unwrap();

    ws.cmd(&project)
        .args(["destroy", "--yes"])
        .assert()
        .success();

    assert!(!project.join(".agents/skills").exists());
    assert_eq!(fs::read(&foreign).unwrap(), b"keep\n");
    assert!(project.join(".agents").is_dir());
    assert!(!ws.catalog_meta("app").exists());
}

// --- X*: skill remove ---

#[test]
fn x1_remove_deletes_project_skill_keeps_library_drops_catalog() {
    let ws = Workspace::new();
    let project = ws.project("app");
    ws.cmd(&project).arg("init").assert().success();
    let source = ws.root.join("demo-skill");
    write_skill(&source, "demo-skill", "Do the work.");
    ws.cmd(&project)
        .args(["skill", "add", source.to_str().unwrap()])
        .assert()
        .success();
    assert!(Workspace::skill_path(&project, "demo-skill").is_dir());
    ws.assert_cataloged("app", "demo-skill");

    ws.cmd(&project)
        .args(["skill", "remove", "demo-skill"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Removed"));

    assert!(!Workspace::skill_path(&project, "demo-skill").exists());
    ws.cmd(&project)
        .args(["skill", "list"])
        .assert()
        .success()
        .stdout(predicate::str::contains("demo-skill").not());
    assert!(
        ws.library_skill("demo-skill").join("SKILL.md").is_file(),
        "library must remain after project remove"
    );
    ws.cmd(&project)
        .args(["skill", "list", "--catalog"])
        .assert()
        .success()
        .stdout(
            predicate::str::contains("demo-skill")
                .not()
                .and(predicate::str::contains("manage-tink")),
        );
}

#[test]
fn x2_remove_missing_fails() {
    let ws = Workspace::new();
    let project = ws.project("app");
    ws.cmd(&project).arg("init").assert().success();
    ws.cmd(&project)
        .args(["skill", "remove", "missing-skill"])
        .assert()
        .failure()
        .stderr(
            predicate::str::contains("not found")
                .or(predicate::str::contains("missing"))
                .or(predicate::str::contains("Missing")),
        );
    assert!(Workspace::skill_path(&project, "manage-tink").is_dir());
}

#[test]
fn x3_remove_refuses_agents_symlink() {
    let ws = Workspace::new();
    let project = ws.project("app");
    let real = ws.root.join("real-agents");
    fs::create_dir_all(real.join("skills").join("demo-skill")).unwrap();
    write_skill(
        &real.join("skills").join("demo-skill"),
        "demo-skill",
        "body",
    );
    std::os::unix::fs::symlink(&real, project.join(".agents")).unwrap();
    ws.cmd(&project)
        .args(["skill", "remove", "demo-skill"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("symlink").or(predicate::str::contains("Symlink")));
    assert!(project.join(".agents").is_symlink());
    assert!(
        real.join("skills")
            .join("demo-skill")
            .join("SKILL.md")
            .is_file()
    );
}

#[test]
fn x4_remove_does_not_delete_library() {
    let ws = Workspace::new();
    let project = ws.project("app");
    ws.cmd(&project).arg("init").assert().success();
    let source = ws.root.join("keep-stash");
    write_skill(&source, "keep-stash", "body");
    ws.cmd(&project)
        .args(["skill", "add", source.to_str().unwrap()])
        .assert()
        .success();
    let library_md = ws.library_skill("keep-stash").join("SKILL.md");
    let before = fs::read(&library_md).unwrap();
    ws.cmd(&project)
        .args(["skill", "remove", "keep-stash"])
        .assert()
        .success();
    assert!(!Workspace::skill_path(&project, "keep-stash").exists());
    let after = fs::read(&library_md).unwrap();
    assert_eq!(before, after);
}

#[test]
fn x5_manage_tink_documents_remove_and_catalog_sync() {
    let ws = Workspace::new();
    let project = ws.project("app");
    ws.cmd(&project).arg("init").assert().success();
    let skill_md =
        fs::read_to_string(Workspace::skill_path(&project, "manage-tink").join("SKILL.md"))
            .expect("manage-tink SKILL.md");
    let commands = fs::read_to_string(
        Workspace::skill_path(&project, "manage-tink")
            .join("references")
            .join("commands.md"),
    )
    .expect("manage-tink commands.md");

    assert!(
        skill_md.contains("tink skill remove NAME"),
        "manage-tink must authorize skill remove"
    );
    assert!(
        skill_md.contains("tink skill add NAME")
            && skill_md.contains("bare library")
            && !skill_md.contains("add --library"),
        "manage-tink must teach bare-name library promote, not add --library: {skill_md}"
    );
    assert!(
        commands.contains("tink skill add NAME") && !commands.contains("add --library"),
        "commands.md must list bare-name library promote, not add --library: {commands}"
    );
    assert!(
        skill_md.contains("by-project catalog")
            && (skill_md.contains("drops") || skill_md.contains("drop")),
        "manage-tink must state remove/destroy sync the catalog: {skill_md}"
    );
    assert!(
        !skill_md.contains("prune is out of v1"),
        "manage-tink must not claim catalog prune is out of v1"
    );
    assert!(
        commands.contains("tink skill remove NAME"),
        "commands.md must list skill remove"
    );
    assert!(
        commands.contains("catalog")
            && (commands.contains("drops")
                || commands.contains("drop")
                || commands.contains("updates")),
        "commands.md must describe catalog sync on remove/destroy: {commands}"
    );
    for command in [
        "tink inspect GITHUB_URL",
        "tink skill harvest",
        "tink skill lock",
        "tink skill verify",
        "tink skill sync",
        "tink skillset add NAME-skillset",
        "tink skillset list",
        "tink skillset list --library",
        "tink skillset refresh NAME-skillset",
        "tink skillset remove NAME-skillset",
        "tink update",
        "tink destroy",
    ] {
        assert!(
            commands.contains(command),
            "manage-tink commands.md must document {command}: {commands}"
        );
    }
    for contract in [
        "project is authoritative",
        "catalog/by-skillset/NAME-skillset/meta.json",
        ".tink-skillset.json",
        ".agents/skills/NAME-skillset/<member>/SKILL.md",
    ] {
        assert!(
            skill_md.contains(contract),
            "manage-tink SKILL.md must document {contract}: {skill_md}"
        );
    }
    assert!(
        skill_md.contains("no definition-writer command")
            && skill_md.contains("explicitly authorizes")
            && skill_md.contains("does not authorize installing"),
        "manage-tink must bound external skillset-definition authoring: {skill_md}"
    );
    for field in [
        "\"source\":",
        "\"revision\":",
        "\"sourceRoot\":",
        "\"members\":",
    ] {
        assert!(
            commands.contains(field),
            "commands.md must show the complete skillset-definition field {field}: {commands}"
        );
    }
    assert!(
        commands.contains("has no command that writes it")
            && commands.contains("Definition authoring does not authorize"),
        "commands.md must document the external-authoring and install boundary: {commands}"
    );
    for section in [
        "## When to Use",
        "## Inputs",
        "## Procedure",
        "## Validation",
        "## Common Pitfalls",
        "## Related Skills",
    ] {
        assert!(
            skill_md.contains(section),
            "manage-tink SKILL.md must contain {section}: {skill_md}"
        );
    }
    assert!(
        skill_md.contains("**Expected:**") && skill_md.contains("**On failure:**"),
        "manage-tink procedures must state expected and failure behavior: {skill_md}"
    );
    assert!(
        !skill_md.contains("tink skill remove manage-tink && tink init"),
        "manage-tink must not chain re-embedding onto binary updates: {skill_md}"
    );
    assert!(
        skill_md.contains("After any binary update or observed contract mismatch")
            && skill_md.contains("tink destroy --help"),
        "manage-tink must treat every update as possible contract drift: {skill_md}"
    );
    assert!(
        skill_md.contains("current requested shell session")
            && skill_md.contains("exact startup file"),
        "manage-tink must separate session completion from persistent mutation: {skill_md}"
    );
    for proof in [
        "After `init`",
        "After add, promotion, refresh, or sync",
        "After harvest",
        "After skillset add or refresh",
        "After update",
    ] {
        assert!(
            skill_md.contains(proof),
            "manage-tink must document operation-specific proof {proof}: {skill_md}"
        );
    }
    assert!(
        skill_md.contains("possibly changed")
            && skill_md.contains("describe the failure as a no-op"),
        "manage-tink must report reachable partial publication: {skill_md}"
    );
    assert!(
        commands.contains("every path must resolve inside the project"),
        "commands.md must document the local lock source boundary: {commands}"
    );
    assert!(
        commands.contains("CLI-owned supported harness roots")
            && !commands.contains("~/.claude/skills"),
        "commands.md must leave volatile harvest-root ownership with the CLI: {commands}"
    );
}

#[test]
fn x6_remove_fails_when_catalog_meta_is_malformed() {
    let ws = Workspace::new();
    let project = ws.project("app");
    ws.cmd(&project).arg("init").assert().success();
    let source = ws.root.join("demo-skill");
    write_skill(&source, "demo-skill", "body");
    ws.cmd(&project)
        .args(["skill", "add", source.to_str().unwrap()])
        .assert()
        .success();

    fs::write(ws.catalog_meta("app"), "{not-json}").unwrap();

    ws.cmd(&project)
        .args(["skill", "remove", "demo-skill"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("Invalid catalog meta"));

    assert!(Workspace::skill_path(&project, "demo-skill").is_dir());
    assert!(ws.library_skill("demo-skill").join("SKILL.md").is_file());
}

#[test]
fn x7_remove_refuses_symlinked_catalog_without_external_or_project_writes() {
    let ws = Workspace::new();
    let project = ws.project("app");
    ws.cmd(&project)
        .args(["init", "--no-zen", "--no-tink-skills", "--no-manage-tink"])
        .assert()
        .success();
    let source = ws.root.join("demo-skill");
    write_skill(&source, "demo-skill", "body");
    ws.cmd(&project)
        .args(["skill", "add", source.to_str().unwrap()])
        .assert()
        .success();

    let catalog = ws.inventory.join("catalog");
    let catalog_meta = ws.catalog_meta("app");
    let catalog_meta_relative = catalog_meta.strip_prefix(&catalog).unwrap().to_path_buf();
    let external = ws.root.join("external-catalog-remove");
    fs::rename(&catalog, &external).unwrap();
    std::os::unix::fs::symlink(&external, &catalog).unwrap();
    let external_meta = external.join(catalog_meta_relative);
    let catalog_before = fs::read(&external_meta).unwrap();
    let project_skill = Workspace::skill_path(&project, "demo-skill");
    let project_before = fs::read(project_skill.join("SKILL.md")).unwrap();

    ws.cmd(&project)
        .args(["skill", "remove", "demo-skill"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("symlink"));

    assert!(catalog.is_symlink());
    assert_eq!(fs::read(&external_meta).unwrap(), catalog_before);
    assert_eq!(
        fs::read(project_skill.join("SKILL.md")).unwrap(),
        project_before
    );
    assert!(ws.library_skill("demo-skill").join("SKILL.md").is_file());
}

// --- S*: safety ---

#[test]
fn s1_init_does_not_create_git_repo() {
    let ws = Workspace::new();
    let project = ws.project("app");
    assert!(!project.join(".git").exists());
    ws.cmd(&project).arg("init").assert().success();
    assert!(!project.join(".git").exists());
}

#[test]
fn s3_remove_and_destroy_complete_when_implicit_home_cannot_resolve() {
    let ws = Workspace::new();
    let remove_project = ws.project("remove-app");
    ws.cmd(&remove_project).arg("init").assert().success();
    let remove_catalog_before = fs::read(ws.catalog_meta("remove-app")).unwrap();

    Command::cargo_bin("tink")
        .unwrap()
        .current_dir(&remove_project)
        .env_remove("TINK_HOME")
        .env_remove("HOME")
        .args(["skill", "remove", "manage-tink"])
        .assert()
        .success();
    assert!(!Workspace::skill_path(&remove_project, "manage-tink").exists());
    assert_eq!(
        fs::read(ws.catalog_meta("remove-app")).unwrap(),
        remove_catalog_before
    );
    assert!(!remove_project.join(".tink").exists());

    let destroy_project = ws.project("destroy-app");
    ws.cmd(&destroy_project).arg("init").assert().success();
    let destroy_catalog_before = fs::read(ws.catalog_meta("destroy-app")).unwrap();

    Command::cargo_bin("tink")
        .unwrap()
        .current_dir(&destroy_project)
        .env_remove("TINK_HOME")
        .env_remove("HOME")
        .args(["destroy", "--yes"])
        .assert()
        .success();
    assert!(!destroy_project.join(".agents").exists());
    assert_eq!(
        fs::read(ws.catalog_meta("destroy-app")).unwrap(),
        destroy_catalog_before
    );
    assert!(!destroy_project.join(".tink").exists());
}

// --- U*: tink update ---

fn host_release_target() -> &'static str {
    if cfg!(all(target_os = "macos", target_arch = "aarch64")) {
        "aarch64-apple-darwin"
    } else if cfg!(all(target_os = "macos", target_arch = "x86_64")) {
        "x86_64-apple-darwin"
    } else if cfg!(all(target_os = "linux", target_arch = "aarch64")) {
        "aarch64-unknown-linux-gnu"
    } else if cfg!(all(target_os = "linux", target_arch = "x86_64")) {
        "x86_64-unknown-linux-gnu"
    } else {
        "unsupported"
    }
}

fn package_version() -> String {
    let output = cargo_bin_cmd!("tink")
        .arg("--version")
        .output()
        .expect("tink --version");
    assert!(output.status.success());
    let text = String::from_utf8_lossy(&output.stdout);
    // "tink 0.2.0"
    text.split_whitespace()
        .nth(1)
        .expect("version")
        .trim()
        .to_string()
}

fn write_release_fixture(dir: &Path, version: &str, binary_src: &Path) -> PathBuf {
    write_release_fixture_with_mode(dir, version, binary_src, 0o755)
}

fn write_release_fixture_with_mode(
    dir: &Path,
    version: &str,
    binary_src: &Path,
    unix_mode: u32,
) -> PathBuf {
    let target = host_release_target();
    assert_ne!(
        target, "unsupported",
        "update fixtures need a supported host"
    );
    let asset = format!("tink-{version}-{target}.tar.gz");
    let stage = dir.join("stage");
    fs::create_dir_all(&stage).unwrap();
    let staged_bin = stage.join("tink");
    fs::copy(binary_src, &staged_bin).unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&staged_bin, fs::Permissions::from_mode(unix_mode)).unwrap();
    }
    let archive = dir.join(&asset);
    let status = StdCommand::new("tar")
        .args(["-czf"])
        .arg(&archive)
        .arg("-C")
        .arg(&stage)
        .arg("tink")
        .status()
        .expect("tar");
    assert!(status.success());
    let archive_digest = {
        use sha2::{Digest, Sha256};
        let bytes = fs::read(&archive).unwrap();
        format!("{:x}", Sha256::digest(bytes))
    };

    let archive_url = format!("file://{}", archive.display());
    let meta = dir.join("release.json");
    let body = format!(
        r#"{{
  "tag_name": "v{version}",
  "assets": [
    {{
      "name": "{asset}",
      "browser_download_url": "{archive_url}",
      "digest": "sha256:{archive_digest}"
    }}
  ]
}}"#
    );
    fs::write(&meta, body).unwrap();
    meta
}

fn rewrite_release_digest(meta: &Path, algorithm: &str, uppercase_hex: bool) {
    let mut release: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(meta).expect("read release metadata"))
            .expect("parse release metadata");
    let digest = release["assets"][0]["digest"]
        .as_str()
        .expect("release digest")
        .to_string();
    let (_, hex) = digest.split_once(':').expect("digest prefix");
    let hex = if uppercase_hex {
        hex.to_ascii_uppercase()
    } else {
        hex.to_string()
    };
    release["assets"][0]["digest"] = serde_json::Value::String(format!("{algorithm}:{hex}"));
    fs::write(
        meta,
        format!("{}\n", serde_json::to_string_pretty(&release).unwrap()),
    )
    .expect("write uppercase release digest");
}

#[cfg(unix)]
fn wait_for_marker(path: &Path, child: &mut std::process::Child) {
    let started = std::time::Instant::now();
    while !path.exists() {
        if let Some(status) = child.try_wait().expect("poll fixture process") {
            panic!(
                "fixture process exited with {status} before marker: {}",
                path.display()
            );
        }
        if started.elapsed() >= std::time::Duration::from_secs(60) {
            interrupt_process_group(child.id());
            let _ = child.wait();
            panic!("timed out waiting for fixture marker: {}", path.display());
        }
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
}

#[cfg(unix)]
fn interrupt_process_group(process_group: u32) {
    let status = StdCommand::new("python3")
        .args([
            "-c",
            "import os, signal, sys; os.killpg(int(sys.argv[1]), signal.SIGINT)",
            &process_group.to_string(),
        ])
        .status()
        .expect("signal process group");
    assert!(
        status.success(),
        "could not interrupt fixture process group"
    );
}

#[cfg(unix)]
fn terminate_process(process: u32) {
    let status = StdCommand::new("python3")
        .args([
            "-c",
            "import os, signal, sys; os.kill(int(sys.argv[1]), signal.SIGTERM)",
            &process.to_string(),
        ])
        .status()
        .expect("signal fixture process");
    assert!(status.success(), "could not terminate fixture process");
}

#[cfg(unix)]
fn wait_for_output_bounded(
    mut child: std::process::Child,
    timeout: std::time::Duration,
    context: &str,
) -> std::process::Output {
    let process_group = child.id();
    let started = std::time::Instant::now();
    loop {
        if child.try_wait().expect("poll bounded fixture").is_some() {
            return child.wait_with_output().expect("collect bounded fixture");
        }
        if started.elapsed() >= timeout {
            interrupt_process_group(process_group);
            let _ = child.kill();
            let _ = child.wait();
            panic!("{context} exceeded {timeout:?} after candidate start");
        }
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
}

#[cfg(unix)]
#[test]
fn v4_closed_stdout_does_not_panic_or_exit_101() {
    use std::os::fd::OwnedFd;
    use std::os::unix::net::UnixStream;
    use std::process::Stdio;

    let ws = Workspace::new();
    let project = ws.project("app");
    ws.cmd(&project)
        .args(["init", "--no-zen", "--no-tink-skills", "--no-manage-tink"])
        .assert()
        .success();

    let (read_end, write_end) = UnixStream::pair().expect("stdout pipe");
    drop(read_end);
    let write_end: OwnedFd = write_end.into();
    let output = StdCommand::new(assert_cmd::cargo::cargo_bin!("tink"))
        .current_dir(&project)
        .env("TINK_HOME", &ws.inventory)
        .args(["skill", "list"])
        .stdout(Stdio::from(write_end))
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn tink with closed stdout")
        .wait_with_output()
        .expect("wait for tink");
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert_eq!(
        output.status.code(),
        Some(0),
        "closed stdout must exit cleanly: stderr={stderr:?}"
    );
    assert!(!stderr.contains("panicked at"));
}

#[test]
fn v7_failure_paths_escape_terminal_controls() {
    let ws = Workspace::new();
    let project = ws.project("app");
    let inventory = ws.root.join("inventory-\u{1b}\nunsafe");
    ws.cmd(&project)
        .env("TINK_HOME", &inventory)
        .args(["init", "--no-zen", "--no-tink-skills", "--no-manage-tink"])
        .assert()
        .success();

    let by_project = inventory.join("catalog").join("by-project");
    fs::remove_dir_all(&by_project).expect("remove catalog directory fixture");
    fs::write(&by_project, "not a directory").expect("write catalog file fixture");

    let output = cargo_bin_cmd!("tink")
        .current_dir(&project)
        .env("TINK_HOME", &inventory)
        .args(["skill", "list", "--catalog"])
        .output()
        .expect("run failing catalog listing");
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(!output.status.success(), "catalog listing should fail");
    assert!(
        !output.stderr.contains(&0x1b),
        "stderr leaked raw escape: {stderr:?}"
    );
    assert!(
        stderr.contains("\\x1b") && stderr.contains("\\nunsafe"),
        "stderr did not visibly escape the unsafe path: {stderr:?}"
    );
    assert_eq!(
        stderr.lines().count(),
        1,
        "escaped error must remain one line"
    );
}

#[cfg(unix)]
#[test]
fn v5_closed_stderr_does_not_hide_command_failure() {
    use std::os::fd::OwnedFd;
    use std::os::unix::net::UnixStream;
    use std::process::Stdio;

    let ws = Workspace::new();
    let project = ws.project("app");
    let missing = project.join("missing-skill");
    let (read_end, write_end) = UnixStream::pair().expect("stderr pipe");
    drop(read_end);
    let write_end: OwnedFd = write_end.into();

    let status = StdCommand::new(assert_cmd::cargo::cargo_bin!("tink"))
        .current_dir(&project)
        .env("TINK_HOME", &ws.inventory)
        .args(["skill", "add"])
        .arg(&missing)
        .stdout(Stdio::null())
        .stderr(Stdio::from(write_end))
        .status()
        .expect("run tink with closed stderr");

    assert_eq!(
        status.code(),
        Some(1),
        "closed stderr must not turn an underlying command failure into success"
    );
}

#[cfg(unix)]
#[test]
fn u6_install_script_rejects_invalid_payload_and_preserves_existing_binary() {
    use std::os::unix::fs::PermissionsExt;

    let ws = Workspace::new();
    let fixture = ws.root.join("install-release-fixture");
    fs::create_dir_all(&fixture).unwrap();
    let invalid_payload = fixture.join("invalid-tink");
    fs::write(&invalid_payload, b"this is not a tink executable\n").unwrap();
    fs::set_permissions(&invalid_payload, fs::Permissions::from_mode(0o644)).unwrap();
    let meta = write_release_fixture_with_mode(&fixture, "99.0.0", &invalid_payload, 0o644);

    let install_dir = ws.root.join("install");
    fs::create_dir_all(&install_dir).unwrap();
    let installed = install_dir.join("tink");
    let known_good = b"#!/bin/sh\nprintf 'tink 0.3.14\\n'\n";
    fs::write(&installed, known_good).unwrap();
    fs::set_permissions(&installed, fs::Permissions::from_mode(0o755)).unwrap();

    let output = StdCommand::new("sh")
        .arg(Path::new(env!("CARGO_MANIFEST_DIR")).join("install.sh"))
        .env("TINK_INSTALL_DIR", &install_dir)
        .env("TINK_RELEASES_API", format!("file://{}", meta.display()))
        .output()
        .expect("run install.sh");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let installed_after = fs::read(&installed).unwrap();

    assert!(
        !output.status.success()
            && !stdout.contains("Installed tink")
            && installed_after == known_good,
        "invalid installer payload must fail without success output or clobbering: status={:?}, stdout={stdout:?}, stderr={stderr:?}, preserved={}",
        output.status.code(),
        installed_after == known_good
    );
}

#[cfg(unix)]
#[test]
fn u7_install_script_publishes_verified_payload_after_successful_probe() {
    use std::os::unix::fs::PermissionsExt;

    let ws = Workspace::new();
    let fixture = ws.root.join("valid-install-release-fixture");
    fs::create_dir_all(&fixture).unwrap();
    let replacement = fixture.join("replacement-tink");
    fs::write(&replacement, b"#!/bin/sh\nprintf 'tink 99.0.0\\n'\n").unwrap();
    fs::set_permissions(&replacement, fs::Permissions::from_mode(0o755)).unwrap();
    let meta = write_release_fixture(&fixture, "99.0.0", &replacement);

    let install_dir = ws.root.join("install");
    fs::create_dir_all(&install_dir).unwrap();
    let installed = install_dir.join("tink");
    fs::write(&installed, b"#!/bin/sh\nprintf 'tink 0.3.14\\n'\n").unwrap();
    fs::set_permissions(&installed, fs::Permissions::from_mode(0o755)).unwrap();

    let output = StdCommand::new("sh")
        .arg(Path::new(env!("CARGO_MANIFEST_DIR")).join("install.sh"))
        .env("TINK_INSTALL_DIR", &install_dir)
        .env("TINK_RELEASES_API", format!("file://{}", meta.display()))
        .output()
        .expect("run install.sh");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(output.status.success(), "install failed: {stdout}");
    assert!(stdout.contains("Installed tink v99.0.0"));
    assert!(stdout.contains("tink 99.0.0"));

    assert_eq!(
        fs::read(&installed).unwrap(),
        fs::read(&replacement).unwrap()
    );
}

#[cfg(unix)]
#[test]
fn u8_install_script_rejects_non_semver_metadata_cleanly() {
    use std::os::unix::fs::PermissionsExt;

    let ws = Workspace::new();
    let fixture = ws.root.join("invalid-semver-release-fixture");
    fs::create_dir_all(&fixture).unwrap();
    let metadata = fixture.join("release.json");
    fs::write(&metadata, r#"{"tag_name":"v01.2.3","assets":[]}"#).unwrap();
    let install_dir = ws.root.join("install");
    fs::create_dir_all(&install_dir).unwrap();
    let installed = install_dir.join("tink");
    let known_good = b"#!/bin/sh\nprintf 'tink 0.3.14\\n'\n";
    fs::write(&installed, known_good).unwrap();
    fs::set_permissions(&installed, fs::Permissions::from_mode(0o755)).unwrap();

    let output = StdCommand::new("sh")
        .arg(Path::new(env!("CARGO_MANIFEST_DIR")).join("install.sh"))
        .env("TINK_INSTALL_DIR", &install_dir)
        .env(
            "TINK_RELEASES_API",
            format!("file://{}", metadata.display()),
        )
        .output()
        .expect("run install.sh");
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(!output.status.success());
    assert!(stderr.contains("invalid semantic version"), "{stderr}");
    assert!(!stderr.contains("Traceback"), "{stderr}");
    assert_eq!(fs::read(&installed).unwrap(), known_good);
}

#[test]
fn u9_install_script_rejects_and_redacts_unsafe_api_url() {
    let ws = Workspace::new();
    let install_dir = ws.root.join("install");
    let output = StdCommand::new("sh")
        .arg(Path::new(env!("CARGO_MANIFEST_DIR")).join("install.sh"))
        .env("TINK_INSTALL_DIR", &install_dir)
        .env(
            "TINK_RELEASES_API",
            "https://user:supersecret@example.test/releases?token=private",
        )
        .output()
        .expect("run install.sh");
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(!output.status.success());
    assert!(stderr.contains("not an allowed release URL"), "{stderr}");
    assert!(!stderr.contains("supersecret"), "{stderr}");
    assert!(!stderr.contains("private"), "{stderr}");
    assert!(!install_dir.exists());
}

#[cfg(unix)]
#[test]
fn u10_install_script_bounds_candidate_probe_and_preserves_existing_binary() {
    use std::os::unix::fs::PermissionsExt;
    use std::os::unix::process::CommandExt;
    use std::process::Stdio;

    let ws = Workspace::new();
    let fixture = ws.root.join("hanging-release-fixture");
    fs::create_dir_all(&fixture).unwrap();
    let hanging = fixture.join("hanging-tink");
    fs::write(
        &hanging,
        b"#!/bin/sh\nprintf started > \"$TINK_PROBE_STARTED\"\nexec sleep 120\n",
    )
    .unwrap();
    fs::set_permissions(&hanging, fs::Permissions::from_mode(0o755)).unwrap();
    let metadata = write_release_fixture(&fixture, "99.0.0", &hanging);
    let install_dir = ws.root.join("install");
    fs::create_dir_all(&install_dir).unwrap();
    let installed = install_dir.join("tink");
    let known_good = b"#!/bin/sh\nprintf 'tink 0.3.14\\n'\n";
    fs::write(&installed, known_good).unwrap();
    fs::set_permissions(&installed, fs::Permissions::from_mode(0o755)).unwrap();
    let candidate_started = ws.root.join("hanging-candidate-started");

    let mut command = StdCommand::new("sh");
    command
        .arg(Path::new(env!("CARGO_MANIFEST_DIR")).join("install.sh"))
        .env("TINK_INSTALL_DIR", &install_dir)
        .env("TINK_PROBE_STARTED", &candidate_started)
        .env(
            "TINK_RELEASES_API",
            format!("file://{}", metadata.display()),
        )
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .process_group(0);
    let mut child = command.spawn().expect("spawn bounded installer fixture");
    wait_for_marker(&candidate_started, &mut child);
    let output = wait_for_output_bounded(
        child,
        std::time::Duration::from_secs(25),
        "bounded installer probe",
    );

    assert!(!output.status.success());
    assert_eq!(fs::read(&installed).unwrap(), known_good);
}

#[cfg(unix)]
#[test]
fn u11_install_script_rejects_non_ascii_semver_digits_and_preserves_existing_binary() {
    use std::os::unix::fs::PermissionsExt;

    let ws = Workspace::new();
    let fixture = ws.root.join("non-ascii-semver-release-fixture");
    fs::create_dir_all(&fixture).unwrap();
    let metadata = fixture.join("release.json");
    fs::write(&metadata, r#"{"tag_name":"v١.2.3","assets":[]}"#).unwrap();
    let install_dir = ws.root.join("install");
    fs::create_dir_all(&install_dir).unwrap();
    let installed = install_dir.join("tink");
    let known_good = b"#!/bin/sh\nprintf 'tink 0.3.14\\n'\n";
    fs::write(&installed, known_good).unwrap();
    fs::set_permissions(&installed, fs::Permissions::from_mode(0o755)).unwrap();

    let output = StdCommand::new("sh")
        .arg(Path::new(env!("CARGO_MANIFEST_DIR")).join("install.sh"))
        .env("TINK_INSTALL_DIR", &install_dir)
        .env(
            "TINK_RELEASES_API",
            format!("file://{}", metadata.display()),
        )
        .output()
        .expect("run install.sh");
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(!output.status.success());
    assert!(stderr.contains("invalid semantic version"), "{stderr}");
    assert!(!stderr.contains("Traceback"), "{stderr}");
    assert_eq!(fs::read(&installed).unwrap(), known_good);
}

#[cfg(unix)]
#[test]
fn u12_install_script_kills_timed_out_candidate_descendants() {
    use std::os::unix::fs::PermissionsExt;
    use std::os::unix::process::CommandExt;
    use std::process::Stdio;

    let ws = Workspace::new();
    let fixture = ws.root.join("descendant-release-fixture");
    fs::create_dir_all(&fixture).unwrap();
    let candidate = fixture.join("descendant-tink");
    fs::write(
        &candidate,
        b"#!/bin/sh\nprintf started > \"$TINK_PROBE_STARTED\"\n(sleep 6; printf survived > \"$TINK_DESCENDANT_MARKER\") &\nexit 0\n",
    )
    .unwrap();
    fs::set_permissions(&candidate, fs::Permissions::from_mode(0o755)).unwrap();
    let metadata = write_release_fixture(&fixture, "99.0.0", &candidate);
    let marker = ws.root.join("descendant-survived");
    let install_dir = ws.root.join("install");
    fs::create_dir_all(&install_dir).unwrap();
    let installed = install_dir.join("tink");
    let known_good = b"#!/bin/sh\nprintf 'tink 0.3.14\\n'\n";
    fs::write(&installed, known_good).unwrap();
    fs::set_permissions(&installed, fs::Permissions::from_mode(0o755)).unwrap();
    let candidate_started = ws.root.join("descendant-candidate-started");

    let mut command = StdCommand::new("sh");
    command
        .arg(Path::new(env!("CARGO_MANIFEST_DIR")).join("install.sh"))
        .env("TINK_INSTALL_DIR", &install_dir)
        .env("TINK_PROBE_STARTED", &candidate_started)
        .env("TINK_DESCENDANT_MARKER", &marker)
        .env(
            "TINK_RELEASES_API",
            format!("file://{}", metadata.display()),
        )
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .process_group(0);
    let mut child = command
        .spawn()
        .expect("spawn descendant-cleanup installer fixture");
    wait_for_marker(&candidate_started, &mut child);
    let output = wait_for_output_bounded(
        child,
        std::time::Duration::from_secs(25),
        "bounded installer descendant cleanup",
    );

    assert!(!output.status.success());
    std::thread::sleep(std::time::Duration::from_secs(2));
    assert!(!marker.exists(), "timed-out candidate descendant survived");
    assert_eq!(fs::read(&installed).unwrap(), known_good);
}

#[cfg(unix)]
#[test]
fn u13_install_script_rolls_back_when_published_probe_fails() {
    use std::os::unix::fs::PermissionsExt;

    let ws = Workspace::new();
    let fixture = ws.root.join("published-probe-release-fixture");
    fs::create_dir_all(&fixture).unwrap();
    let candidate = fixture.join("path-sensitive-tink");
    fs::write(
        &candidate,
        b"#!/bin/sh\ncase \"$0\" in */install/tink) exit 7 ;; esac\nprintf 'tink 99.0.0\\n'\n",
    )
    .unwrap();
    fs::set_permissions(&candidate, fs::Permissions::from_mode(0o755)).unwrap();
    let metadata = write_release_fixture(&fixture, "99.0.0", &candidate);
    let install_dir = ws.root.join("install");
    fs::create_dir_all(&install_dir).unwrap();
    let installed = install_dir.join("tink");
    let known_good = b"#!/bin/sh\nprintf 'tink 0.3.14\\n'\n";
    fs::write(&installed, known_good).unwrap();
    fs::set_permissions(&installed, fs::Permissions::from_mode(0o755)).unwrap();

    let output = StdCommand::new("sh")
        .arg(Path::new(env!("CARGO_MANIFEST_DIR")).join("install.sh"))
        .env("TINK_INSTALL_DIR", &install_dir)
        .env(
            "TINK_RELEASES_API",
            format!("file://{}", metadata.display()),
        )
        .output()
        .expect("run install.sh");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(!output.status.success());
    assert!(!stdout.contains("Installed"), "{stdout}");
    assert!(stderr.contains("published tink failed"), "{stderr}");
    assert_eq!(fs::read(&installed).unwrap(), known_good);
    assert_eq!(
        fs::metadata(&installed).unwrap().permissions().mode() & 0o777,
        0o755
    );
}

#[cfg(unix)]
#[test]
fn u14_install_interrupt_kills_candidate_group_without_traceback() {
    use std::os::unix::fs::PermissionsExt;
    use std::os::unix::process::CommandExt;
    use std::process::Stdio;

    let ws = Workspace::new();
    let fixture = ws.root.join("install-cancel-release-fixture");
    fs::create_dir_all(&fixture).unwrap();
    let candidate = fixture.join("cancel-tink");
    fs::write(
        &candidate,
        b"#!/bin/sh\nprintf started > \"$TINK_CANCEL_STARTED\"\nsleep 3\nprintf survived > \"$TINK_CANCEL_SURVIVED\"\nprintf 'tink 99.0.0\\n'\n",
    )
    .unwrap();
    fs::set_permissions(&candidate, fs::Permissions::from_mode(0o755)).unwrap();
    let metadata = write_release_fixture(&fixture, "99.0.0", &candidate);
    let install_dir = ws.root.join("install");
    fs::create_dir_all(&install_dir).unwrap();
    let installed = install_dir.join("tink");
    let known_good = b"#!/bin/sh\nprintf 'tink 0.3.14\\n'\n";
    fs::write(&installed, known_good).unwrap();
    fs::set_permissions(&installed, fs::Permissions::from_mode(0o755)).unwrap();
    let started = ws.root.join("install-candidate-started");
    let survived = ws.root.join("install-candidate-survived");

    let mut command = StdCommand::new("sh");
    command
        .arg(Path::new(env!("CARGO_MANIFEST_DIR")).join("install.sh"))
        .env("TINK_INSTALL_DIR", &install_dir)
        .env("TINK_CANCEL_STARTED", &started)
        .env("TINK_CANCEL_SURVIVED", &survived)
        .env(
            "TINK_RELEASES_API",
            format!("file://{}", metadata.display()),
        )
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .process_group(0);
    let mut child = command.spawn().expect("spawn install cancellation fixture");
    let process_group = child.id();
    wait_for_marker(&started, &mut child);
    interrupt_process_group(process_group);
    let output = child
        .wait_with_output()
        .expect("wait for interrupted installer");
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(!output.status.success());
    assert!(!stderr.contains("Traceback"), "{stderr}");
    std::thread::sleep(std::time::Duration::from_millis(3300));
    assert!(
        !survived.exists(),
        "interrupted installer left candidate running"
    );
    assert_eq!(fs::read(&installed).unwrap(), known_good);
}

#[cfg(unix)]
#[test]
fn u18_install_handles_interrupt_raised_during_candidate_spawn() {
    use std::os::unix::fs::PermissionsExt;

    let installer = include_str!("../install.sh");
    let (run_bounded, probe_exact) = installer
        .split_once("probe_exact()")
        .expect("installer probe_exact function");
    for (name, supervisor) in [("run_bounded", run_bounded), ("probe_exact", probe_exact)] {
        let handler = supervisor
            .find("signal.signal(watched, interrupt)")
            .unwrap_or_else(|| panic!("{name} is missing interrupt handlers"));
        let spawn = supervisor
            .find("subprocess.Popen(")
            .unwrap_or_else(|| panic!("{name} is missing supervised spawn"));
        assert!(
            handler < spawn,
            "{name} must arm interrupt handlers before spawning its child"
        );
    }

    let ws = Workspace::new();
    let fixture = ws.root.join("install-spawn-cancel-release-fixture");
    fs::create_dir_all(&fixture).unwrap();
    let candidate = fixture.join("spawn-cancel-tink");
    let survived = ws.root.join("spawn-cancel-candidate-survived");
    fs::write(
        &candidate,
        b"#!/bin/sh\nkill -TERM \"$PPID\"\nsleep 2\nprintf survived > \"$TINK_CANCEL_SURVIVED\"\nprintf 'tink 99.0.0\\n'\n",
    )
    .unwrap();
    fs::set_permissions(&candidate, fs::Permissions::from_mode(0o755)).unwrap();
    let metadata = write_release_fixture(&fixture, "99.0.0", &candidate);
    let install_dir = ws.root.join("install");
    fs::create_dir_all(&install_dir).unwrap();
    let installed = install_dir.join("tink");
    let known_good = b"#!/bin/sh\nprintf 'tink 0.3.14\\n'\n";
    fs::write(&installed, known_good).unwrap();
    fs::set_permissions(&installed, fs::Permissions::from_mode(0o755)).unwrap();

    let output = StdCommand::new("sh")
        .arg(Path::new(env!("CARGO_MANIFEST_DIR")).join("install.sh"))
        .env("TINK_INSTALL_DIR", &install_dir)
        .env("TINK_CANCEL_SURVIVED", &survived)
        .env(
            "TINK_RELEASES_API",
            format!("file://{}", metadata.display()),
        )
        .output()
        .expect("run spawn-interrupted installer");

    assert!(!output.status.success());
    assert!(
        !String::from_utf8_lossy(&output.stderr).contains("Traceback"),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    std::thread::sleep(std::time::Duration::from_millis(2300));
    assert!(
        !survived.exists(),
        "spawn-time interruption left candidate running"
    );
    assert_eq!(fs::read(&installed).unwrap(), known_good);
}

#[cfg(unix)]
#[test]
fn u15_update_interrupt_kills_candidate_group_and_preserves_binary() {
    use std::os::unix::fs::PermissionsExt;
    use std::os::unix::process::CommandExt;
    use std::process::Stdio;

    let ws = Workspace::new();
    let fixture = ws.root.join("update-cancel-release-fixture");
    fs::create_dir_all(&fixture).unwrap();
    let candidate = fixture.join("cancel-tink");
    fs::write(
        &candidate,
        b"#!/bin/sh\nprintf started > \"$TINK_CANCEL_STARTED\"\nsleep 3\nprintf survived > \"$TINK_CANCEL_SURVIVED\"\nprintf 'tink 99.0.0\\n'\n",
    )
    .unwrap();
    fs::set_permissions(&candidate, fs::Permissions::from_mode(0o755)).unwrap();
    let metadata = write_release_fixture(&fixture, "99.0.0", &candidate);
    let install_dir = ws.root.join("install");
    fs::create_dir_all(&install_dir).unwrap();
    let installed = install_dir.join("tink");
    fs::copy(assert_cmd::cargo::cargo_bin!("tink"), &installed).unwrap();
    fs::set_permissions(&installed, fs::Permissions::from_mode(0o755)).unwrap();
    let known_good = fs::read(&installed).unwrap();
    let started = ws.root.join("update-candidate-started");
    let survived = ws.root.join("update-candidate-survived");

    let mut command = StdCommand::new(&installed);
    command
        .arg("update")
        .env("TINK_HOME", &ws.inventory)
        .env("TINK_CANCEL_STARTED", &started)
        .env("TINK_CANCEL_SURVIVED", &survived)
        .env(
            "TINK_RELEASES_API",
            format!("file://{}", metadata.display()),
        )
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .process_group(0);
    let mut child = command.spawn().expect("spawn update cancellation fixture");
    let process_group = child.id();
    wait_for_marker(&started, &mut child);
    interrupt_process_group(process_group);
    let output = child
        .wait_with_output()
        .expect("wait for interrupted updater");

    assert!(!output.status.success());
    std::thread::sleep(std::time::Duration::from_millis(3300));
    assert!(
        !survived.exists(),
        "interrupted update left candidate running"
    );
    assert_eq!(fs::read(&installed).unwrap(), known_good);
}

#[cfg(unix)]
#[test]
fn u16_update_output_escapes_terminal_controls_in_binary_path() {
    use std::os::unix::fs::PermissionsExt;

    let ws = Workspace::new();
    let fixture = ws.root.join("escaped-update-output-fixture");
    fs::create_dir_all(&fixture).unwrap();
    let version = package_version();
    let bin = assert_cmd::cargo::cargo_bin!("tink");
    let metadata = write_release_fixture(&fixture, &version, bin);
    let install_dir = ws.root.join("install\u{1b}[31m\nrow");
    fs::create_dir_all(&install_dir).unwrap();
    let installed = install_dir.join("tink");
    fs::copy(bin, &installed).unwrap();
    fs::set_permissions(&installed, fs::Permissions::from_mode(0o755)).unwrap();

    let output = StdCommand::new(&installed)
        .arg("update")
        .env("NO_COLOR", "1")
        .env("TINK_HOME", &ws.inventory)
        .env(
            "TINK_RELEASES_API",
            format!("file://{}", metadata.display()),
        )
        .output()
        .expect("run updater from control-bearing path");
    let stdout = String::from_utf8_lossy(&output.stdout);

    assert!(output.status.success(), "update failed: {stdout:?}");
    assert!(
        !output.stdout.contains(&0x1b),
        "raw ESC in stdout: {stdout:?}"
    );
    assert_eq!(
        output.stdout.iter().filter(|byte| **byte == b'\n').count(),
        1
    );
    assert!(stdout.contains("install\\x1b[31m\\nrow"), "{stdout:?}");
}

#[cfg(unix)]
#[test]
fn u17_install_script_rejects_probe_output_over_capture_limit() {
    use std::os::unix::fs::PermissionsExt;
    use std::os::unix::process::CommandExt;
    use std::process::Stdio;

    let ws = Workspace::new();
    let fixture = ws.root.join("overflowing-probe-release-fixture");
    fs::create_dir_all(&fixture).unwrap();
    let candidate = fixture.join("overflowing-tink");
    fs::write(
        &candidate,
        b"#!/bin/sh\nprintf started > \"$TINK_PROBE_STARTED\"\npython3 - <<'PY'\nimport sys\nsys.stdout.buffer.write(b'x' * (17 * 1024 * 1024))\nsys.stdout.flush()\nPY\nexec sleep 120\n",
    )
    .unwrap();
    fs::set_permissions(&candidate, fs::Permissions::from_mode(0o755)).unwrap();
    let metadata = write_release_fixture(&fixture, "99.0.0", &candidate);
    let install_dir = ws.root.join("install");
    fs::create_dir_all(&install_dir).unwrap();
    let installed = install_dir.join("tink");
    let known_good = b"#!/bin/sh\nprintf 'tink 0.3.14\\n'\n";
    fs::write(&installed, known_good).unwrap();
    fs::set_permissions(&installed, fs::Permissions::from_mode(0o755)).unwrap();
    let candidate_started = ws.root.join("overflowing-candidate-started");

    let mut command = StdCommand::new("sh");
    command
        .arg(Path::new(env!("CARGO_MANIFEST_DIR")).join("install.sh"))
        .env("TINK_INSTALL_DIR", &install_dir)
        .env("TINK_PROBE_STARTED", &candidate_started)
        .env(
            "TINK_RELEASES_API",
            format!("file://{}", metadata.display()),
        )
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .process_group(0);
    let mut child = command
        .spawn()
        .expect("spawn overflowing installer fixture");
    wait_for_marker(&candidate_started, &mut child);
    let output = wait_for_output_bounded(
        child,
        std::time::Duration::from_secs(4),
        "installer probe output limit",
    );

    assert!(!output.status.success());
    assert_eq!(fs::read(&installed).unwrap(), known_good);
}

#[cfg(unix)]
#[test]
fn v6_install_script_keeps_verified_install_successful_when_stdout_is_closed() {
    use std::os::fd::OwnedFd;
    use std::os::unix::fs::PermissionsExt;
    use std::os::unix::net::UnixStream;
    use std::process::Stdio;

    let ws = Workspace::new();
    let fixture = ws.root.join("closed-stdout-install-release-fixture");
    fs::create_dir_all(&fixture).unwrap();
    let replacement = fixture.join("replacement-tink");
    let replacement_bytes = b"#!/bin/sh\nprintf 'tink 99.0.0\\n'\n";
    fs::write(&replacement, replacement_bytes).unwrap();
    fs::set_permissions(&replacement, fs::Permissions::from_mode(0o755)).unwrap();
    let metadata = write_release_fixture(&fixture, "99.0.0", &replacement);
    let install_dir = ws.root.join("install");
    fs::create_dir_all(&install_dir).unwrap();
    let installed = install_dir.join("tink");
    fs::write(&installed, b"#!/bin/sh\nprintf 'tink 0.3.14\\n'\n").unwrap();
    fs::set_permissions(&installed, fs::Permissions::from_mode(0o755)).unwrap();
    let (read_end, write_end) = UnixStream::pair().expect("stdout pipe");
    drop(read_end);
    let write_end: OwnedFd = write_end.into();

    let output = StdCommand::new("sh")
        .arg(Path::new(env!("CARGO_MANIFEST_DIR")).join("install.sh"))
        .env("TINK_INSTALL_DIR", &install_dir)
        .env(
            "TINK_RELEASES_API",
            format!("file://{}", metadata.display()),
        )
        .stdout(Stdio::from(write_end))
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn install.sh with closed stdout")
        .wait_with_output()
        .expect("wait for install.sh");
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert_eq!(
        output.status.code(),
        Some(0),
        "verified install must survive closed advisory stdout: {stderr:?}"
    );
    assert_eq!(fs::read(&installed).unwrap(), replacement_bytes);
}

#[test]
fn u1_update_fails_closed_on_bad_releases_api() {
    let ws = Workspace::new();
    let project = ws.project("app");
    ws.cmd(&project)
        .arg("update")
        .env(
            "TINK_RELEASES_API",
            "http://127.0.0.1:1/tink-update-missing",
        )
        .assert()
        .failure()
        .stderr(predicate::str::contains("not an allowed release URL"));
}

#[test]
fn u2_update_reports_up_to_date_for_current_version() {
    let ws = Workspace::new();
    let fixture = ws.root.join("release-fixture");
    fs::create_dir_all(&fixture).unwrap();
    let version = package_version();
    let bin = assert_cmd::cargo::cargo_bin!("tink");
    let meta = write_release_fixture(&fixture, &version, bin);
    let api = format!("file://{}", meta.display());

    let install_dir = ws.root.join("install");
    fs::create_dir_all(&install_dir).unwrap();
    let installed = install_dir.join("tink");
    fs::copy(bin, &installed).unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&installed, fs::Permissions::from_mode(0o755)).unwrap();
    }

    Command::new(&installed)
        .arg("update")
        .env("TINK_RELEASES_API", &api)
        .env("TINK_HOME", &ws.inventory)
        .assert()
        .success()
        .stdout(
            predicate::str::contains("Up to date")
                .and(predicate::str::contains(format!("v{version}"))),
        );
}

#[test]
fn u3_update_replaces_binary_when_newer_release_exists() {
    let ws = Workspace::new();
    let fixture = ws.root.join("release-fixture");
    fs::create_dir_all(&fixture).unwrap();
    let bin = assert_cmd::cargo::cargo_bin!("tink");
    let replacement = fixture.join("replacement-tink");
    fs::write(&replacement, b"#!/bin/sh\nprintf 'tink 99.0.0\\n'\n").unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&replacement, fs::Permissions::from_mode(0o755)).unwrap();
    }
    let meta = write_release_fixture(&fixture, "99.0.0", &replacement);
    let api = format!("file://{}", meta.display());

    let install_dir = ws.root.join("install");
    fs::create_dir_all(&install_dir).unwrap();
    let installed = install_dir.join("tink");
    fs::copy(bin, &installed).unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&installed, fs::Permissions::from_mode(0o755)).unwrap();
    }
    let before = fs::metadata(&installed).unwrap().modified().unwrap();

    // Ensure mtime can advance.
    std::thread::sleep(std::time::Duration::from_millis(20));

    Command::new(&installed)
        .arg("update")
        .env("TINK_RELEASES_API", &api)
        .env("TINK_HOME", &ws.inventory)
        .assert()
        .success()
        .stdout(predicate::str::contains("Updated").and(predicate::str::contains("v99.0.0")));

    assert!(installed.is_file());
    assert_eq!(
        fs::read(&installed).unwrap(),
        fs::read(&replacement).unwrap()
    );
    let after = fs::metadata(&installed).unwrap().modified().unwrap();
    assert!(after >= before);
}

#[cfg(unix)]
#[test]
fn u4_update_rejects_invalid_payload_and_preserves_running_binary() {
    use std::os::unix::fs::PermissionsExt;

    let ws = Workspace::new();
    let fixture = ws.root.join("invalid-release-fixture");
    fs::create_dir_all(&fixture).unwrap();
    let invalid_payload = fixture.join("invalid-tink");
    fs::write(&invalid_payload, b"this is not a tink executable\n").unwrap();
    fs::set_permissions(&invalid_payload, fs::Permissions::from_mode(0o644)).unwrap();
    let meta = write_release_fixture_with_mode(&fixture, "99.0.0", &invalid_payload, 0o644);
    let api = format!("file://{}", meta.display());

    let install_dir = ws.root.join("install");
    fs::create_dir_all(&install_dir).unwrap();
    let installed = install_dir.join("tink");
    fs::copy(assert_cmd::cargo::cargo_bin!("tink"), &installed).unwrap();
    fs::set_permissions(&installed, fs::Permissions::from_mode(0o755)).unwrap();
    let before = fs::read(&installed).unwrap();

    let output = StdCommand::new(&installed)
        .arg("update")
        .env("TINK_RELEASES_API", &api)
        .env("TINK_HOME", &ws.inventory)
        .output()
        .expect("run tink update");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let after = fs::read(&installed).unwrap();

    assert!(
        !output.status.success() && !stdout.contains("Updated") && after == before,
        "invalid update payload must fail without success output or replacing the running binary: status={:?}, stdout={stdout:?}, stderr={stderr:?}, preserved={}",
        output.status.code(),
        after == before
    );
}

#[cfg(unix)]
#[test]
fn u5_update_refuses_downgrade_and_preserves_running_binary() {
    use std::os::unix::fs::PermissionsExt;

    let ws = Workspace::new();
    let fixture = ws.root.join("older-release-fixture");
    fs::create_dir_all(&fixture).unwrap();
    let payload = fixture.join("older-tink");
    fs::write(&payload, b"#!/bin/sh\nprintf 'tink 0.0.1\\n'\n").unwrap();
    fs::set_permissions(&payload, fs::Permissions::from_mode(0o755)).unwrap();
    let meta = write_release_fixture(&fixture, "0.0.1", &payload);

    let install_dir = ws.root.join("install");
    fs::create_dir_all(&install_dir).unwrap();
    let installed = install_dir.join("tink");
    fs::copy(assert_cmd::cargo::cargo_bin!("tink"), &installed).unwrap();
    fs::set_permissions(&installed, fs::Permissions::from_mode(0o755)).unwrap();
    let before = fs::read(&installed).unwrap();

    Command::new(&installed)
        .arg("update")
        .env("TINK_RELEASES_API", format!("file://{}", meta.display()))
        .env("TINK_HOME", &ws.inventory)
        .assert()
        .failure()
        .stderr(predicate::str::contains("refusing to downgrade"));

    assert_eq!(fs::read(&installed).unwrap(), before);
}

#[cfg(unix)]
#[test]
fn u19_update_and_installer_accept_case_insensitive_sha256_metadata() {
    use std::os::unix::fs::PermissionsExt;

    let ws = Workspace::new();
    let fixture = ws.root.join("uppercase-digest-release-fixture");
    fs::create_dir_all(&fixture).unwrap();
    let replacement = fixture.join("replacement-tink");
    let replacement_bytes = b"#!/bin/sh\nprintf 'tink 99.0.0\\n'\n";
    fs::write(&replacement, replacement_bytes).unwrap();
    fs::set_permissions(&replacement, fs::Permissions::from_mode(0o755)).unwrap();
    let metadata = write_release_fixture(&fixture, "99.0.0", &replacement);
    rewrite_release_digest(&metadata, "ShA256", true);
    let api = format!("file://{}", metadata.display());

    let update_dir = ws.root.join("update-install");
    fs::create_dir_all(&update_dir).unwrap();
    let updated = update_dir.join("tink");
    fs::copy(assert_cmd::cargo::cargo_bin!("tink"), &updated).unwrap();
    fs::set_permissions(&updated, fs::Permissions::from_mode(0o755)).unwrap();
    Command::new(&updated)
        .arg("update")
        .env("TINK_RELEASES_API", &api)
        .env("TINK_HOME", &ws.inventory)
        .assert()
        .success()
        .stdout(predicate::str::contains("Updated"));
    assert_eq!(fs::read(&updated).unwrap(), replacement_bytes);

    let install_dir = ws.root.join("script-install");
    fs::create_dir_all(&install_dir).unwrap();
    Command::new("sh")
        .arg(Path::new(env!("CARGO_MANIFEST_DIR")).join("install.sh"))
        .env("TINK_INSTALL_DIR", &install_dir)
        .env("TINK_RELEASES_API", &api)
        .assert()
        .success()
        .stdout(predicate::str::contains("Installed tink v99.0.0"));
    assert_eq!(
        fs::read(install_dir.join("tink")).unwrap(),
        replacement_bytes
    );
}

#[cfg(unix)]
#[test]
fn u20_update_and_installer_reject_non_ascii_sha256_algorithm() {
    use std::os::unix::fs::PermissionsExt;

    let ws = Workspace::new();
    let fixture = ws.root.join("non-ascii-digest-release-fixture");
    fs::create_dir_all(&fixture).unwrap();
    let replacement = fixture.join("replacement-tink");
    fs::write(&replacement, b"#!/bin/sh\nprintf 'tink 99.0.0\\n'\n").unwrap();
    fs::set_permissions(&replacement, fs::Permissions::from_mode(0o755)).unwrap();
    let metadata = write_release_fixture(&fixture, "99.0.0", &replacement);
    rewrite_release_digest(&metadata, "ſha256", false);
    let api = format!("file://{}", metadata.display());

    let update_dir = ws.root.join("update-install");
    fs::create_dir_all(&update_dir).unwrap();
    let updated = update_dir.join("tink");
    fs::copy(assert_cmd::cargo::cargo_bin!("tink"), &updated).unwrap();
    fs::set_permissions(&updated, fs::Permissions::from_mode(0o755)).unwrap();
    let update_before = fs::read(&updated).unwrap();
    Command::new(&updated)
        .arg("update")
        .env("TINK_RELEASES_API", &api)
        .env("TINK_HOME", &ws.inventory)
        .assert()
        .failure()
        .stderr(predicate::str::contains(
            "release asset digest must use sha256:<hex>",
        ));
    assert_eq!(fs::read(&updated).unwrap(), update_before);

    let install_dir = ws.root.join("script-install");
    fs::create_dir_all(&install_dir).unwrap();
    let installed = install_dir.join("tink");
    fs::copy(assert_cmd::cargo::cargo_bin!("tink"), &installed).unwrap();
    fs::set_permissions(&installed, fs::Permissions::from_mode(0o755)).unwrap();
    let install_before = fs::read(&installed).unwrap();
    Command::new("sh")
        .arg(Path::new(env!("CARGO_MANIFEST_DIR")).join("install.sh"))
        .env("TINK_INSTALL_DIR", &install_dir)
        .env("TINK_RELEASES_API", &api)
        .assert()
        .failure()
        .stderr(predicate::str::contains("invalid SHA-256 digest"));
    assert_eq!(fs::read(&installed).unwrap(), install_before);
}

// --- G*: GitHub source inspection ---

fn inspect_fixture() -> (Workspace, PathBuf, Vec<(String, String)>) {
    let ws = Workspace::new();
    let repo = ws.root.join("skills-repo");
    init_repo(&repo);
    for (group, names) in [
        ("deprecated", Vec::<&str>::new()),
        ("engineering", vec!["alpha", "beta"]),
        ("in-progress", vec!["gamma"]),
        ("misc", vec!["delta"]),
        ("productivity", vec!["epsilon"]),
    ] {
        fs::create_dir_all(repo.join("skills").join(group)).unwrap();
        if group == "deprecated" {
            fs::write(
                repo.join("skills").join(group).join("README.md"),
                "empty peer\n",
            )
            .unwrap();
        }
        for name in names {
            write_skill(&repo.join("skills").join(group).join(name), name, "fixture");
        }
    }
    fs::write(repo.join("README.md"), "repository documentation\n").unwrap();
    fs::create_dir_all(repo.join("docs")).unwrap();
    fs::write(repo.join("docs").join("overview.md"), "unrelated sibling\n").unwrap();
    commit_all(&repo, "fixture");
    let public = "https://github.com/example/skills.git";
    let redirect = github_redirect(&repo, public);
    (ws, repo, redirect)
}

#[test]
fn g1_inspect_repository_reports_groups_empty_peers_and_sorted_skills() {
    let (ws, _repo, redirect) = inspect_fixture();
    let project = ws.project("app");
    let mut cmd = ws.cmd(&project);
    cmd.args(["inspect", "https://github.com/example/skills"]);
    for (key, value) in &redirect {
        cmd.env(key, value);
    }
    cmd.assert().success().stdout(
        predicate::str::contains("Skillsets (5, 5 member skills)")
            .and(predicate::str::contains("deprecated-skillset"))
            .and(predicate::str::contains("empty structural candidate"))
            .and(predicate::str::contains("engineering-skillset"))
            .and(predicate::str::contains("skills/engineering/"))
            .and(predicate::str::contains("    alpha"))
            .and(predicate::str::contains("skills/productivity/"))
            .and(predicate::str::contains("    epsilon"))
            .and(predicate::str::contains("Standalone skills (0)")),
    );
    assert!(!project.join(".agents").exists());
    assert!(!ws.inventory.exists());
}

#[test]
fn g2_inspect_group_tree_limits_boundary() {
    let (ws, _repo, redirect) = inspect_fixture();
    let project = ws.project("app");
    let mut cmd = ws.cmd(&project);
    cmd.args([
        "inspect",
        "https://github.com/example/skills/tree/master/skills/productivity",
    ]);
    for (key, value) in &redirect {
        cmd.env(key, value);
    }
    cmd.assert().success().stdout(
        predicate::str::contains("Skillsets (1, 1 member skills)")
            .and(predicate::str::contains("productivity-skillset"))
            .and(predicate::str::contains("skills/productivity/"))
            .and(predicate::str::contains("    epsilon"))
            .and(predicate::str::contains("Standalone skills (0)"))
            .and(predicate::str::contains("engineering/alpha").not()),
    );
}

#[test]
fn g3_inspect_skill_tree_reports_one_skill_and_no_skillset() {
    let (ws, _repo, redirect) = inspect_fixture();
    let project = ws.project("app");
    let mut cmd = ws.cmd(&project);
    cmd.args([
        "inspect",
        "https://github.com/example/skills/tree/master/skills/productivity/epsilon",
    ]);
    for (key, value) in &redirect {
        cmd.env(key, value);
    }
    cmd.assert().success().stdout(
        predicate::str::contains("Skillsets (0, 0 member skills)")
            .and(predicate::str::contains("Standalone skills (1)"))
            .and(predicate::str::contains("epsilon")),
    );
}

#[test]
fn g4_inspect_infers_wrapper_without_skills_convention() {
    let ws = Workspace::new();
    let repo = ws.root.join("wrapped-repo");
    init_repo(&repo);
    write_skill(
        &repo.join("bundles").join("one").join("first"),
        "first",
        "fixture",
    );
    write_skill(
        &repo.join("bundles").join("two").join("second"),
        "second",
        "fixture",
    );
    commit_all(&repo, "wrapped fixture");
    let public = "https://github.com/example/wrapped.git";
    let redirect = github_redirect(&repo, public);
    let project = ws.project("app");
    let mut cmd = ws.cmd(&project);
    cmd.args(["inspect", public]);
    for (key, value) in &redirect {
        cmd.env(key, value);
    }
    cmd.assert().success().stdout(
        predicate::str::contains("Skillsets (2, 2 member skills)")
            .and(predicate::str::contains("one-skillset"))
            .and(predicate::str::contains("two-skillset")),
    );
}

#[test]
fn g4b_inspect_flat_repository_does_not_expose_checkout_directory_name() {
    let ws = Workspace::new();
    let repo = ws.root.join("flat-repo");
    init_repo(&repo);
    write_skill(&repo.join("alpha"), "alpha", "fixture");
    write_skill(&repo.join("beta"), "beta", "fixture");
    commit_all(&repo, "flat fixture");
    let public = "https://github.com/example/flat.git";
    let redirect = github_redirect(&repo, public);
    let project = ws.project("app");
    let mut cmd = ws.cmd(&project);
    cmd.args(["inspect", public]);
    for (key, value) in &redirect {
        cmd.env(key, value);
    }
    cmd.assert().success().stdout(
        predicate::str::contains("Skillsets (1, 2 member skills)")
            .and(predicate::str::contains("(unnamed proposal)"))
            .and(predicate::str::contains("repository-skillset").not())
            .and(predicate::str::contains("invalid skillset folder name").not()),
    );
}

#[test]
fn g4c_inspect_mixed_root_refuses_to_collapse_unrelated_levels() {
    let ws = Workspace::new();
    let repo = ws.root.join("mixed-repo");
    init_repo(&repo);
    write_skill(&repo.join("template"), "template", "fixture");
    write_skill(&repo.join("skills/alpha"), "alpha", "fixture");
    write_skill(&repo.join("skills/beta"), "beta", "fixture");
    commit_all(&repo, "mixed fixture");
    let public = "https://github.com/example/mixed.git";
    let redirect = github_redirect(&repo, public);
    let project = ws.project("app");
    let mut cmd = ws.cmd(&project);
    cmd.args(["inspect", public]);
    for (key, value) in &redirect {
        cmd.env(key, value);
    }
    cmd.assert().success().stdout(
        predicate::str::contains("Skillsets (0, 0 member skills)")
            .and(predicate::str::contains("Standalone skills (3)"))
            .and(predicate::str::contains("mixed skill layout"))
            .and(predicate::str::contains("narrower GitHub tree URL")),
    );
}

#[test]
fn g4d_inspect_reserves_skills_as_a_collection_root() {
    let ws = Workspace::new();
    let repo = ws.root.join("collection-repo");
    init_repo(&repo);
    write_skill(&repo.join("skills/alpha"), "alpha", "fixture");
    write_skill(&repo.join("skills/beta"), "beta", "fixture");
    commit_all(&repo, "collection fixture");
    let public = "https://github.com/example/collection.git";
    let redirect = github_redirect(&repo, public);
    let project = ws.project("app");

    for url in [
        public,
        "https://github.com/example/collection/tree/master/skills",
    ] {
        let mut cmd = ws.cmd(&project);
        cmd.args(["inspect", url]);
        for (key, value) in &redirect {
            cmd.env(key, value);
        }
        cmd.assert().success().stdout(
            predicate::str::contains("Skillsets (0, 0 member skills)")
                .and(predicate::str::contains("Standalone skills (2)"))
                .and(predicate::str::contains("skills-skillset").not()),
        );
    }
}

#[test]
fn g4e_inspect_does_not_propose_an_overlong_canonical_skillset_name() {
    let ws = Workspace::new();
    let repo = ws.root.join("long-name-repo");
    init_repo(&repo);
    let group = "a".repeat(64);
    write_skill(
        &repo.join("skills").join(&group).join("alpha"),
        "alpha",
        "fixture",
    );
    write_skill(&repo.join("skills/short/beta"), "beta", "fixture");
    commit_all(&repo, "long group name");
    let public = "https://github.com/example/long-name.git";
    let redirect = github_redirect(&repo, public);
    let project = ws.project("app");
    let mut cmd = ws.cmd(&project);
    cmd.args(["inspect", public]);
    for (key, value) in &redirect {
        cmd.env(key, value);
    }
    cmd.assert().success().stdout(
        predicate::str::contains("(unnamed proposal)")
            .and(predicate::str::contains("no valid canonical skillset name"))
            .and(predicate::str::contains(format!("{group}-skillset")).not()),
    );
}

#[test]
fn g5_inspect_reports_duplicate_names_and_invalid_candidates() {
    let ws = Workspace::new();
    let repo = ws.root.join("diagnostic-repo");
    init_repo(&repo);
    write_skill(
        &repo.join("skills").join("one").join("same"),
        "same",
        "fixture",
    );
    write_skill(
        &repo.join("skills").join("two").join("same"),
        "same",
        "fixture",
    );
    fs::create_dir_all(repo.join("skills").join("bad")).unwrap();
    fs::write(
        repo.join("skills").join("bad").join("SKILL.md"),
        "not frontmatter\n",
    )
    .unwrap();
    commit_all(&repo, "diagnostics");
    let public = "https://github.com/example/diagnostics.git";
    let redirect = github_redirect(&repo, public);
    let project = ws.project("app");
    let mut cmd = ws.cmd(&project);
    cmd.args(["inspect", public]);
    for (key, value) in &redirect {
        cmd.env(key, value);
    }
    cmd.assert().success().stdout(
        predicate::str::contains("Diagnostics (2)")
            .and(predicate::str::contains("duplicate skill name: same"))
            .and(predicate::str::contains("invalid SKILL.md"))
            .and(predicate::str::contains(repo.to_string_lossy().as_ref()).not()),
    );
}

#[test]
fn g6_inspect_empty_boundary_succeeds() {
    let ws = Workspace::new();
    let repo = ws.root.join("empty-repo");
    init_repo(&repo);
    fs::create_dir_all(repo.join("empty")).unwrap();
    fs::write(repo.join("empty").join("README.md"), "empty\n").unwrap();
    commit_all(&repo, "empty");
    let public = "https://github.com/example/empty.git";
    let redirect = github_redirect(&repo, public);
    let project = ws.project("app");
    let mut cmd = ws.cmd(&project);
    cmd.args([
        "inspect",
        "https://github.com/example/empty/tree/master/empty",
    ]);
    for (key, value) in &redirect {
        cmd.env(key, value);
    }
    cmd.assert().success().stdout(
        predicate::str::contains("Skillsets (0, 0 member skills)")
            .and(predicate::str::contains("Standalone skills (0)"))
            .and(predicate::str::contains("Diagnostics (1)"))
            .and(predicate::str::contains("no valid skills found"))
            .and(predicate::str::contains("could not be inferred").not()),
    );
}

#[test]
fn g7_inspect_rejects_bad_urls_refs_and_boundaries() {
    let ws = Workspace::new();
    let project = ws.project("app");
    ws.cmd(&project)
        .args(["inspect", "https://gitlab.com/example/skills"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("GitHub"));

    let repo = ws.root.join("failure-repo");
    init_repo(&repo);
    fs::write(repo.join("README.md"), "failure\n").unwrap();
    commit_all(&repo, "failure");
    let public = "https://github.com/example/failure.git";
    let redirect = github_redirect(&repo, public);
    for url in [
        "https://github.com/example/failure/tree/missing",
        "https://github.com/example/failure/tree/master/missing",
    ] {
        let mut cmd = ws.cmd(&project);
        cmd.args(["inspect", url]);
        for (key, value) in &redirect {
            cmd.env(key, value);
        }
        cmd.assert()
            .failure()
            .stderr(predicate::str::contains("ref").or(predicate::str::contains("boundary")));
    }

    git(&repo, &["checkout", "-b", "feature/grouped"]);
    fs::create_dir_all(repo.join("skills")).unwrap();
    write_skill(&repo.join("skills/alpha"), "alpha", "slash ref");
    commit_all(&repo, "slash ref");
    let mut cmd = ws.cmd(&project);
    cmd.args([
        "inspect",
        "https://github.com/example/failure/tree/feature/grouped/skills",
    ]);
    for (key, value) in &redirect {
        cmd.env(key, value);
    }
    cmd.assert().failure().stderr(
        predicate::str::contains("ambiguous")
            .and(predicate::str::contains("feature/grouped"))
            .and(predicate::str::contains("contains `/`")),
    );
}

#[test]
fn g8_inspect_preserves_project_and_home_bytes() {
    let (ws, _repo, redirect) = inspect_fixture();
    let project = ws.project("app");
    fs::create_dir_all(project.join(".tink")).unwrap();
    fs::write(project.join(".tink").join("marker"), "project").unwrap();
    fs::create_dir_all(&ws.inventory).unwrap();
    fs::write(ws.inventory.join("marker"), "home").unwrap();
    let project_before = fs::read(project.join(".tink").join("marker")).unwrap();
    let home_before = fs::read(ws.inventory.join("marker")).unwrap();
    let mut cmd = ws.cmd(&project);
    cmd.args(["inspect", "https://github.com/example/skills"]);
    for (key, value) in &redirect {
        cmd.env(key, value);
    }
    cmd.assert().success();
    assert_eq!(
        fs::read(project.join(".tink").join("marker")).unwrap(),
        project_before
    );
    assert_eq!(fs::read(ws.inventory.join("marker")).unwrap(), home_before);
    assert!(!project.join(".agents").exists());
}

#[cfg(unix)]
#[test]
fn g9_inspect_termination_reaps_git_process_group() {
    use std::os::unix::fs::PermissionsExt;
    use std::process::Stdio;

    let ws = Workspace::new();
    let project = ws.project("app");
    let fake_bin = ws.root.join("fake-git-bin");
    fs::create_dir_all(&fake_bin).unwrap();
    let fake_git = fake_bin.join("git");
    fs::write(
        &fake_git,
        b"#!/bin/sh\nprintf started > \"$TINK_GIT_STARTED\"\nsleep 3\nprintf survived > \"$TINK_GIT_SURVIVED\"\nexit 1\n",
    )
    .unwrap();
    fs::set_permissions(&fake_git, fs::Permissions::from_mode(0o755)).unwrap();
    let started = ws.root.join("git-started");
    let survived = ws.root.join("git-survived");
    let path = std::env::join_paths(std::iter::once(fake_bin.clone()).chain(
        std::env::split_paths(&std::env::var_os("PATH").unwrap_or_default()),
    ))
    .unwrap();

    let mut command = StdCommand::new(assert_cmd::cargo::cargo_bin!("tink"));
    command
        .current_dir(&project)
        .args(["inspect", "https://github.com/example/repository"])
        .env("TINK_HOME", &ws.inventory)
        .env("PATH", path)
        .env("TINK_GIT_STARTED", &started)
        .env("TINK_GIT_SURVIVED", &survived)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut child = command.spawn().expect("spawn Git cancellation fixture");
    wait_for_marker(&started, &mut child);
    terminate_process(child.id());
    let output = wait_for_output_bounded(
        child,
        std::time::Duration::from_secs(10),
        "Git cancellation fixture",
    );

    assert!(!output.status.success());
    std::thread::sleep(std::time::Duration::from_millis(3300));
    assert!(!survived.exists(), "terminated Tink left Git running");
}

#[cfg(unix)]
#[test]
fn g10_inspect_escapes_terminal_controls_in_remote_paths() {
    let ws = Workspace::new();
    let project = ws.project("app");
    let repo = ws.root.join("control-path-repo");
    init_repo(&repo);
    let unsafe_name = "evil\u{1b}[31m\nrow\tpart";
    write_skill(&repo.join(unsafe_name), "safe-skill", "control fixture");
    commit_all(&repo, "control path");
    let public = "https://github.com/example/control-path.git";
    let redirect = github_redirect(&repo, public);
    let mut command = ws.cmd(&project);
    command.args(["inspect", "https://github.com/example/control-path"]);
    for (key, value) in &redirect {
        command.env(key, value);
    }
    let output = command.output().expect("inspect control path fixture");
    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();

    assert!(
        !stdout.contains('\u{1b}'),
        "raw escape reached stdout: {stdout:?}"
    );
    assert!(
        stdout.contains("evil\\x1b[31m\\nrow\\tpart"),
        "escaped path missing from stdout: {stdout:?}"
    );
}
