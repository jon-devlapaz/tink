#!/usr/bin/env python3
"""E2E: `tink skill add` no longer knows tink-route's removed ephemeral ledger.

A stale `.tink/ephemeral.json` (left by an old tink-route) naming the skill being added
must be neither read nor modified, with or without the old TINK_ROUTE_INSTALL handshake,
and `tink init` must not write an `ephemeral.*` ignore rule.

Run:   python3 tests/e2e/no_ledger.py
Env:   TINK_E2E_SKIP_BUILD=1  reuse target/debug/tink
Artifact (repeatable): target/e2e/no-ledger.json
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
TINK = REPO / "target" / "debug" / "tink"
ARTIFACT = REPO / "target" / "e2e" / "no-ledger.json"
SENTINELS = {
    "valid": b'{"version": 1,   "skills": ["alpha", "keep"]}\n',
    "corrupt": b"{bad",
}


def run(env, proj, *cmd, extra=None):
    return subprocess.run(list(cmd), cwd=proj, env=dict(env, **(extra or {})), capture_output=True, text=True)


def setup(root: Path):
    proj, home = root / "proj", root / "home"
    proj.mkdir()
    env = dict(os.environ, TINK_HOME=str(home))
    env.pop("TINK_ROUTE_INSTALL", None)
    run(env, proj, "git", "init", "-q", ".")
    run(env, proj, TINK, "init")
    (home / "skills" / "alpha").mkdir(parents=True)
    (home / "skills" / "alpha" / "SKILL.md").write_text("---\nname: alpha\ndescription: Alpha fixture.\n---\n# alpha\n")
    return env, proj


def n1_add_leaves_ledger_untouched(kind, handshake):
    def case():
        with tempfile.TemporaryDirectory(prefix="tink-e2e-") as t:
            env, proj = setup(Path(t))
            ledger = proj / ".tink" / "ephemeral.json"
            ledger.parent.mkdir(exist_ok=True)
            ledger.write_bytes(SENTINELS[kind])
            p = run(env, proj, TINK, "skill", "add", "alpha", extra={"TINK_ROUTE_INSTALL": "1"} if handshake else None)
            same = ledger.read_bytes() == SENTINELS[kind]
            no_lock = not (proj / ".tink" / "ephemeral.lock").exists()
            quiet = "ephemeral" not in (p.stdout + p.stderr).lower()
            ok = p.returncode == 0 and same and no_lock and quiet and (proj / ".agents/skills/alpha").is_dir()
            return ok, f"exit={p.returncode} ledger_identical={same} no_lock={no_lock} quiet={quiet} stderr={p.stderr.strip()[:200]!r}"
    hs = "with" if handshake else "without"
    case.__doc__ = f"N1: add of a ledger-named skill leaves a {kind} .tink/ephemeral.json byte-identical, no lock file, no warning ({hs} handshake env)"
    return case


def n2_init_writes_no_ephemeral_rule():
    """N2: `tink mount` creates .tink/.gitignore with .active/ and cache/ rules and no `ephemeral.*` rule."""
    with tempfile.TemporaryDirectory(prefix="tink-e2e-") as t:
        env, proj = setup(Path(t))
        run(env, proj, TINK, "mount", "alpha")  # creates .tink/.gitignore
        rules = (proj / ".tink" / ".gitignore").read_text().splitlines()
        ok = ".active/" in rules and "cache/" in rules and not any("ephemeral" in r for r in rules)
        return ok, f"rules={rules}"


CASES = [
    ("N1a", n1_add_leaves_ledger_untouched("valid", False)),
    ("N1b", n1_add_leaves_ledger_untouched("valid", True)),
    ("N1c", n1_add_leaves_ledger_untouched("corrupt", False)),
    ("N2", n2_init_writes_no_ephemeral_rule),
]


def main() -> int:
    if os.environ.get("TINK_E2E_SKIP_BUILD") != "1":
        b = subprocess.run(["cargo", "build", "-q"], cwd=REPO, capture_output=True, text=True)
        if b.returncode != 0:
            print(b.stderr, file=sys.stderr)
            return 1
    report = {"ran_at": time.strftime("%Y-%m-%dT%H:%M:%S%z"), "cases": []}
    failed = 0
    for cid, fn in CASES:
        try:
            ok, detail = fn()
        except Exception as exc:
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
