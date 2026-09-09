# Verification spike: issue #29 durable atomic skill-tree swap

Spike date: 2026-09-09. Base: `main` @ `4552a04` (includes #68 / post-#26 close).

Related: [#29](https://github.com/jon-devlapaz/tink/issues/29), [#26](https://github.com/jon-devlapaz/tink/issues/26) (closed), [#68](https://github.com/jon-devlapaz/tink/pull/68).

## Verdict

**Mostly done for the safety invariant; still needed for consolidation and durable naming.**

| #29 design pillar | Status on `main` |
|---|---|
| Stage beside destination | **Mitigated** — all tree replace paths use `tempdir_in` / `tempfile_in` beside the destination root |
| Move live → backup before publish | **Mitigated** for replace paths via `publish_staged_tree` (`target → staging/old`) |
| Publish staged → live | **Mitigated** — rename publish in `publish_staged_tree`, `install_local`, create-only paths |
| On failure: restore; if restore fails, **keep** backup and name path | **Mitigated** for `publish_staged_tree`, `manifest::write_atomic`, `update::replace_binary`, and `library::deposit_at` divergent repair; **Missing** for first-install-only paths |
| Shared helper across call sites | **Missing** — three independent implementations remain |
| Durable orphan names (`.tink-orphan-*`) | **Done** for replace paths — tree, manifest, and binary double failures rename recovery backups to `.tink-orphan-*` beside the destination root |

**Recommendation:** Keep #29 open but **narrow the next implement PR** to `publish_staged_tree` only: introduce a small internal helper + durable orphan rename on double failure. Defer manifest/binary unification and `deposit_at` divergent repair to follow-up issues. Do **not** close #29 until consolidation scope is explicitly split or the first slice lands.

---

## Refreshed acceptance checklist (#29)

| Item | Status | Evidence |
|---|---|---|
| `replace_verified` / new install use a shared helper | **Open** | Replace uses `publish_staged_tree`; fresh install uses direct rename in `install_local` — no shared abstraction |
| Failure after live-move leaves recoverable path + error names it | **Done** for replace/publish paths | `rollback_or_retain_backup` + `TempDir::keep()` (#53/#68); tests below |
| Characterization for restore-failure path | **Done** for tree helper | `rollback_failure_retains_recovery_backup`, `publish_staged_tree_restores_target_when_publish_fails`; manifest/binary keep paths characterized in this spike |
| Closes or fully addresses #26 | **Done** | #26 closed; silent `TempDir` drop on refresh replace is fixed |

---

## Call-site inventory

Legend: **Keep** = explicit `TempDir::keep()` / `NamedTempFile::keep()` / `TempPath::keep()` on double failure. **Drop-risk** = staging/backup lives only in a `Drop`-deleting temp with no keep-on-failure path.

### Tree swap core (`src/skills.rs`)

| Lines | Function | Stage | Live→backup | Publish | Rollback | Keep on double failure |
|---|---|---|---|---|---|---|
| 736–745 | `install_local` (Ready) | `.tink-stage-*` beside dest | — (no prior live tree) | `rename(staged → target)` | — | **Drop-risk** for staged new tree only; live tree untouched |
| 752–773 | `rollback_or_retain_backup` | — | — | — | `rename(backup → target)` | **Keep** staging dir; error names `recovery backup` |
| 778–788 | `publish_staged_tree` | caller-owned staging | `rename(target → old)` | `rename(staged → target)` | via `rollback_or_retain_backup` | **Keep** |
| 805–820 | `replace_verified_inner` | `.tink-update-*` + copy to `new/` | via `publish_staged_tree` | via `publish_staged_tree` | via `publish_staged_tree` | **Keep** |

### Callers of `publish_staged_tree`

| File:line | Caller | Staging prefix | Notes |
|---|---|---|---|
| `src/refresh.rs:177` | `skills::replace_verified` (via `apply_refresh`) | `.tink-update-*` | Skill refresh; pre-image snapshotted separately for `skill rollback` |
| `src/rollback.rs:169` | `skills::replace_verified` | `.tink-update-*` | Restores refresh snapshot |
| `src/manage_tink.rs:146` | `skills::replace_embedded_verified` | `.tink-update-*` | Embedded manage-tink replace |
| `src/library.rs:163` | `library::promote_at` (replace branch) | `.tink-promote-*` | Project → library promotion |
| `src/skillsets.rs:527` | `replace_from_checkout` | `.tink-skillset-stage-*` | Skillset refresh replace |
| `src/skillsets.rs:1252` | `copy_project_tree` (replace branch) | `.tink-skillset-library-*` | Project → library skillset mirror |

### Other tree paths (no `publish_staged_tree`)

| File:line | Function | Pattern | Keep / Drop-risk |
|---|---|---|---|
| `src/library.rs:166` | `promote_at` (create) | single `rename(staged → target)` | **Drop-risk** (staging only; no prior live tree) |
| `src/library.rs` | `deposit_at` (Divergent) via `repair_divergent_deposit` | `.tink-deposit-*` + `publish_staged_tree` | **Mitigated** — same restore-or-orphan semantics as other replace paths |
| `src/skillsets.rs:448` | `install_from_checkout` | single rename; `_staging` dropped on success | **Drop-risk** for staging on failure before rename |
| `src/skillsets.rs:1248` | `copy_project_tree` (create) | single rename | **Drop-risk** (staging only) |
| `src/inventory.rs:38,57` | `publish` / `publish_from_library` | `install_local` | see `install_local` |

### Manifest pair (`src/manifest.rs`)

| Lines | Function | Stage | Backup | Publish | Rollback | Keep |
|---|---|---|---|---|---|---|
| 296–368 | `write_atomic` | `.skills-toml-*`, `.skills-lock-*` temps | `.skills-manifest-backup-*` temp file of prior manifest | rename manifest then lock | restore manifest from backup or remove new manifest | **Orphan** backup at `.tink-orphan-skills.toml-*` on manifest rollback failure (`keep()` fallback); error names `recovery backup` |
| 256 | `lock` (caller) | — | — | calls `write_atomic` | — | — |

### Binary update (`src/update.rs`)

| Lines | Function | Stage | Backup | Publish | Rollback | Keep |
|---|---|---|---|---|---|---|
| 375–454 | `replace_binary` | `.tink-update-*` temp file | `.tink-backup-*` copy of current | `TempPath::persist` | `backup.persist(current)` on probe failure | **Orphan** backup at `.tink-orphan-<binary-name>-*` on restore failure (`keep()` fallback); error names `recovery backup` |
| 492 | `verify_and_replace` | — | — | calls `replace_binary` | — | — |

---

## Gap matrix vs #29 design target

Design target: **stage beside dest → durable backup name → publish → restore-or-keep**.

| Site | Stage | Durable backup | Publish | Restore-or-keep | Overall |
|---|---|---|---|---|---|
| `publish_staged_tree` (+ callers) | Mitigated | Mitigated (`.tink-orphan-*` on double failure) | Mitigated | Mitigated (orphan rename + named error) | **Partial** (shared helper deferred) |
| `install_local` (Ready) | Mitigated | Missing (N/A — no live tree) | Mitigated | N/A | **Partial** (different shape; not #26-class) |
| `library::deposit_at` (Divergent) | Mitigated | Mitigated (via `publish_staged_tree`) | Mitigated | Mitigated | **Partial** (shared helper deferred) |
| `library::promote` create / skillset create paths | Mitigated | Missing | Mitigated | N/A | **Partial** |
| `manifest::write_atomic` | Mitigated | Mitigated (`.tink-orphan-*` on double failure) | Mitigated | Mitigated (orphan rename + named error) | **Partial** (shared helper deferred) |
| `update::replace_binary` | Mitigated | Mitigated (`.tink-orphan-*` on double failure) | Mitigated | Mitigated (orphan rename + named error) | **Partial** (shared helper deferred) |

---

## Executable proof (tests run on spike branch)

```bash
cargo test rollback_failure_retains_recovery_backup_at_durable_orphan_path -- --nocapture
cargo test publish_staged_tree_restores_target_when_publish_fails -- --nocapture
cargo test deposit_diverge_repairs -- --nocapture
cargo test deposit_divergent_repair_restores_original_on_publish_failure -- --nocapture
cargo test deposit_divergent_repair_retains_orphan_on_double_failure -- --nocapture
cargo test write_atomic_restores_manifest_when_lock_publish_fails -- --nocapture
cargo test write_atomic_retains_orphan_on_double_failure -- --nocapture
cargo test replace_binary_retains_recovery_backup_when_rollback_fails -- --nocapture
cargo test replace_binary_rolls_back_when_published_probe_fails -- --nocapture
cargo test --workspace --all-targets --locked
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --locked -- -D warnings
```

| Test | Proves |
|---|---|
| `skills::tests::rollback_failure_retains_recovery_backup_at_durable_orphan_path` | Tree double failure: orphan rename + recovery path in error |
| `skills::tests::publish_staged_tree_restores_target_when_publish_fails` (#68) | Single failure: rollback restores live bytes; no orphan |
| `manifest::tests::write_atomic_restores_manifest_when_lock_publish_fails` (this spike) | Manifest pair: lock publish failure rolls back manifest |
| `manifest::tests::write_atomic_retains_orphan_on_double_failure` | Manifest double failure: orphan rename + recovery path |
| `update::tests::replace_binary_retains_recovery_backup_when_rollback_fails` | Binary double failure: orphan rename + recovery path in error |
| `update::tests::replace_binary_rolls_back_when_published_probe_fails` | Binary single failure: successful rollback |

### Not proven (honest gaps)

- End-to-end double failure injected through full `replace_verified` rename window without calling `rollback_or_retain_backup` directly (would need concurrent target recreation or test hooks).
- End-to-end double failure injected through full `deposit_at` rename window without calling rollback helpers directly (library tests characterize restore and orphan beside the library root).
- Shared helper unifying tree, manifest, and binary swap implementations.
- Windows, concurrent locks, cross-filesystem rename (explicitly out of v1).

---

## Residual risks

1. **Three parallel implementations** — tree, manifest, and binary paths share orphan naming but not a single swap helper.
2. **`deposit_at` divergent repair** — mitigated via `repair_divergent_deposit` + `publish_staged_tree` (PR after spike).
3. **First-install staging** — failed rename drops staged new tree only; existing live content unaffected.
4. **Concurrent target recreation** — documented; recovery depends on winning the race after rollback failure.

---

## Recommended next implement slice (one PR)

**Scope:** `publish_staged_tree` only (~1 module + call-site prefix constants unchanged).

1. Extract a private `stage_tree_swap(staging, staged, target) -> Result<_, SwapFailure>` that implements steps 2–4 of the design target.
2. On rollback failure, **rename** `staging/old` to `<dest-root>/.tink-orphan-<skill-name>-<pid or random>` before returning (durable name beside destination, not temp prefix).
3. Wire `replace_verified_inner`, `library::promote_at`, and skillset callers through the helper without changing staging prefix behavior on the happy path.
4. Add one characterization test asserting orphan dir name pattern on double failure.

**Defer:** manifest/binary unification, `install_local` first-install staging, shared crate-level abstraction across file vs tree shapes.

**Alternative:** Close #29 as “safety done” and open `#29a` (orphan naming), `#29b` (shared helper), `#29c` (`deposit_at` divergent backup) — if maintainers prefer smaller tracked units.
