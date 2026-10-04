# Independent review: mount-guidance

Result: **No actionable findings.**
Reviewer: independent Codex agent `patch_tink_release_review`, 2026-10-04.

## Scope

Reviewed base `0e8e6f481aacb91df6e77e390efd12bbb35002de` through review HEAD `130ca2fab247f12cad0f43a1698819f34238bd2d`. Production and test source matches supplied candidate `27671f3`; the later commit adds only the stage-5 routing record and an empty writer-lock file. The existing generated deployment rules in AGENTS.md and stage-5 snapshot were preserved.

Read the repository instructions, SDLC contract, stage-5 context, review policy, approved brief and checklist, accepted baseline and test lock, implementation diff, and retained verification evidence. One focused pass covered this small guidance change, its acceptance criteria, and the unchanged approval and mount boundaries.

## Independent verification

- `python3 -B tests/e2e/mount_guidance.py`: 8 checks passed. Actual help names both JSON and approval requirements and shows the complete command. Generated guidance uses `(read: tink mount plain --json --payload)`; following it returns the entire fixture in `payload.content`. Payload without JSON still exits 2; unapproved payload is refused; approved prompt-only delivery creates no link; plain mount creates one.
- `python3 -B tests/e2e/mount_trust.py`: 13 cases passed, including complete references, scripted mounts, exact JSON schema, changed-digest and symlink refusals, and unchanged plain mounting.
- `python3 -B tests/e2e/use_skillset.py`: 14 cases passed, including order, exact output, idempotence, description handling, byte limits, snapshots, and refusal behavior.
- `cargo check`, `cargo test --test acceptance zero_footprint` (3 passed), `cargo test --test acceptance mount` (4 passed), `cargo fmt --check`, and `git diff --check 0e8e6f4..HEAD` passed.
- Locked reproduction SHA-256 remains `4f269d65ec09af7e1f1fe89ed089342d1847ab86502e45df9ef9802334744290`. Status reports stage 3 approved, all 3 checklist checks passed, and current verification.

The source diff changes only help wording, the generated read instruction, and two corresponding test expectations. Approval checks, JSON schema, digest calculation, and mount implementation are unchanged. Independent E2E reports are in `target/e2e/`; retained run evidence was not edited.

## Skills and limits

Loaded `principle-prove-it-works` in full through the serialized SDLC wrapper with `--json --payload`; it directed this review to run the real CLI and follow generated guidance. The optional `manage-tink` routing pick remains unapproved and was not used, approved, or installed.

The installed Tink launcher still generated the older root rule format; this source-only patch does not install or release a new binary. Candidate CLI behavior was checked from this checkout's build. Full repository tests and remote CI were not run. This review does not provide human release approval, forge approval, or deployment evidence. No source edits, commits, push, PR, or merge were performed.
