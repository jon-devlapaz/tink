#!/usr/bin/env python3
"""E2E: tink + tink-route interop against the tink-route CLI contract
(`tink-route [--skillset NAME | --anywhere] [--receipt PATH] [--inline-max N] [--json] [--pick] "<task>"`;
the phase decides the shelf via the `tink:rules` block `tink use` writes into AGENTS.md).

Drives this checkout's `tink` (target/debug/tink, built here) and the sibling
tink-route checkout (../tink-route/src) together in a throwaway git project with
an isolated TINK_HOME. Neither the caller's repo nor ~/.tink-library is touched.

Deterministic cases need no network. Live cases (L*) call TypeSafe Jev and are
SKIPped without TYPESAFE_API_KEY.

Cases: D1 empty task, D2 shelf misuse fails open offline, D3 removed flags, D4 --version,
       D5 mount git-ignore (all deterministic); L1 delivery, L2 no-skill, L3 --pick writes
       nothing, L4 --skillset excludes a pin's `required`, L5 the phase decides the shelf and an
       off-shelf task gets a hint, never a delivery (live, <=16 API calls).

Run:   python3 tests/e2e/tink_route_interop.py [--only D1,D3] [--no-live]
Env:   TINK_ROUTE_SRC   path to tink-route `src/` (default ../tink-route/src)
       TINK_E2E_SKIP_BUILD=1  reuse target/debug/tink
Artifact (repeatable, overwritten each run): target/e2e/tink-route-interop.json
Exit:  0 all executed cases pass, 1 any FAIL.
"""
import json
import os
import re
import shutil
import subprocess
import sys
import tempfile
import time
from pathlib import Path

REPO = Path(__file__).resolve().parents[2]
ROUTE_SRC = Path(os.environ.get("TINK_ROUTE_SRC", REPO.parent / "tink-route" / "src")).resolve()
ARTIFACT = REPO / "target" / "e2e" / "tink-route-interop.json"
LIVE_KEY = bool(os.environ.get("TYPESAFE_API_KEY"))

SKILLS = {
    "alpha": "Alpha skill for compiling epistemic matrix audits of what is known and unknown.",
    "beta": "Beta skill for reviewing clean code, naming, and function size in pull requests.",
    "gamma": "Gamma skill for planning database schema migrations and rollback scripts.",
    "eli5": (
        "Explain any topic, code, concept, or error tailored to a specific audience's level. "
        "Use when the user says 'explain like I am five', 'ELI5', or 'dumb it down'."
    ),
}


class Env:
    """One throwaway project + library + PATH shim per case."""

    def __init__(self, root: Path):
        self.root = root
        self.proj = root / "proj"
        self.home = root / "home"
        self.bin = root / "bin"
        for d in (self.proj, self.bin):
            d.mkdir(parents=True)
        (self.bin / "tink").symlink_to(REPO / "target" / "debug" / "tink")
        shim = self.bin / "tink-route"
        shim.write_text(
            "#!/bin/sh\n"
            f'PYTHONPATH="{ROUTE_SRC}" exec "{sys.executable}" -c '
            "'import sys; sys.argv[0]=\"tink-route\"; from tink_route.cli import main; sys.exit(main())' \"$@\"\n"
        )
        shim.chmod(0o755)
        self.env = dict(os.environ, TINK_HOME=str(self.home), PATH=f"{self.bin}:{os.environ['PATH']}")
        self.run("git", "init", "-q", ".")
        self.run("tink", "init")
        skills = self.home / "skills"
        for name, desc in SKILLS.items():
            d = skills / name
            d.mkdir(parents=True, exist_ok=True)
            (d / "SKILL.md").write_text(f"---\nname: {name}\ndescription: {desc}\n---\n# {name}\n", encoding="utf-8")
        self.run("tink", "library", "approve", "--all")

    def snapshot(self):
        """Every path, file content, and symlink target under project + TINK_HOME (git internals excluded)."""
        out = {}
        for base in (self.proj, self.home):
            for dp, dns, fns in os.walk(base):
                dns[:] = sorted(d for d in dns if d != ".git")
                for n in sorted(dns + fns):
                    f = Path(dp) / n
                    key = str(f)
                    if f.is_symlink():
                        out[key] = "L" + os.readlink(f)
                    elif f.is_file():
                        out[key] = "F" + f.read_bytes().hex()
                    else:
                        out[key] = "D"
        return out

    def run(self, *cmd, cwd=None, extra_env=None):
        env = dict(self.env, **(extra_env or {}))
        return subprocess.run(list(cmd), cwd=cwd or self.proj, env=env, capture_output=True, text=True, check=False)


def _tail(p):
    return f"exit={p.returncode} stdout={p.stdout.strip()[:300]!r} stderr={p.stderr.strip()[:300]!r}"


# ---- deterministic cases: each returns (ok, detail) --------------------------------

def d1_empty_task_is_a_two_line_usage_error(e: Env):
    """D1: empty/missing task exits 2 with a two-line usage error, never a usage dump; the sdlc wrapper fails too."""
    rows, ok = [], True
    for args in (["--json", ""], ["--json"], [""], []):
        p = e.run("tink-route", *args)
        lines = [l for l in p.stderr.splitlines() if l.strip()]
        good = p.returncode == 2 and len(lines) == 2 and "options:" not in p.stderr and not p.stdout.strip()
        ok &= good
        rows.append({"args": args, "exit": p.returncode, "stderr_lines": len(lines), "stdout": p.stdout[:60]})
    w = None
    if (REPO / "_system").is_dir():
        shutil.copytree(REPO / "_system", e.proj / "_system")
        w = e.run(sys.executable, "_system/scripts/sdlc.py", "skills", "tink-route", "--", "--json", "")
    if w is not None:
        ok &= w.returncode != 0
    return ok, f"{rows} wrapper_exit={None if w is None else w.returncode}"


def d2_shelf_misuse_fails_open_offline(e: Env):
    """D2: `--anywhere` with `--skillset` is a two-line usage error; a malformed or unresolvable rules block exits 2
    with a plain fail-open sentence and never widens to the whole library (a dummy key proves no routing call is needed)."""
    env = {"TYPESAFE_API_KEY": "dummy-not-a-real-key"}
    rows, ok = {}, True
    p = e.run("tink-route", "--anywhere", "--skillset", "x", "t", extra_env=env)
    lines = [l for l in p.stderr.splitlines() if l.strip()]
    good = p.returncode == 2 and len(lines) == 2 and not p.stdout.strip()
    ok &= good
    rows["anywhere+skillset"] = (p.returncode, len(lines))
    agents = e.proj / "AGENTS.md"
    base = agents.read_text()
    agents.write_text(base + "\n<!-- tink:rules begin skillset=x-skillset digest=abc -->\nrules\n")  # no end marker
    p = e.run("tink-route", "t", extra_env=env)
    good = p.returncode == 2 and "malformed tink:rules block" in p.stdout and "proceed without a skill" in p.stdout
    ok &= good
    rows["unbalanced"] = (p.returncode, p.stdout.strip()[:100])
    agents.write_text(base + "\n<!-- tink:rules begin skillset=nope-skillset digest=abc -->\nrules\n<!-- tink:rules end -->\n")
    p = e.run("tink-route", "t", extra_env=env)
    good = p.returncode == 2 and "nope-skillset" in p.stdout and "proceed without a skill" in p.stdout
    ok &= good
    rows["unknown skillset"] = (p.returncode, p.stdout.strip()[:100])
    agents.write_text(base)
    return ok, str(rows)


def d3_removed_flags_are_rejected(e: Env):
    """D3: the removed surface (-i, --install, --prune, --stage, --multi, --strict) exits 2 and touches nothing."""
    before = e.snapshot()
    rows, ok = [], True
    for args in (["-i", "x"], ["--install", "x"], ["--prune"], ["--stage", "build", "x"], ["--multi", "x"], ["--strict", "x"]):
        p = e.run("tink-route", *args)
        ok &= p.returncode == 2 and not p.stdout.strip()
        rows.append((args, p.returncode))
    ok &= e.snapshot() == before
    return ok, f"{rows} unchanged={e.snapshot() == before}"


def d4_version_matches_sibling_pyproject(e: Env):
    """D4: `tink-route --version` reports the sibling pyproject version."""
    pyproject = ROUTE_SRC.parent / "pyproject.toml"
    m = re.search(r'^version\s*=\s*"([^"]+)"', pyproject.read_text(), re.M)
    p = e.run("tink-route", "--version")
    ok = bool(m) and p.returncode == 0 and m.group(1) in (p.stdout + p.stderr)
    return ok, f"pyproject={m and m.group(1)} {_tail(p)}"


def d5_mount_ignores_active_dir(e: Env):
    """D5: `tink mount` must not leave .tink/.active symlinks untracked-visible to git."""
    m = e.run("tink", "mount", "alpha")
    status = e.run("git", "status", "--porcelain", "--untracked-files=all").stdout
    leaked = [l for l in status.splitlines() if ".tink" in l and ".gitignore" not in l]
    ignored = e.run("git", "check-ignore", ".tink/.active/alpha").returncode == 0
    ok = m.returncode == 0 and ignored and not leaked
    return ok, f"check-ignore={ignored} leaked={leaked} mount:[{_tail(m)}]"


# ---- live cases (TypeSafe Jev; at most 12 API calls in total) -----------------------

ELI5 = "Explain this Rust borrow checker error like I'm five"


def _json(p):
    try:
        return json.loads(p.stdout)
    except json.JSONDecodeError:
        return {}


def l1_default_delivers_eli5(e: Env):
    """L1: a clear ELI5 prompt delivers eli5 on stdout (exit 0, header line, digest == `tink mount --json`, no mount link)."""
    p = e.run("tink-route", ELI5)  # 1-3 API calls
    header = p.stdout.splitlines()[0] if p.stdout else ""
    m = re.match(r"# tink skill: eli5 +\(digest (\S+), \d+ chars, confidence [\d.]+\)", header)
    # Delivery is inline: the route itself must leave no mount link.
    linked = [k for k in e.snapshot() if "/.tink/.active/" in k]
    mount = _json(e.run("tink", "mount", "eli5", "--json"))
    e.run("tink", "unmount", "eli5")
    digest = mount.get("tree_digest")
    digest_ok = bool(m and digest and digest.startswith(m.group(1).rstrip(".…")))  # header may abbreviate
    body_ok = "# eli5" in p.stdout
    ok = p.returncode == 0 and bool(m) and digest_ok and body_ok and not linked
    return ok, f"header={header!r} tree_digest={digest} linked={linked} {_tail(p)}"


def l2_unrelated_prompt_exits_1(e: Env):
    """L2: a prompt no fixture skill covers exits 1 with the no-skill message and delivers nothing."""
    p = e.run("tink-route", "What is the weather in Paris?")  # 1-3 API calls
    ok = p.returncode == 1 and "# tink skill:" not in p.stdout and bool((p.stdout + p.stderr).strip())
    return ok, _tail(p)


def l3_pick_json_decides_and_writes_nothing(e: Env):
    """L3: `--pick --json` on the ELI5 prompt has winner eli5 and leaves project + TINK_HOME byte-identical."""
    before = e.snapshot()
    p = e.run("tink-route", "--pick", "--json", ELI5)  # 1-3 API calls
    j = _json(p)
    after = e.snapshot()
    changed = sorted(k for k in set(before) | set(after) if before.get(k) != after.get(k))
    ok = p.returncode == 0 and j.get("winner") == "eli5" and "contract_version" in j and j.get("status") == "routed" and not changed
    return ok, f"winner={j.get('winner')} status={j.get('status')} changed={changed[:5]}"


def l4_skillset_excludes_required_skills(e: Env):
    """L4: with a project pin whose `required` names eli5, --skillset never routes to eli5 (--pick --json)."""
    sk = e.proj / ".tink" / "skillsets"
    sk.mkdir(parents=True, exist_ok=True)
    (sk / "x-skillset.json").write_text(json.dumps({
        "source": "https://github.com/e2e-org/upstream.git", "revision": "0" * 40, "sourceRoot": "skills",
        "members": ["alpha", "beta", "eli5"], "required": ["eli5"],
    }, indent=2))
    p = e.run("tink-route", "--pick", "--json", "--skillset", "x", ELI5)  # 1-3 API calls
    j = _json(p)
    listed = json.dumps(j.get("shortlist", j.get("candidates", [])))
    ok = p.returncode in (0, 1) and j.get("winner") != "eli5" and "eli5" not in listed
    return ok, f"winner={j.get('winner')} status={j.get('status')} shortlist={listed[:200]} {_tail(p)}"


def l5_phase_decides_the_shelf_and_hints(e: Env):
    """L5: after `tink use x-skillset` the ELI5 task delivers with NO flags; after `tink use y-skillset` (eli5 not on the
    shelf) the same task exits 1 with a Hint naming x-skillset and delivers nothing (no mount link, no payload)."""
    sk = e.proj / ".tink" / "skillsets"
    sk.mkdir(parents=True, exist_ok=True)
    for name, members, required in (("x", ["alpha", "eli5"], ["alpha"]), ("y", ["alpha", "beta"], ["beta"])):
        (sk / f"{name}-skillset.json").write_text(json.dumps({
            "source": "https://github.com/e2e-org/upstream.git", "revision": "0" * 40, "sourceRoot": "skills",
            "members": members, "required": required}, indent=2))
    u = e.run("tink", "use", "x-skillset")
    on = e.run("tink-route", ELI5)  # 1-3 API calls
    on_ok = u.returncode == 0 and on.returncode == 0 and on.stdout.startswith("# tink skill: eli5")
    u2 = e.run("tink", "use", "y-skillset")
    off = e.run("tink-route", ELI5)  # 2-5 API calls (shelf + hint)
    linked = [k for k in e.snapshot() if "/.tink/.active/" in k]
    off_ok = (u2.returncode == 0 and off.returncode == 1 and "y-skillset shelf" in off.stdout
              and "Hint: eli5" in off.stdout and "x-skillset" in off.stdout
              and "# tink skill:" not in off.stdout and "# eli5" not in off.stdout and not linked)
    return on_ok and off_ok, f"on:[{_tail(on)}] off:[{_tail(off)}] linked={linked}"


CASES = [
    ("D1", d1_empty_task_is_a_two_line_usage_error, False),
    ("D2", d2_shelf_misuse_fails_open_offline, False),
    ("D3", d3_removed_flags_are_rejected, False),
    ("D4", d4_version_matches_sibling_pyproject, False),
    ("D5", d5_mount_ignores_active_dir, False),
    ("L1", l1_default_delivers_eli5, True),
    ("L2", l2_unrelated_prompt_exits_1, True),
    ("L3", l3_pick_json_decides_and_writes_nothing, True),
    ("L4", l4_skillset_excludes_required_skills, True),
    ("L5", l5_phase_decides_the_shelf_and_hints, True),
]


def main() -> int:
    only = None
    for a in sys.argv[1:]:
        if a.startswith("--only"):
            only = set((a.split("=", 1)[1] if "=" in a else sys.argv[sys.argv.index(a) + 1]).split(","))
    no_live = "--no-live" in sys.argv
    if not ROUTE_SRC.is_dir():
        print(f"tink-route src not found: {ROUTE_SRC}", file=sys.stderr)
        return 1
    if os.environ.get("TINK_E2E_SKIP_BUILD") != "1":
        b = subprocess.run(["cargo", "build", "-q"], cwd=REPO, capture_output=True, text=True)
        if b.returncode != 0:
            print(b.stderr, file=sys.stderr)
            return 1
    report = {"ran_at": time.strftime("%Y-%m-%dT%H:%M:%S%z"), "tink": None, "cases": []}
    report["tink"] = subprocess.run([str(REPO / "target/debug/tink"), "version"], capture_output=True, text=True).stdout.strip()
    failed = 0
    for cid, fn, live in CASES:
        if only and cid not in only:
            continue
        if live and (no_live or not LIVE_KEY):
            report["cases"].append({"id": cid, "result": "SKIP", "why": "live case; needs TYPESAFE_API_KEY"})
            print(f"SKIP {cid}")
            continue
        with tempfile.TemporaryDirectory(prefix="tink-e2e-") as t:
            try:
                ok, detail = fn(Env(Path(t)))
            except Exception as exc:  # harness error counts as failure
                ok, detail = False, f"harness error: {exc!r}"
        report["cases"].append({"id": cid, "result": "PASS" if ok else "FAIL", "claim": fn.__doc__.strip(), "detail": detail})
        print(f"{'PASS' if ok else 'FAIL'} {cid}: {fn.__doc__.strip()}")
        if not ok:
            print(f"     {detail}")
            failed += 1
    ARTIFACT.parent.mkdir(parents=True, exist_ok=True)
    ARTIFACT.write_text(json.dumps(report, indent=2))
    print(f"artifact: {ARTIFACT.relative_to(REPO)}  failures: {failed}")
    return 1 if failed else 0


if __name__ == "__main__":
    sys.exit(main())
