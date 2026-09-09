# Verification spike: issue #29 durable atomic skill-tree swap

Spike date: 2026-09-09. Base: `main` @ `4552a04` (includes #68 / post-#26 close). Consolidation PR base: `main` @ `dcd3a05` (includes #72 durable orphan naming for manifest/binary).

Related: [#29](https://github.com/jon-devlapaz/tink/issues/29), [#26](https://github.com/jon-devlapaz/tink/issues/26) (closed), [#68](https://github.com/jon-devlapaz/tink/pull/68), [#72](https://github.com/jon-devlapaz/tink/pull/72).

## Verdict

**Done for #29 consolidation scope.** Safety invariant and shared restore-or-orphan policy are landed; first-install-only staging remains intentionally out of scope.

| #29 design pillar | Status on `main` |
|---|---|
| Stage beside destination | **Mitigated** — all tree replace paths use `tempdir_in` / `tempfile_in` beside the destination root |
| Move live → backup before publish | **Mitigated** for replace paths via `publish_staged_tree` (`target → staging/old`) |
| Publish staged → live | **Mitigated** — rename publish in `publish_staged_tree`, `install_local`, create-only paths |
| On failure: restore; if restore fails, **keep** backup and name path | **Mitigated** for `publish_staged_tree`, `manifest::write_atomic`, `update::replace_binary`, and `library::deposit_at` divergent repair; **Missing** for first-install-only paths |
| Shared helper across call sites | **Done** — `paths::restore_or_orphan` / `paths::orphan_or_retain_after_restore_failure` own the policy; tree, manifest, and binary call sites format errors at the edges |
| Durable orphan names (`.tink-orphan-*`) | **Done** for replace paths — tree, manifest, and binary double failures rename recovery backups to `.tink-orphan-*` beside the destination root |

**Recommendation:** Close #29 when the consolidation PR merges. Track first-install-only staging as a separate follow-up if desired; it is a different shape (no live tree to restore) and not #26-class.

---

## Refreshed acceptance checklist (#29)

| Item | Status | Evidence |
|---|---|---|
| Replace paths share restore-or-orphan policy | **Done** | `paths::restore_or_orphan` used by `rollback_or_retain_backup` and `write_atomic`; `orphan_or_retain_after_restore_failure` used by `replace_binary` after consuming restore |
| Failure after live-move leaves recoverable path + error names it | **Done** for replace/publish paths | Characterization tests below |
| Characterization for restore-failure path | **Done** | Tree, manifest, binary, and library deposit paths |
| Closes or fully addresses #26 | **Done** | #26 closed; silent `TempDir` drop on refresh replace is fixed |

---

## Call-site inventory

Legend: **Keep** = explicit `TempDir::keep()` / `NamedTempFile::keep()` / `TempPath::keep()` on double failure. **Drop-risk** = staging/backup lives only in a `Drop`-deleting temp with no keep-on-failure path.

### Shared policy (`src/paths.rs`)

| Function | Role |
|---|---|
| `orphan_recovery_path` / `move_file_to_orphan` | Durable `.tink-orphan-*` naming beside destination root |
| `restore_or_orphan` | Try restore; on failure orphan backup or retain via caller fallback |
| `orphan_or_retain_after_restore_failure` | Orphan-or-retain when restore already failed (consuming restore APIs) |

### Tree swap core (`src/skills.rs`)

| Lines | Function | Stage | Live→backup | Publish | Rollback | Keep on double failure |
|---|---|---|---|---|---|---|
| 736–745 | `install_local` (Ready) | `.tink-stage-*` beside dest | — (no prior live tree) | `rename(staged → target)` | — | **Drop-risk** for staged new tree only; live tree untouched |
| 762–803 | `rollback_or_retain_backup` | — | — | — | via `paths::restore_or_orphan` | **Orphan** or **Keep** staging dir |
| 805–820 | `publish_staged_tree` | caller-owned staging | `rename(target → old)` | `rename(staged → target)` | via `rollback_or_retain_backup` | **Orphan** or **Keep** |
| 832+ | `replace_verified_inner` | `.tink-update-*` + copy to `new/` | via `publish_staged_tree` | via `publish_staged_tree` | via `publish_staged_tree` | **Orphan** or **Keep** |

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
| 296–376 | `write_atomic` | `.skills-toml-*`, `.skills-lock-*` temps | `.skills-manifest-backup-*` temp file of prior manifest | rename manifest then lock | via `paths::restore_or_orphan` | **Orphan** or **Keep** fallback |
| 256 | `lock` (caller) | — | — | calls `write_atomic` | — | — |

### Binary update (`src/update.rs`)

| Lines | Function | Stage | Backup | Publish | Rollback | Keep |
|---|---|---|---|---|---|---|
| 375–461 | `replace_binary` | `.tink-update-*` temp file | `.tink-backup-*` copy of current | `TempPath::persist` | `backup.persist(current)` on probe failure | via `paths::orphan_or_retain_after_restore_failure` |
| 492 | `verify_and_replace` | — | — | calls `replace_binary` | — | — |

---

## Gap matrix vs #29 design target

Design target: **stage beside dest → durable backup name → publish → restore-or-keep**.

| Site | Stage | Durable backup | Publish | Restore-or-keep | Overall |
|---|---|---|---|---|---|
| `publish_staged_tree` (+ callers) | Mitigated | Mitigated (`.tink-orphan-*` on double failure) | Mitigated | Mitigated (shared helper) | **Done** |
| `install_local` (Ready) | Mitigated | Missing (N/A — no live tree) | Mitigated | N/A | **Partial** (different shape; not #26-class) |
| `library::deposit_at` (Divergent) | Mitigated | Mitigated (via `publish_staged_tree`) | Mitigated | Mitigated | **Done** |
| `library::promote` create / skillset create paths | Mitigated | Missing | Mitigated | N/A | **Partial** |
| `manifest::write_atomic` | Mitigated | Mitigated (`.tink-orphan-*` on double failure) | Mitigated | Mitigated (shared helper) | **Done** |
| `update::replace_binary` | Mitigated | Mitigated (`.tink-orphan-*` on double failure) | Mitigated | Mitigated (shared helper) | **Done** |

---

## Executable proof (tests run on consolidation branch)

```bash
cargo test restore_or_orphan -- --nocapture
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
| `paths::tests::restore_or_orphan_*` | Shared helper: successful rollback and double-failure orphan |
| `skills::tests::rollback_failure_retains_recovery_backup_at_durable_orphan_path` | Tree double failure via shared policy |
| `skills::tests::publish_staged_tree_restores_target_when_publish_fails` (#68) | Single failure: rollback restores live bytes; no orphan |
| `manifest::tests::write_atomic_restores_manifest_when_lock_publish_fails` | Manifest pair: lock publish failure rolls back manifest |
| `manifest::tests::write_atomic_retains_orphan_on_double_failure` | Manifest double failure via shared policy |
| `update::tests::replace_binary_retains_recovery_backup_when_rollback_fails` | Binary double failure via shared policy |
| `update::tests::replace_binary_rolls_back_when_published_probe_fails` | Binary single failure: successful rollback |

### Not proven (honest gaps)

- End-to-end double failure injected through full `replace_verified` rename window without calling rollback helpers directly (would need concurrent target recreation or test hooks).
- End-to-end double failure injected through full `deposit_at` rename window without calling rollback helpers directly (library tests characterize restore and orphan beside the library root).
- First-install-only staging drop-risk (`install_local`, create-only promote/skillset paths).
- Windows, concurrent locks, cross-filesystem rename (explicitly out of v1).

---

## Residual risks

1. **First-install staging** — failed rename drops staged new tree only; existing live content unaffected.
2. **Concurrent target recreation** — documented; recovery depends on winning the race after rollback failure.

---

## Consolidation landed

Shared restore-or-orphan policy lives in `src/paths.rs`. Staging, publish, and backup capture remain with each owner (`publish_staged_tree`, `write_atomic`, `replace_binary`). File vs tree shapes differ only at the edges (restore closure and error formatting).
