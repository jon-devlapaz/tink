# Stage 04: Verify

Inputs: approved run artifacts, candidate checkout, `_system/verification.json`,
and the accepted `test-lock.json` for bug runs.
Run `_system/scripts/verify.sh <slug>`; no alternate runner bypasses the gate.

Outputs: generated `runs/<slug>/04-test/output/test-log.md` and `verification.json`.
The gate requires successful configured checks bound to current candidate and
input digests. Missing configuration, execution errors, stale inputs, changed test
baselines, and timeouts fail. A file's existence never proves success.

Failures return to stage 03. Incorrect reproduction tests require independent
review and a replacement run; do not weaken assertions to obtain green output.
Local locking detects changes. Strict enforcement requires trusted CI with an
independently retrieved baseline and protected runner/policy, as in `_system/SDLC.md`.
