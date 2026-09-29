#!/usr/bin/env python3
"""E2E: committed project skillset pins (`<project>/.tink/skillsets/<name>-skillset.json`)
and `tink library fetch <PIN>...`, which deposits exactly a reviewed pin's members into
the STANDALONE library ($TINK_HOME/skills/<member>) with `.tink-source.json` provenance and
approve-on-write digests.

The upstream is a LOCAL git repository served under an HTTPS URL through GIT_CONFIG
`url.<file://...>.insteadOf`; nothing here touches the network. Every case runs this
checkout's `tink` (target/debug/tink) in a throwaway project with an isolated TINK_HOME.
~/.tink-library is never touched.

Run:   python3 tests/e2e/library_fetch.py [--only F1,F2]
Env:   TINK_E2E_SKIP_BUILD=1  reuse target/debug/tink
Artifact (repeatable, overwritten each run): target/e2e/library-fetch.json
Exit:  0 all cases pass, 1 any FAIL.
"""
import hashlib
import json
import os
import struct
import subprocess
import sys
import tempfile
import time
from pathlib import Path

REPO = Path(__file__).resolve().parents[2]
ARTIFACT = REPO / "target" / "e2e" / "library-fetch.json"

PUBLIC = "https://github.com/e2e-org/upstream.git"
MISSING = "https://github.com/e2e-org/missing.git"
TRAILER = "Their digests were approved because you ran fetch on a reviewed pin."
ERROR_KEYS = {"contract_version": int, "error": str, "code": str}


def skill_md(name, description=None, body="Body text.\n"):
    return f"---\nname: {name}\ndescription: {description or f'Fixture skill {name}.'}\n---\n# {name}\n\n{body}"


def py_tree_digest(root: Path) -> str:
    """Independent re-implementation of tink's v2 tree digest (skills::tree_digest)."""
    entries = []
    for dirpath, dirnames, filenames in os.walk(root):
        rel_dir = Path(dirpath).relative_to(root)
        if rel_dir.parts[:1] == (".git",):
            continue
        for d in dirnames:
            if (rel_dir / d).parts[:1] != (".git",):
                entries.append((rel_dir / d, None))
        for f in filenames:
            entries.append((rel_dir / f, Path(dirpath) / f))
    entries.sort(key=lambda e: e[0].parts)
    h = hashlib.sha256(b"tink-tree-digest-v2\0")
    for rel, full in entries:
        pb = str(rel).encode()
        h.update(struct.pack(">Q", len(pb)) + pb)
        if full is None:
            h.update(b"d")
        else:
            data = full.read_bytes()
            mode = 0o755 if os.stat(full).st_mode & 0o111 else 0o644
            h.update(b"f" + struct.pack(">I", mode) + struct.pack(">Q", len(data)) + data)
    return "sha256:" + h.hexdigest()


def home_hash(home: Path) -> str:
    """Hash of every path, file mode-independent content, and symlink target under home."""
    h = hashlib.sha256()
    if not home.exists():
        return "absent"
    for dirpath, dirnames, filenames in os.walk(home):
        dirnames.sort()
        for name in sorted(dirnames + filenames):
            full = Path(dirpath) / name
            h.update(str(full.relative_to(home)).encode() + b"\0")
            if full.is_symlink():
                h.update(b"L" + os.readlink(full).encode())
            elif full.is_file():
                h.update(b"F" + full.read_bytes())
            else:
                h.update(b"D")
    return h.hexdigest()


def _tail(p):
    return f"exit={p.returncode} stdout={p.stdout.strip()[:700]!r} stderr={p.stderr.strip()[:700]!r}"


def schema_errors(j, keys):
    if not isinstance(j, dict):
        return [f"not an object: {j!r}"]
    errs = []
    for k, t in keys.items():
        if k not in j:
            errs.append(f"missing {k}")
        elif not isinstance(j[k], t) or (t is int and isinstance(j[k], bool)):
            errs.append(f"{k} has type {type(j[k]).__name__}")
    if set(j) != set(keys):
        errs.append(f"key set {sorted(j)} != {sorted(keys)}")
    return errs


class Env:
    def __init__(self, root: Path, init=True):
        self.root = root
        self.proj = root / "proj"
        self.home = root / "home"
        self.bin = root / "bin"
        self.repo = root / "upstream"
        for d in (self.proj, self.bin, self.repo):
            d.mkdir(parents=True)
        (self.bin / "tink").symlink_to(REPO / "target" / "debug" / "tink")
        self.env = dict(
            os.environ,
            TINK_HOME=str(self.home),
            PATH=f"{self.bin}:{os.environ['PATH']}",
            GIT_CONFIG_COUNT="2",
            GIT_CONFIG_KEY_0=f"url.file://{self.repo.resolve()}.insteadOf",
            GIT_CONFIG_VALUE_0=PUBLIC,
            GIT_CONFIG_KEY_1=f"url.file://{root.resolve()}/no-such-repo.insteadOf",
            GIT_CONFIG_VALUE_1=MISSING,
            GIT_TERMINAL_PROMPT="0",
        )
        if init:
            self.run("git", "init", "-q", ".")
            r = self.run("tink", "init", "--no-tink-skills", "--no-manage-tink", "--no-sdlc")
            assert r.returncode == 0, r.stderr
        self.lib = self.home / "skills"
        self.pins = self.proj / ".tink" / "skillsets"
        self._git("init", "-q")
        self._git("config", "user.email", "e2e@example.com")
        self._git("config", "user.name", "e2e")

    # -- process helpers
    def run(self, *cmd, cwd=None):
        return subprocess.run(list(cmd), cwd=cwd or self.proj, env=self.env, capture_output=True, text=True, check=False)

    def tink(self, *args, js=False):
        p = self.run("tink", *args, *(["--json"] if js else []))
        try:
            j = json.loads(p.stdout)
        except json.JSONDecodeError:
            j = None
        return p, j

    def _git(self, *a):
        r = subprocess.run(["git", *a], cwd=self.repo, capture_output=True, text=True)
        assert r.returncode == 0, r.stderr
        return r.stdout.strip()

    # -- upstream helpers
    def member(self, name, root="skills", text=None, extra=None):
        d = self.repo / root / name
        d.mkdir(parents=True, exist_ok=True)
        (d / "SKILL.md").write_text(text if text is not None else skill_md(name))
        for rel, data in (extra or {}).items():
            (d / rel).parent.mkdir(parents=True, exist_ok=True)
            (d / rel).write_text(data)
        return d

    def commit(self, msg="c"):
        self._git("add", "-A")
        self._git("commit", "-qm", msg)
        return self._git("rev-parse", "HEAD")

    # -- pin helpers
    def pin(self, name, members, revision, source=PUBLIC, source_root="skills", required=None, home=False, **extra):
        doc = {"source": source, "revision": revision, "sourceRoot": source_root, "members": members}
        if required is not None:
            doc["required"] = required
        doc.update(extra)
        d = self.home / "skillsets" if home else self.pins
        d.mkdir(parents=True, exist_ok=True)
        p = d / f"{name}.json"
        p.write_text(json.dumps(doc, indent=2) + "\n")
        return p

    def fetch(self, *pins, js=False):
        return self.tink("library", "fetch", *[str(p) for p in pins], js=js)

    def lib_names(self):
        return sorted(p.name for p in self.lib.iterdir()) if self.lib.exists() else []


def prov_text(rev, path, source=PUBLIC):
    return f'{{\n  "source": "{source}",\n  "revision": "{rev}",\n  "path": "{path}"\n}}\n'


def std_upstream(e: Env):
    for n in ("m-a", "m-b", "m-c"):
        e.member(n)
    return e.commit("v1")


# ---- cases: each returns (ok, detail) ---------------------------------------------

def f1_happy_two_pins_shared_members(e: Env):
    """F1: two project pins with a shared member deposit exact members with provenance, recorded digests, and `tink use` then works with `required`."""
    rev = std_upstream(e)
    p1 = e.pin("one-skillset", ["m-a", "m-b"], rev, required=["m-a"])
    p2 = e.pin("two-skillset", ["m-b", "m-c"], rev)
    p, _ = e.fetch(p1, p2)
    out = p.stdout
    r7 = rev[:7]
    want_lines = [
        line
        for line in [
            f"  fetched m-a {py_tree_digest(e.lib / 'm-a')}",
            f"  fetched m-b {py_tree_digest(e.lib / 'm-b')}",
            "  unchanged m-b",
            f"  fetched m-c {py_tree_digest(e.lib / 'm-c')}",
            f"Fetched 2 new, 0 unchanged skill(s) from {PUBLIC}@{r7}. {TRAILER}",
            f"Fetched 1 new, 1 unchanged skill(s) from {PUBLIC}@{r7}. {TRAILER}",
        ]
    ]
    lines = out.splitlines()
    order_ok = all(l in lines for l in want_lines) and lines.index(want_lines[0]) < lines.index(want_lines[4]) < lines.index(want_lines[2])
    prov_ok = all(
        (e.lib / n / ".tink-source.json").read_text() == prov_text(rev, f"skills/{n}") for n in ("m-a", "m-b", "m-c")
    )
    body_ok = (e.lib / "m-a" / "SKILL.md").read_text() == skill_md("m-a") and not (e.lib / "m-a" / ".git").exists()
    ap, _ = e.tink("library", "approvals")
    approvals_ok = all(f"{n} {py_tree_digest(e.lib / n)}" in ap.stdout for n in ("m-a", "m-b", "m-c"))
    use, _ = e.tink("use", "one-skillset")
    agents = (e.proj / "AGENTS.md").read_text()
    use_ok = use.returncode == 0 and "- m-a:" in agents and "- m-b:" not in agents
    ok = p.returncode == 0 and order_ok and prov_ok and body_ok and approvals_ok and use_ok
    return ok, f"order_ok={order_ok} prov_ok={prov_ok} body_ok={body_ok} approvals_ok={approvals_ok} use:[{_tail(use)}] fetch:[{_tail(p)}]"


def f2_idempotent_second_run_writes_nothing(e: Env):
    """F2: a second identical fetch reports `unchanged` and changes nothing under $TINK_HOME."""
    rev = std_upstream(e)
    p1 = e.pin("one-skillset", ["m-a", "m-b"], rev)
    first, _ = e.fetch(p1)
    before = home_hash(e.home)
    second, _ = e.fetch(p1)
    after = home_hash(e.home)
    lines = second.stdout.splitlines()
    ok = (
        first.returncode == 0
        and second.returncode == 0
        and before == after
        and "  unchanged m-a" in lines
        and "  unchanged m-b" in lines
        and not any(l.startswith("  fetched") for l in lines)
        and f"Nothing new: 2 skill(s) already in the library from {PUBLIC}@{rev[:7]}; digests unchanged." in lines
    )
    return ok, f"same_hash={before == after} second:[{_tail(second)}]"


def f3_divergent_library_copy_refused_no_writes(e: Env):
    """F3: a different tree already at $TINK_HOME/skills/<name> refuses the WHOLE command (exit 2, library_divergent) with no writes to any member."""
    rev = std_upstream(e)
    p1 = e.pin("one-skillset", ["m-a", "m-b"], rev)
    (e.lib / "m-b").mkdir(parents=True)
    (e.lib / "m-b" / "SKILL.md").write_text(skill_md("m-b", description="local edit"))
    before = home_hash(e.home)
    p, j = e.fetch(p1, js=True)
    pj, _ = e.fetch(p1)
    after = home_hash(e.home)
    ok = (
        p.returncode == 2
        and j is not None
        and j.get("code") == "library_divergent"
        and "m-b" in j.get("error", "")
        and "remove or rename" in j.get("error", "")
        and str(e.lib / "m-b") in j.get("error", "")
        and pj.returncode == 2
        and "library_divergent" in pj.stderr
        and before == after
        and "m-a" not in e.lib_names()
        and (e.lib / "m-b" / "SKILL.md").read_text() == skill_md("m-b", description="local edit")
    )
    return ok, f"unchanged={before == after} names={e.lib_names()} json:[{_tail(p)}] text:[{_tail(pj)}]"


def f4_symlink_in_upstream_member_refused(e: Env):
    """F4: a symlink anywhere inside an upstream member refuses the command (symlink_refused) with no writes."""
    e.member("m-a")
    e.member("m-b")
    (e.repo / "skills" / "m-b" / "refs").mkdir()
    os.symlink("../../m-a/SKILL.md", e.repo / "skills" / "m-b" / "refs" / "link.md")
    rev = e.commit("with symlink")
    p1 = e.pin("one-skillset", ["m-a", "m-b"], rev)
    before = home_hash(e.home)
    p, j = e.fetch(p1, js=True)
    after = home_hash(e.home)
    ok = p.returncode == 2 and j and j.get("code") == "symlink_refused" and "m-b" in j["error"] and before == after and e.lib_names() == []
    return ok, f"names={e.lib_names()} {_tail(p)}"


def f5_refusal_codes(e: Env):
    """F5: missing member, unknown revision, identity mismatch, and unreachable source each refuse with their own code and write nothing."""
    e.member("m-a")
    e.member("wrong-dir", text=skill_md("other-name"))
    rev = e.commit("v1")
    before = home_hash(e.home)
    results = {}
    cases = {
        "member_not_found": e.pin("c1-skillset", ["m-a", "ghost"], rev),
        "revision_not_found": e.pin("c2-skillset", ["m-a"], "b" * 40),
        "identity_mismatch": e.pin("c3-skillset", ["m-a", "wrong-dir"], rev),
        "clone_failed": e.pin("c4-skillset", ["m-a"], rev, source=MISSING),
    }
    bad = []
    for code, pin in cases.items():
        p, j = e.fetch(pin, js=True)
        results[code] = (p.returncode, j and j.get("code"), (j or {}).get("error", "")[:160])
        if not (p.returncode == 2 and j and j.get("code") == code and not schema_errors(j, ERROR_KEYS)):
            bad.append(code)
    after = home_hash(e.home)
    ok = not bad and before == after and e.lib_names() == []
    return ok, f"bad={bad} unchanged={before == after} results={results}"


def f6_invalid_and_missing_pins_refused(e: Env):
    """F6: non-https or non-GitHub source, short revision, unknown field, bad JSON refuse as pin_invalid; a missing pin file or name refuses as pin_not_found."""
    rev = std_upstream(e)
    before = home_hash(e.home)
    bad = []
    pins = {
        "http": e.pin("h-skillset", ["m-a"], rev, source="http://github.com/e2e-org/upstream.git"),
        "file": e.pin("f-skillset", ["m-a"], rev, source="file:///tmp/x"),
        "host": e.pin("o-skillset", ["m-a"], rev, source="https://git.example.test/team/up.git"),
        "short": e.pin("s-skillset", ["m-a"], rev[:7]),
        "field": e.pin("u-skillset", ["m-a"], rev, bogus=True),
        "root": e.pin("r-skillset", ["m-a"], rev, source_root="../etc"),
    }
    (e.pins / "j-skillset.json").write_text("{not json")
    pins["json"] = e.pins / "j-skillset.json"
    detail = {}
    for label, pin in pins.items():
        p, j = e.fetch(pin, js=True)
        detail[label] = (p.returncode, j and j.get("code"), str(pin) in (j or {}).get("error", ""))
        if not (p.returncode == 2 and j and j.get("code") == "pin_invalid" and str(pin) in j.get("error", "")):
            bad.append(label)
    pm, jm = e.fetch(e.pins / "nope-skillset.json", js=True)
    pn, jn = e.tink("library", "fetch", "nope", js=True)
    ok_missing = all(x[1] and x[1].returncode == 2 and x[0] and x[0].get("code") == "pin_not_found" for x in [(jm, pm), (jn, pn)])
    after = home_hash(e.home)
    ok = not bad and ok_missing and before == after
    return ok, f"bad={bad} detail={detail} missing:[{_tail(pm)}] [{_tail(pn)}]"


def f7_fetch_by_name_project_before_home(e: Env):
    """F7: `library fetch <name>` resolves the project pin before a differing home pin (canonical then bare filename), and falls back to the home pin."""
    rev = std_upstream(e)
    e.pin("x-skillset", ["m-c"], rev, home=True)
    e.pin("x-skillset", ["m-a"], rev)  # project pin, canonical file name
    p, _ = e.tink("library", "fetch", "x")
    got1 = e.lib_names()
    (e.pins / "x-skillset.json").rename(e.pins / "y.json")  # project pin, bare file name
    e.pin("y-skillset", ["m-c"], rev, home=True)
    p2, _ = e.tink("library", "fetch", "y")
    got2 = e.lib_names()
    e.pin("z-skillset", ["m-b"], rev, home=True)  # home only
    p3, _ = e.tink("library", "fetch", "z-skillset")
    got3 = e.lib_names()
    ok = (
        p.returncode == 0
        and got1 == ["m-a"]
        and p2.returncode == 0
        and got2 == ["m-a"]  # bare project file y.json ([m-a]) won over home y-skillset ([m-c])
        and p3.returncode == 0
        and got3 == ["m-a", "m-b"]
    )
    return ok, f"got1={got1} got2={got2} got3={got3} {_tail(p)} {_tail(p2)} {_tail(p3)}"


def f8_use_project_pin_beats_home_pin(e: Env):
    """F8: `tink use` prefers the project pin over a differing home pin, accepts the bare file name, and errors show the file actually used."""
    rev = std_upstream(e)
    p1 = e.pin("gates-skillset", ["m-a"], rev, required=["m-a"])
    e.pin("gates-skillset", ["m-b"], rev, required=["m-b"], home=True)
    ff, _ = e.fetch(p1)
    e.tink("library", "fetch", str(e.home / "skillsets" / "gates-skillset.json"))  # approve m-b too
    use, _ = e.tink("use", "gates")
    agents = (e.proj / "AGENTS.md").read_text()
    wins = use.returncode == 0 and "- m-a:" in agents and "- m-b:" not in agents
    # bare-named project file
    p1.rename(e.pins / "gates.json")
    (e.pins / "gates.json").write_text(json.dumps({"source": PUBLIC, "revision": rev, "sourceRoot": "skills", "members": ["m-a"], "required": ["m-a"]}))
    use2, _ = e.tink("use", "gates-skillset")
    bare_ok = use2.returncode == 0 and "- m-a:" in (e.proj / "AGENTS.md").read_text() and "- m-b:" not in (e.proj / "AGENTS.md").read_text()
    # errors name the project pin, not the home one
    (e.pins / "gates.json").write_text(json.dumps({"source": PUBLIC, "revision": rev, "sourceRoot": "skills", "members": ["m-a"], "required": ["ghost"]}))
    use3, j3 = e.tink("use", "gates", js=True)
    named = str(e.pins / "gates.json")
    (e.pins / "gates.json").write_text(json.dumps({"source": PUBLIC, "revision": "abc", "sourceRoot": "skills", "members": ["m-a"]}))
    use4, _ = e.tink("use", "gates")
    ok_err3 = use3.returncode == 2 and j3 and j3["code"] == "required_invalid"
    invalid_names_path = str(e.pins / "gates.json") in use4.stderr and use4.returncode != 0
    (e.pins / "gates.json").unlink()
    use5, _ = e.tink("use", "gates")  # falls back to the home pin
    fallback_ok = use5.returncode == 0 and "- m-b:" in (e.proj / "AGENTS.md").read_text()
    (e.home / "skillsets" / "gates-skillset.json").unlink()
    use6, _ = e.tink("use", "gates")
    notfound = use6.returncode == 2 and "gates-skillset" in use6.stderr and ".tink/skillsets/gates-skillset.json" in use6.stderr
    ok = wins and bare_ok and ok_err3 and invalid_names_path and fallback_ok and notfound and named in use4.stderr
    return ok, f"wins={wins} bare_ok={bare_ok} err3=[{_tail(use3)}] invalid:[{_tail(use4)}] fallback=[{_tail(use5)}] notfound=[{_tail(use6)}]"


def f9_multiple_pins_validate_all_before_write(e: Env):
    """F9: when a later PIN is bad (member, revision, divergence), nothing from an earlier good PIN is written."""
    rev = std_upstream(e)
    good = e.pin("good-skillset", ["m-a", "m-b"], rev)
    bad_member = e.pin("bad-skillset", ["m-c", "ghost"], rev)
    bad_rev = e.pin("badrev-skillset", ["m-c"], "c" * 40)
    before = home_hash(e.home)
    p1, j1 = e.fetch(good, bad_member, js=True)
    p2, j2 = e.fetch(good, bad_rev, js=True)
    (e.lib / "m-c").mkdir(parents=True)
    (e.lib / "m-c" / "SKILL.md").write_text(skill_md("m-c", description="diverged"))
    mid = home_hash(e.home)
    good2 = e.pin("good2-skillset", ["m-c"], rev)
    p3, j3 = e.fetch(good, good2, js=True)
    after = home_hash(e.home)
    ok = (
        p1.returncode == 2 and j1 and j1["code"] == "member_not_found"
        and p2.returncode == 2 and j2 and j2["code"] == "revision_not_found"
        and p3.returncode == 2 and j3 and j3["code"] == "library_divergent"
        and before != mid
        and mid == after
        and e.lib_names() == ["m-c"]
    )
    return ok, f"names={e.lib_names()} {_tail(p1)} {_tail(p2)} {_tail(p3)}"


def f10_json_shapes(e: Env):
    """F10: --json prints exactly the documented success and error documents."""
    rev = std_upstream(e)
    p1 = e.pin("one-skillset", ["m-a", "m-b"], rev)
    p, j = e.fetch(p1, js=True)
    d = py_tree_digest(e.lib / "m-a")
    want = {
        "contract_version": 1,
        "pins": [
            {
                "pin": str(p1),
                "source": PUBLIC,
                "revision": rev,
                "skills": [
                    {"name": "m-a", "status": "fetched", "tree_digest": d},
                    {"name": "m-b", "status": "fetched", "tree_digest": py_tree_digest(e.lib / "m-b")},
                ],
            }
        ],
    }
    p2, j2 = e.fetch(p1, js=True)
    want2 = json.loads(json.dumps(want))
    for s in want2["pins"][0]["skills"]:
        s["status"] = "unchanged"
    pe, je = e.tink("library", "fetch", str(e.pins / "gone.json"), js=True)
    one_doc = p.stdout.count("\n") == 1 and p2.stdout.count("\n") == 1 and pe.stdout.count("\n") == 1
    ok = (
        p.returncode == 0 and j == want and p2.returncode == 0 and j2 == want2
        and pe.returncode == 2 and je and not schema_errors(je, ERROR_KEYS) and je["code"] == "pin_not_found" and je["contract_version"] == 1
        and one_doc
    )
    return ok, f"got={j} second={j2} err={je}"


def f11_doctor_pins_row(e: Env):
    """F11: `tink doctor` shows one `pins` row when project pins exist: warn with a fetch hint while members are missing, ok afterwards; none without pins."""
    rev = std_upstream(e)
    d0 = e.run("tink", "doctor")
    none_ok = "pins" not in d0.stdout
    e.pin("one-skillset", ["m-a", "m-b"], rev)
    p2 = e.pin("two-skillset", ["m-b", "m-c"], rev)
    d1 = e.run("tink", "doctor")
    rows1 = [l for l in d1.stdout.splitlines() if " pins " in l]
    hint = "run `tink library fetch .tink/skillsets/one-skillset.json`"
    warn_ok = (
        d1.returncode == 0
        and len(rows1) == 1
        and rows1[0].startswith("warn")
        and "2 project pin(s); 3 member(s) not in the library" in rows1[0]
        and hint in rows1[0]
    )
    e.fetch(e.pins / "one-skillset.json", p2)
    d2 = e.run("tink", "doctor")
    rows2 = [l for l in d2.stdout.splitlines() if " pins " in l]
    ok_ok = d2.returncode == 0 and len(rows2) == 1 and rows2[0].startswith("ok") and "2 project pin(s); 0 member(s) not in the library" in rows2[0]
    (e.pins / "broken-skillset.json").write_text("{nope")
    d3 = e.run("tink", "doctor")
    never_fails = d3.returncode == 0 and len([l for l in d3.stdout.splitlines() if " pins " in l]) == 1
    return none_ok and warn_ok and ok_ok and never_fails, f"none={d0.stdout!r} warn={rows1} ok={rows2} broken:[{_tail(d3)}]"


def f12_no_init_and_no_project_files(e: Env):
    """F12: fetch works in an uninitialised directory with no TINK_HOME yet, creating the home layout but no project files."""
    rev = std_upstream(e)
    bare = e.root / "bare"
    bare.mkdir()
    pin = e.pin("one-skillset", ["m-a"], rev)
    moved = bare / "pin.json"
    moved.write_text(pin.read_text())
    import shutil

    shutil.rmtree(e.home, ignore_errors=True)
    before = sorted(p.name for p in bare.iterdir())
    p = e.run("tink", "library", "fetch", "pin.json", cwd=bare)
    after = sorted(p_.name for p_ in bare.iterdir())
    ok = p.returncode == 0 and (e.lib / "m-a" / "SKILL.md").is_file() and before == after == ["pin.json"] and (e.home / "layout.json").exists()
    return ok, f"before={before} after={after} {_tail(p)}"


def f13_existing_commands_unaffected(e: Env):
    """F13: skill add, mount, library approve/list, and home-pin `use` behave as before."""
    src = e.root / "local-skill"
    src.mkdir()
    (src / "SKILL.md").write_text(skill_md("local-skill"))
    add = e.run("tink", "skill", "add", str(src))
    lib_list = e.run("tink", "library", "list")
    mount = e.run("tink", "mount", "local-skill")
    approve = e.run("tink", "library", "approve", "local-skill")
    mount_json = e.run("tink", "mount", "local-skill", "--json", "--payload")
    e.pin("plain-skillset", ["local-skill"], "a" * 40, required=["local-skill"], home=True)
    use = e.run("tink", "use", "plain")
    help_ = e.run("tink", "library", "--help")
    ok = (
        add.returncode == 0
        and "local-skill" in lib_list.stdout
        and mount.returncode == 0
        and approve.returncode == 0
        and mount_json.returncode == 0
        and use.returncode == 0
        and "fetch" in help_.stdout
    )
    return ok, f"add:[{_tail(add)}] mount:[{_tail(mount)}] use:[{_tail(use)}]"


CASES = [
    ("F1", f1_happy_two_pins_shared_members),
    ("F2", f2_idempotent_second_run_writes_nothing),
    ("F3", f3_divergent_library_copy_refused_no_writes),
    ("F4", f4_symlink_in_upstream_member_refused),
    ("F5", f5_refusal_codes),
    ("F6", f6_invalid_and_missing_pins_refused),
    ("F7", f7_fetch_by_name_project_before_home),
    ("F8", f8_use_project_pin_beats_home_pin),
    ("F9", f9_multiple_pins_validate_all_before_write),
    ("F10", f10_json_shapes),
    ("F11", f11_doctor_pins_row),
    ("F12", f12_no_init_and_no_project_files),
    ("F13", f13_existing_commands_unaffected),
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
        with tempfile.TemporaryDirectory(prefix="tink-e2e-fetch-") as t:
            try:
                ok, detail = fn(Env(Path(t).resolve()))
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
