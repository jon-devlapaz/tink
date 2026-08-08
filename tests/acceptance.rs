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

    fn skill_path(project: &Path, name: &str) -> PathBuf {
        project.join(".agents").join("skills").join(name)
    }

    fn catalog_meta(&self, project_name: &str) -> PathBuf {
        self.inventory
            .join("catalog")
            .join("by-project")
            .join(project_name)
            .join("meta.json")
    }

    fn library_skill(&self, skill: &str) -> PathBuf {
        self.inventory.join("skills").join(skill)
    }

    fn assert_cataloged(&self, project_name: &str, skill: &str) {
        let raw = fs::read_to_string(self.catalog_meta(project_name))
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
            !self
                .inventory
                .join("catalog")
                .join("by-project")
                .join(project_name)
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
fn c4_check_fails_with_corrupt_installed_skill() {
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
        "version = 1\nskills = []\n",
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
fn m2_skill_verify_requires_manifest() {
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
fn l6_skill_list_catalog_skips_malformed_meta_entries() {
    let ws = Workspace::new();
    let project = ws.project("app");

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

// --- D*: destroy ---

#[test]
fn d1_destroy_yes_removes_agents_zen_agents_md() {
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
    assert!(ws.inventory.join("layout.json").is_file());
    ws.assert_cataloged("app", "manage-tink");
    ws.assert_cataloged("app", "extra-skill");

    ws.cmd(&project)
        .args(["destroy", "--yes"])
        .assert()
        .success();

    assert!(!project.join(".agents").exists());
    assert!(!project.join("ZEN.md").exists());
    assert!(!project.join("AGENTS.md").exists());
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

// --- S*: safety ---

#[test]
fn s1_init_does_not_create_git_repo() {
    let ws = Workspace::new();
    let project = ws.project("app");
    assert!(!project.join(".git").exists());
    ws.cmd(&project).arg("init").assert().success();
    assert!(!project.join(".git").exists());
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
        fs::set_permissions(&staged_bin, fs::Permissions::from_mode(0o755)).unwrap();
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

    let archive_url = format!("file://{}", archive.display());
    let meta = dir.join("release.json");
    let body = format!(
        r#"{{
  "tag_name": "v{version}",
  "assets": [
    {{
      "name": "{asset}",
      "browser_download_url": "{archive_url}"
    }}
  ]
}}"#
    );
    fs::write(&meta, body).unwrap();
    meta
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
        .stderr(predicate::str::contains("could not download"));
}

#[test]
fn u2_update_reports_up_to_date_for_current_version() {
    let ws = Workspace::new();
    let fixture = ws.root.join("release-fixture");
    fs::create_dir_all(&fixture).unwrap();
    let version = package_version();
    let bin = assert_cmd::cargo::cargo_bin!("tink");
    let meta = write_release_fixture(&fixture, &version, &bin);
    let api = format!("file://{}", meta.display());

    let install_dir = ws.root.join("install");
    fs::create_dir_all(&install_dir).unwrap();
    let installed = install_dir.join("tink");
    fs::copy(&bin, &installed).unwrap();
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
    let meta = write_release_fixture(&fixture, "99.0.0", &bin);
    let api = format!("file://{}", meta.display());

    let install_dir = ws.root.join("install");
    fs::create_dir_all(&install_dir).unwrap();
    let installed = install_dir.join("tink");
    fs::copy(&bin, &installed).unwrap();
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
    let after = fs::metadata(&installed).unwrap().modified().unwrap();
    assert!(after >= before);
}
