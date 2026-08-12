# Implementation Plan: Tink v1 Stabilization and Release Readiness

## Overview

Treat the current Tink feature set as complete and prepare it for an honest
`v1.0.0` decision. The work closes or precisely narrows the three proof gaps
declared in `ACCEPTANCE.md`, preserves the existing command and on-disk
contracts, verifies the full supported-platform release gate, and makes the
major-version promotion safe under the existing automatic release workflow.
No new user capability belongs in this plan.

## Outcome and Scope Boundary

The outcome is a release candidate whose current behavior is demonstrably safe
to label `v1.0.0`, followed by a human go/no-go decision and, only after explicit
approval, a verified public release.

In scope:

- Automated or explicitly bounded proof for acceptance rows C4, S1, and S2.
- Current local, CI, and supported-platform release verification.
- A safe one-time transition from the `0.3.x` patch train to `1.0.0`.
- Minimal version and user-facing maturity documentation changes.
- Post-release install and update smoke tests.

Out of scope:

- New commands, flags, storage formats, sources, or lifecycle behavior.
- Windows, private GitHub authentication, concurrency, or other items already
  listed as out of v1.
- A new release framework, roadmap system, governance document, or agent team.
- Refactors that are not required to establish the named proof.

## Architecture Decisions

- **Freeze the v1 behavior boundary.** `ACCEPTANCE.md` remains the evaluator;
  this plan improves evidence without expanding the product contract.
- **Test only claims Tink owns.** C4 and S1 are Tink process guarantees. S2 must
  be phrased as a testable Tink boundary—library-only skills are not treated as
  project-live skills—rather than claiming control over every external agent
  harness.
- **Guard Git at the centralized boundary.** Tink may use read-only Git
  operations and temporary clones, but its Git process owner should make the
  forbidden project mutations (`init`, `add`, `commit`, and `push`) impossible
  to introduce accidentally.
- **Promote versions through existing automation.** Extend the current release
  workflow only enough to publish an intentionally pre-bumped, untagged version.
  Do not add a second release workflow or manually race the patch bumper.
- **Separate readiness from publication.** Passing tests and CI creates a
  release candidate; publishing `v1.0.0` remains a distinct human-approved
  action.

## Dependency Graph

```text
Task 1: Establish baseline evidence
    |
    +--> Task 2: Prove check is read-only
    |        |
    +--> Task 3: Guard forbidden Git mutations
    |        |
    +--> Task 4: Prove library/live isolation
             |
             +--> Checkpoint A: v1 contract proof
                       |
                       +--> Task 5: Make major promotion safe
                                  |
                                  +--> Checkpoint B: release mechanism
                                             |
                                             +--> Task 6: Prepare v1 candidate
                                                        |
                                                        +--> Task 7: Publish and verify
```

Tasks 2–4 are conceptually independent, but they share `ACCEPTANCE.md` and
`tests/acceptance.rs`; implement them sequentially to keep review and rollback
small. There is no useful multi-agent parallelism for this plan.

## Task 1: Establish the frozen baseline

**Description:** Prove that the clean `v0.3.20` checkout satisfies the current
local quality gate before changing its evidence or release machinery. Record
failures in the implementation session rather than weakening acceptance rows.

**Acceptance criteria:**

- [ ] The worktree is clean and `main` is reconciled with `origin/main`.
- [ ] Every command in the `ACCEPTANCE.md` proof block passes locally where the
      host supports it.
- [ ] Current GitHub CI and the `v0.3.20` release are confirmed healthy; any
      environmental limitation is identified separately from a product defect.

**Verification:**

- [ ] `git status --short --branch`
- [ ] `cargo fmt --all -- --check`
- [ ] `cargo check --workspace --all-targets --locked`
- [ ] `cargo clippy --workspace --all-targets --locked -- -D warnings`
- [ ] `cargo test --workspace --all-targets --locked`
- [ ] `cargo test --workspace --locked --doc`
- [ ] `cargo build --workspace --release --locked`
- [ ] `cargo audit --file Cargo.lock`
- [ ] Inspect the current GitHub Actions and release result without mutating it.

**Dependencies:** None

**Files likely touched:** None

**Estimated scope:** XS (verification only)

## Task 2: Prove `skill check` is read-only

**Description:** Replace C4's manual marker with an automated acceptance sensor.
Run `skill check` against a valid fixture while external command lookup is
unavailable, and compare the relevant project and Tink-home trees before and
after execution. The sensor must detect content, entry-type, executable-mode,
or path changes without relying on access times.

**Acceptance criteria:**

- [ ] C4 fails if `skill check` creates, removes, or changes project or home
      state.
- [ ] C4 succeeds without Git, curl, or other external commands available on
      `PATH`, establishing the owned no-network/no-child-process boundary.
- [ ] `ACCEPTANCE.md` names C4 as an automated sensor without broadening its
      claim beyond observable Tink behavior.

**Verification:**

- [ ] `cargo test --test acceptance c4_`
- [ ] `cargo test --test acceptance_traceability`
- [ ] `cargo test --workspace --all-targets --locked`

**Dependencies:** Task 1

**Files likely touched:**

- `tests/acceptance.rs`
- `ACCEPTANCE.md`

**Estimated scope:** S (2 files)

## Task 3: Guard forbidden Git mutations

**Description:** Turn S1 from an init-only observation into an enforceable
process-boundary guarantee. Keep all Tink Git execution behind `src/git.rs`,
reject the forbidden project-mutating verbs before process spawn, and exercise
representative successful local and remote command paths through acceptance
coverage. Temporary clone/fetch behavior remains allowed.

**Acceptance criteria:**

- [ ] The centralized Git boundary refuses `init`, `add`, `commit`, and `push`
      before spawning Git.
- [ ] Legitimate inspection/import/refresh Git operations continue to pass.
- [ ] S1's acceptance wording and sensor describe the proven boundary and no
      longer claim broader coverage than the test provides.

**Verification:**

- [ ] Focused unit tests for the Git argument guard pass.
- [ ] `cargo test --test acceptance s1_`
- [ ] `cargo test --test acceptance_traceability`
- [ ] `cargo test --workspace --all-targets --locked`

**Dependencies:** Task 1; perform after Task 2 to avoid shared-file conflicts

**Files likely touched:**

- `src/git.rs`
- `tests/acceptance.rs`
- `ACCEPTANCE.md`

**Estimated scope:** M (3 files)

## Task 4: Prove library and live-skill isolation

**Description:** Replace S2's untestable external-harness phrasing with the
strongest claim Tink owns: a library-only skill never appears in project skill
listing or validation and becomes live only through explicit promotion. Keep
the README's distinction between inventory and discovery consistent.

**Acceptance criteria:**

- [ ] A library-only fixture is absent from `tink skill list` project output and
      does not affect `tink skill check`.
- [ ] Explicit `tink skill add <library-name>` makes that skill project-live,
      after which list and check observe it normally.
- [ ] S2 and the README state the owned isolation guarantee without claiming
      control over third-party agent discovery configuration.

**Verification:**

- [ ] `cargo test --test acceptance s2_`
- [ ] `cargo test --test acceptance_traceability`
- [ ] `cargo test --workspace --all-targets --locked`
- [ ] Manually compare the revised wording in `README.md` and `ACCEPTANCE.md`.

**Dependencies:** Task 1; perform after Task 3 to avoid shared-file conflicts

**Files likely touched:**

- `tests/acceptance.rs`
- `ACCEPTANCE.md`
- `README.md`

**Estimated scope:** M (3 files)

## Checkpoint A: v1 contract proof

- [ ] Tasks 1–4 acceptance criteria are satisfied.
- [ ] `ACCEPTANCE.md` has no unexplained manual or partial sensor marker.
- [ ] The full local proof block passes.
- [ ] The diff contains no new user-facing capability or storage contract.
- [ ] Human review confirms the original v1 boundary is still intact.

Stop here if closing a proof gap requires new product machinery. Reclassify the
claim precisely or seek a revised scope instead of expanding Tink.

## Task 5: Make intentional major-version promotion safe

**Description:** Teach the existing `bump-release` workflow to distinguish an
intentional untagged version already present on `main` from the normal
tagged-current-version patch path. The intended `1.0.0` commit should be tagged
and dispatched as `v1.0.0`, not automatically transformed into `1.0.1`.

**Acceptance criteria:**

- [ ] A tagged current version still produces the next patch release exactly as
      it does today.
- [ ] An untagged, higher, manifest/lock-consistent version is published at that
      exact version after all gates pass.
- [ ] Existing, non-ancestor, mismatched, lower, or malformed tags/versions fail
      closed before any push or dispatch.

**Verification:**

- [ ] Focused workflow-contract tests cover normal patching and intentional
      pre-bumped promotion.
- [ ] `cargo test --test workflow_contract`
- [ ] Review the workflow's atomic push, tag validation, and dispatch paths.
- [ ] Full local proof block passes.

**Dependencies:** Checkpoint A

**Files likely touched:**

- `.github/workflows/bump-release.yml`
- `tests/workflow_contract.rs`

**Estimated scope:** S (2 files)

## Checkpoint B: release mechanism

- [ ] Task 5 passes focused and full verification.
- [ ] A dry review proves `1.0.0` will not become `1.0.1` accidentally.
- [ ] Failure paths cannot publish a partial main/tag pair.
- [ ] Human approval is obtained before preparing the version candidate.

## Task 6: Prepare the `v1.0.0` candidate

**Description:** Change only the package maturity markers and concise
user-facing positioning needed for the major release. Do not change commands,
formats, dependencies, or behavior in the version-preparation increment.

**Acceptance criteria:**

- [ ] `Cargo.toml` and the Tink entry in `Cargo.lock` both declare `1.0.0`.
- [ ] README installation and usage remain accurate and identify the accepted
      v1 support boundary without adding a roadmap subsystem.
- [ ] The candidate diff contains no runtime behavior change.

**Verification:**

- [ ] `cargo metadata --locked --no-deps` reports package version `1.0.0`.
- [ ] `git diff --check`
- [ ] Full local proof block passes.
- [ ] Pull-request CI passes on all four supported target runners.

**Dependencies:** Checkpoint B

**Files likely touched:**

- `Cargo.toml`
- `Cargo.lock`
- `README.md`

**Estimated scope:** M (3 files)

## Task 7: Publish and verify `v1.0.0`

**Description:** After an explicit human go decision, merge the focused
candidate, allow the existing automation to atomically tag and dispatch the
release, and prove the public artifacts and real install/update paths. This is
an external mutation and requires its own approval at execution time.

**Acceptance criteria:**

- [ ] GitHub publishes non-draft `v1.0.0` with exactly four expected regular
      archives and matching SHA-256 asset digests.
- [ ] A clean install reports `tink 1.0.0`; updating the prior public version
      reaches `1.0.0` without damaging the existing binary on failure.
- [ ] `main`, tag, Cargo version, GitHub release, and installed binary reconcile
      to the same version.

**Verification:**

- [ ] Required PR checks pass before merge.
- [ ] Release workflow quality, audit, build, and publish jobs pass.
- [ ] Run the installer in a temporary destination and execute `tink --version`.
- [ ] Run the supported update smoke path from the preceding public version in
      an isolated temporary destination.
- [ ] Confirm `git status`, `git tag`, release metadata, asset inventory, and
      checksums agree.

**Dependencies:** Task 6 and an explicit human go decision

**Files likely touched:** None beyond Task 6; external GitHub state changes

**Estimated scope:** M (release operation and live verification)

## Checkpoint C: Final go/no-go

Before publication:

- [ ] No unresolved correctness, security, or compatibility finding remains.
- [ ] All task acceptance criteria and the standing Definition of Done pass.
- [ ] CI is green on macOS/Linux and x86_64/arm64.
- [ ] The candidate adds no post-freeze feature.
- [ ] Rollback is understood: do not move/delete a published tag; fix a failed
      draft before publication, or ship a subsequent patch after publication.
- [ ] Human explicitly approves public release.

After publication:

- [ ] Install and update smoke tests pass against public release infrastructure.
- [ ] Public documentation resolves to the released behavior.
- [ ] Tink enters maintenance mode: bug fixes, security, compatibility,
      onboarding friction, and documentation only unless observed user evidence
      justifies reopening the feature boundary.

## Definition of Done

Every implementation task must satisfy its acceptance criteria plus the
repository-wide bar:

- Correctness is exercised at runtime, including failure paths.
- Focused tests fail without the proof/change and pass with it.
- The full existing suite, formatting, Clippy, docs, build, and audit pass.
- Public behavior and compatibility remain documented accurately.
- Security and rollback implications are reviewed.
- No unrelated refactor, dead code, debug output, or process artifact remains.
- A human reviews each release-boundary increment before merge/publication.

## Risks and Mitigations

| Risk | Impact | Mitigation |
|---|---|---|
| A test overclaims control over third-party agent discovery | High | Rewrite S2 around observable Tink project/library behavior only |
| Read-only testing observes access-time noise as a write | Medium | Snapshot content, paths, kinds, modes, and stable metadata; exclude atime |
| Git guard blocks required clone/inspection operations | High | Deny only named forbidden verbs and retain focused positive-path tests |
| Major bump is auto-incremented to `1.0.1` | High | Complete Task 5 and its checkpoint before changing package versions |
| Automatic release partially publishes | High | Preserve quality dependencies, atomic main/tag push, draft-first assets, and digest checks |
| “v1 cleanup” becomes feature development | High | Reject new behavior during this plan; require a separate evidence-backed proposal |
| Local green checks hide platform failure | Medium | Require the four-target GitHub matrix before the go decision |
| Public smoke test damages an installed binary | High | Use isolated temporary destinations and verify rollback behavior |

## Open Questions Requiring Human Decisions

- At Checkpoint A: Are the narrowed, automated S2 semantics strong enough to
  retire the manual claim, or should the external-harness statement remain an
  explicitly accepted manual assertion?
- At Checkpoint B: Approve preparing the `1.0.0` candidate only after reviewing
  the release-automation change.
- At Checkpoint C: Approve or decline the public `v1.0.0` release based on the
  complete evidence package.
