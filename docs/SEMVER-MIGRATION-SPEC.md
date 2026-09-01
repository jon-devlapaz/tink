# Standard SemVer Migration Specification

## Status

Implementation-ready specification for migrating `src/update.rs` to the
official `semver` crate.

## Purpose

`src/update.rs` currently implements a custom ~115-line parser for
Semantic Versioning 2.0.0 (`SemVersion`, `PrereleaseIdentifier`,
`valid_numeric_identifier`, and custom `Ord`). The only production use is
comparing `CARGO_PKG_VERSION` to a GitHub release `tag_name` after
stripping a leading `v`.

Replace that parser with `semver = "1.0"`.

## Requirements

Per [`ZEN.md`](../ZEN.md): delete the custom parser; keep existing
user-facing error strings and the current `update_binary` comparison
shape (equal → already latest, less → refuse downgrade, greater →
download).

## Changes

### 1. `Cargo.toml`

```toml
semver = "1.0"
```

### 2. Delete custom types

Remove `SemVersion`, `PrereleaseIdentifier`, `valid_numeric_identifier`,
`valid_identifiers`, and their `Ord` / `PartialOrd` impls from
`src/update.rs`.

### 3. Parse helper

```rust
use semver::Version;

fn parse_version(value: &str) -> Result<Version, Error> {
    let clean = value.strip_prefix('v').unwrap_or(value);
    Version::parse(clean)
        .map_err(|_| Error::msg(format!("invalid semantic version: {value}")))
}
```

Do **not** append the crate error (`({e})`). Acceptance and unit tests
look for `invalid semantic version` without a crate diagnostic suffix.
`release_version` must keep mapping parse failure to
`release metadata has invalid tag_name` (do not leak the inner parse
error there either).

`asset.version` and `CARGO_PKG_VERSION` have no `v` prefix; stripping
`v` is still correct for `tag_name`.

### 4. Comparison in `update_binary()`

Keep comparing **precedence**, not `Eq` or derived `Ord`.
`semver::Version` stores build metadata and derives `Ord` over all
fields, so `Version::cmp` is not SemVer precedence. Use
`Version::cmp_precedence`.

Keep the existing user-facing strings (`v{current_version}`,
`v{asset.version}`) rather than formatting `Version` via `Display`.

```rust
let current_version = env!("CARGO_PKG_VERSION");
match parse_version(&asset.version)?.cmp_precedence(&parse_version(current_version)?) {
    Ordering::Equal => { /* AlreadyLatest with asset.version */ }
    Ordering::Less => {
        return Err(Error::msg(format!(
            "refusing to downgrade tink from v{current_version} to v{}",
            asset.version
        )));
    }
    Ordering::Greater => {}
}
```

## Tests

Update `semantic_version_comparison_is_numeric_and_prerelease_aware` and
`semantic_version_parser_rejects_ambiguous_versions` to call
`parse_version`.

Keep:

1. `0.3.10` > `0.3.9` (numeric, not lexicographic)
2. `1.0.0` > `1.0.0-rc.1`
3. `1.0.0-rc.10` > `1.0.0-rc.2`
4. Rejection of `"1.2"`, `"01.2.3"`, `"1.2.3-01"`, `"1.2.3+"`, `"1.2.3/evil"`

Change:

5. Build metadata: assert
   `parse_version("1.0.0+build.2").unwrap().cmp_precedence(&parse_version("1.0.0+build.1").unwrap()) == Ordering::Equal`.
   Do **not** use `assert_eq!` on the `Version` values. The old type
   discarded build metadata so `Eq` held; the crate keeps it. Derived
   `Ord` also disagrees (`+build.2` > `+build.1`).

Drop:

6. `1.0.0-100000000000000000000` vs `1.0.0-99999999999999999999`.
   That was overflow-safe string comparison for identifiers that do not
   fit in `u64`. Tink release tags never use it. Do not preserve a
   custom big-int path to keep this assertion.

`cargo test` must pass, including `src/update.rs` unit tests and
`tests/acceptance.rs` update/install cases that mention semantic
versions.

## Non-goals

- Changing release asset selection, digest checks, or download URL rules.
- Changing `install.sh` (it has its own parser).
- Re-exporting `semver::Version` from the crate API.
