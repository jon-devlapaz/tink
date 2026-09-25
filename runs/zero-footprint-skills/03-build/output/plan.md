# Plan: Zero-Footprint Dynamic Skill Routing (Tink CLI)

**Derived from:** `02-design/output/spec.md`  
**Status:** draft  
**Implementer:** System Architect & Lead Engineer  
**Execution Framework:** [pi-pstack](https://github.com/McCune1224/pi-pstack) (Poteto Mode & TDD Discipline)

---

## 1. Poteto / pi-pstack Implementation Disciplines

We execute the build under the following core `pi-pstack` principles:
1. **`tdd` & `principle-test-behavior-not-implementation`:** Write the failing end-to-end acceptance tests before writing a single line of production code.
2. **`principle-sequence-verifiable-units`:** Decompose the build into isolated, independently verifiable units (`init` scaffold $\to$ `mount` atomicity $\to$ `unmount` cleanup $\to$ zero-footprint `check`).
3. **`principle-boundary-discipline`:** Strict separation of responsibilities: `tink-route` evaluates semantic intent read-only; `tink mount` executes filesystem mutations atomically.
4. **`no-comments` & `deslop`:** Zero conversational narration comments in the diff; clean, idiomatic Rust code first.

---

## 2. System Failure Modes (Written Before Code)
Per project testing rules, all system failure modes are cataloged prior to implementation:

| ID | Failure Mode | Cause | Expected Behavior / Guard |
|---|---|---|---|
| **F1** | Skill Missing from Library | `tink mount <name>` where `<name>` is not in `~/.tink-library/skills/` | Exit `1` with explicit error: `"Skill '{name}' not found in library"`. No filesystem mutations. |
| **F2** | Directory Traversal Attack | `tink mount ../escaped` or symlink pointing outside library | Canonicalize source path; if `!canonical.starts_with(library_root)`, abort with exit `2`: `"Security error: path traversal outside library"`. |
| **F3** | Stale / Broken Mount Point | Previous mount target in `.tink/.active/<name>` has broken destination | Clean stale link atomically during mount or unmount; do not panic or leave corrupted state. |
| **F4** | Concurrent Mount Collision | Two processes mount the same skill simultaneously | Stage to `.tink/.active/.tmp-<skill>-<uuid>` and atomic `rename(2)` to target. |
| **F5** | Unmounting Missing / Non-Mounted Target | `tink unmount <name>` when not mounted | Exit `0` with informative message: `"Skill '{name}' is not mounted"`. |
| **F6** | Windows Directory Copy vs. Symlink | Platform lacks symlink permission and falls back to directory copy | Unmount detects if target is symlink, junction, or directory; uses `fs::remove_file` or `fs::remove_dir_all` safely. |
| **F7** | Missing Project Manifest in Zero-Footprint | Running `tink check` in zero-footprint project without `.agents/skills/` | Treat absent `.agents/skills/` as a clean zero-footprint project; do not panic with `"Missing .agents/skills"`. |

---

## 2. Files That Change

### Core Rust CLI (`tink`)
- **Create:**
  - `src/mount.rs`: Atomic mounting (`tink mount <name>`), unmounting (`tink unmount <name>`), traversal validation, and platform-specific junction/symlink handling.
- **Edit:**
  - `src/lib.rs`: Expose `Mount` and `Unmount` subcommands; add `--zero-footprint` flag to `tink init`.
  - `src/init.rs`: Support `--zero-footprint` mode: scaffolds `.tink/`, `.tink/.gitignore`, `.tink/skills.toml`, and `AGENTS.md` without creating `.agents/skills/`.
  - `src/home.rs`: Add `.tink/.active/` path helper; relax hard requirement on `.agents/skills/` presence.
  - `src/check.rs`: Gracefully report 0 skills in zero-footprint mode instead of failing.
  - `src/manifest.rs`: Support `.tink/skills.toml` and `.tink/skills.lock` generation and validation for zero-footprint projects.

### E2E Tests (Written First)
- **Edit:**
  - `tests/acceptance.rs`: Add end-to-end tests covering all failure modes F1–F7:
    - `zero_footprint_init_leaves_git_clean`
    - `atomic_mount_creates_ephemeral_active_link`
    - `atomic_unmount_restores_clean_state`
    - `mount_missing_skill_fails_cleanly` (F1)
    - `mount_path_traversal_aborts` (F2)
    - `mount_handles_stale_symlink` (F3)

---

## 3. Order of Work

### Phase 1: Author E2E Acceptance Tests (Failing First)
1. In `tests/acceptance.rs`, write test functions covering:
   - Zero-footprint initialization and git purity.
   - Atomic mounting and unmounting.
   - Failure modes F1 (missing), F2 (traversal), F3 (stale link), and F5 (unmount missing).
2. Run `cargo test --test acceptance zero_footprint` and confirm expected compilation / test failures.

### Phase 2: Implementation of Core Mount & Init Modules
1. Implement `src/mount.rs`:
   - Enforce canonical library boundary check (F2).
   - Atomic temporary staging via `.tmp-<uuid>` and `fs::rename` (F4).
   - Safe removal distinguishing symlink, junction, and directory (F6).
2. Wire subcommands in `src/lib.rs` and `src/main.rs`.
3. Update `src/init.rs` to support `--zero-footprint` (scaffolds `.tink/`, `.tink/.gitignore`, `skills.toml`, and `AGENTS.md`).
4. Update `src/check.rs` and `src/home.rs` to support zero-footprint projects cleanly (F7).

### Phase 3: E2E Test Verification
1. Run acceptance suite: `cargo test --test acceptance zero_footprint`.
2. Verify all failure mode test cases pass.
3. Verify existing acceptance tests remain green (zero regression).

### Phase 4: Stage 4 Verification & Repeatable Artifact
1. Run `_system/scripts/verify.sh zero-footprint-skills`.
2. Inspect generated receipt: `runs/zero-footprint-skills/04-test/output/verification.json`.

---

## 4. Blast Radius & Regressions

- **Blast Radius:** Low and strictly additive.
  - `tink init` defaults to standard behavior; `--zero-footprint` activates the new paradigm.
  - `tink mount` and `tink unmount` are new subcommands that do not mutate existing project trees.
  - Existing `.agents/skills` workflows and tests continue functioning without modification.
