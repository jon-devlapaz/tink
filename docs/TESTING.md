# Testing

Tests are Tink's behavior sensors. [`ACCEPTANCE.md`](../ACCEPTANCE.md) records the
intended CLI and on-disk boundary; workflow files define delivery automation. Unit and
acceptance tests provide executable evidence. A repository-local traceability test
keeps row and sensor identifiers unique and exposes explicit manual or partial gaps.
A passing check proves only its assertions.

## Required local gate

Run from the repository root:

```console
cargo fmt --all -- --check
cargo check --workspace --all-targets --locked
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --all-targets --locked
cargo test --workspace --locked --doc
cargo build --workspace --release --locked
cargo audit --file Cargo.lock
sh -n install.sh
shellcheck -x install.sh
git diff --check
```

The checked-in toolchain pins Rust 1.95.0. `cargo test` runs module unit tests,
`tests/acceptance.rs`, and the acceptance traceability guard. `cargo audit` requires
the separately installed `cargo-audit` binary. Use a focused acceptance filter while
iterating, then run the complete gate before closure:

```console
cargo test --test acceptance h13_exact_cache_match_does_not_publish_skillset_as_standalone -- --nocapture
```

`./tink-test <args>` builds and executes this checkout with an isolated Tink home. It
is a dogfood runner, not an automated test suite by itself.

## Automated CI and release gates

`.github/workflows/ci.yml` runs the pinned quality gate on pull requests and `main`.
Its platform matrix runs native tests and release builds on macOS and Linux for both
x86_64 and arm64. `.github/workflows/bump-release.yml` repeats the complete quality,
audit, and release-build gate plus all four native test/build jobs before atomically
publishing the release commit and tag. A rerun can safely retry dispatch for an
existing incomplete tag. The tag `.github/workflows/release.yml` entrypoints,
including manual dispatch from the matching `v*` tag, repeat quality and audit first,
then test and build all four artifacts. Publication uploads the exact asset set to an
invisible draft and makes it public only after GitHub reports every asset as uploaded
with a SHA-256 digest matching the local archive; reruns converge an incomplete draft.
Publication is serialized across tags and refuses to replace GitHub's latest release
with an older version. Branch-based, version-mismatched, or version-regressing manual
dispatches fail before publication.

The bump workflow intentionally skips its own `chore: release v*` commit to avoid a
release loop. That commit was created only after the preceding bump job passed; direct
tags and matching-tag manual release entrypoints no longer bypass repository
verification. External branch/tag protection remains outside this repository and is
not assumed.

## Measured workload envelope

The predeclared local workload envelope is either 100 installed skills with 10
auxiliary 4 KiB files each, or one nested skill with 4,096 auxiliary 8 KiB files
(32 MiB payload). For both shapes, `init`, local `skill add`, `skill check`, `skill
lock`, and `skill verify` must succeed; every measured phase must finish in less than
60 seconds and below 512 MiB peak resident memory. This is a verified operating
envelope, not an input rejection limit or a performance promise beyond those shapes.

The 2026-08-11 bounded experiment used native `arm64-apple-darwin`, macOS 26.6.1,
Rust 1.95.0, base commit `00eb81e51001294cce5a99f1fb645796d8d10355`
plus the evaluated worktree, and release-binary SHA-256
`42b10bd83469f4a3242024d90816a4ffef71e7305dd09b845da0bb86993e564d`.
Every command ran with a fresh temporary project and `TINK_HOME`, `NO_COLOR=1`, and
`/usr/bin/time -l`. Sources lived below `source-fixtures/` inside the project so lock
paths exercised the production containment rule. The breadth add row sums 100
individually measured adds and reports their maximum RSS.

| Shape and phase | Wall seconds | Peak RSS bytes |
|---|---:|---:|
| 100 skills: init | 0.00 | 2,719,744 |
| 100 skills: add all | 0.29 | 3,276,800 |
| 100 skills: check | 0.02 | 2,899,968 |
| 100 skills: lock | 0.06 | 3,768,320 |
| 100 skills: verify | 0.05 | 3,768,320 |
| 4,096-file/32 MiB skill: init | 0.00 | 2,703,360 |
| 4,096-file/32 MiB skill: add | 1.55 | 37,437,440 |
| 4,096-file/32 MiB skill: check | 0.01 | 2,736,128 |
| 4,096-file/32 MiB skill: lock | 0.23 | 37,306,368 |
| 4,096-file/32 MiB skill: verify | 0.19 | 37,355,520 |

To reproduce the fixture exactly, create each skill with the normal frontmatter plus
the stated number of fixed-size files (`x` bytes for breadth, `y` bytes for depth;
depth fans out 64 files into each of 64 directories). Run:

```console
tink init --no-manage-tink --no-tink-skills
tink skill add SOURCE                    # repeat once per source
tink skill check
tink skill lock --source NAME=SOURCE     # repeat --source for every skill
tink skill verify
```

Wrap each command with `/usr/bin/time -l -o RESULT`; for the breadth add phase, sum
the `real` values and take the maximum `maximum resident set size`. Network/update,
concurrent writers, and larger shapes are outside this measurement.

## Test topology

| Boundary | Primary executable sensors |
|---|---|
| Bootstrap and layout | `I*` acceptance tests; `home.rs` unit tests |
| Local and remote standalone add | `A*`, `R*`; `sources.rs` and `skills.rs` unit tests, including Unix path-byte/mode preservation |
| Skillsets | `K*`; `skillsets.rs` validation and receipt-classification unit tests |
| GitHub inspection | `G*` |
| Project validation and listing | `C*`, `L*` |
| Library, promotion, cache, and harvest | `H*`; `library.rs` unit tests |
| Standalone refresh and removal | `P*`, `X*` |
| Manifest lock/sync/verify | `M*`; `manifest.rs` framing, mode, legacy-lock, exact-source, and preflight unit tests |
| CLI surface and lifecycle safety | `V*`, `D*`, `U*`, `S*`; `output.rs`, `update.rs`, `destroy.rs`, `catalog.rs`, `style.rs`, and `git.rs` unit tests |

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
- A16 and H14 prove regular and dangling skillset receipts are refused at direct add,
  manifest preparation, and harvest boundaries before standalone publication.
- K1, K5, K6, K8, K10, and K11 cover pinned install, collision refusal, receipt safety,
  offline re-add, staged refresh, version-2 receipts, and member-name validation.
- P1-P7 cover clean-project proof and project/library refresh direction.
- M7-M9 cover all-entry hash validation before mutation, later library refusal before
  publication, and the explicit version-1-to-version-2 relock path. Manifest unit tests
  also preflight project and catalog owners and prove prepared local snapshots retain
  exact bytes.
- A14 plus `skills.rs` unit tests cover portable executable-mode propagation, umask
  normalization, and distinct non-UTF-8 Unix names. Digest tests pin unambiguous
  framing and executable-mode sensitivity.
- V4-V6 pin closed stdout/stderr exit semantics for the CLI and installer. V7
  and the updater unit tests pin terminal-safe rendering for representative
  catalog and updater failure paths. U4-U20 cover invalid payloads, downgrade
  refusal, strict semantic versions,
  case-insensitive SHA-256 metadata, URL
  redaction/policy, bounded candidate probes and output capture, exact published
  version probes, terminal-safe update output, and preservation or rollback of an
  existing binary.
- G9-G10 pin Git process-group cleanup on parent-only termination and visible escaping
  of terminal controls in untrusted repository paths.
- L10-L13 and `catalog.rs` unit tests cover hashed catalog identity, bounded
  components, raw Unix paths, legacy migration ownership, same-basename projects,
  and stable three-column catalog output for hidden, delimiter-bearing, and empty
  project sets.
- D1 and D5 prove guidance and unrelated `.agents/` siblings survive destroy.
- C5-C7 characterize installed-skill name/path mismatch, missing YAML
  frontmatter, and an unclosed frontmatter block.

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
  inter-process project or library lock. Concurrent mutation is explicitly unsupported.
- No fault-injection test proves recovery from every unexpected I/O failure between
  sequential project, library, and catalog publications. Expected validation and
  ownership refusals are preflighted; retry is the operational recovery model.
