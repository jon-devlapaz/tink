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
| `src/*.rs` (all modules) | Load-bearing owners: `init`→I*, `add`/`inventory`→A*/R*, `skillsets`→K*, `inspect`→G*, `check`→C*, `manifest`→M*, `read`→RD*, `library`/`harvest`/`promote`→H*, `refresh`→P*, `remove`→X*, `destroy`→D*, `update`→U*, `git`/`process`/`output`→S1/V*/G9, `manage_tink`→I6/X5/P9–P14/C8, `catalog`/`home`→I4/L*/catalog contracts |
| `skills/manage-tink/**` | Embedded payload; I6, X5, C8, P9–P14, M4 |
| `tests/acceptance.rs` | Named sensors (`i1_…`, `a1_…`, …) for nearly every row |
| `tests/acceptance_traceability.rs` | Row↔sensor uniqueness guard |
| `tests/workflow_contract.rs` | Delivery-gate shape for ci/bump/release (Proof matrix) |
| `.github/workflows/{ci,bump-release,release}.yml` | Delivery gate in ACCEPTANCE header |
| `install.sh` | U6–U20 / V6 installer contract |
| `tink-test` | Dogfood runner (ACCEPTANCE Dogfood; TESTING) |
| `Cargo.toml`, `Cargo.lock`, `rust-toolchain.toml` | Locked 1.95.0 Proof toolchain |
| `LICENSE`, `assets/logo.png` | Product packaging |

### Trim / archive (closed)

Deltas 1–5 consumed the Trim and Archive/delete candidates below
(Commands/ARCHITECTURE drift, TESTING/TECH-DEBT collapse, completed-spec
deletes, `tink-test.sh`). Do not reopen those rows as open work.

Remaining clarity splits on large `src` owners stay optional and must keep
owning sensors green (`K*`, `L*`, `V*`, `U*`, `A*`/`R*`) with **no behavior
change**.

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

### Next verified deltas

None — map closed. Optional lint/dead slices stay outside this map.

### Status

- **Delta 1:** done (2026-09-06) — six docs deleted; `tink-test.sh` removed; ARCHITECTURE/TESTING/TECH-DEBT cross-links cleaned.
- **Delta 2:** done (2026-09-06) — `tink skillset update [name]` added to ACCEPTANCE Commands; ARCHITECTURE.md updated from "no CLI writer" to create-only add / update.
- **Delta 3:** done (2026-09-06) — TECH-DEBT.md collapsed to single live backlog line (frontmatter field-shape beyond C5–C7).
- **Delta 4:** done (2026-09-06) — verified no `tink-test.sh` in tree; `./tink-test` retained as dogfood runner.
- **Delta 5:** done (2026-09-06) — TESTING.md gap list refreshed (stale C4 removed; S1/S2/Out of v1 preserved).
- **Inventory publish seam:** done (`a0b8fef`) — `inventory::{publish, publish_from_library}`.
- **Phase-1 dead/lint:** done (`ab8f885`) — DC-04/06/07/01; DC-05 parked.
