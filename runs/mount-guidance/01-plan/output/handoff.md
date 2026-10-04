# Stage 1 handoff

Definition review is pending. No production code, approval receipts, test locks, stage launches, or pushes were made.

## Evidence

- Source head: `0e8e6f481aacb91df6e77e390efd12bbb35002de`.
- Run: light / bug; stage 3 is the definition gate.
- Reproduce: `python3 tests/e2e/mount_guidance.py`.
- Actual pre-fix result: exit 1; FAIL help_states_json_requirement, help_has_complete_example, guidance_has_no_unconditional_active_pointer, generated_read_command_delivers_payload. PASS payload_alone_still_refused, unapproved_payload_refused, approved_prompt_payload_without_link, plain_mount_still_creates_link.
- Full command outputs and generated guidance: `baseline.json` beside this file. Reruns write `target/e2e/mount-guidance.json`.
- Source: `src/lib.rs` Mount variant, `src/use_skillset.rs` compiled line, `src/mount.rs` mount_skill and mount_skill_report.

The active-path issue warrants a bounded guidance change. `tink use` does not create the named path. Plain mount can create it, but JSON payload delivery intentionally avoids it for prompt-only skills. Replace the generated path/command pair with the complete read command; leave both mount modes alone.

## Next steps after review

1. Human reviews brief and checklist; launcher records the actual stage-3 decision.
2. An independent reviewer examines the reproduction and actual baseline. Record the real review source and lock `tests/e2e/mount_guidance.py` before production changes. Its fixture and runner are self-contained.
3. Launcher opens the implementation stage. Make only the scoped help/generated-guidance changes and exact-output expectation adjustments.
4. Commit, complete configured verification and checklist commands, then follow the separate release-review gate.

## Skill used

`principle-build-the-lever` was read through the authorized SDLC Tink wrapper with `mount principle-build-the-lever --json --payload`. It changed the evidence approach: a standalone rerunnable E2E checker replaces an unrepeatable manual transcript. The returned prompt-only skill had `mounted: false`, consistent with the behavior under investigation.

Installed scaffold prose is unchanged. Its missing-JSON example is owned by the separate shared-source patch. The existing generated planning block in root AGENTS.md came from the launcher; this planning task did not rewrite it.
