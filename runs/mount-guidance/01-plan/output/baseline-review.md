# Baseline review: mount-guidance

Decision: **ACCEPT** the baseline as a reproduction of the reported guidance defects.
Reviewer: independent Codex agent `patch_baseline_review`, 2026-10-04.
This accepts the test baseline only. It is not human brief approval or release approval.

## Evidence

Reviewed the root instructions, installed SDLC baseline rules, stage 01 context, brief, checklist, test file, retained JSON evidence, and relevant parser and generated-guidance source. Independently ran:

```sh
python3 -B tests/e2e/mount_guidance.py
```

At HEAD `0e8e6f481aacb91df6e77e390efd12bbb35002de`: exit 1. Four intended guidance checks fail: help omits the JSON prerequisite, help lacks the complete example, generated guidance advertises the absent active file, and no generated payload command can be followed. Four behavior controls pass: payload alone is refused, unapproved payload is refused, approved prompt-only payload returns complete content without a link, and plain mount creates the link. Results match the retained baseline.

## Assessment and limits

- The test builds and invokes the real CLI, creates an isolated library and Git project, reads generated guidance, and follows the requested command when present. It does not mock the implementation.
- Expected flag order and guidance text match explicit brief requirements. The test checks user-facing output rather than Rust function structure. It permits different surrounding prose.
- Approval and mount controls distinguish a guidance correction from relaxing parser or trust requirements. Complete fixture content is compared after payload delivery.
- This narrow baseline does not alone enforce every brief detail: approval wording, the exact `(read: ...)` wrapper, all JSON fields, digest checks, script-bearing skills, and compiler ordering/size/idempotence still need the planned source review and existing suites.
- The test is self-contained for locking at `tests/e2e/mount_guidance.py`: all temporary fixture content, command handling, assertions, and evidence generation live there. Cargo inputs and production sources are the application under test.
- Side effects: local Cargo build output and `target/e2e/mount-guidance.json`; all library approval, skillset, project, and mount writes stay in the temporary directory under isolated `TINK_HOME`. The retained baseline and production/test files were unchanged. No global install, test lock, stage launch, commit, push, or human approval was made.

Required skill: `principle-build-the-lever`; the existing rerunnable baseline was used as the review artifact. No additional review framework was needed.

## SHA-256

| File | Digest |
| --- | --- |
| `tests/e2e/mount_guidance.py` | `4f269d65ec09af7e1f1fe89ed089342d1847ab86502e45df9ef9802334744290` |
| `runs/mount-guidance/01-plan/output/baseline.json` | `b3ebac03e2167da5eccfd2fb05de936defd2bad50084832016813bfd5c62f854` |
| `src/lib.rs` | `40f30228aa97664ab4f4768d9db0168058270ff5f4eeb1fb156c1175bed8dd9a` |
| `src/use_skillset.rs` | `8bcd6b62e5b1459560c34a31f172ec70c4e8f93f59c3f457f4d0faac9d695560` |
| `target/e2e/mount-guidance.json` (independent rerun) | `aaf24f46038d8c11ff87f16534c2f13d0ac604b67971bb896aaa9d75a34be2f7` |
