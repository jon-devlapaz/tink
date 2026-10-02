# SDLC workspace

A small filesystem workflow: define a change, implement and verify it in an isolated checkout, then obtain independent release review. Requires Python 3.9+, Bash, and Git on a POSIX system. Native Windows is not supported. Set `require_tink: true` when project policy requires Tink integrity checks.

## Start a run

Run scripts from any working directory. Paths resolve from the script location.

```sh
python3 _system/scripts/sdlc.py new fix-example --kind bug
python3 _system/scripts/sdlc.py status fix-example
```

### Claude Code command access

Dogfood observed Claude Code permission prompts for `python3` and `git`. Review each
command before allowing it; this note is not a blanket allowlist. Optional Tink
integrations also invoke `tink` and `tink-route` when installed. Recheck the actual
prompts and commands before release.

The default `light` profile creates one `brief.md`: problem, acceptance criteria, approach, risks, and verification, plus `checklist.json` for the implementation checklist. Use `--profile full` for consequential architecture or policy changes; it creates the existing intent/spec/plan artifacts. A human selects the appropriate profile. Legacy artifacts without run metadata remain drafts; text approval tags are not imported as evidence.

After the human accepts the brief, record the actual review reference:

```sh
python3 _system/scripts/sdlc.py decide fix-example 3 approved \
  --reviewer 'Reviewer name' --source 'Actual review URL or decision reference' \
  --reason 'Accepted scope, approach, and acceptance criteria'
```

Light runs have ONE definition gate, recorded as stage 3 (`sdlc.py decide <run> 3 ...`): the approved `brief.md` + `checklist.json` are the intent, design and plan. Full runs record decisions for stages 1, 2, and 3 in order. Markdown stays editable. `changes-requested` records rejection using the same command. Editing an artifact or contract makes dependent decisions stale; older receipts remain available. No agent should invent a reviewer or approval. These local receipts track workflow; they do not authenticate identity or authorize a release.

## Checklist

`runs/<slug>/checklist.json` holds item definitions only: `{"schema":1,"items":[{"id","description","verify","check"?}]}`. Definitions are approved scope: stage 3 cannot be approved with zero or invalid items, and changing them makes that approval stale. Never hand-edit receipts or add status fields to the definitions.

An item may carry `check`: `{"argv":[...],"timeout_seconds":N}`, validated like `verification.json` checks. `verify` runs every item's `check` in the repository root after the configured checks, logging to `test-log.md`; a failure or timeout fails verification (`Checklist check failed: <id>`). A passing check proves the item in that verify run and needs no mark, so `mark` is refused for checked items. Review the commands at approval: they execute like verification checks and are bound to the approval digest, so editing one makes approval stale.

An item without `check` is attested. Agents record it with `sdlc.py mark <run> <item-id> passed|failed --evidence <text>`, which needs a current stage 3 approval and appends a receipt under `marks/`; the latest receipt per item wins, and `verify` requires it to be `passed`. The passed verification records a digest of each attested item's latest receipt (so editing one also stales it), so any later `mark` (pass or fail) makes it stale: mark first, then run `verify` again. Attested marks are self-reported claims: independent review should challenge them. Status flags an attested item marked on an older candidate as attested before the latest changes; re-check it only if the change affects it. The passed verification receipt records each item as `proven` or `attested`. Runs without `checklist.json` are legacy and unaffected (so deleting the file downgrades a run to legacy; status says `Checklist: none (legacy run)`; local files are not a tamper boundary, and neither are receipts you can delete or `git update-index --skip-worktree`, so trusted CI and forge review are the boundary); status reports a checklist deleted after approval as MISSING.

## Implementation and verification

Use a separate Git worktree or clone for each code-writing run, never just two branches in one checkout. Keep each run's artifacts in its own checkout. Do not share writable skill directories. Ports, databases, and credentials need separate isolation when used.

Configure `_system/verification.json` with nonempty command argument arrays and timeouts. Fresh installations have an empty check list and fail verification until real project tests, build, lint, and other required checks are configured. Missing commands, missing configuration, failed checks, and timeouts fail verification. Required Tink checks fail if Tink is unavailable. The tool does not infer test coverage from an exit code. A failed check's error ends with `(output: runs/<slug>/04-test/output/test-log.md)`, where the full output is logged.

```sh
python3 _system/scripts/sdlc.py verify fix-example
```

Verification writes the actual check output and a generated receipt under `04-test/output/`. The receipt binds the Git revision, tracked and nonignored untracked file contents/modes (excluding disposable untracked Python caches), artifact inputs, policy, test lock, and log digest. It excludes `runs/`; do not place application source or test infrastructure there. Ignored files and external services are outside this fingerprint: pin dependencies and environments in trusted CI. Generated `tink:rules` blocks in the root `AGENTS.md` are session context and are excluded from the candidate fingerprint (`sdlc.py walk` and `tink use --check` verify them); all other `AGENTS.md` text still counts. If the candidate changes while checks run, the error lists up to five changed paths and hints when tracked bytecode files are the cause. Commit code changes before final verification. The receipt records the observed commit for provenance; status compares candidate contents, so committing only run evidence does not invalidate unchanged code. CI must still verify the actual revision being merged. Avoid modifying the checkout while checks run.

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

## Who does what at a stage boundary

- The launcher (the human, or a script acting for them) records the gate, then runs
  `python3 _system/scripts/sdlc.py stage <run> <n>` (`--check` previews, `--worktree PATH` or `--here`
  picks the checkout, `--seed-contract PATH` adds the confirmed seed contract to a stage-1 prompt; pass only a seed contract the operator has confirmed in seed-me (the launcher cannot tell, and does not guess from wording), and the run keeps its own copy at `runs/<slug>/seed-contract.md`, committed with the run; that copy's sha256 is recorded in `run.json` and bound to approvals and verification, so editing the copy makes them stale, and re-running stage 1 with a revised original re-copies and re-binds it). It commits
  `runs/<run>`, creates the worktree (stage 3: branch `<run>`; stage 5: detached review checkout),
  runs `tink use <skillset> --snapshot runs/<run>/<stage-dir>` inside it, and prints the launch prompt
  for a NEW session. It refuses unless the entry gates are approved and current (stage 5 also needs
  current verification), and it fails closed when the skillset does not compile. Stage 4 runs inside the
  stage-3 session. Manual fallback: approve, commit `runs/<run>`, `git worktree add`, `tink use`, prompt.
- The agent never creates the run, never runs `tink use`, and never approves. It reads
  `AGENTS.md`, its stage `CONTEXT.md`, and `python3 _system/scripts/sdlc.py status <run>`.
- Build order in stages 3 and 4: implement, then `sdlc.py mark` the attested items (marks
  need an approved gate), then `python3 _system/scripts/sdlc.py verify`. Checked items are proven by `verify` and need no
  mark. Commit before verifying, because the candidate fingerprint includes uncommitted files.
- The reviewer works in a separate checkout, writes
  `runs/<slug>/05-deploy/output/REVIEW-findings.md`, and does not modify code or evidence.

## Skills and concurrency

Commit baseline `.tink/skills.toml` and `.tink/skills.lock` when Tink has created them through an authorized operation. Restore into each worktree through Tink; do not copy writable directories by symlink. Lockfile updates are reviewed dependency changes.

At stage open `sdlc.py stage` also asks `tink-route --pick --anywhere` once, over the whole library, with the stage's whole input document as the question (stage 1 the seed contract, 2 `intent.md`, 3 and 5 the brief or `spec.md`, 6 the review findings; no truncation, up to 120000 characters). The pick and the document's sha256 are recorded in `runs/<slug>/skills/stage-<n>-pick.json` and a routed skill is named in the launch prompt (read it with `tink mount <skill> --payload` before relying on it). Abstention, an over-long document, a missing router and any router error only print a line and record a receipt: the stage still opens. In our eval a document-only pick was correct on every routed case; it abstains on generic documents, so universal disciplines come from the stage's required rules, not the pick. Load mandatory skills deterministically. Route only genuine capability gaps with `tink-route`; allow abstention. `tink-route` does not mutate project skills: it mounts the delivered skill into the git-ignored `.active` directory inside `.tink`, prints it on stdout, and needs no cleanup. Run `tink` mutations through the wrapper:

```sh
python3 _system/scripts/sdlc.py skills tink -- skill check
```

The wrapper covers `tink` operations only. It serializes them across this host and rejects symlinked skill state. It does not grant mutation authority, isolate a shared home library, or coordinate direct CLI calls outside the wrapper. Command shapes after `--` belong to the installed Tink CLI; confirm them against `tink --help`. Cross-host shared storage requires external coordination.

Preserve the `tink-route` receipt with the run. Read the delivered skill before relying on it. Low confidence or a failed delivery leaves the capability unresolved.

## Stage skills

Each stage has a skillset pin committed at `.tink/skillsets/<name>-skillset.json`: 01-plan `planning-skillset`, 02-design `design-skillset`, 03-build `build-skillset`, 04-test `testing-skillset`, 05-deploy `deployment-skillset`, 06-maintain `maintenance-skillset`. A pin selects exact upstream code (`source`, `revision`, `sourceRoot`, `members`), so review pin changes like a lockfile: a pin change is a supply-chain change.

```sh
tink library fetch .tink/skillsets/<name>-skillset.json    # once per machine/library, after reviewing the pin
tink use <name>-skillset --snapshot runs/<slug>/<stage-dir>  # at stage open, then start a NEW session
tink-route --receipt runs/<slug>/skills.jsonl "<what you need>"
```

`required` lists the disciplines that `tink use` compiles into `AGENTS.md` at stage open, so each stage gets a fresh session that re-reads it. The starting set is every `principle-*` member of the pin; it is provisional and to be tuned later by ablation. Capability skills come only through `tink-route` for genuine gaps (stdout is the skill); exit 1 or 2 means continue without a skill.

`tink-route` searches the whole skill library and never reads `AGENTS.md`, so the command is identical in every stage. In our routing eval 56% of the skills a stage needed were not on that stage's shelf, and the whole library was as precise as the shelf where both applied. `--skillset NAME` restricts a call to one skillset (exit 1 may then add `Hint: <skill> fits but is on another shelf (<skillset>); it was not delivered.`); `--anywhere` is the default spelled out.

`seed-me` is human-invoked and belongs to no stage skillset. Install it separately with `tink skill add jon-devlapaz/tink-skills --skill seed-me`. Its confirmed seed contract is the input to 01-plan: give the agent that file's path in the stage-1 launch prompt.

`tink use` and `tink-route` are optional integrations; runs work without them. `tink-route` 0.10.0 or newer is required for whole-library routing (older versions scope routing to the stage shelf in the `AGENTS.md` rules block); `sdlc.py stage` warns when an older one is on PATH.

## Recovery and release

Creation validates names and publishes a complete run atomically. Each run has one writer at a time. Busy or interrupted operations fail with the lock path. After a crash, confirm no process owns the operation before removing its lock directory, then rerun. Status is derived from receipts; do not hand-edit generated records. To check that the workspace stays legible to an agent with no memory, run the structural walk lint (see Walk lint).

Use protected branches, current required CI results, independent code-owner approval, and deployment checks in your forge. When to open a PR: once the candidate is committed and verification is current (end of stage 4, and `stage <slug> 5` enforces it). Plans and design (stages 1-2) need no PR; a draft PR during build is optional and is not review. The agent may open the PR; only a human merges it. Changing code after it is open means verifying again. PR creation and review findings do not prove deployment. Retain deployed revision, deployment result, and rollback references in the deployment system. Close the run and strip only when review/rework is finished.

Maintenance is optional intake, not a required completion stage. Enable it only after defining metric-specific thresholds, deduplication, cooldowns, and run limits. Alerts create drafts for service-owner triage, never fabricated approvals.

## Walk lint

```sh
python3 _system/scripts/sdlc.py walk [--json]
```

A read-only, deterministic check of the structural preconditions of the ICM walk test: an agent with no memory should orient, act, and report status from the files alone. No network, no model, and it writes nothing. It checks: W1 the `AGENTS.md` entry file (router markers present, at most 60 lines outside generated blocks); W2 backticked `_system/`, `_shared/`, `stages/`, `.tink/`, `.agents/` pointers in the router and stage contracts resolve (`runs/` paths are exempt); W3 each stage contract names Inputs, Output, and a human Gate; W4 estimated context tokens per stage (warn above 8000, fail above 16000); W5 every `*-skillset` name has a pin and every pin is named; W6 `sdlc.py status` runs for every run; W7 the compiled `tink:rules` block is current (`tink use <name> --check`, skipped when `tink` is absent).

This is structural. It cannot prove that a cold agent can actually orient; a real walk by a fresh agent stays the test. A `tink:rules` block written by `tink use` (delimited by `<!-- tink:rules begin ... -->` and `<!-- tink:rules end -->`) is allowed router payload up to 8192 bytes, a deliberate divergence from ICM's rule that the router holds no content; any other content in `AGENTS.md` is bounded by the 60-line rule. Exit 0 when nothing fails (warnings do not fail), 1 when a check fails, 2 for usage errors.

## Installation and migration

The installed `_system/scaffold.json` identifies the package and its original hashes.
Verification starts unconfigured (`checks: []`). Select real checks before verification;
file-name heuristics are not evidence that a test command is correct. An exit code
alone does not establish useful coverage; ensure the selected runner rejects zero tests.
Git must have an initial commit before candidate verification can run.

The installer requires an explicit target. `--check` previews a fresh installation
or validates an existing one. Repeat initialization preserves the project verification
configuration. Modified or missing managed files are reported as errors; a scaffold
from a different package version is refused with a pointer to `--upgrade`. Preserve run
evidence and customizations; never update factory files during an active feature run.

## Upgrading the scaffold

Preview, then apply (the receipt `_system/scaffold.json` is the trust baseline):

    python3 <package>/scripts/init.py <target> --upgrade --check
    python3 <package>/scripts/init.py <target> --upgrade

- Managed files (stages, `_shared/`, `_system/` except `verification.json`) are updated
  when unmodified, restored when missing, removed when dropped upstream and unmodified.
- Project-owned files (`_system/verification.json` and `.tink/skillsets/*.json`) are never
  overwritten; locally changed ones are kept and reported. New upstream pins are created.
- A managed file you customized blocks the upgrade: nothing is written, and the plan lists
  each file with a `diff` to compare. Merge your changes by hand, or commit them and rerun
  with `--overwrite-customized` to take the package version (git keeps your old copy).
- Only the router block of `AGENTS.md` is replaced; the rest of the file is preserved.
  Missing or duplicated router markers refuse the upgrade. Downgrades are refused.
- `runs/` is never modified. When stage `CONTEXT.md` files change, approvals in runs that
  have decision receipts read as stale until re-approved.
- The receipt is written last, so an interrupted upgrade is safe to rerun.
- Afterward run `python3 _system/scripts/sdlc.py walk`.

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
