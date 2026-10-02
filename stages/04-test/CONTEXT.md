# Stage 04: Verify

Inputs: approved run artifacts, candidate checkout, `_system/verification.json`,
and the accepted `test-lock.json` for bug runs.
Run `python3 _system/scripts/sdlc.py verify <slug>`; no alternate runner bypasses the gate.

Outputs: generated `runs/<slug>/04-test/output/test-log.md` and `verification.json`.
The gate requires successful configured checks bound to current candidate and
input digests. Missing configuration, execution errors, stale inputs, changed test
baselines, and timeouts fail. A file's existence never proves success.

Gate: verification passes on current evidence and a human reviewer accepts the
result; a green local run alone is not release approval.

Commit the candidate before final verification; a passing, committed run is the point at which a PR may be opened (stage 05 reviews it).
Failures return to stage 03. Incorrect reproduction tests require independent
review and a replacement run; do not weaken assertions to obtain green output.
Local locking detects changes. Strict enforcement requires trusted CI with an
independently retrieved baseline and protected runner/policy, as in `_system/SDLC.md`.

## Skills

Skillset: `testing-skillset` (pin: `.tink/skillsets/testing-skillset.json`).
- Once per machine/library, after reviewing the pin (it selects exact upstream code):
  `tink library fetch .tink/skillsets/testing-skillset.json`
- At stage open, compile the required disciplines, then start a NEW session so
  `AGENTS.md` is re-read: `tink use testing-skillset --snapshot runs/<slug>/04-test`
  The launcher does this for you: `python3 _system/scripts/sdlc.py stage <slug> 4`.
- For a capability gap: `tink-route --receipt runs/<slug>/skills.jsonl "<what you need>"`
  (searches the whole library; prints the skill on stdout. Exit 1 means nothing fits; exit 1 or 2 means
  continue without a skill).

Stage skills (always for this stage; `tink use` compiles the same set):
- `principle-prove-it-works`: Before declaring done: run the real artifact and show its output; a green build or "it compiles" is not proof.
- `principle-build-the-lever`: Non-trivial work: build the script or tool that does or proves it, so a reviewer can rerun it, instead of doing it by hand.
When one of these triggers fires, or the stage-open pick (`runs/<slug>/skills/stage-<n>-pick.json`) names a skill, read that skill in full before acting on it. In the handoff note, name each skill that changed a decision and the decision it changed.
