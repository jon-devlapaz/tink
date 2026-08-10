# Architecture

Tink is a local CLI that installs Agent Skills into a project's
`.agents/skills/`. That project directory is the only live agent-discovery root.
`$TINK_HOME` (default `~/.tink`) is offline inventory: it stores reusable skill
trees, project-name indexes, and pinned skillset definitions.

This document is the current navigation map. [`ACCEPTANCE.md`](../ACCEPTANCE.md)
records intended CLI and on-disk behavior, the workflow files own delivery automation,
[`TESTING.md`](TESTING.md) maps their executable sensors and known drift, and
[`DEEP-REFACTOR-LOG.md`](DEEP-REFACTOR-LOG.md) preserves experiment history.

## State and authority

| State | Owner | Authority and allowed direction |
|---|---|---|
| `<project>/.agents/skills/` | `home.rs`, `skills.rs`, `check.rs`, `skillsets.rs` | Sole live discovery root. Standalone project divergence blocks overwrite; skillsets use their validated nested lifecycle. |
| `<project>/.tink/skills.toml` and `skills.lock` | `manifest.rs`, `sources.rs` | Project-owned standalone-skill intent and resolved pins for `lock`, `sync`, and `verify`. |
| `$TINK_HOME/layout.json` and root directories | `home.rs` | Marks and migrates offline inventory; never an agent discovery root. |
| `$TINK_HOME/skills/<name>/` | `library.rs`, `skillsets.rs` | Reusable standalone trees or derived skillset copies. A skillset receipt decides which lifecycle owns a root. |
| `$TINK_HOME/catalog/by-project/<project>/meta.json` | `catalog.rs` | Derived project-name index. Add/refresh deposit names; remove/destroy withdraw them. It is not runtime state. |
| `$TINK_HOME/catalog/by-skillset/<name>/meta.json` | `skillsets.rs` | Authored desired definition: HTTPS source, immutable revision, source root, and explicit members. Tink reads it but has no CLI writer for it. |
| `.tink-source.json` | `provenance.rs` | Optional standalone remote provenance: source, revision, and path. It does not classify a root. |
| `.tink-skillset.json` | `skillsets.rs` | Skillset ownership and digest evidence. Presence classifies; validated contents prove the installed tree. |

Use the qualified terms **project-name index**, **skillset definition**, **source
receipt**, and **skillset receipt**. Bare “catalog” and “receipt” hide different
owners and should not carry architectural decisions.

## Module ownership

| Area | Modules | Responsibility |
|---|---|---|
| Process edge | `main.rs`, `lib.rs`, `style.rs`, `error.rs` | Completion and argument parsing, command dispatch, output narration, styling, and user-facing failure shape. |
| Layout and persisted state | `home.rs`, `catalog.rs`, `manifest.rs`, `provenance.rs` | Project/home paths, project-name index, standalone manifest/lock, and source receipts. |
| Skill mechanisms | `skills.rs`, `sources.rs`, `git.rs`, `paths.rs`, `library.rs` | Skill discovery/validation/copy/digest, typed source classification, Git checkout, filesystem refusals, and standalone library policy. |
| Project workflows | `init.rs`, `add.rs`, `check.rs`, `refresh.rs`, `remove.rs`, `harvest.rs` | Bootstrap and the standalone skill lifecycle. |
| Skillset workflow | `skillsets.rs` | Canonical names, definition validation, staged install/refresh, receipt validation, grouped listing, removal, and project-to-library mirroring. |
| Read-only source inspection | `inspect.rs` | GitHub structure inspection and source-defined skillset inference; no project or home writes. |
| Supporting workflows | `destroy.rs`, `update.rs`, `manage_tink.rs`, `templates.rs` | Project teardown, binary update, embedded `manage-tink`, and init templates/defaults. |

`lib.rs` owns the public command vocabulary. There is no `skillset check` or
`skillset status`: `skill check` validates both standalone and receipt-backed
roots, while `skillset list` provides the grouped member view.

## Cross-cutting invariants

1. **Home is inventory, not runtime.** Skills become agent-visible only under the
   current project's `.agents/skills/`.
2. **Classification precedes trust.** `skillsets::has_receipt_entry` treats a
   `.tink-skillset.json` entry—including a dangling symlink—as a claim on the root.
   Parsing, member checks, and digest verification happen separately.
3. **Existing managed roots route by receipt.** Project list/check/remove and library
   list/load/deposit/cache operations route an existing receipt-classified root to the
   skillset lifecycle. Standalone source discovery and project add preflight do not yet
   provide the same categorical gate; see Known boundaries.
4. **Standalone project state is protected first.** A source add rejects project
   divergence before it may repair the reusable library and publish the project name.
5. **Installed skillsets flow project to library.** A skillset definition selects the
   desired pinned source. Once installed and validated, the project tree is primary;
   it may create or repair the library copy, never the reverse.
6. **Removal is scoped.** Standalone and skillset removal delete only their project
   trees. Shared library trees and skillset definitions remain.
7. **Filesystem safety bounds convenience.** Managed roots and copied trees reject
   symlinks and unsafe path shapes before mutation.

## Standalone `skill add`

`lib.rs` dispatches to `add.rs`, which asks `sources.rs` to classify the input once:

| Input | Route |
|---|---|
| Local path | Canonicalize, discover/select one valid skill, then use the source-install path. |
| GitHub source | Resolve/checkout through `git.rs`, select one skill, and create a source receipt. |
| Bare library name | `library.rs` loads one structurally validated standalone tree; receipt-classified roots are directed to `skillset add`. Source-receipt validation happens later during project checks. |

For a local or GitHub source, the complete publication order is:

1. Ensure the project layout and validate/select the source skill.
2. Reuse an exact standalone library match when available; skillset roots cannot be
   cache hits.
3. Preflight the project target and refuse divergence.
4. Deposit the source into the standalone library: create, no-op, or repair. A
   receipt-classified target refuses before mutation.
5. Install the project tree and then update the project-name index.

A bare-name promotion starts from step 3 with the structurally validated library tree.
Positive cache and repair behavior is intentional; receipt ownership is the boundary,
not a ban on library reuse.

## `skillset add`

`lib.rs` dispatches directly to `skillsets.rs`:

1. Validate the canonical `-skillset` name and read its authored skillset definition.
2. Refuse an ordinary or invalidly owned library collision before network or project
   publication.
3. If a matching valid project tree already exists, keep the operation offline and
   synchronize its library copy.
4. Otherwise checkout the pinned revision, copy only the explicit members into a
   staging tree, compute its digest, write the skillset receipt, and validate the
   complete staged tree.
5. Atomically rename the staged tree into the project and mirror that validated tree
   to the library.

`skillset refresh` reads the authored definition, then proves the installed project
clean. It either repairs the library from an unchanged project or stages and replaces
the project with best-effort rollback, then mirrors project to library. `skillset
remove` requires a valid owned receipt and deletes only the project tree.

## Other command owners

| Command family | Owner and effect |
|---|---|
| `skill list` / `skill check` | `check.rs` reads project entries, `skillsets.rs` validates receipt roots, and `lib.rs` owns grouped counts/output. Standalone listing excludes skillset roots; check validates both lifecycles. |
| `skill refresh` | `refresh.rs` uses the source receipt to prove the project clean. At an unchanged upstream revision it repairs the library directly from the project; after an upstream move it preflights the library, replaces the project, then updates the library and project-name index. |
| `skill remove` | `remove.rs` refuses skillset roots, withdraws the project-name index first, deletes the project tree, and preserves the library. |
| `skill harvest` | `harvest.rs` validates known harness trees and calls the create-only library path; it never writes a project. |
| `skill lock` / `sync` / `verify` | `manifest.rs` records standalone intent and pins, installs missing pinned skills, and verifies declarations, pins, receipts, and tree hashes. |
| `inspect` | `inspect.rs` reports GitHub structure without writing project or home state. |
| `destroy` / `update` | `destroy.rs` removes project scaffolding and its name index; `update.rs` replaces only the CLI binary. |

## Known boundaries

- Standalone source discovery and generic project preflight do not classify a source or
  project target by skillset receipt. With a missing library target, a receipt-bearing
  arbitrary source can therefore publish when the project target is missing or
  byte-identical. Its intended routing, and harvest's create-only reporting, remain
  separate design questions.
- Tink has no production inter-process library lock. Concurrent mutations are not a
  proven operating mode.
- Release sensors and their bypasses are documented in [`TESTING.md`](TESTING.md).
