#!/usr/bin/env python3
"""E2E: tink + tink-route interop (dogfood findings D1-D6).

Drives this checkout's `tink` (target/debug/tink, built here) and the sibling
tink-route checkout (../tink-route/src) together in a throwaway git project with
an isolated TINK_HOME. Neither the caller's repo nor ~/.tink-library is touched.

Deterministic cases need no network. Live cases (L*) call TypeSafe Jev and are
SKIPped without TYPESAFE_API_KEY.

Run:   python3 tests/e2e/tink_route_interop.py [--only D1,D3] [--no-live]
Env:   TINK_ROUTE_SRC   path to tink-route `src/` (default ../tink-route/src)
       TINK_E2E_SKIP_BUILD=1  reuse target/debug/tink
Artifact (repeatable, overwritten each run): target/e2e/tink-route-interop.json
Exit:  0 all executed cases pass, 1 any FAIL.
"""
import json
import os
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
        self.env.pop("TINK_ROUTE_INSTALL", None)
        self.run("git", "init", "-q", ".")
        self.run("tink", "init")
        skills = self.home / "skills"
        for name, desc in SKILLS.items():
            d = skills / name
            d.mkdir(parents=True, exist_ok=True)
            (d / "SKILL.md").write_text(f"---\nname: {name}\ndescription: {desc}\n---\n# {name}\n", encoding="utf-8")

    def run(self, *cmd, cwd=None, extra_env=None):
        env = dict(self.env, **(extra_env or {}))
        return subprocess.run(list(cmd), cwd=cwd or self.proj, env=env, capture_output=True, text=True, check=False)

    def installed(self):
        d = self.proj / ".agents" / "skills"
        return sorted(p.name for p in d.iterdir() if p.is_dir())

    def ledger_path(self):
        return self.proj / ".tink" / "ephemeral.json"

    def write_ledger(self, skills):
        p = self.ledger_path()
        p.parent.mkdir(exist_ok=True)
        p.write_text(json.dumps({"version": 1, "skills": skills}, indent=2))


def _tail(p):
    return f"exit={p.returncode} stdout={p.stdout.strip()[:300]!r} stderr={p.stderr.strip()[:300]!r}"


# ---- deterministic cases: each returns (ok, detail) --------------------------------

def d1_prune_keeps_explicitly_added_skill(e: Env):
    """D1: an explicit `tink skill add` of a ledger-tracked skill must survive --prune."""
    e.run("tink", "skill", "add", "alpha")
    e.write_ledger(["alpha"])  # what `tink-route -i` records
    add = e.run("tink", "skill", "add", "alpha")  # user deliberately keeps it
    prune = e.run("tink-route", "--prune")
    ok = "alpha" in e.installed()
    return ok, f"after prune installed={e.installed()} add:[{_tail(add)}] prune:[{_tail(prune)}]"


def d1b_prune_still_removes_route_installed_skill(e: Env):
    """D1b guard: a skill only installed via tink-route's ledger path is still pruned."""
    e.run("tink", "skill", "add", "alpha", extra_env={"TINK_ROUTE_INSTALL": "1"})
    e.write_ledger(["alpha"])
    prune = e.run("tink-route", "--prune")
    ok = "alpha" not in e.installed()
    return ok, f"after prune installed={e.installed()} prune:[{_tail(prune)}]"


def d2_empty_task_is_a_usage_error(e: Env):
    """D2: empty/missing task must exit 2 (not 1 = 'no skill applies'), and the wrapper must fail."""
    results = []
    for args in (["--json", ""], ["--json"]):
        p = e.run("tink-route", *args)
        results.append((args, p.returncode))
    shutil.copytree(REPO / "_system", e.proj / "_system")
    w = e.run(sys.executable, "_system/scripts/sdlc.py", "skills", "tink-route", "--", "--json", "")
    ok = all(rc == 2 for _, rc in results) and w.returncode != 0
    return ok, f"route exits={results} wrapper exit={w.returncode}"


def d3_corrupt_ledger_is_refused(e: Env):
    """D3: a corrupt ledger must be reported (non-zero, names the file) and left in place."""
    p = e.ledger_path()
    p.parent.mkdir(exist_ok=True)
    p.write_text("{bad")
    prune = e.run("tink-route", "--prune")
    still_there = p.exists() and p.read_text() == "{bad"
    mentions = "ephemeral" in (prune.stdout + prune.stderr).lower() and "no ephemeral skills" not in prune.stdout.lower()
    ok = prune.returncode != 0 and still_there and mentions
    return ok, f"ledger_preserved={still_there} prune:[{_tail(prune)}]"


def d6_mount_ignores_active_dir(e: Env):
    """D6: `tink mount` must not leave .tink/.active symlinks untracked-visible to git."""
    m = e.run("tink", "mount", "alpha")
    status = e.run("git", "status", "--porcelain", "--untracked-files=all").stdout
    leaked = [l for l in status.splitlines() if ".tink" in l and ".gitignore" not in l]
    ignored = e.run("git", "check-ignore", ".tink/.active/alpha").returncode == 0
    ok = m.returncode == 0 and ignored and not leaked
    return ok, f"check-ignore={ignored} leaked={leaked} mount:[{_tail(m)}]"


# ---- live cases (Jev) ---------------------------------------------------------------

def l4_tri_gate_accepts_clear_eli5_prompts(e: Env):
    """L4: unambiguous ELI5 prompts route to eli5; unrelated prompts still abstain."""
    prompts = [
        "Explain this Rust borrow checker error like I'm five",
        "ELI5: what is a mutex?",
        "Explain how TCP works to my mom, dumb it down",
    ]
    rows, ok = [], True
    for t in prompts:
        p = e.run("tink-route", "--json", t)
        try:
            j = json.loads(p.stdout)
        except json.JSONDecodeError:
            j = {"status": "unparseable", "raw": p.stdout[:120]}
        hit = j.get("status") == "routed" and j.get("winner") == "eli5"
        ok &= hit
        rows.append({"task": t, "status": j.get("status"), "winner": j.get("winner"), "noul": j.get("specialist_noul")})
    # Negative control: the gate must still abstain on tasks no fixture skill covers.
    for t in ("What is the weather in Paris?", "Write a haiku about autumn"):
        p = e.run("tink-route", "--json", t)
        try:
            j = json.loads(p.stdout)
        except json.JSONDecodeError:
            j = {"status": "unparseable"}
        abstained = j.get("status") == "no_skill_needed"
        ok &= abstained
        rows.append({"negative_control": t, "status": j.get("status"), "winner": j.get("winner")})
    return ok, json.dumps(rows)


def l5_multi_output_is_consistent(e: Env):
    """L5: multi_routed => confidence >= threshold, winner == candidates[0], candidates non-empty."""
    p = e.run("tink-route", "--json", "--multi", "Compile an epistemic matrix audit and review clean code")
    try:
        j = json.loads(p.stdout)
    except json.JSONDecodeError:
        return False, _tail(p)
    if j.get("status") != "multi_routed":
        return True, f"status={j.get('status')} (invariant vacuous)"
    cands = j.get("candidates") or []
    ok = (
        j["confidence"] >= j["threshold"]
        and bool(cands)
        and cands[0].get("skill") == j.get("winner")
    )
    return ok, f"confidence={j['confidence']} threshold={j['threshold']} winner={j.get('winner')} candidates={cands}"


CASES = [
    ("D1", d1_prune_keeps_explicitly_added_skill, False),
    ("D1b", d1b_prune_still_removes_route_installed_skill, False),
    ("D2", d2_empty_task_is_a_usage_error, False),
    ("D3", d3_corrupt_ledger_is_refused, False),
    ("D6", d6_mount_ignores_active_dir, False),
    ("L4", l4_tri_gate_accepts_clear_eli5_prompts, True),
    ("L5", l5_multi_output_is_consistent, True),
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
