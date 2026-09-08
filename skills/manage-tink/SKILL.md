---
name: manage-tink
description: >
  Runs the Tink CLI for project skills and skillsets. Use when the user asks to
  init Tink; add, list, read, check, lock, verify, sync, refresh, or remove
  skills; manage skillsets; use the library; harvest harness skills;
  inspect a GitHub skill source; configure completion; update Tink; refresh
  embedded manage-tink; or destroy project agent scaffolding.
---

# Manage Tink

Live skills and skillsets live under the project's `.agents/skills/`. The
project is authoritative; the home library conforms to it. A Tink **refusal**
ends the turn: report it and stop.

## When to Use

Use this workflow only for the Tink request that triggered the skill. Treat
inspection authority and mutation authority as separate grants.

## Inputs

- The current project root.
- The requested Tink operation and its explicit mutation **authority**.
- Any skill source, skill name, or canonical `NAME-skillset` name.

## Procedure

### Step 1: Inspect the requested state

If `tink` is missing, report the install command and stop:
`curl -fsSL https://raw.githubusercontent.com/jon-devlapaz/tink/main/install.sh | sh`.

Otherwise run only the read command that matches the request:

- Project state or names: `tink skill check` or `tink skill list`.
- One installed skill's description: `tink skill read NAME` (`--library` for the
  home copy; `--raw` for the description line only).
- Library: `tink library list`
  (`tink skill list --library` remains a compatibility alias).
- Project or library skillsets: `tink skillset list` or
  `tink skillset list --library`.
- Public GitHub structure: `tink inspect GITHUB_URL`.

**Expected:** The command's exit status and output are known, with no writes.
`tink skillset list` may exit 0 while marking divergent trees in the listing;
treat row-level errors as actionable without stopping inspection of other trees.
`tink skill check` remains the integrity gate: non-zero means stop.

**On failure:** For `skill check` and mutation-bound reads that exit non-zero,
report the exact refusal or error and stop. Prefer the CLI over hand-parsing
`~/.tink/skills` or skillset pin files.

### Step 2: Select one authorized mutation

For an explicit mutation request, load
[references/commands.md](references/commands.md) and select one primary
command. Match the CLI's flags and surfaces exactly.

Require canonical skillset names ending in `-skillset`. `skillset add` reads
`$TINK_HOME/skillsets/NAME-skillset.json`; inspection does not
create that definition. URL `skillset add` and `skillset update` write pins; name-based add reads them.
Only when the user explicitly authorizes creating or changing a pinned skillset
definition by hand, author that exact external-input file using the schema in
`references/commands.md`. Hand authoring does not authorize installing it or editing receipts,
installed trees.

**Expected:** One primary command exactly matches the user's authority.

**On failure:** Stop and ask for the missing authority or canonical name.

### Step 3: Execute the primary mutation

Execute the selected mutation once. For authorized external definition
authoring, write only the exact `skillsets/NAME-skillset.json`
input and stop unless installation was separately authorized. Honor create-only
and divergence refusals. Use `tink skill add NAME` to promote a bare library
skill. Receipt-backed roots remain skillsets. Use `tink skill harvest` to fill
the library from known harness roots, and only the matching
`tink skillset add`, `refresh`, `update`, or `remove` command for a skillset mutation.

**Expected:** The command finishes and its stdout, stderr, and exit status are
known.

**On failure:** Stop and report the failure. Leave Tink-managed state to Tink.
Init, add, skillset refresh, and embedded-skill refresh can fail after an
earlier project, library, or guidance write succeeded. Report each
surface known or possibly changed; do not describe the failure as a no-op
unless that was proved.

### Step 4: Offer reproducible project state

After adding or changing project skills, check for `.tink/skills.toml` and
`.tink/skills.lock`. If absent, ask whether the user wants a reproducible
manifest. Only after approval, run `tink skill lock`, supplying
`--source NAME=PATH` for each local skill. Each local source path must resolve
inside the project. If `skill verify` reports a legacy version-1 lock digest,
ask before rerunning `skill lock`; that explicit relock rewrites version 2 and
includes raw Unix path bytes and portable executable modes. If both files
already exist, use `tink skill sync` only when the user requested restoration.

**Expected:** Manifest files are created or synchronized only with explicit
approval; divergent content remains untouched.

**On failure:** Report missing local source mappings or divergence and stop.
Keep divergent trees. If an unexpected operational error interrupted sequential
sync publication, report the potentially partial state and explain that
rerunning the same idempotent `tink skill sync` is the recovery path; ask
before retrying.

### Step 4b: Elevate skillset router on ask

Tink automatically generates a verify-clean baseline `SKILL.md` router during
skillset installation. When the user asks to elevate, optimize, or customize
the router (e.g., "Use manage-tink to create a skillset router for NAME" or
"elevate the router for NAME"), load
[references/skillset-router.md](references/skillset-router.md) and follow it in
**overwrite** mode.

Do not overwrite unprompted. Refresh and remove preserve the existing router.

**Expected:** The elevated router passes `verify-router.mjs` and replaces the
baseline router without altering receipt digests.

**On failure:** Report the skillset-router failure. Leave receipts and member
skills untouched.

### Step 5: Configure shell completion when requested

Use the shell-specific command only after the user asks for completion:

- Zsh: after `autoload -Uz compinit` and `compinit`, use
  `source <(COMPLETE=zsh tink)`.
- Bash: use `source <(COMPLETE=bash tink)`.
- Fish: use `COMPLETE=fish tink | source`.

**Expected:** Completion is configured for the current requested shell session
only. Editing a startup file for future sessions is a separate filesystem
mutation and requires authority for that exact file.

**On failure:** Report the shell and command failure. Leave unrelated shell
configuration alone.

### Step 6: Refresh manage-tink when separately authorized

After any binary update or observed contract mismatch, explain that the live
skill may be stale. Wait for an explicit refresh grant. Before `destroy`,
compare the active `tink destroy --help` boundary with this skill's ownership
contract; if it is broader, stop and renew approval. When the user authorizes
refreshing the embedded package, run `tink skill refresh manage-tink`.

**Expected:** Missing copies are installed, current copies report `Unchanged`,
and differing receipt-free copies are atomically replaced. The project,
library, and skillset pins are reconciled, and the binary's embedded copy becomes
the live project skill. A same-named skill with remote provenance is refused.

**On failure:** Stop after the failing command and report which project,
library, and pin states were proven. Keep partial publication visible.

### Step 7: Prove the post-state

- After `init`, run `tink skill check`, project/library listings, and
  verify `AGENTS.md` plus the requested optional bundle state.
- After add, promotion, refresh, or sync, run `tink skill check` and verify the
  affected project and library entries. After sync, also run
  `tink skill verify`.
- After harvest, use its summary and `tink library list`; project check does not
  prove a library-only mutation.
- After `skill lock`, run `tink skill verify`.
- After skillset add or refresh, run `tink skill check`, `tink skillset list`,
  and `tink skillset list --library`.
- After `skill remove`, verify project absence and library retention.
- After `skillset remove`, verify project absence and `$TINK_HOME/skillsets/`
  presence; its
  pin file should remain.
- After update, resolve the active binary and probe its exact version. After
  refreshing embedded `manage-tink`, run project/library listings plus
  `tink skill check`; check compares the live payload with the active binary.
- After `destroy`, confirm `.agents/skills/` is gone, `.agents/` is gone only if
  it became empty, files outside `.agents/` (including `AGENTS.md`) are
  preserved, and unrelated `.agents/` siblings remain; skip `skill check`.

**Expected:** The proof matching the mutation is reported to the user.

**On failure:** Report the unproven post-state and stop. Treat mutation
command success as incomplete until this proof lands.

## Validation

- [ ] The selected command matches the user's explicit authority.
- [ ] Every refusal was honored.
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
  divergent managed trees.
- Treating command completion as proof of the requested outcome.

## Related Skills

- `skill-scout` — Scout candidate skills with evidence before choosing one to
  add through Tink.
- [references/skillset-router.md](references/skillset-router.md) — Required
  skillset-root router after add; also overwrite when the user asks.

## Authority

| User said… | Authorizes… |
|---|---|
| Set up / init Tink (no extras) | `tink init --no-tink-skills` (embeds `manage-tink`) |
| …and tink-skills / skip manage-tink | Only the matching `--with-*` / `--no-*` flags |
| Add / list / read / check / refresh / remove … | The matching `tink skill …` command |
| List library / promote from library | `tink library list` (`tink skill list --library` compatibility alias) / `tink skill add NAME` |
| Harvest harness skills into library | `tink skill harvest` |
| Inspect a public GitHub skill source | `tink inspect GITHUB_URL` (read-only) |
| List project / library skillsets | `tink skillset list` / `tink skillset list --library` |
| Add / refresh / update / remove a canonical skillset | The matching `tink skillset …` command (`tink skillset add <url> [name]`, `tink skillset update [name]`, or `tink skillset add <name>`) |
| Elevate / regenerate a skillset router | Elevate via [references/skillset-router.md](references/skillset-router.md) |
| Author a pinned skillset definition | Only the exact `skillsets/NAME-skillset.json` input; does not authorize install |
| Configure shell completion | Only the matching shell command |
| Persist shell completion | Only the exact startup file the user authorizes |
| Lock / verify / sync reproducible state | Only the matching `tink skill …` command; lock requires a project-contained source mapping for each local skill |
| Refresh embedded manage-tink | `tink skill refresh manage-tink` |
| Remove one project skill | `tink skill remove NAME` |
| Update the Tink binary | `tink update` only; refreshing embedded `manage-tink` requires separate authority |
| Remove managed project skills / destroy Tink setup | `tink destroy` (TTY) or `tink destroy --yes` (scripts); guidance and unrelated `.agents/` siblings are preserved |

"Set up Tink" grants only `tink init --no-tink-skills`. Tink-skills, embedded
refresh, and destroy each need their own ask.

## Ownership

- `.tink-source.json` is the refresh **receipt**; leave it as Tink wrote it.
- `.tink-skillset.json` is skillset ownership and digest evidence. Its presence
  owns the root even when a root `SKILL.md` router also exists; standalone
  library commands leave that root alone. The receipt digest ignores root
  `SKILL.md` so manage-tink can author the required router. A missing root
  router fails `skill check`; clean add/refresh/update restore it.
- Skillset pins under `$TINK_HOME/skillsets/<name>.json` are the only externally
  authored Tink-home metadata. Create or change one only with explicit authority;
  keep revision and members tied to that authorized file, not to inspection
  proposals.
- Authorized deletes are only:
  - `tink skill remove NAME` → `.agents/skills/<name>/` only (library preserved)
  - `tink skillset remove NAME-skillset` → only the receipt-backed project
    skillset tree; preserves its pin file and `$TINK_HOME/skillsets/` copy
  - `tink destroy` → `.agents/skills/`, then `.agents/` only if empty;
    preserves files outside `.agents/` (including `AGENTS.md`) and unrelated
    `.agents/` siblings
- Local skills stay non-refreshable unless they carry a valid receipt.
- Skill instructions stay unread as executable code while Tink manages the tree.
- Library (`~/.tink/skills/`) is not an agent discovery root; promote with
  `tink skill add NAME`.
- Skillsets nest at `.agents/skills/NAME-skillset/<member>/SKILL.md`; each
  member remains a valid named skill, kept nested.
- Project removals preserve library trees; leave library pruning to Tink.
- Run one Tink mutation at a time against a project or shared Tink home; Tink
  has no inter-process lock.
