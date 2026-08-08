# Architecture Notes (Interim)

## Intent of this run
Keep the CLI behavior-safe while improving module boundaries so that high-risk command
orchestration does not sit directly on raw infrastructure details.

## Layer map (practical)
- **Edge / orchestration (`src/lib.rs`, `src/main.rs`)**
  Parses arguments and routes into skill commands.
- **Application flow (`src/add.rs`, `src/remove.rs`, `src/check.rs`, `src/refresh.rs`, `src/update.rs`, `src/list.rs`, `src/harvest.rs`, `src/destroy.rs`)
  Owns command-specific validation, sequencing, and side effects.
- **Domain helpers (`src/skills.rs`, `src/catalog.rs`, `src/provenance.rs`, `src/home.rs`)
  Encapsulate project/library policy and path/domain contracts.
- **Infrastructure adapters (`src/git.rs`, `src/sources.rs`, `src/paths.rs`)
  Perform external process and filesystem operations, exposing narrow result contracts.

## Notable boundary clarifications in this phase
- **Catalog aggregate (Phase 8)**: `src/catalog.rs` now models by-project catalog data as
  `CatalogMeta` (`name`, `root`, `skills`) and centralizes ownership checks + mutation for install/withdraw/forget.

- **Path layout ownership**: path rooting for project skill directories was moved to
  `src/home.rs` (`project_agents_path`, `project_skills_path`) so callers do not build
  `./.agents` fragments directly.
- **Receipt naming ownership**: sidecar filename is now defined once as
  `provenance::SIDECAR_FILE` and referenced by callers that interact with source
  receipts.
- **Git boundary extraction (Phase 5)**: `src/git.rs` now centralizes command invocation
  and error shaping in internal helpers:
  - `run_git` for command launch + process I/O preamble
  - `git_detail` for stderr-last-line extraction

  This keeps `remote_head`, `checkout`, and `checkout_revision` focused on intent while
  preserving their existing error contracts and command semantics.

## Decision principle used
Each module boundary change is additive and reversible:
- extract one helper,
- keep control flow and outputs unchanged,
- verify with `cargo test -- --nocapture`,
- then mark the phase complete.
