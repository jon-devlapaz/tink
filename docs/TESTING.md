# Testing

Tests are Tink's behavior sensors. [`ACCEPTANCE.md`](../ACCEPTANCE.md) records the
intended CLI and on-disk boundary; workflow files define delivery automation. Unit and
acceptance tests provide executable evidence. A repository-local traceability test
keeps row and sensor identifiers unique and exposes explicit manual or partial gaps.
A passing check proves only its assertions.

## Required local gate

Run from the repository root:

```console
cargo fmt --check
cargo test --locked
git diff --check
```

`cargo test --locked` runs module unit tests, `tests/acceptance.rs`, and the acceptance
traceability guard. Use a focused acceptance filter while iterating, then run the
complete gate before closure:

```console
cargo test --test acceptance h13_exact_cache_match_does_not_publish_skillset_as_standalone -- --nocapture
```

`./tink-test <args>` builds and executes this checkout with an isolated Tink home. It
is a dogfood runner, not an automated test suite by itself.

## Automated release gate

`.github/workflows/bump-release.yml` handles non-release pushes to `main`. Before it
changes `Cargo.toml` or `Cargo.lock`, commits, tags, pushes, or dispatches a release, it
installs stable Rust with `rustfmt` and runs:

```console
cargo fmt --check
cargo test --locked
```

That gate protects the automatic `main`-push release path. It does **not** protect the
two direct entrypoints accepted by `.github/workflows/release.yml`:

- a direct `v*` tag push;
- a manual `workflow_dispatch`.

Those two paths perform locked release builds but do not run formatting or behavior
tests. Separately, the entire bump job skips a `main` push whose head message starts
with `chore: release v`; such a push is neither verified nor released by these
workflows. External branch/tag protection is outside this repository and must not be
assumed.

## Test topology

| Boundary | Primary executable sensors |
|---|---|
| Bootstrap and layout | `I*` acceptance tests; `home.rs` unit tests |
| Local and remote standalone add | `A*`, `R*`; `sources.rs` and `skills.rs` unit tests |
| Skillsets | `K*`; `skillsets.rs` validation and receipt-classification unit tests |
| GitHub inspection | `G*` |
| Project validation and listing | `C*`, `L*` |
| Library, promotion, cache, and harvest | `H*`; `library.rs` unit tests |
| Standalone refresh and removal | `P*`, `X*` |
| Manifest lock/sync/verify | `M*`; `manifest.rs` unit tests |
| CLI surface and lifecycle safety | `V*`, `D*`, `U*`, `S*`; `update.rs`, `style.rs`, and `git.rs` unit tests |

The most important ownership sensors are:

- `skillsets::tests::receipt_entry_presence_includes_dangling_symlinks` pins
  classification before validation.
- H11 excludes a receipt-classified library root from standalone listing and directs a
  bare-name add to `skillset add`.
- H12 refuses a divergent standalone collision before mutation or project publication
  and preserves all seeded library bytes.
- H13 prevents an exact-cache hit from publishing a skillset root as a standalone
  project skill.
- A6 and A8 preserve the positive standalone library repair and cache paths; ownership
  protection must not disable legitimate reuse.
- K1, K5, K6, K8, K10, and K11 cover pinned install, collision refusal, receipt safety,
  offline re-add, staged refresh, and member-name validation.
- P1-P7 cover clean-project proof and project/library refresh direction.

Use [`ACCEPTANCE.md`](../ACCEPTANCE.md) for detailed input/output contracts rather
than copying every row here.

## Change workflow

1. State the outcome, invariant, and acceptance boundary.
2. For a behavior change or bug fix, add a failing acceptance test first. For a pure
   refactoring, begin and end on a green suite.
3. Change the smallest complete owner; do not distribute a domain decision across
   callers.
4. Run the focused sensor, then the complete local gate.
5. When ownership or proof changes, update this map and
   [`ARCHITECTURE.md`](ARCHITECTURE.md). Record experiment history in
   [`DEEP-REFACTOR-LOG.md`](DEEP-REFACTOR-LOG.md).

## Traceability and known sensor gaps

`tests/acceptance_traceability.rs` rejects duplicate row IDs, duplicate executable
sensor IDs, missing named sensors, and executable sensors without a row. Most rows map
to the same-named `tests/acceptance.rs` function. An explicit `Sensor: <ID>` marker
records deliberate bundled coverage; `Sensor: manual` records an unresolved proof gap.

- C4 still needs no-network/no-write instrumentation.
- S1 is automated only for `init` not creating a Git repository; its command-wide
  no-stage/commit/push claim remains partial.
- S2 (home is never an agent discovery root) remains manual.
- No automated test proves concurrent Tink mutations safe; production code has no
  inter-process library lock.
- Direct-tag and manual release entrypoints bypass behavior verification, while a
  `chore: release v` head-message prefix skips both verification and automatic release,
  as described above.
