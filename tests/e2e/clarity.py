#!/usr/bin/env python3
"""E2E: clarity batch. Messages and hints only: every case asserts the new fix-oriented
wording AND that exit codes (and refusal semantics) are unchanged.

Every case runs this checkout's `tink` (target/debug/tink) in a throwaway git project
with an isolated TINK_HOME. ~/.tink-library is never touched. No network: remote
adds use a local git repo via GIT_CONFIG insteadOf.

Run:   python3 tests/e2e/clarity.py [--only K1,K2]
Env:   TINK_E2E_SKIP_BUILD=1  reuse target/debug/tink
Artifact (repeatable, overwritten each run): target/e2e/clarity.json
Exit:  0 all cases pass, 1 any FAIL.
"""
import json
import os
import subprocess
import sys
import tempfile
import time
from pathlib import Path

REPO = Path(__file__).resolve().parents[2]
ARTIFACT = REPO / "target" / "e2e" / "clarity.json"

SOURCE = "https://github.com/e2e-org/fixtures.git"
REVISION = "a" * 40


def skill_md(name, rule=None):
    fm = f"---\nname: {name}\ndescription: Fixture skill {name}.\n"
    if rule is not None:
        fm += f"rule: {rule}\n"
    return fm + f"---\n# {name}\n\nBody text.\n"


def _tail(p):
    return f"exit={p.returncode} stdout={p.stdout.strip()[:600]!r} stderr={p.stderr.strip()[:600]!r}"


class Env:
    def __init__(self, root: Path, init=True):
        self.root = root
        self.proj = root / "proj"
        self.home = root / "home"
        self.bin = root / "bin"
        for d in (self.proj, self.bin):
            d.mkdir(parents=True)
        (self.bin / "tink").symlink_to(REPO / "target" / "debug" / "tink")
        self.env = dict(os.environ, TINK_HOME=str(self.home), PATH=f"{self.bin}:{os.environ['PATH']}")
        self.run("git", "init", "-q", ".")
        if init:
            self.run("tink", "init", "--no-tink-skills", "--no-manage-tink", "--no-sdlc")
        self.lib = self.home / "skills"
        self.lib.mkdir(parents=True, exist_ok=True)
        (self.home / "skillsets").mkdir(parents=True, exist_ok=True)

    def run(self, *cmd, cwd=None, env=None):
        return subprocess.run(list(cmd), cwd=cwd or self.proj, env=env or self.env, capture_output=True, text=True, check=False)

    def skill(self, name, rule=None):
        d = self.lib / name
        d.mkdir(parents=True, exist_ok=True)
        (d / "SKILL.md").write_text(skill_md(name, rule), encoding="utf-8")
        return d

    def pin(self, skillset, mem, required=None, **over):
        doc = {"source": SOURCE, "revision": REVISION, "sourceRoot": "skills", "members": mem}
        if required is not None:
            doc["required"] = required
        doc.update(over)
        p = self.home / "skillsets" / f"{skillset}.json"
        p.write_text(json.dumps(doc, indent=2) + "\n")
        return p

    def git_url_env(self, repo: Path):
        return dict(
            self.env,
            GIT_CONFIG_COUNT="1",
            GIT_CONFIG_KEY_0=f"url.file://{repo.resolve()}.insteadOf",
            GIT_CONFIG_VALUE_0="https://github.com/e2e-org/upstream.git",
            GIT_TERMINAL_PROMPT="0",
        )


def missing(text, *needles):
    return [n for n in needles if n not in text]


# ---- cases: each returns (ok, detail) ---------------------------------------------

def k1_doctor_missing_layout_says_init(e: Env):
    """K1: doctor's `fail skills` row for a missing layout tells the user to run `tink init`; still exit 1."""
    p = e.run("tink", "doctor")
    want = "fail skills Missing .agents/skills; run `tink init`"
    return p.returncode == 1 and want in p.stdout, _tail(p)


def k2_doctor_library_warn_says_approve(e: Env):
    """K2: doctor's library warn row appends the approve fix; still a warn (exit 0)."""
    e.skill("alpha")
    p = e.run("tink", "doctor")
    want = "; review, then run `tink library approve <name>` (or --all)"
    row = [l for l in p.stdout.splitlines() if l.startswith("warn library")]
    return p.returncode == 0 and bool(row) and want in row[0] and "1 unapproved" in row[0], _tail(p)


def k3_bad_pin_names_path_and_value(e: Env):
    """K3: invalid_pin refusals name the pin file and the offending value; exit 2, code unchanged."""
    e.skill("a")
    e.approve = e.run("tink", "library", "approve", "--all")
    errs = []
    cases = [
        ("revision", {"revision": "deadbeef"}, 'Skillset revision must be a full Git object ID (got "deadbeef" in {path})'),
        ("sourceRoot", {"sourceRoot": "/abs"}, 'Skillset sourceRoot must be a non-empty relative POSIX path (got "/abs" in {path})'),
        ("source", {"source": "http://x/y"}, 'Skillset source must be an absolute HTTPS Git URL (got "http://x/y" in {path})'),
        ("members", {"members": []}, "Skillset members must not be empty (in {path})"),
    ]
    for label, over, tmpl in cases:
        path = e.pin("build-skillset", ["a"], ["a"], **over)
        p = e.run("tink", "use", "build")
        want = "tink use: " + tmpl.format(path=path) + " [invalid_pin]"
        if p.returncode != 2 or want not in p.stderr:
            errs.append(f"{label}: {_tail(p)} want={want!r}")
    return not errs, " | ".join(errs)


def k4_library_approve_missing_points_to_list(e: Env):
    """K4: `library approve <missing>` points at `tink library list`; exit unchanged (1)."""
    p = e.run("tink", "library", "approve", "ghost")
    want = "Skill 'ghost' not found in library; see `tink library list`"
    return p.returncode == 1 and want in p.stderr, _tail(p)


def _upstream(e: Env, files):
    repo = e.root / "upstream"
    repo.mkdir()

    def git(*a):
        r = subprocess.run(["git", *a], cwd=repo, capture_output=True, text=True)
        assert r.returncode == 0, r.stderr
        return r.stdout.strip()

    git("init", "-q")
    git("config", "user.email", "e2e@example.com")
    git("config", "user.name", "e2e")
    for rel, text in files.items():
        (repo / rel).parent.mkdir(parents=True, exist_ok=True)
        (repo / rel).write_text(text)
    git("add", "-A")
    git("commit", "-qm", "v1")
    return repo, git("rev-parse", "--abbrev-ref", "HEAD")


def k5_skillset_add_no_members_hides_tmp_path(e: Env):
    """K5: `skillset add` with no members prints the boundary relative to the repo root plus an inspect hint, never the temp checkout path."""
    repo, branch = _upstream(e, {"README.md": "nothing here\n", "skills/.keep": ""})
    genv = e.git_url_env(repo)
    errs = []
    for url, boundary in [
        ("https://github.com/e2e-org/upstream", "."),
        (f"https://github.com/e2e-org/upstream/tree/{branch}/skills", "skills"),
    ]:
        p = e.run("tink", "skillset", "add", url, env=genv)
        want = f"No member skills found in boundary: {boundary}; run `tink inspect <url>` to see installable skills and skillsets"
        bad = p.returncode != 1 or want not in p.stderr or "/private/" in p.stderr or "/var/" in p.stderr or "/tmp" in p.stderr
        if bad:
            errs.append(f"{boundary}: {_tail(p)} want={want!r}")
    return not errs, " | ".join(errs)


def _use_ready(e: Env):
    e.skill("a", "Rule A.")
    e.skill("b", "Rule B.")
    e.pin("gates-skillset", ["a", "b"], ["a", "b"])
    e.agents = e.proj / "AGENTS.md"
    e.agents.write_text("# Project\n")
    return e.run("tink", "library", "approve", "--all")


def k6_use_check_mismatches_end_with_fix(e: Env):
    """K6: `use --check` mismatch reasons end with the rewrite fix (or the approve fix for trust reasons); exit 1 unchanged."""
    _use_ready(e)
    fix = "run `tink use gates` to rewrite it"
    errs = []
    # missing block
    p = e.run("tink", "use", "gates", "--check")
    if p.returncode != 1 or f"missing block: no tink:rules block in {e.agents}; {fix}" not in p.stderr:
        errs.append(f"missing: {_tail(p)}")
    # compile, then differs
    c = e.run("tink", "use", "gates")
    text = e.agents.read_text()
    e.agents.write_text(text.replace("Rule A.", "Rule A tampered."))
    p = e.run("tink", "use", "gates", "--check")
    if p.returncode != 1 or f"block differs: {e.agents} does not match the compiled rules; {fix}" not in p.stderr:
        errs.append(f"differs: {c.returncode} {_tail(p)}")
    e.agents.write_text(text)
    # unapproved keeps trust hint
    (e.lib / "a" / "SKILL.md").write_text(skill_md("a", "Rule A.") + "\nchanged\n")
    p = e.run("tink", "use", "gates", "--check")
    want = "unapproved a (digest_mismatch); review, then run `tink library approve a`"
    if p.returncode != 1 or want not in p.stderr:
        errs.append(f"unapproved: {_tail(p)}")
    # snapshot differs / missing
    e.run("tink", "library", "approve", "--all")
    c = e.run("tink", "use", "gates", "--snapshot", "snap")
    snap_out = c.stdout + c.stderr
    p2 = e.run("tink", "use", "gates", "--check", "--snapshot", "snap")
    if p2.returncode != 0:
        errs.append(f"snapshot baseline not clean: {_tail(c)} {_tail(p2)}")
    else:
        snaps = list((e.proj / "snap").glob("*.md"))
        if snaps:
            snaps[0].write_text("tampered\n")
            p3 = e.run("tink", "use", "gates", "--check", "--snapshot", "snap")
            if p3.returncode != 1 or f"snapshot differs: {snaps[0]}; {fix}" not in p3.stderr:
                errs.append(f"snapshot differs: {_tail(p3)}")
            snaps[0].unlink()
            p4 = e.run("tink", "use", "gates", "--check", "--snapshot", "snap")
            if p4.returncode != 1 or f"snapshot missing: {snaps[0]}; {fix}" not in p4.stderr:
                errs.append(f"snapshot missing: {_tail(p4)}")
        else:
            errs.append(f"no snapshot rules file found ({snap_out[:200]})")
    return not errs, " | ".join(errs)


def k7_approve_all_prints_summary(e: Env):
    """K7: `library approve --all` prints one summary line after the per-skill lines (only when N>0); exit 0."""
    e.skill("one")
    e.skill("two")
    p = e.run("tink", "library", "approve", "--all")
    summary = "Approved 2 skill(s). Approval records a digest only; review skills you have not read."
    lines = p.stdout.strip().splitlines()
    ok = p.returncode == 0 and lines and lines[-1] == summary and p.stdout.count("Approved ") == 3
    # zero skills: no summary
    empty = e.run("tink", "library", "approve", "--all", env=dict(e.env, TINK_HOME=str(e.root / "home2")))
    ok = ok and "Approval records a digest only" not in empty.stdout
    # single-skill approve: unchanged, no summary
    single = e.run("tink", "library", "approve", "one")
    ok = ok and single.returncode == 0 and "Approval records a digest only" not in single.stdout
    return bool(ok), f"{_tail(p)} | empty:{_tail(empty)} | single:{_tail(single)}"


def k8_mount_reports_gitignore_creation(e: Env):
    """K8: plain `mount` prints a one-line note only when it creates .tink/.gitignore; --json and repeat mounts stay silent."""
    e.skill("runner")
    (e.lib / "runner" / "scripts").mkdir()
    (e.lib / "runner" / "scripts" / "run.sh").write_text("#!/bin/sh\necho hi\n")
    note = "Created .tink/.gitignore; commit it so .tink/.active stays ignored."
    errs = []
    gi = e.proj / ".tink" / ".gitignore"
    if gi.exists():
        gi.unlink()
    p = e.run("tink", "mount", "runner")
    if p.returncode != 0 or p.stdout.count(note) != 1 or not gi.exists():
        errs.append(f"first: {_tail(p)}")
    p = e.run("tink", "mount", "runner")
    if p.returncode != 0 or note in p.stdout:
        errs.append(f"repeat: {_tail(p)}")
    gi.unlink()
    e.run("tink", "library", "approve", "runner")
    p = e.run("tink", "mount", "runner", "--json")
    try:
        json.loads(p.stdout)
    except json.JSONDecodeError:
        errs.append(f"json not parseable: {_tail(p)}")
    if p.returncode != 0 or note in p.stdout + p.stderr:
        errs.append(f"json: {_tail(p)}")
    return not errs, " | ".join(errs)


def _no_init(fn):
    fn.no_init = True
    return fn


k1_doctor_missing_layout_says_init = _no_init(k1_doctor_missing_layout_says_init)

CASES = [
    ("K1", k1_doctor_missing_layout_says_init),
    ("K2", k2_doctor_library_warn_says_approve),
    ("K3", k3_bad_pin_names_path_and_value),
    ("K4", k4_library_approve_missing_points_to_list),
    ("K5", k5_skillset_add_no_members_hides_tmp_path),
    ("K6", k6_use_check_mismatches_end_with_fix),
    ("K7", k7_approve_all_prints_summary),
    ("K8", k8_mount_reports_gitignore_creation),
]


def main() -> int:
    only = None
    args = sys.argv[1:]
    for i, a in enumerate(args):
        if a.startswith("--only"):
            only = set((a.split("=", 1)[1] if "=" in a else args[i + 1]).split(","))
    if os.environ.get("TINK_E2E_SKIP_BUILD") != "1":
        b = subprocess.run(["cargo", "build", "-q"], cwd=REPO, capture_output=True, text=True)
        if b.returncode != 0:
            print(b.stderr, file=sys.stderr)
            return 1
    tink = REPO / "target" / "debug" / "tink"
    report = {
        "ran_at": time.strftime("%Y-%m-%dT%H:%M:%S%z"),
        "tink": subprocess.run([str(tink), "version"], capture_output=True, text=True).stdout.strip(),
        "cases": [],
    }
    failed = 0
    for cid, fn in CASES:
        if only and cid not in only:
            continue
        with tempfile.TemporaryDirectory(prefix="tink-e2e-clarity-") as t:
            try:
                ok, detail = fn(Env(Path(t).resolve(), init=not getattr(fn, "no_init", False)))
            except Exception as exc:  # harness error counts as failure
                ok, detail = False, f"harness error: {exc!r}"
        report["cases"].append({"id": cid, "result": "PASS" if ok else "FAIL", "claim": fn.__doc__.strip(), "detail": detail})
        print(f"{'PASS' if ok else 'FAIL'} {cid}: {fn.__doc__.strip()}")
        if not ok:
            print(f"     {detail[:1500]}")
            failed += 1
    report["failures"] = failed
    ARTIFACT.parent.mkdir(parents=True, exist_ok=True)
    ARTIFACT.write_text(json.dumps(report, indent=2))
    print(f"artifact: {ARTIFACT.relative_to(REPO)}  failures: {failed}")
    return 1 if failed else 0


if __name__ == "__main__":
    sys.exit(main())
