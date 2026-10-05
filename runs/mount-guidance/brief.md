# Mount guidance

## Problem and outcome

At `0e8e6f481aacb91df6e77e390efd12bbb35002de`, `tink mount --help` advertises `--payload` without saying it requires `--json`. Following that incomplete guidance exits 2 with a missing-required-flag error. The parser intentionally requires both flags; preserve that behavior.

`tink use` also generates `(full: .tink/.active/NAME/SKILL.md; run: tink mount NAME)` before that file exists. Plain mount subsequently creates the link, so the command itself is valid. However, the apparent ready-to-read path is misleading and directs prompt-only readers through an unnecessary mount. JSON payload delivery already returns the complete approved skill without a link for prompt-only skills. Make that existing read route explicit.

## Acceptance criteria

1. The `--payload` option description explicitly says it requires `--json` and approval. Mount help includes the complete example `tink mount <skill> --json --payload`.
2. Generated required-skill guidance uses `(read: tink mount NAME --json --payload)`. It does not advertise an unconditional `.tink/.active/NAME/SKILL.md` file. Running the generated command delivers the complete approved skill.
3. Preserve parser, JSON schema, approval checks, tree-digest verification, and mount behavior: payload without JSON fails; unapproved payload is refused; approved prompt-only payload has no active link; plain mount still creates a link. Existing script-bearing and trust tests remain green.
4. Keep rules text, order, digest calculation, size limits, and idempotence unchanged except for the intended generated guidance bytes and corresponding digest. Existing generated-block expectations are updated narrowly.

## Approach

After human approval and independent acceptance of the failing baseline, update the mount help in `src/lib.rs` and the generated read instruction in `src/use_skillset.rs`. Update the existing exact-output helper and its description-extraction delimiter in `tests/e2e/use_skillset.py`; review any further failures before changing expectations. Do not edit installed `_system/`, `_shared/`, or stage guidance in this run. The separate SDLC-source patch owns its command examples. No library, global installation, CLI semantics, or release changes are included.

The pre-fix E2E test is `tests/e2e/mount_guidance.py`. It builds this checkout, runs in a temporary Git project with isolated `TINK_HOME`, checks help, follows generated guidance, and exercises approval and prompt-only mount controls. It uses no editable fixture helper. Retained baseline: `01-plan/output/baseline.json`; four expected guidance failures and four passing behavior controls. It is a reproduction, not an accepted test lock or passing verification.

The implementation checklist lives in `checklist.json`. Checked items are proven by `sdlc.py verify`; do not hand-edit receipt state.

## Risks and verification

- Changing generated rule bytes makes old blocks stale until an authorized `tink use` refresh. Do not hand-edit generated root guidance to hide this.
- Exact-output tests encode the old format, including one ` (full:` delimiter; change only expected guidance formatting. Keep their behavioral assertions intact.
- The new test is intentionally narrow. Run it plus the complete mount-trust and use-skillset E2E suites and the configured project verification. Existing mount-trust cases cover scripted skills and adversarial library inputs.
- Request independent review of `tests/e2e/mount_guidance.py` and `01-plan/output/baseline.json`, then record the real review reference with `lock-tests`. No approval or test lock has been fabricated.
- A generated read command returns JSON; the reader consumes `payload.content`. This is the already-supported whole-skill delivery contract.
