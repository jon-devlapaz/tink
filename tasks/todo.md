# Tink v1 Stabilization Checklist

This checklist executes `tasks/plan.md`. No new commands, formats, or product
capabilities are part of the v1 stabilization boundary.

## Phase 1: Establish the baseline

### Task 1: Establish the frozen baseline

**Dependencies:** None

**Estimated scope:** XS; verification only

**Files likely touched:** None

**Acceptance criteria:**

- [ ] Clean `main` is reconciled with `origin/main` at `v0.3.20`.
- [ ] The complete local proof block passes on the current host.
- [ ] Current CI and release health are confirmed; environmental limitations
      are separated from product defects.

**Verification:**

- [ ] `git status --short --branch`
- [ ] `cargo fmt --all -- --check`
- [ ] `cargo check --workspace --all-targets --locked`
- [ ] `cargo clippy --workspace --all-targets --locked -- -D warnings`
- [ ] `cargo test --workspace --all-targets --locked`
- [ ] `cargo test --workspace --locked --doc`
- [ ] `cargo build --workspace --release --locked`
- [ ] `cargo audit --file Cargo.lock`
- [ ] Inspect GitHub CI and `v0.3.20` release state read-only.

## Phase 2: Close declared proof gaps

### Task 2: Prove `skill check` is read-only

**Dependencies:** Task 1

**Estimated scope:** S; `tests/acceptance.rs`, `ACCEPTANCE.md`

**Acceptance criteria:**

- [ ] C4 detects project/home path, content, kind, or mode changes.
- [ ] C4 passes with external commands unavailable.
- [ ] C4 is an automated, accurately bounded acceptance sensor.

**Verification:**

- [ ] `cargo test --test acceptance c4_`
- [ ] `cargo test --test acceptance_traceability`
- [ ] `cargo test --workspace --all-targets --locked`

### Task 3: Guard forbidden Git mutations

**Dependencies:** Task 1; run after Task 2 because files overlap

**Estimated scope:** M; `src/git.rs`, `tests/acceptance.rs`, `ACCEPTANCE.md`

**Acceptance criteria:**

- [ ] The centralized Git boundary rejects `init`, `add`, `commit`, and `push`
      before process spawn.
- [ ] Required clone, inspect, and refresh paths remain functional.
- [ ] S1 states and tests the precise enforced boundary.

**Verification:**

- [ ] Focused Git-boundary unit tests pass.
- [ ] `cargo test --test acceptance s1_`
- [ ] `cargo test --test acceptance_traceability`
- [ ] `cargo test --workspace --all-targets --locked`

### Task 4: Prove library and live-skill isolation

**Dependencies:** Task 1; run after Task 3 because files overlap

**Estimated scope:** M; `tests/acceptance.rs`, `ACCEPTANCE.md`, `README.md`

**Acceptance criteria:**

- [ ] A library-only skill is absent from project list/check behavior.
- [ ] Explicit promotion makes the skill project-live and observable.
- [ ] Documentation claims only the isolation behavior Tink owns.

**Verification:**

- [ ] `cargo test --test acceptance s2_`
- [ ] `cargo test --test acceptance_traceability`
- [ ] `cargo test --workspace --all-targets --locked`
- [ ] Manually compare README and acceptance wording.

## Checkpoint A: v1 contract proof

- [ ] Tasks 1–4 satisfy their acceptance criteria.
- [ ] No unexplained manual or partial sensor remains.
- [ ] Full local proof passes.
- [ ] No new feature or storage contract entered the diff.
- [ ] Human confirms the v1 behavior boundary remains intact.

## Phase 3: Make the major promotion safe

### Task 5: Support intentional major-version promotion

**Dependencies:** Checkpoint A

**Estimated scope:** S; `.github/workflows/bump-release.yml`,
`tests/workflow_contract.rs`

**Acceptance criteria:**

- [ ] Tagged current versions retain normal patch-bump behavior.
- [ ] An intentional higher untagged version publishes at that exact version.
- [ ] Invalid, lower, conflicting, or unsafe release states fail before push or
      dispatch.

**Verification:**

- [ ] Focused workflow-contract tests cover both version paths.
- [ ] `cargo test --test workflow_contract`
- [ ] Atomic push, validation, and dispatch paths are reviewed manually.
- [ ] Full local proof passes.

## Checkpoint B: release mechanism

- [ ] Task 5 passes focused and full verification.
- [ ] Dry review proves `1.0.0` cannot become `1.0.1` accidentally.
- [ ] No partial main/tag publication path is introduced.
- [ ] Human approves preparation of the version candidate.

## Phase 4: Prepare the candidate

### Task 6: Prepare `v1.0.0`

**Dependencies:** Checkpoint B

**Estimated scope:** M; `Cargo.toml`, `Cargo.lock`, `README.md`

**Acceptance criteria:**

- [ ] Manifest and lockfile both declare `1.0.0`.
- [ ] README accurately states the accepted v1 support boundary.
- [ ] The candidate contains no runtime behavior change.

**Verification:**

- [ ] `cargo metadata --locked --no-deps` reports `1.0.0`.
- [ ] `git diff --check`
- [ ] Full local proof passes.
- [ ] PR CI passes on all four supported targets.

## Checkpoint C: Final go/no-go

- [ ] All acceptance criteria and the standing Definition of Done pass.
- [ ] No unresolved correctness, security, or compatibility finding remains.
- [ ] Four-target CI is green.
- [ ] Candidate contains no post-freeze feature.
- [ ] Release failure and post-publication rollback paths are understood.
- [ ] Human explicitly approves public release.

## Phase 5: Publish and verify

### Task 7: Publish `v1.0.0`

**Dependencies:** Task 6 and explicit human go approval

**Estimated scope:** M; external GitHub release state only

**Acceptance criteria:**

- [ ] Non-draft `v1.0.0` contains exactly four expected archives with matching
      GitHub SHA-256 digests.
- [ ] Clean install and prior-version update both reach `tink 1.0.0` in isolated
      temporary destinations.
- [ ] `main`, tag, manifest, release, and installed binary agree on `1.0.0`.

**Verification:**

- [ ] Required PR checks pass before merge.
- [ ] Release quality, audit, build, and publish jobs pass.
- [ ] Temporary clean-install smoke test passes.
- [ ] Temporary prior-version update smoke test passes.
- [ ] Git and GitHub release metadata reconcile.

## Completion

- [ ] Public documentation matches released behavior.
- [ ] Tink enters maintenance mode: bugs, security, compatibility, onboarding,
      and documentation unless observed evidence justifies new capability.
- [ ] No temporary verification artifact or unrelated change remains.

## Risks to re-check at every checkpoint

- [ ] Tests do not claim control over third-party agent harnesses.
- [ ] S1 protection does not block legitimate Git reads or temporary clones.
- [ ] Release automation cannot turn `1.0.0` into `1.0.1`.
- [ ] Automatic publication remains atomic and draft-first.
- [ ] Platform CI, not local tests alone, supports the release decision.
- [ ] Public smoke tests use isolated destinations and preserve existing tools.
