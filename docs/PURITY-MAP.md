# Purity map (stabilize candidates)

Snapshot against `ACCEPTANCE.md` (2026-09-06). **No code changes in this pass.** Prefer deleting dead docs over rewriting live gates.

### Boundary (1 paragraph)

v1 is the Rust CLI that mutates/validates project `.agents/skills/`, records standalone intent in `.tink/skills.toml` + `.tink/skills.lock`, maintains rebuildable `$TINK_HOME` inventory, inspects public GitHub without writes, destroys only managed project state, and updates the binary from verified Releases—proved by named rows (`I*`, `A*`, `H*`, `R*`, `K*`, `G*`, `C*`, `M*`, `L*`, `RD*`, `V*`, `P*`, `X*`, `D*`, `U*`, `S*`) plus the Proof cargo/audit gate. Authority is `ACCEPTANCE.md`; `tests/acceptance.rs` + `acceptance_traceability.rs` are the executable sensors. Out of v1 (do not implement as product work): weekly GitHub update workflows, private GitHub auth, Windows, library pruning, concurrent mutations in one project/home, and cross-filesystem rollback after unexpected I/O failure.

### Keep

| Candidate | Why (row / Proof) |
|---|---|
| `ACCEPTANCE.md` | Evaluator; all row ids + Proof |
| `README.md`, `AGENTS.md` | User surface; `AGENTS.md` is I5 create-only contract |
| `docs/ARCHITECTURE.md` | Module/state ownership map (trim drift below) |
| `docs/TESTING.md` | Sensor topology + local/CI gate (trim stale gaps) |
| `docs/RELIABILITY.md` | Process/exits/install trust aligned with V*/U*/Proof |
| `docs/PROJECT-MANIFEST-DESIGN.md` | Implemented companion for M1–M9 / on-disk manifest+lock |
| `src/*.rs` (all modules) | Load-bearing owners: `init`→I*, `add`→A*/R*, `skillsets`→K*, `inspect`→G*, `check`→C*, `manifest`→M*, `read`→RD*, `library`/`harvest`/`promote`→H*, `refresh`→P*, `remove`→X*, `destroy`→D*, `update`→U*, `git`/`process`/`output`→S1/V*/G9, `manage_tink`→I6/X5/P9–P14/C8, `catalog`/`home`→I4/L*/catalog contracts |
| `skills/manage-tink/**` | Embedded payload; I6, X5, C8, P9–P14, M4 |
| `tests/acceptance.rs` | Named sensors (`i1_…`, `a1_…`, …) for nearly every row |
| `tests/acceptance_traceability.rs` | Row↔sensor uniqueness guard |
| `tests/workflow_contract.rs` | Delivery-gate shape for ci/bump/release (Proof matrix) |
| `.github/workflows/{ci,bump-release,release}.yml` | Delivery gate in ACCEPTANCE header |
| `install.sh` | U6–U20 / V6 installer contract |
| `tink-test` | Dogfood runner (ACCEPTANCE Dogfood; TESTING) |
| `Cargo.toml`, `Cargo.lock`, `rust-toolchain.toml` | Locked 1.95.0 Proof toolchain |
| `LICENSE`, `assets/logo.png` | Product packaging |

### Trim

| Candidate | Drift / shrink |
|---|---|
| `docs/ARCHITECTURE.md` | State table still claims by-skillset catalog has **“no CLI writer”**; false vs Commands/K12 create-only authoring and K13 `skillset update`. Fix one sentence; do not rewrite the map. |
| `ACCEPTANCE.md` Commands table | Missing `tink skillset update …` while K13 + README ship it. Add the row; do not reopen Out of v1. |
| `docs/TESTING.md` | Gap list still says “C4 still needs no-network/no-write instrumentation”; `c4_check_preserves_…` already clears `PATH` and snapshots trees. Refresh gaps to match sensors (keep honest S1 partial / S2 manual / concurrent Out of v1). Drop “record history in DEEP-REFACTOR-LOG” once that log is archived. |
| `docs/TECH-DEBT.md` | Ledger is almost all `done`/`pinned` archaeology. Collapse to the one live smell (broader frontmatter field-shape beyond C5–C7) or delete after promoting that line into TESTING backlog. |
| Large `src` owners (`skillsets.rs`, `catalog.rs`, `lib.rs`, `update.rs`, `skills.rs`, `add.rs`) | Stabilize-by-clarity only; **no behavior change**. Any split must stay green on the owning prefix (`K*`, `L*`/`catalog` units, CLI dispatch V*, `U*`, `A*`/`R*`). |

### Archive/delete candidates (high confidence only)

| Candidate | Reason |
|---|---|
| `docs/REMOVE-TECHNICAL-DEBT-PLAN.md` | All phases checked done; process artifact only |
| `docs/SEMVER-MIGRATION-SPEC.md` | Landed (`semver::Version` in `update.rs`); status still “implementation-ready” |
| `docs/HOME-ISOLATION-SPEC.md` | Landed (`*_at(home: Option<&Path>, …)` throughout); not a live gate |
| `docs/GITHUB-INSPECTION-SPEC.md` | Superseded by G1–G10 + ARCHITECTURE/TESTING |
| `docs/SKILL-READ-SPEC.md` | Superseded by RD1–RD14 |
| `docs/DEEP-REFACTOR-LOG.md` | Experiment ledger; findings already promoted or obsolete |
| Working-tree `tink-test.sh` | **Gitignored** local smoke duplicating K12-ish paths; not a Proof sensor. Prefer `./tink-test` + `cargo test --test acceptance`. Delete from disk / stop maintaining |

Do **not** archive `PROJECT-MANIFEST-DESIGN.md` or `RELIABILITY.md` in the first pass—they still encode live contracts.

### Out of v1 (do not touch as product work)

- Explicit ACCEPTANCE Out of v1: weekly GitHub update workflows, private GitHub auth, Windows, pruning the library, concurrent Tink mutations in one project/home, cross-filesystem rollback after unexpected I/O failure.
- Local dogfood under `.agents/skills/*` (gitignored): perspective/review skills, etc. Not acceptance surface; do not “productize” or rewrite as stabilize work.
- Expanding beyond named Commands / inventing top-level `add`/`check`/`refresh` aliases (explicitly removed).
- Softening partial sensors (S1 command-wide Git refusal, S2 manual) into “done” without new real proof.

### Sensor risks (do not soften)

| Sensor | Risk if “cleaned” |
|---|---|
| `tests/acceptance_traceability.rs` | Orphan/duplicate row blindness |
| `tests/acceptance.rs` refusal/preflight rows | Especially A3/A4/A10/A11/A16, H11–H14, K5/K6/K11, M7–M9, P2/P5/P8/P12–P14, R7/R9/R12/R15, G8–G10, U4–U20, V4–V7, S1–S3 |
| `c4` / `rd11` / `c8` / closed-pipe V4–V6 / A6B | Easy to weaken into “still exits 0” without tree/mode/`PATH` assertions |
| `skills/manage-tink/**` + X5 / C8 | Doc or script “cleanup” drifts embedded skill vs binary |
| `install.sh` + workflow_contract | Installer/release serialization and digest case rules |
| Proof block in ACCEPTANCE | fmt/clippy/`-D warnings`/audit/release-build are the stabilize gate—not optional |

### Next verified deltas (≤5)

1. **Delete completed specs + debt plan + deep-refactor log** (`REMOVE-TECHNICAL-DEBT-PLAN`, `SEMVER-MIGRATION-SPEC`, `HOME-ISOLATION-SPEC`, `GITHUB-INSPECTION-SPEC`, `SKILL-READ-SPEC`, `DEEP-REFACTOR-LOG`). Update `ARCHITECTURE.md` / `TESTING.md` cross-links that point at the log. **Verify:** `rg` no broken links; `cargo test --test acceptance_traceability --locked`; full `cargo test --workspace --all-targets --locked` optional if only docs.
2. **Fix Commands + ARCHITECTURE drift for skillset catalog writers:** add `tink skillset update` to ACCEPTANCE Commands; correct “no CLI writer” to create-only `skillset add <url>` + advancing `skillset update` (K12/K13). **Verify:** `cargo test --test acceptance_traceability`; `cargo test --test acceptance k12_ k13_ -- --nocapture`.
3. **Collapse `TECH-DEBT.md`** to a single live backlog line (frontmatter field-shape beyond C5–C7) or delete the file and point TESTING at that backlog. **Verify:** docs-only; no sensor change.
4. **Remove gitignored `tink-test.sh`** from the working tree; keep `./tink-test` as dogfood. **Verify:** `./tink-test --help` (or `init` smoke); do not treat shell smoke as replacing `K*`/`U*` cargo sensors.
5. **Refresh `TESTING.md` gap list** to match reality (C4 instrumented; keep S1 partial, S2 manual, concurrent/I/O Out of v1). **Verify:** docs-only; `cargo test --test acceptance c4_ -- --nocapture` unchanged.



### Status

- **Delta 1:** done (2026-09-06) — six docs deleted; `tink-test.sh` removed; ARCHITECTURE/TESTING/TECH-DEBT cross-links cleaned.
- **Delta 2:** done (2026-09-06) — `tink skillset update [name]` added to ACCEPTANCE Commands; ARCHITECTURE.md updated from "no CLI writer" to create-only add / update.
- **Delta 3:** done (2026-09-06) — TECH-DEBT.md collapsed to single live backlog line (frontmatter field-shape beyond C5–C7).
- **Delta 4:** done (2026-09-06) — verified no `tink-test.sh` in tree; `./tink-test` retained as dogfood runner.
- **Delta 5:** done (2026-09-06) — TESTING.md gap list refreshed (stale C4 removed; S1/S2/Out of v1 preserved).
