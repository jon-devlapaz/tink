#!/usr/bin/env python3
"""Exercise mount guidance against this checkout in a temporary project/library.
Run: python3 tests/e2e/mount_guidance.py
Evidence: target/e2e/mount-guidance.json. Exit 1 means a contract failed.
"""
import json
import os
from pathlib import Path
import re
import shlex
import subprocess
import tempfile

REPO = Path(__file__).resolve().parents[2]
ARTIFACT = REPO / "target/e2e/mount-guidance.json"


def main():
    subprocess.run(["cargo", "build", "--quiet"], cwd=REPO, check=True)
    evidence = {"revision": subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=REPO, text=True).strip(), "commands": [], "checks": {}}
    with tempfile.TemporaryDirectory(prefix="tink-mount-guidance-") as tmp:
        root = Path(tmp)
        project = root / "project"
        project.mkdir()
        home = root / "library"
        env = dict(os.environ, TINK_HOME=str(home))

        def run(*args):
            proc = subprocess.run([str(REPO / "target/debug/tink"), *args], cwd=project, env=env, text=True, capture_output=True)
            evidence["commands"].append({"argv": ["tink", *args], "exit": proc.returncode, "stdout": proc.stdout, "stderr": proc.stderr})
            return proc

        help_result = run("mount", "--help")
        payload_help = re.search(r"--payload\s+(.*?)(?=\n\s*--|\n\s*-h|\Z)", help_result.stdout, re.S)
        evidence["checks"]["help_states_json_requirement"] = help_result.returncode == 0 and payload_help is not None and "--json" in payload_help.group(1)
        evidence["checks"]["help_has_complete_example"] = re.search(r"tink mount \S+ --json --payload", help_result.stdout) is not None
        bad = run("mount", "plain", "--payload")
        evidence["checks"]["payload_alone_still_refused"] = bad.returncode == 2 and "--json" in bad.stderr
        subprocess.run(["git", "init", "-q", str(project)], check=True)
        assert run("init", "--no-tink-skills", "--no-manage-tink", "--no-sdlc").returncode == 0
        skill = home / "skills/plain"
        skill.mkdir(parents=True)
        content = "---\nname: plain\ndescription: Read the fixture.\n---\n# Plain\n\nUnique fixture instruction.\n"
        (skill / "SKILL.md").write_text(content)
        unapproved = run("mount", "plain", "--json", "--payload")
        evidence["checks"]["unapproved_payload_refused"] = unapproved.returncode == 2 and json.loads(unapproved.stdout).get("code") == "unapproved"
        assert run("library", "approve", "plain").returncode == 0
        pins = home / "skillsets"
        pins.mkdir(exist_ok=True)
        (pins / "guidance-skillset.json").write_text(json.dumps({"source": "https://github.com/e2e-org/fixtures.git", "revision": "a" * 40, "sourceRoot": "skills", "members": ["plain"], "required": ["plain"]}))
        assert run("use", "guidance").returncode == 0
        guidance = (project / "AGENTS.md").read_text()
        evidence["generated_guidance"] = guidance
        active = project / ".tink/.active/plain/SKILL.md"
        evidence["active_entrypoint_after_use"] = active.exists()
        evidence["checks"]["guidance_has_no_unconditional_active_pointer"] = "full: .tink/.active/plain/SKILL.md" not in guidance
        match = re.search(r"tink mount plain --json --payload", guidance)
        evidence["checks"]["generated_read_command_delivers_payload"] = False
        if match:
            delivered = run(*shlex.split(match.group(0))[1:])
            evidence["checks"]["generated_read_command_delivers_payload"] = delivered.returncode == 0 and json.loads(delivered.stdout).get("payload", {}).get("content") == content
        good = run("mount", "plain", "--json", "--payload")
        report = json.loads(good.stdout)
        evidence["checks"]["approved_prompt_payload_without_link"] = good.returncode == 0 and report.get("payload", {}).get("content") == content and report.get("mounted") is False and report.get("target") is None and not active.exists()
        plain = run("mount", "plain")
        evidence["checks"]["plain_mount_still_creates_link"] = plain.returncode == 0 and active.exists()
    ARTIFACT.parent.mkdir(parents=True, exist_ok=True)
    ARTIFACT.write_text(json.dumps(evidence, indent=2) + "\n")
    for name, passed in evidence["checks"].items():
        print(f"{'PASS' if passed else 'FAIL'} {name}")
    print(f"Evidence: {ARTIFACT}")
    return 0 if all(evidence["checks"].values()) else 1


if __name__ == "__main__":
    raise SystemExit(main())
