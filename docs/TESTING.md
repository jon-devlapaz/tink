# Testing

## Test Strategy
- Stack baseline: Rust 2024 CLI, validated with `cargo test`.
- Verification ladder:
  1. keep current suite green,
  2. map entrypoint behavior with characterizing tests,
  3. only then refactor in the smallest complete unit.
- Green gate: any refactor keeps `cargo test -- --nocapture` green before acceptance.
- Tooling: unit tests in `src/*` modules and acceptance tests in `tests/acceptance.rs`.

## Entry Effect Sketch (Phase 1)
- `main.rs::main` parses `Cli` with `clap` and calls `run(cli, cwd)`.
- `run` delegates to `dispatch`:
  - `Init {}` path: validates project layout and optional flags
  - `Skill { ... }` path: route to `dispatch_skill`
  - `Destroy {}` path: remove project scaffolding + catalog entry
  - `Update` path: perform binary replacement from GitHub release metadata
- `dispatch_skill` routes to `add`, `list`, `check`, `refresh`, `remove`, `harvest`.

### Pinch points identified
- **`dispatch_skill` routing** — the central control fan-out for all operational behavior.
- **`add_skill_inner` branching** — local path vs filesystem-ish path vs remote vs library name.
- **Catalog side-effects ordering** (`add` deposit before/while install; `remove` withdraw-before-delete) and failure safety.
- **`check_project` preflight** — symlink/structure refusals and `ZEN` coupling checks.

### Chosen seams
- Do not change `run -> dispatch` signatures or command enums in Phase 1.
- Characterize first, then isolate changes to one behavior path per iteration.

## Safety Net Map
| Module | Pinned behaviors | Test files | Gaps |
|---|---|---|---|
| `src/lib.rs` | CLI dispatch is exhaustive by command enum; success prints are constrained and failures return exit code 1 via error pathway. | `tests/acceptance.rs` (`i*`, `a*`, `c*`, `l*`, `x*`) | No unit coverage for exact styled output in each branch (intentional for now). |
| `src/add.rs` | Add source resolution precedence: local path/remote/library; idempotence and divergence refusal semantics are preserved. | `tests/acceptance.rs` (`a1..a8`, `r*`) | Add-path coverage for malformed local-tree receipt combinations not in scope yet. |
| `src/check.rs` | `skill check` requires `.agents/skills` and validates each installed skill directory/name, and enforces ZEN/AGENTS coupling rule. | `tests/acceptance.rs` (`c1`, `c2`, `c3`, `c4`, `l4`, `l5`) | Add targeted checks for additional malformed installed-skill states (e.g., non-YAML metadata variants, missing required fields). |
| `src/remove.rs` | `remove` withdraws catalog entry before deleting project tree; never touches library content. | `tests/acceptance.rs` (`x1`, `x2`, `x4`), plus malformed-catalog regression (`x6_remove_fails_when_catalog_meta_is_malformed`) | Explicitly covers malformed catalog state behavior on remove. |
| `src/refresh.rs` | `refresh_one` computes old/new remote skill repositories and refresh logic without mutating project state until checks pass. | `tests/acceptance.rs` (`p1`, `p2`, `p3`, `p4`, `p5`, `p6`, `p7`) | Refactor extraction added for clarity; now verify this invariant remains covered. |
| `src/update.rs` | `replace_binary` staging and replacement behavior is clear and single-purpose (`staging_binary_path`, `stage_new_binary`, `commit_staged_binary`). | `tests/update.rs` (`asset_name_*`) | Add a direct behavior test only if binary replacement edge-cases appear in future. |
| `src/catalog.rs` | `CatalogMeta` models catalog persistence as a domain aggregate (`name`, `root`, `skills`) and owns ownership/purge logic for by-project entries. | `tests/acceptance.rs` (`a*`, `l*`, `x*`) + `catalog::tests::catalog_meta_models_skill_set_and_ownership` | Keep behavior compatibility by preserving existing catalog JSON schema and legacy fallback behavior for legacy/non-JSON skips. |
| `src/git.rs` | `run_git` centralizes command execution and status/error translation while preserving `remote_head`, `checkout`, and `checkout_revision` error contracts. | `tests/acceptance.rs` (`r*`, `p*`) | Continue to rely on acceptance flows as end-to-end behavior proof; add unit coverage only if message-contract changes appear. |
| `src/update.rs` | `curl_to_file` applies bounded timeouts and bounded retries on external artifact/API fetch, so update can fail fast instead of hanging. | `tests/acceptance.rs` (`u1`, `u2`, `u3`) + unit `curl_transport_args_*` | Keep behavior compatibility by leaving success path unchanged; add failure-path telemetry if needed by CI pipelines. |
| `src/main.rs` | Binary entrypoint fails gracefully if process working directory is unavailable, rather than panicking. | `tests/acceptance.rs` (`v*`, including `v3_main_fails_when_current_directory_is_unavailable`) | Pin startup failure-path behavior and confirm exact failure class and exit code without path-dependent timing assumptions. |
| `src/check.rs` | `load_project_skills` explicitly distinguishes ignorable entries (`README.md`, hidden dirs/files) from malformed entries before skill read/validate. | `tests/acceptance.rs` (`c3`, `c4`, `l*`) | New helper split is design-oriented; existing tests already cover key entry/validation failures. |

## Characterization Backlog
- [x] Pin add idempotence on unchanged local source (`tink skill add`, no-op path). (`a2_add_identical_is_noop`)
- [x] Pin behavior when removing project skill leaves library intact and updates catalog. (`x1_remove_deletes_project_skill_keeps_library_drops_catalog`)
- [x] Pin `tink skill check` pass path after install and failure path without `.agents/skills`. (`c1_check_passes_valid_project`, `c2_check_fails_without_skills_dir`)
- [x] Add characterization test for a deliberately corrupted installed `SKILL.md` (`c4_check_fails_with_corrupt_installed_skill`).
- [x] Pin behavior for malformed catalog metadata entries during catalog listing (`l6_skill_list_catalog_skips_malformed_meta_entries`).
- [x] Pin malformed-catalog behavior for remove (`x6_remove_fails_when_catalog_meta_is_malformed`).

## CI Gates
- `cargo test -- --nocapture`
- `./tink-test` acceptance smoke path when integration behavior changes.

