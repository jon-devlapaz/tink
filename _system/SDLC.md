# SDLC workspace

A small filesystem workflow: define a change, implement and verify it in an isolated checkout, then obtain independent release review. Requires Python 3.9+, Bash, and Git on a POSIX system. Native Windows is not supported. Set `require_tink: true` when project policy requires Tink integrity checks.

## Start a run

Run scripts from any working directory. Paths resolve from the script location.

```sh
_system/scripts/new-run.sh fix-example --kind bug
_system/scripts/status.sh fix-example
```

The default `light` profile creates one `brief.md`: problem, acceptance criteria, approach, implementation checklist, risks, and verification. Use `--profile full` for consequential architecture or policy changes; it creates the existing intent/spec/plan artifacts. A human selects the appropriate profile. Legacy artifacts without run metadata remain drafts; text approval tags are not imported as evidence.

After the human accepts the brief, record the actual review reference:

```sh
python3 _system/scripts/sdlc.py decide fix-example 3 approved \
  --reviewer 'Reviewer name' --source 'Actual review URL or decision reference' \
  --reason 'Accepted scope, approach, and acceptance criteria'
```

Full runs record decisions for stages 1, 2, and 3 in order. Markdown stays editable. `changes-requested` records rejection using the same command. Editing an artifact or contract makes dependent decisions stale; older receipts remain available. No agent should invent a reviewer or approval. These local receipts track workflow; they do not authenticate identity or authorize a release.

## Implementation and verification

Use a separate Git worktree or clone for each code-writing run, never just two branches in one checkout. Keep each run's artifacts in its own checkout. Do not share writable skill directories. Ports, databases, and credentials need separate isolation when used.

Configure `_system/verification.json` with nonempty command argument arrays and timeouts. Fresh installations have an empty check list and fail verification until real project tests, build, lint, and other required checks are configured. Missing commands, missing configuration, failed checks, and timeouts fail verification. Required Tink checks fail if Tink is unavailable. The tool does not infer test coverage from an exit code.

```sh
_system/scripts/verify.sh fix-example
```

Verification writes the actual check output and a generated receipt under `04-test/output/`. The receipt binds the Git revision, tracked and nonignored untracked file contents/modes (excluding disposable untracked Python caches), artifact inputs, policy, test lock, and log digest. It excludes `runs/`; do not place application source or test infrastructure there. Ignored files and external services are outside this fingerprint: pin dependencies and environments in trusted CI. Commit code changes before final verification. The receipt records the observed commit for provenance; status compares candidate contents, so committing only run evidence does not invalidate unchanged code. CI must still verify the actual revision being merged. Avoid modifying the checkout while checks run.

A preexisting log is never passing evidence. Failed or interrupted verification cannot reuse an older passing receipt. Local status is not deployment status.

## Bug reproduction baseline

Before fixing a bug, demonstrate that a meaningful reproduction fails for the expected reason on the unfixed code, and have an independent reviewer accept it. Then record the actual failure and review references:

```sh
python3 _system/scripts/sdlc.py lock-tests fix-example tests/test_regression.py \
  --source 'Actual accepted test review reference' \
  --failure-evidence 'Actual failing CI job or retained reproduction evidence'
```

Include relevant fixtures, snapshots, discovery configuration, and runner helpers in the locked paths. Bug runs cannot verify without a lock; changed or missing locked files fail. A mistaken baseline requires an independently reviewed replacement run, linked to the previous one. This conservative PoC does not silently unlock tests.

This detects local changes, not adversarial tampering. Strict protection requires CI to fetch the accepted test revision independently, protect verification policy, run against the candidate revision, and require the resulting check before merge. Agent-writable lock files and hooks are not an authorization boundary. This scaffold does not configure a forge or deployment environment.

## Skills and concurrency

Commit baseline `.tink/skills.toml` and `.tink/skills.lock` when Tink has created them through an authorized operation. Restore into each worktree through Tink; do not copy writable directories by symlink. Lockfile updates are reviewed dependency changes, not an automatic consequence of routing.

Load mandatory skills deterministically. Route only genuine capability gaps; allow abstention. Run authorized operations through the wrapper:

```sh
python3 _system/scripts/sdlc.py skills tink-route -- --json 'Capability needed'
python3 _system/scripts/sdlc.py skills tink -- skill check
```

The wrapper serializes cooperating Tink/router operations across this host and rejects symlinked skill state. It does not grant mutation authority, install anything automatically, isolate a shared home library, or coordinate direct CLI calls outside the wrapper. Command shapes after `--` belong to the installed Tink/router CLI; confirm them against that CLI's `--help` — the examples here are illustrative, not a fixed activation schema. Use it consistently; cross-host shared storage requires external coordination.

For `--install --ephemeral`, preserve routing JSON, source revisions/content hashes, and activation outcomes with the run. Validate selected paths, run integrity checks, and explicitly read the selected SKILL.md. Do not assume JSON output activates it. Low confidence or unavailable routing leaves the capability unresolved. Keep ephemeral state worktree-local. Prune only after run closure, beginning with `prune --dry-run`; avoid `--all-unpinned` for routine cleanup. Cleanup cannot unload already-read instructions.

## Recovery and release

Creation validates names and publishes a complete run atomically. Each run has one writer at a time. Busy or interrupted operations fail with the lock path. After a crash, confirm no process owns the operation before removing its lock directory, then rerun. Status is derived from receipts; do not hand-edit generated records.

Use protected branches, current required CI results, independent code-owner approval, and deployment checks in your forge. PR creation and review findings do not prove deployment. Retain deployed revision, deployment result, and rollback references in the deployment system. Close the run and clean up only when review/rework is finished.

Maintenance is optional intake, not a required completion stage. Enable it only after defining metric-specific thresholds, deduplication, cooldowns, and run limits. Alerts create drafts for service-owner triage, never fabricated approvals.

## Installation and migration

The installed `_system/scaffold.json` identifies the package and its original hashes.
Verification starts unconfigured (`checks: []`). Select real checks before verification;
file-name heuristics are not evidence that a test command is correct. An exit code
alone does not establish useful coverage; ensure the selected runner rejects zero tests.
Git must have an initial commit before candidate verification can run.

The installer requires an explicit target. `--check` previews a fresh installation
or validates an existing one. Repeat initialization preserves the project verification
configuration. Modified or missing managed files and version differences are reported;
there is no force overwrite or automatic upgrade. Create a fresh scaffold in a temporary
directory, compare it with the project, and review a separate migration before applying
changes. Preserve run evidence and customizations; never update factory files during an
active feature run. After migration, renew stale approvals and verification evidence.

Initialization uses a cooperating-process lock and preflights collisions and symlinks.
Ordinary write failures roll back files written by that attempt; empty directories may
remain. A killed process can leave a partial installation. Inspect it and the lock,
confirm no writer remains, then recover through a reviewed migration; do not force overwrite.
The installer does not create Git history, CI, deployment, monitoring, or tool credentials.

## ICM conventions

This is an ICM-inspired pipeline with explicit adaptations: stage contracts are
shared factory files rather than duplicated into every run, and machine evidence
is immutable-by-convention JSON alongside editable Markdown. Light profiles merge
planning checkpoints into one reviewed brief. Verification is automated; human
release review remains external. Status reads current artifacts and receipts from
the filesystem; neither folder existence nor Markdown approval labels advance it.
Stage numbers are fixed protocol identifiers, not configurable ordering labels.

Templates live in `_shared/`; `brief-template.md` is the light-profile edit surface.
Run outputs belong under `runs/`. Do not place application code there. Read the
active contract and immediate inputs rather than loading the full factory.
