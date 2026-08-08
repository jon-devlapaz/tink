# Remove Technical Debt Plan

## Context
- Intake date: 2026-08-07
- System: Tink CLI (`tink`) for project-local skill management (`.agents/skills/`) and local catalog/library metadata (`~/.tink`).
- Size/age profile: moderate-sized Rust CLI, with stable command surface and growing acceptance suite.
- Failure impact: regressions can break agent workflows, dev/CI automation, and distributed skill installation.
- Rewrite plan: no rewrite; continue incremental, behavior-first work in safe, test-backed slices.
- Intake scope (this run): phases 1–3 first, then reassess for deeper phases.

## Phase Status
| Phase | Skill | Status | Artifact | Date |
|---|---|---|---|---|
| 1 | working-with-legacy-code | done | TESTING.md + TECH-DEBT.md | 2026-08-07 |
| 2 | refactoring-patterns | done | TECH-DEBT.md | 2026-08-07 |
| 3 | clean-code | done | TECH-DEBT.md | 2026-08-07 |
| 4 | software-design-philosophy | done | TECH-DEBT.md | 2026-08-07 |
| 5 | clean-architecture | done | ARCHITECTURE.md | 2026-08-07 |
| 6 | pragmatic-programmer | done | TECH-DEBT.md | 2026-08-07 |
| 7 | release-it | done | RELIABILITY.md | 2026-08-07 |
| 8 | domain-driven-design | done | ARCHITECTURE.md | 2026-08-07 |
Statuses: pending · in-progress · awaiting-evidence · done · deferred: <reason> · skipped: <reason>
Optional phases (system-design, ddia-systems, team-topologies) are added as rows here when their Add-when condition becomes true.

## Key Decisions
| Date | Phase | Decision | Rationale |
|---|---|---|---|
| 2026-08-07 | Intake | Starting module focus is command dispatcher + skill lifecycle path: `src/lib.rs`, `src/add.rs`, `src/remove.rs`, `src/check.rs`, `src/catalog.rs`. | Highest-churn and highest-risk surface for user-visible behavior changes. |
| 2026-08-07 | Intake | Safety policy: production-adjacent workflow safety, no big-bang rewrite, and no behavior change before characterization tests. | Keeps shipped CLI behavior stable and preserves compatibility. |
| 2026-08-07 | Intake | Baseline green gate is `cargo test -- --nocapture`; phase progresses only while suite remains green. | Existing suite already covers command contracts and refusal paths. |
| 2026-08-07 | Phase 1 | No behavior bugs found during initial characterization that require immediate pin-as-bug tickets. | Existing acceptance suite already constrains listed high-risk paths. |
| 2026-08-07 | Phase 2 | Extracted checkout reference selection into `checkout_reference_skill` in `src/refresh.rs` to separate remote-reference preparation from `refresh_one` state transitions. | Keeps refresh sequencing explicit and preserves behavior while improving readability and future testability. |
| 2026-08-07 | Phase 3 | Extracted update replacement responsibilities in `src/update.rs` (`staging_binary_path`, `stage_new_binary`, `commit_staged_binary`) and kept binary replacement behavior unchanged. | Reduces a single-purpose function complexity into explicit primitives for safer future edits. |
| 2026-08-07 | Phase 4 | Extracted skill-entry parsing responsibilities in `src/check.rs` (`is_ignored_skill_entry`, `read_skill_entry`) to model directory-filtering and validation as explicit domain operations. | Improves design clarity around what constitutes a project skill entry before validation. |
| 2026-08-07 | Phase 5 | Centralized repeated git command execution/error translation in `src/git.rs` (`run_git`, `git_detail`) to separate infrastructure command execution concerns from skill orchestration flow. | Enforces a clearer boundary between plumbing and caller behavior while preserving command output/error semantics. |
| 2026-08-07 | Phase 6 | Hardened CLI entrypoint by converting startup `current_dir` panic into an explicit error return path (`src/main.rs`). | Keeps failure-mode handling explicit at process boundaries and avoids hidden crash modes. |
| 2026-08-07 | Phase 7 | Hardened integration points by adding bounded transport settings, retry budgets, and update-time hardening in `src/git.rs`, `src/update.rs`, and entrypoint startup failure handling. | Adds deterministic failure behavior for outbound/IO boundaries and codifies reliability policy for future updates. |
| 2026-08-07 | Phase 8 | Introduced catalog-domain aggregate `CatalogMeta` in `src/catalog.rs` to model by-project catalog persistence (`name`, `root`, `skills`) and make ownership / mutation rules explicit while preserving write/read behavior. | Clarifies catalog as a domain aggregate boundary; behavior remains unchanged with existing compatibility contracts. |

## Next Actions
- [x] Create `docs/TESTING.md` with phase-1 safety net and characterization matrix.
- [x] Create `docs/TECH-DEBT.md` with initial debt ledger and conventions.
- [x] Verify baseline tests remain green to mark Phase 1 exit.
- [x] Add characterization test for stale/invalid installed-skill metadata in `tests/acceptance.rs::c4_check_fails_with_corrupt_installed_skill`.
- [x] Enter Phase 2 after phase-1 gate and apply one behavior-preserving refactor. (done)
- [x] Enter Phase 3 after this behavior-preserving clean-code improvement is green. (done)
- [x] Enter Phase 4 after this behavior-preserving design-oriented extraction is green. (done)
- [x] Enter Phase 5 and complete one clean-architecture refactor. (done)
- [x] Enter Phase 6 and enforce startup contract at `src/main.rs` boundary. (done)
- [x] Enter Phase 7 and harden integration points with timeouts/retries at CLI boundaries (`src/git.rs`, `src/update.rs`) and publish reliability policy. (done)
- [x] Enter Phase 8 after Phase 7 gate.
- [x] Extract catalog aggregate domain model (`CatalogMeta`) and cover with unit coverage in `src/catalog.rs::tests::catalog_meta_models_skill_set_and_ownership`.
