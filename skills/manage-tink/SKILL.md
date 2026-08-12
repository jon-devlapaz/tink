---
name: manage-tink
description: "Tink CLI for repository-owned Agent Skills. Use when the user asks to initialize Tink; inspect GitHub skill sources; add, list, check, lock, verify, sync, refresh, or remove project skills; manage grouped skillsets; use the home library or catalog; harvest harness skills; configure shell completion; update Tink; refresh embedded manage-tink; or destroy project agent scaffolding."
---

# Manage Tink

**Live skills and skillsets** live only under the project's `.agents/skills/`.
The project is authoritative; the home library conforms to it. Tink's
**refusals** are hard stops — honor them; do not work around them.

## When to Use

Use this workflow only for the Tink request that triggered the skill. Do not
broaden inspection authority into mutation authority.

## Inputs

- The current project root.
- The user's requested Tink operation and its explicit mutation authority.
- Any supplied skill source, skill name, or canonical `NAME-skillset` name.

## Procedure

### Step 1: Inspect the Requested State

If `tink` is missing, report the installation command and stop:
`curl -fsSL https://raw.githubusercontent.com/jon-devlapaz/tink/main/install.sh | sh`.
Otherwise, run only the read command matching the request:

- Project state or names: `tink skill check` or `tink skill list`.
- Catalog or library: `tink skill list --catalog` or `tink library list`
  (`tink skill list --library` remains a compatibility alias).
- Project or library skillsets: `tink skillset list` or
  `tink skillset list --library`.
- Public GitHub structure: `tink inspect GITHUB_URL`.

**Expected:** The command's exit status and output are known without writes.

**On failure:** Report the exact refusal or error and stop. Do not hand-parse
`~/.tink/catalog` or `~/.tink/skills` as a substitute for a failing CLI.

### Step 2: Select One Authorized Mutation

For an explicit mutation request, load
[references/commands.md](references/commands.md) and select one primary
command. Do not invent flags, merge operations, force-overwrite, use private
registries, or substitute symlinks or manual copies.

Require canonical skillset names ending in `-skillset`; never append or remove
the suffix. `skillset add` reads
`$TINK_HOME/catalog/by-skillset/NAME-skillset/meta.json`; inspection does not
create that definition. Tink has no definition-writer command. Only when the user
explicitly authorizes creating or changing a pinned skillset definition, author
that exact external-input file using the schema in `references/commands.md`.
Definition authoring does not authorize installing it or editing receipts,
installed trees, or the derived by-project index.

**Expected:** One primary command exactly matches the user's authority.

**On failure:** If the request does not authorize the required mutation or a
canonical name is missing, stop and ask for the missing authority or name.

### Step 3: Execute the Primary Mutation

Execute the selected mutation once. For explicitly authorized external definition
authoring, write only the exact `catalog/by-skillset/NAME-skillset/meta.json`
input and stop unless installation was separately authorized. Honor create-only
and divergence refusals. Use
`tink skill add NAME` to promote a bare library skill. Receipt-backed roots
remain skillsets. Use `tink skill harvest` to fill the library from known
harness roots, and only the matching `tink skillset add`, `refresh`, or
`remove` command for a skillset mutation.

**Expected:** The command finishes and its stdout, stderr, and exit status are
known.

**On failure:** Stop and report the failure. Do not repair Tink-managed state
by hand. Init, add, skillset refresh, and embedded-skill refresh can fail after an
earlier project, library, catalog, or guidance write succeeded. Report each
surface known or possibly changed; do not describe the failure as a no-op
unless that was proved.

### Step 4: Offer Reproducible Project State

After adding or changing project skills, check for `.tink/skills.toml` and
`.tink/skills.lock`. If absent, ask whether the user wants a reproducible
manifest. Only after approval, run `tink skill lock`, supplying
`--source NAME=PATH` for each local skill. Each local source path must resolve
inside the project. If `skill verify` reports a legacy
version-1 lock digest, ask before rerunning `skill lock`; that explicit relock
rewrites version 2 and includes raw Unix path bytes and portable executable modes. If
both files already exist, use `tink skill sync` only when the user requested
restoration.

**Expected:** Manifest files are created or synchronized only with explicit
approval; divergent content remains untouched.

**On failure:** Report missing local source mappings or divergence and stop.
Do not guess a source or overwrite a project tree. If an unexpected operational
error interrupted sequential sync publication, report the potentially partial
state and explain that rerunning the same idempotent `tink skill sync` is the
recovery path; ask before retrying.

### Step 5: Configure Shell Completion When Requested

Use the shell-specific command only after the user asks for completion:

- Zsh: after `autoload -Uz compinit` and `compinit`, use
  `source <(COMPLETE=zsh tink)`.
- Bash: use `source <(COMPLETE=bash tink)`.
- Fish: use `COMPLETE=fish tink | source`.

**Expected:** Completion is configured for the current requested shell session
only. Editing a startup file for future sessions is a separate filesystem
mutation and requires authority for that exact file.

**On failure:** Report the shell and command failure. Do not edit unrelated
shell configuration.

### Step 6: Refresh Manage Tink When Separately Authorized

After any binary update or observed contract mismatch, explain that the live
skill may be stale. Do not replace it automatically. Before `destroy`, compare
the active `tink destroy --help` boundary with this skill's ownership contract;
if it is broader, stop and renew approval. If the user explicitly authorizes
refreshing the embedded package, run `tink skill refresh manage-tink`.

**Expected:** Missing copies are installed, current copies report `Unchanged`,
and differing receipt-free copies are atomically replaced. The project,
library, and catalog are reconciled, and the binary's embedded copy becomes
the live project skill. A same-named skill with remote provenance is refused.

**On failure:** Stop after the failing command and report which project,
library, and catalog states were proven. Do not conceal a partially completed
publication.

### Step 7: Prove the Post-state

- After `init`, run `tink skill check`, project/catalog/library listings, and
  verify the requested optional guidance or bundle state.
- After add, promotion, refresh, or sync, run `tink skill check` and verify the
  affected project, catalog, and library entries. After sync, also run
  `tink skill verify`.
- After harvest, use its summary and `tink library list`; project check does not
  prove a library-only mutation.
- After `skill lock`, run `tink skill verify`.
- After skillset add or refresh, run `tink skill check`, `tink skillset list`,
  and `tink skillset list --library`.
- After `skill remove`, verify project/catalog absence and library retention.
- After `skillset remove`, verify project absence and library presence; its
  external definition should remain.
- After update, resolve the active binary and probe its exact version. After
  refreshing embedded `manage-tink`, run project/catalog/library listings plus
  `tink skill check`; check compares the live payload with the active binary.
- After `destroy`, confirm `.agents/skills/` is gone, `.agents/` is gone only if
  it became empty, `ZEN.md` and `AGENTS.md` are preserved, unrelated `.agents/`
  siblings remain, and the project has no catalog rows; do not run `skill check`.

**Expected:** The proof matching the mutation is reported to the user.

**On failure:** Report the unproven post-state and stop. Never claim success
from the mutation command alone.

## Validation

- [ ] The selected command matches the user's explicit authority.
- [ ] No Tink refusal was bypassed.
- [ ] Canonical skillset names retain `-skillset`.
- [ ] The mutation-specific post-state check passed.
- [ ] Any remaining uncertainty or partial completion is reported.

## Common Pitfalls

- Treating the home library as an agent discovery root.
- Treating a receipt-backed library skillset as a standalone skill.
- Treating a source with a regular or dangling `.tink-skillset.json` entry as a
  standalone skill; use `tink skillset add NAME-skillset` instead.
- Inferring a skillset suffix or definition from inspection output.
- Combining a binary update with project-skill replacement without approval.
- Treating external skillset-definition authoring as authority to edit receipts,
  derived by-project catalog metadata, or divergent managed trees.
- Treating command completion as proof of the requested outcome.

## Related Skills

- `skill-scout` — Research candidate skills with evidence before choosing one
  to add through Tink.

## Authority

| User said… | Authorizes… |
|---|---|
| Set up / init Tink (no extras) | `tink init --no-zen --no-tink-skills` (embeds `manage-tink`) |
| …and ZEN / tink-skills / skip manage-tink | Only the matching `--with-*` / `--no-*` flags |
| Add / list / check / refresh / remove … | The matching `tink skill …` command |
| List catalog | `tink skill list --catalog` |
| List library / promote from library | `tink library list` (`tink skill list --library` compatibility alias) / `tink skill add NAME` |
| Harvest harness skills into library | `tink skill harvest` |
| Inspect a public GitHub skill source | `tink inspect GITHUB_URL` (read-only) |
| List project / library skillsets | `tink skillset list` / `tink skillset list --library` |
| Add / refresh / remove a canonical skillset | The matching `tink skillset … NAME-skillset` command |
| Author a pinned skillset definition | Only the exact `catalog/by-skillset/NAME-skillset/meta.json` input; does not authorize install |
| Configure shell completion | Only the matching shell command |
| Persist shell completion | Only the exact startup file the user authorizes |
| Lock / verify / sync reproducible state | Only the matching `tink skill …` command; lock requires a project-contained source mapping for each local skill |
| Refresh embedded manage-tink | `tink skill refresh manage-tink` |
| Remove one project skill | `tink skill remove NAME` |
| Update the Tink binary | `tink update` only; refreshing embedded `manage-tink` requires separate authority |
| Remove managed project skills / destroy Tink setup | `tink destroy` (TTY) or `tink destroy --yes` (scripts); guidance and unrelated `.agents/` siblings are preserved |

"Set up Tink" does **not** authorize ZEN, tink-skills, refreshing embedded
`manage-tink`, or destroy.

## Ownership (always)

- Leave `.tink-source.json` untouched — it is the refresh **receipt**.
- Leave `.tink-skillset.json` untouched — it is skillset ownership and digest
  evidence. Its presence takes precedence over a root `SKILL.md`; standalone
  library commands must not expose, promote, or replace that root.
- Skillset definitions under `catalog/by-skillset/` are the only externally
  authored Tink-home metadata. Create or change one only with explicit authority;
  never infer its revision or members from an inspection proposal.
- On a refusal, stop and report it. Authorized deletes are only:
  - `tink skill remove NAME` → `.agents/skills/<name>/` and drops that name
    from the by-project catalog
  - `tink skillset remove NAME-skillset` → only the receipt-backed project
    skillset tree; preserves its catalog definition and library copy
  - `tink destroy` → `.agents/skills/`, then `.agents/` only if empty, and
    drops this project's by-project catalog entry; preserves `ZEN.md`,
    `AGENTS.md`, and unrelated `.agents/` siblings
- Treat local skills as non-refreshable unless they carry a valid receipt.
- Never execute code from a skill while Tink manages it.
- Library (`~/.tink/skills/`) is **not** an agent discovery root; use
  `tink skill add NAME` instead of copying or linking it.
- Skillsets are nested at
  `.agents/skills/NAME-skillset/<member>/SKILL.md`; each member remains a valid
  named skill. Never flatten, merge, or manually copy members.
- Do not prune the library by hand. Project removals preserve library trees.
- Do not run concurrent Tink mutations against one project or shared Tink home;
  Tink has no inter-process lock.
