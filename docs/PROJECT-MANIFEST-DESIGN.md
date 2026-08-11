# Project Skill Manifest Design

## Status

Implemented. `tink skill lock`, `tink skill sync`, and `tink skill verify` own the
project manifest contract described here. The manifest remains version 1; the lockfile
is version 2 after the digest hardening migration.

## Outcome and authority

Tink installs standalone skills into `.agents/skills/`, but the home library and
by-project catalog are machine-local derived state. A repository needs committed intent
and exact pins so another checkout or CI job can reconstruct and verify the same live
skill trees without treating `$TINK_HOME` as authority.

```text
.tink/
  skills.toml       # reviewed dependency intent, version 1
  skills.lock       # exact source pins and version-2 tree digests
.agents/skills/     # installed trees consumed by agents
```

Both project files are intended to be committed. `$TINK_HOME` remains a rebuildable
library/catalog and never becomes an agent discovery root.

## Supported contract

- Review project skill dependencies in Git.
- Reconstruct local, pinned public-GitHub, and embedded `manage-tink` sources.
- Detect missing, extra, modified, incorrectly sourced, or receipt-mismatched skills.
- Refuse symlinks, special files, path traversal, and divergent project overwrites.
- Refuse a skillset-receipt-owned source at the standalone manifest boundary.
- Resolve and preflight every declared exact source before multi-skill publication.
- Never delete unlisted skills as a side effect of sync.

The implementation does not manage skillsets through the manifest, automatically edit
the manifest after `skill add`/`remove`, support private credentials or arbitrary
remote Git hosts, prune live skills, coordinate concurrent writers, or provide
cross-skill rollback after an unexpected operational failure.

## File shape

`.tink/skills.toml` declares intent:

```toml
version = 1

[[skills]]
name = "reviewer"
source = "https://github.com/example/skills.git"
path = "skills/reviewer"
```

`.tink/skills.lock` records exact resolution:

```toml
version = 2

[[skills]]
name = "reviewer"
source = "https://github.com/example/skills.git"
revision = "40-character-full-git-object-id"
path = "skills/reviewer"
sha256 = "version-2-tree-digest"
```

Local sources are normalized project-relative paths and omit `revision`/`path` in the
lock entry. The embedded source is written as `tink:embedded/manage-tink` and is valid
only through the typed lock-source boundary; free-form `skill add` does not accept it.

Rules:

- `name` must be a valid unique standalone skill name and match `SKILL.md`.
- Remote entries use canonical public GitHub HTTPS source identity, a full immutable
  revision, and a normalized repository-relative skill path.
- Local entries must resolve inside the project; absolute paths outside it and path
  traversal are rejected.
- Unknown fields are rejected. Manifest and lock skill sets and typed source identity
  must agree exactly.
- Tink writes entries in stable name order.

## Digest version 2

`sha256` is a domain-separated, unambiguous encoding of the installed tree excluding
the root `.tink-source.json` receipt. Each entry length-frames its path bytes and
content, identifies directory versus regular file, and includes its canonical mode.
On Unix, paths are hashed as raw bytes; any executable bit maps to `0o755`, while a
non-executable file maps to `0o644`. Copy, equality, and digest operations share that
Git-portable semantic mode and discard umask-only and special-bit variation.

Excluding the receipt keeps content identity separate from the source pin. `verify`
checks remote receipt source, revision, and path independently.

A lockfile with `version = 1` is deliberately refused because its digest omitted mode
and did not domain-separate the new encoding. Run `tink skill lock` with the required
local `--source NAME=PATH` mappings to rewrite version 2, review the result, then commit
it. Tink does not silently reinterpret a version-1 digest.

## Commands

### `tink skill lock [--source NAME=PATH ...]`

Read all installed standalone project skills and deterministically rewrite both
project files. Remote skills use validated `.tink-source.json` data; local skills
require an explicit mapping, except embedded `manage-tink`, whose source is known.
Source paths are normalized and every installed tree receives a version-2 digest.

The two files are staged separately beside their destinations. The manifest is renamed
first and, if the lock rename fails, Tink attempts to restore the previous manifest.
This protects the ordinary failure path but is not a crash-safe transaction across the
pair.

### `tink skill verify`

Read-only and offline. It validates both schemas and versions, identical declaration
sets and source identity, installed names and trees, digest equality, and remote
receipt pins. Missing declared skills, unlisted installed standalone skills, and any
receipt mismatch fail the command. Success exits 0; a command failure exits 1.

### `tink skill sync`

Restore the complete locked set without pruning:

1. Parse and validate the full manifest and lockfile before writes.
2. Classify every lock entry as local, pinned GitHub, or embedded.
3. Prepare exact source bytes for every entry. Local trees are copied into retained
   snapshots; remote checkouts/worktrees stay alive through publication; embedded
   `manage-tink` is prepared from the running binary.
4. Validate every candidate name, safe tree, receipt, and version-2 digest.
5. Preflight every project destination and refuse divergence.
6. Reject installed standalone skills that are not declared.
7. Preflight every library destination and the by-project catalog boundary.
8. Publish prepared skills sequentially in manifest-name order through the ordinary
   add lifecycle: library, project, then catalog.
9. Run offline verification after all publications.

Expected bad hash, project divergence, unsafe library target, ownership collision, or
malformed catalog failures occur before the first skill publication. Preparation also
prevents a local source from changing between validation and later publication in the
same run.

Publication is intentionally not a transaction across skills or state owners. ENOSPC,
permission loss, interruption, or another unexpected operational error can leave an
earlier skill completely published and a later one absent. No success is reported for
that run; rerunning `tink skill sync` is the supported recovery path and converges the
idempotent completed steps.

## Catalog and library effects

Each successfully published skill follows the existing add lifecycle. The reusable
library is created, left identical, or repaired from the prepared source; the project
tree is created or repaired only for receipt-only drift; then the project name is
deposited in the by-project catalog. Catalog directories use a bounded project basename
plus SHA-256 identity of the canonical project root, using raw bytes on Unix.

The catalog and library are not part of the committed manifest authority. A catalog
failure can happen after valid library/project copies exist in an ordinary single-skill
add, and retry repairs the missing derived state. Manifest sync preflights predictable
catalog refusals before its first publication.

## Security and operating limits

- Skill contents are copied and hashed, never executed.
- Symlinks and special files are rejected throughout managed trees.
- GitHub network operations use pinned revisions for the lock result; verify is
  network-free.
- Typical operation is human/CI invoked and linear in declared tree bytes.
- There is no daemon, database, cache protocol, inter-process lock, or concurrent
  mutation guarantee.
- Same-directory staging/rename narrows individual write hazards; no atomicity claim
  spans project, library, catalog, or both project files.

## Executable evidence

Acceptance rows M1-M9 cover empty verification, lock generation, local and embedded
restore, missing-local source classification, missing manifests, all-entry hash
preflight, later library refusal before publication, and explicit v1-to-v2 relock.
Focused unit tests additionally cover ambiguous digest framing, mode sensitivity,
exact prepared local snapshots, project/library/catalog preflights, and the embedded
source path. See [`TESTING.md`](TESTING.md) for the complete gate and remaining fault-
injection/concurrency gaps.
