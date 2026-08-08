# Project Skill Manifest Design

## Status

Proposed design for a first implementation of reproducible project skill state.
This document is intentionally written before code changes so the manifest contract
can be reviewed independently of the CLI implementation.

## Problem

Tink currently installs skills into `.agents/skills/` and records machine-local
catalog state under `$TINK_HOME`. Remote installs carry source receipts, but local
sources do not have a committed declaration that another checkout or CI job can
use to reconstruct the intended agent environment.

The missing contract is a version-controlled project manifest that declares the
skills a project intentionally owns and pins reproducible source information.

## Goals

- Make project skill dependencies reviewable in Git.
- Reconstruct declared skills on a fresh machine without reading `$TINK_HOME`.
- Detect missing, extra, modified, or incorrectly sourced installed skills.
- Preserve current safety behavior: no symlink traversal, no silent divergent overwrite,
  and no catalog mutation before a successful install.
- Support local paths and public GitHub sources in the first version.
- Keep the existing `.agents/skills/` layout and CLI behavior backward compatible.

## Non-goals (v1)

- Managing subagents or provider-specific agent profiles.
- Replacing the existing by-project catalog.
- Supporting arbitrary Git hosts, private credentials, or mutable branch pins.
- Automatically deleting unlisted skills.
- A network service, daemon, cache, or distributed coordination protocol.

## Proposed project files

```text
.tink/
  skills.toml       # human-reviewed dependency intent
  skills.lock       # exact resolved revisions and content hashes
.agents/skills/     # installed, checked-in skill trees remain the runtime input
```

Both `.tink/skills.toml` and `.tink/skills.lock` are project-owned and should be
committed. `$TINK_HOME` remains a cache/library and catalog location, not a source
of truth.

## Manifest and lockfile shape

`.tink/skills.toml` declares intent:

```toml
version = 1

[[skills]]
name = "reviewer"
source = "https://github.com/example/skills.git"
path = "skills/reviewer"
```

`.tink/skills.lock` records resolution:

```toml
version = 1

[[skills]]
name = "reviewer"
source = "https://github.com/example/skills.git"
revision = "40-character-full-git-commit-sha"
path = "skills/reviewer"
sha256 = "sha256-of-installed-tree-excluding-receipt"
```

Rules:

- `name` must match the installed directory and `SKILL.md` metadata.
- Remote entries require canonical GitHub HTTPS `source`, full immutable `revision`,
  and normalized relative `path`.
- Local entries require a project-relative path; absolute paths are rejected.
- `sha256` covers the normalized skill tree excluding `.tink-source.json`; it detects
  body drift without making the receipt part of content identity.
- Duplicate names are invalid.
- Unknown top-level fields are rejected in v1 to avoid silently ignored policy.
- Manifest and lockfile ordering is stable by `name` when Tink writes them.

## Commands

### `tink skill sync`

Install or verify every manifest entry. Missing entries may be created. Existing
entries are repaired only when their body is unchanged and only the receipt differs.
Body divergence fails closed and leaves the project tree untouched. Unlisted live
skills are reported but not deleted.

### `tink skill verify`

Read-only validation of the manifest and installed trees. It reports:

- manifest syntax/schema errors;
- missing declared skills;
- name/path or metadata mismatches;
- content hash drift;
- invalid or stale remote receipts;
- unlisted installed skills.

Exit code is 0 only when all declared skills are present and valid. This command
must not require network access; remote reachability is not part of verification.

### Existing commands

- `skill add` remains supported and continues its current behavior.
- Successful `skill add` should offer an explicit `--manifest` mode in v1 rather
  than silently changing existing projects. Default behavior remains unchanged.
- `skill remove` updates the manifest only when the removed skill is declared;
  it must not remove an undeclared local tree.

## Installation and consistency model

Tink uses a local, filesystem-based transaction per skill:

1. Parse and validate the manifest and lockfile before writes.
2. Resolve sources and stage each candidate outside the live skill directory.
3. Validate names, trees, receipts, and hashes.
4. Preflight every destination for missing, identical, receipt-only drift, or body drift.
5. Refuse if any body-divergent destination exists.
6. Commit staged trees and manifest changes.
7. Update the existing catalog only after successful project installation.

A failed sync must not leave a partially rewritten manifest. Cross-skill rollback
is intentionally deferred; the safe v1 contract is that each write is staged and
manifest replacement is atomic, while an interrupted sync may leave some newly
created skill trees that a later sync can reconcile.

## Security and safety

- Reject symlinks and special files in manifests, source trees, and destinations.
- Never execute skill content during sync or verify.
- Reject path traversal, absolute local paths, and non-GitHub remotes.
- Use temporary files plus atomic rename for manifest writes.
- Preserve existing refusal messages for divergent project skill bodies.
- Do not embed credentials or tokens in the manifest.

## Backward compatibility and migration

- Existing projects without `.tink/skills.toml` continue to use current commands.
- `tink skill manifest import` may be added later to generate a manifest from the
  current catalog and installed trees; it is not required for the first slice.
- Existing remote receipts remain valid and are used as inputs when generating pins.
- Local skills without receipts can be added to a manifest only with an explicit
  source path and computed hash.

## Scale and operational assumptions

This is a repository-local tool, not a distributed service:

- Typical manifest: 1–100 skills; upper target: 1,000 entries.
- Typical skill tree: under 10 MB; hash calculation is linear in checked-in bytes.
- Sync is human/CI invoked, not a long-running process.
- No server availability or QPS requirement; GitHub/network failures must be bounded
  and must not corrupt installed state.

## Tradeoffs

- **Manifest plus installed copies:** larger repositories, but offline execution and
  reviewable agent context. Preferred for reliability.
- **Manifest plus generated copies:** smaller repositories, but every checkout needs
  network access and agent behavior becomes bootstrap-dependent. Deferred.
- **Two files under `.tink/`:** separates human intent from machine resolution, following
  the Cargo.toml/Cargo.lock model while preserving the native `.agents/skills/` convention.
- **No automatic deletion:** avoids destructive surprises; a future `--prune` can be
  explicit and separately designed.
- **SHA-256 tree hash:** catches local tampering and drift, while remaining independent
  of Git implementation details. It adds hashing cost but no meaningful cost at the
  target scale.

## Implementation slices

1. Add manifest parser/validator and unit tests; no CLI changes.
2. Add `skill verify` for read-only manifest and tree validation.
3. Add manifest-aware `skill add --manifest` and atomic manifest writes.
4. Add `skill sync` with staged installs and bounded remote resolution.
5. Add acceptance coverage, docs, and a migration/import command if needed.

## System-design diagnostic

This local tool scores 5/8 for the distributed-system checklist: requirements,
reliability boundaries, deployment/rollback behavior, and observability are explicit;
QPS, database scaling, caching, and queues are intentionally not applicable to a
repository-local CLI. Adding distributed infrastructure would be overengineering.
