# Home Isolation Specification

## Status

Implementation-ready. Replaces the previous draft, which only named
`add` / `refresh` / `manage_tink` and would have left ambient
`resolve_home()` on the same workflows.

## Outcome

Every internal helper that reads or writes `$TINK_HOME` accepts
`home: Option<&Path>`. `None` means `home::resolve_home()` /
`ensure_inventory_root(None)` at the existing resolution layer in
`src/home.rs`. `Some(path)` is a hermetic inventory root for in-process
tests.

CLI dispatch in `src/lib.rs` keeps passing `None`. User-facing commands,
exit codes, and messages do not change.

## Authority and constraints

- Keep the change small and complete; do not add a
  `Home` context type, thread-local, or extra env var.
- Reuse the existing house pattern: `_at(home: Option<&Path>, …)` for
  helpers that touch inventory. Keep a thin `foo()` → `foo_at(None, …)`
  wrapper **only** when CLI dispatch (or another remaining caller) still
  uses the old name. Do not keep unused aliases.
- Private helpers take `home: Option<&Path>` on the existing function.
  Do not invent a second name for a private function.
- Put `home` first, matching `catalog::deposit_skill_at` and
  `library::deposit_at`. (`destroy_project_at` puts it last; do not
  copy that.)
- After an `_at` form exists, delete `Some => foo_at(Some(home)) /
  None => foo()` branches (see `manifest::sync_at`). Call `foo_at(home)`.
- Do not use `env::set_var("TINK_HOME", …)` in unit tests. Process env
  races under `cargo test`. Pass `Some(&temp_home)`.
- Do not change `home::resolve_home`, `ensure_inventory_root`, or
  `existing_inventory_root`. They already take `Option<&Path>` (or *are*
  the `None` resolution).

## Why this is the complete change

CLI acceptance tests already isolate via `TINK_HOME` on child processes.
The gap is in-process: a helper that calls `library_root(None)` or
`resolve_home()` will create or mutate the developer’s `~/.tink` when
tests run in the same process.

Threading `home` through `place_from_library` alone is not enough.
That function calls `catalog::deposit_skill` (ambient) and
`init::ensure_project_layout` (ambient `ensure_inventory_root(None)`).
`refresh_skill` calls `library::preflight_refresh` / `sync_from_installed`
/ `deposit_refresh`, which still use `library_root(None)`. Those callees
are in scope.

## Remaining ambient lookups

`None` at `home.rs` is correct. Everything below is not.

### `src/library.rs`

| Function | Current leak | Target |
| :--- | :--- | :--- |
| `matching` | `library_root(None)` | `matching_at(home, skill, provenance)`; wrapper `matching` → `None` |
| `load` | `library_root(None)` | `load_at(home, name)`; wrapper `load` → `None` |
| `for_remote_tip` | `ensure_inventory_root(None)` | `for_remote_tip_at(home, url, revision, selected_name)`; wrapper keeps old name |
| `preflight_refresh` | `library_root(None)` | `preflight_refresh_at(home, …)`; wrapper keeps old name |
| `sync_from_installed` | `deposit(…)` | `sync_from_installed_at(home, installed)` → `deposit_at` |
| `deposit_refresh` | `deposit(…)` | `deposit_refresh_at(home, …)` → `deposit_at` |
| `deposit_create_only_at` | `resolve_home()` on the standalone-source skip path | Use the `home` argument already in scope (`Some` → that path, `None` → `resolve_home()`) |

`deposit_at`, `preflight_deposit_at`, `promote_at`, `list_names`, and
`library_root(home)` already take `home`. Do not re-wrap them.

### `src/init.rs`

| Function | Current leak | Target |
| :--- | :--- | :--- |
| `ensure_project_layout` | `ensure_inventory_root(None)` | `ensure_project_layout_at(home, project_root)`; wrapper keeps old name |
| `init_project` | `ensure_inventory_root(None)` then `install_manage_tink` / `add_skill_quiet` | `init_project_at(home, project_root, options)`; pass `home` into those callees |

`ensure_project_layout` is on the add / skillset-add path. Leaving it
ambient would create `~/.tink` even after add is parameterized.

### `src/add.rs`

| Function | Current leak | Target |
| :--- | :--- | :--- |
| `place_from_library` | `catalog::deposit_skill`; `ensure_project_layout` | Add `home`. Call `deposit_skill_at` and `ensure_project_layout_at`. |
| `place_skill` | Hardcodes `None` into `place_skill_inner` | Pass `home` through. |
| `install_from_checkout` | `library::matching`; `place_from_library` / `place_skill` | Add `home`. Call `matching_at`. |
| `add_from_remote` | `library::for_remote_tip`; `place_from_library`; `install_from_checkout` | Add `home`. Call `for_remote_tip_at`. |
| `add_skill_inner` | `library::load`; `place_from_library`; `install_from_checkout`; `add_from_remote` | Add `home`. Call `load_at`. |
| `add_skill` / `add_skill_quiet` | No home | `_at` variants plus wrappers that pass `None`. |

`PreparedLockedSkill::publish_at` already takes `home`. Leave it.

### `src/refresh.rs`

| Function | Current leak | Target |
| :--- | :--- | :--- |
| `prepare_refresh` | `library::preflight_refresh` | Add `home`. Call `preflight_refresh_at`. |
| `apply_refresh` | `sync_from_installed` / `deposit_refresh` | Add `home`. Call the `_at` forms. |
| `refresh_skill` / `refresh_all` | `catalog::deposit_skill` plus the above | `refresh_skill_at` / `refresh_all_at`; wrappers pass `None`. |

### `src/manage_tink.rs`

| Function | Current leak | Target |
| :--- | :--- | :--- |
| `refuse_remote_library_collision` | `existing_inventory_root(None)` | Add `home`. Pass it through. |
| `refresh_manage_tink` | `preflight_deposit` / `deposit` / `deposit_skill` / `sync_from_installed` | `refresh_manage_tink_at(home, project_root)`; call `_at` forms. |
| `install_manage_tink` | `add_skill_quiet` | `install_manage_tink_at(home, project_root)`. |

### `src/skillsets.rs`

| Function | Current leak | Target |
| :--- | :--- | :--- |
| `read_catalog` | `home::resolve_home()` | Add `home`. `None` → `resolve_home()`. |
| `library_root` | `ensure_inventory_root(None)` | `library_root(home: Option<&Path>)`. |
| `preflight_library_target` / `sync_library_from_project` | via `library_root()` | Thread `home`. |
| `add_skillset` / `refresh_skillset` | `read_catalog`, `ensure_project_layout`, library sync | `_at` variants plus wrappers. |

`list_library` already takes `Option<&Path>`. `remove_skillset` does not
touch home; leave it.

### `src/harvest.rs`

| Function | Current leak | Target |
| :--- | :--- | :--- |
| `harvest` | `ensure_inventory_root(None)`; `deposit_create_only` | `harvest_at(home, cwd)`; wrapper `harvest` → `None`. |

### `src/manifest.rs`

`sync_at` already takes `home` but branches to the ambient wrappers.
After the `_at` forms exist, call them unconditionally with `home`.

### Out of scope

- `src/home.rs` resolvers.
- `src/lib.rs` dispatch (keeps `None`).
- `src/catalog.rs` (`deposit_skill_at` already exists).
- `src/destroy.rs` (already parameterized).
- Changing CLI flags or `TINK_HOME` env behavior.
- Asserting via filesystem that `~/.tink` was never touched (not
  reliably testable in-process without stubbing `HOME`).

## Verification

1. `cargo test` (full suite). Existing acceptance tests that set
   `TINK_HOME` on child processes must still pass unchanged.
2. New **in-process** unit tests that pass `Some(&home)` and prove the
   temp inventory is the one written. Reuse the `TempHome` helper in
   `src/library.rs` tests (copy the struct if needed; do not share via a
   new crate module). Minimum:
   - `library::load_at(Some(&home), name)` reads a skill deposited into
     that home, and `load_at(Some(&other), name)` does not see it.
   - `add_skill_at(Some(&home), project, library_name, None)` with a
     preloaded library skill installs into the project and writes catalog
     under `home`, not under `resolve_home()`.
   - `refresh_manage_tink_at(Some(&home), project)` after `init_project_at`
     (or an equivalent layout) deposits `manage-tink` under `home`.
3. CLI entrypoints still compile as `foo(…)` / `foo_at(None, …)` with no
   new arguments at `src/lib.rs`.
4. No `env::set_var` for `TINK_HOME` or `HOME` in the new tests.

## Non-goals

- Hermetic CLI tests (already done via `Workspace` + `TINK_HOME`).
- Concurrent safety of `resolve_home()` itself.
- Parameterizing path display or project `.agents/skills/` (those are
  project-rooted, not home-rooted).
