//! Acceptance rows from ACCEPTANCE.md. These must fail until each row is implemented.

use assert_cmd::cargo::cargo_bin_cmd;
use assert_cmd::Command;
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

    fn catalog_skills(&self, project: &Path) -> PathBuf {
        self.inventory
            .join("skills")
            .join("by-project")
            .join(project.file_name().unwrap())
            .join("skills")
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
    let file_url = format!("file://{}", local_repo.canonicalize().expect("canon").display());
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
        .args(["init", "--no-zen", "--no-twotink"])
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
    assert!(ws.inventory.join("skills").join("by-project").is_dir());
    let layout = fs::read_to_string(ws.inventory.join("layout.json")).unwrap();
    assert!(layout.contains("tink-skill-inventory"));
}

#[test]
fn i5_init_with_zen_writes_agents_reference() {
    let ws = Workspace::new();
    let project = ws.project("app");
    ws.cmd(&project)
        .args(["init", "--with-zen", "--no-twotink"])
        .assert()
        .success();
    assert!(project.join("ZEN.md").is_file());
    let agents = fs::read_to_string(project.join("AGENTS.md")).unwrap();
    assert!(agents.contains("[ZEN.md](ZEN.md)"));
    ws.cmd(&project).arg("check").assert().success();
}

// --- A*: local add ---

#[test]
fn a1_add_local_skill_installs_and_deposits() {
    let ws = Workspace::new();
    let project = ws.project("app");
    ws.cmd(&project).arg("init").assert().success();
    let source = ws.root.join("demo-skill");
    write_skill(&source, "demo-skill", "Do the work.");
    ws.cmd(&project)
        .args(["add", source.to_str().unwrap()])
        .assert()
        .success();
    let installed = Workspace::skill_path(&project, "demo-skill");
    assert!(installed.join("SKILL.md").is_file());
    assert!(
        ws.catalog_skills(&project)
            .join("demo-skill")
            .join("SKILL.md")
            .is_file()
    );
}

#[test]
fn a2_add_identical_is_noop() {
    let ws = Workspace::new();
    let project = ws.project("app");
    ws.cmd(&project).arg("init").assert().success();
    let source = ws.root.join("demo-skill");
    write_skill(&source, "demo-skill", "Do the work.");
    ws.cmd(&project)
        .args(["add", source.to_str().unwrap()])
        .assert()
        .success();
    let first = fs::read(Workspace::skill_path(&project, "demo-skill").join("SKILL.md")).unwrap();
    ws.cmd(&project)
        .args(["add", source.to_str().unwrap()])
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
        .args(["add", source.to_str().unwrap()])
        .assert()
        .success();
    write_skill(&source, "demo-skill", "changed");
    ws.cmd(&project)
        .args(["add", source.to_str().unwrap()])
        .assert()
        .failure()
        .stderr(predicate::str::contains("Refusing to overwrite"));
    let body = fs::read_to_string(Workspace::skill_path(&project, "demo-skill").join("SKILL.md"))
        .unwrap();
    assert!(body.contains("original"));
    assert!(!body.contains("changed"));
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
        .args(["add", source.to_str().unwrap()])
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
        .args(["add", repo.to_str().unwrap()])
        .assert()
        .failure()
        .stderr(
            predicate::str::contains("alpha")
                .and(predicate::str::contains("beta"))
                .and(predicate::str::contains("--skill").or(predicate::str::contains("skill"))),
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
    cmd.args(["add", "example/skills", "--skill", "remote-skill"]);
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
        .args(["add", "https://gitlab.com/example/skills.git"])
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
        .args(["add", "./relative-missing"])
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
        .args(["add", missing.to_str().unwrap()])
        .assert()
        .failure()
        .stderr(predicate::str::contains("Path does not exist"));
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
        .args(["add", source.to_str().unwrap()])
        .assert()
        .success();
    ws.cmd(&project).arg("check").assert().success();
}

#[test]
fn c2_check_fails_without_skills_dir() {
    let ws = Workspace::new();
    let project = ws.project("app");
    ws.cmd(&project)
        .arg("check")
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
        .arg("check")
        .assert()
        .failure()
        .stderr(predicate::str::contains("symlink").or(predicate::str::contains("Symlink")));
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
    add.args(["add", "example/skills", "--skill", "remote-skill"]);
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
    refresh.args(["refresh", "remote-skill"]);
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
    add.args(["add", "example/skills", "--skill", "remote-skill"]);
    for (k, v) in &redirect {
        add.env(k, v);
    }
    add.assert().success();

    let skill_md = Workspace::skill_path(&project, "remote-skill").join("SKILL.md");
    let mut body = fs::read_to_string(&skill_md).unwrap();
    body.push_str("\ndirty\n");
    fs::write(&skill_md, body).unwrap();

    let mut refresh = ws.cmd(&project);
    refresh.args(["refresh", "remote-skill"]);
    for (k, v) in &redirect {
        refresh.env(k, v);
    }
    refresh
        .assert()
        .failure()
        .stderr(predicate::str::contains("local modifications"));
}

// --- V*: inventory ---

#[test]
fn v1_inventory_list_empty() {
    let ws = Workspace::new();
    let project = ws.project("app");
    ws.cmd(&project).arg("init").assert().success();
    ws.cmd(&project)
        .args(["inventory", "list"])
        .assert()
        .success();
}

#[test]
fn v2_inventory_list_after_add() {
    let ws = Workspace::new();
    let project = ws.project("app");
    ws.cmd(&project).arg("init").assert().success();
    let source = ws.root.join("demo-skill");
    write_skill(&source, "demo-skill", "ok");
    ws.cmd(&project)
        .args(["add", source.to_str().unwrap()])
        .assert()
        .success();
    ws.cmd(&project)
        .args(["inventory", "list"])
        .assert()
        .success()
        .stdout(predicate::str::contains("demo-skill"));
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
