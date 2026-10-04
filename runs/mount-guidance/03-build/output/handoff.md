# Build and verification handoff

Candidate commit: `efc18b5f8c835ac5ab3cf25ec7524413cb73c76a`.

## Change

Mount help now states that `--payload` requires `--json` and approval, and gives the complete read command. Generated required-skill lines use `(read: tink mount NAME --json --payload)`. The existing use-skillset helper and description delimiter changed only to match that format. Parser, approval, digest verification, and mount implementation are unchanged.

The locked test `tests/e2e/mount_guidance.py` is unchanged. Its SHA-256 remains `4f269d65ec09af7e1f1fe89ed089342d1847ab86502e45df9ef9802334744290`.

## Verification

`python3 _system/scripts/sdlc.py verify mount-guidance` passed after the candidate commit:

- `cargo check` passed.
- Acceptance zero-footprint tests: 3 passed.
- Acceptance mount tests: 4 passed.
- Mount guidance E2E: 8 passed.
- Mount trust E2E: 13 passed.
- Use-skillset E2E: 14 passed.

`cargo fmt --check` and `git diff --check` also passed. The generated test log and verification receipt are in `04-test/output/`, along with copies of the three repeatable E2E JSON reports.

## Skills and limits

`principle-build-the-lever` kept the accepted E2E script as the repeatable proof. `principle-prove-it-works` required running the real CLI and following the generated command. `unslop` guided the help and handoff wording.

The optional stage-open pick, `manage-tink`, refused payload delivery with `code=unapproved`. The local source was inspected to understand its scope; no approval, library mutation, installation, or refresh followed. The required disciplines were loaded through the serialized wrapper using `--json --payload`.

Stage 4 ran within the build session. No installed SDLC contracts changed. Only the configured checks and the approved E2E suites ran; the full repository test suite and remote CI did not run. Independent review and release approval remain pending. No PR, push, merge, or stage 5 launch was performed.

The launcher-generated root AGENTS.md rules remain uncommitted and unchanged by this worker. Runtime lock files remain on disk and are excluded from the final tracked diff. The candidate commit briefly included the empty run writer lock; the evidence commit removes it from version control without deleting the runtime file.
