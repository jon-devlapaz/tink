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

When the user asks which skill fits a task and no mutation is authorized yet,
load [references/tink-jev.md](references/tink-jev.md) and follow it instead of
Step 2. Step 2 stays mutation-only.

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
- Library: `tink library list`.
- Project or library skillsets: `tink skillset list` or
  `tink skillset list --library`.
- Public GitHub structure: `tink inspect GITHUB_URL`.

**Expected:** The command's exit status and output are known, with no writes.
`tink skill check` remains the integrity gate: non-zero means stop.

**On failure:** Report the exact refusal or error and stop. Prefer the CLI over hand-parsing
`~/.tink-library` or skillset pin files.

### Step 2: Execute the one authorized mutation

Load [references/commands.md](references/commands.md) and select the single
command matching the user's authority. Match flags and canonical
`-skillset` names exactly. Execute it once. Honor create-only and divergence
refusals. Receipt-backed roots stay skillsets; bare library names promote with
`tink skill add NAME`.

**Expected:** The command finishes and its stdout, stderr, and exit status are
known.

**On failure:** Stop and report. Leave Tink-managed state to Tink. A failure
after an earlier write may leave partial state: report each surface known or
possibly changed; do not describe the failure as a no-op
unless that was proved.

### Step 3: Offer reproducible project state

After adding or changing project skills, check for `.tink/skills.toml` and
`.tink/skills.lock`. If absent, ask whether the user wants a reproducible
manifest. Only after approval, run `tink skill lock`, supplying
`--source NAME=PATH` for each local skill (paths must resolve inside the
project). If both files already exist, use `tink skill sync` only when the
user requested restoration.

**Expected:** Manifest files are created or synchronized only with explicit
approval; divergent content remains untouched.

**On failure:** Report missing source mappings or divergence and stop. Rerunning
the same idempotent `tink skill sync` is the recovery path after an operational
interruption; ask before retrying.

### Step 4: Configure shell completion when requested

Use the shell-specific command only after the user asks for completion:

- Zsh: after `autoload -Uz compinit` and `compinit`, use
  `source <(COMPLETE=zsh tink)`.
- Bash: use `source <(COMPLETE=bash tink)`.
- Fish: use `COMPLETE=fish tink | source`.

**Expected:** Completion is configured for the current requested shell session
only. Editing a startup file for future sessions is a separate filesystem
mutation and requires authority for that exact startup file.

**On failure:** Report the shell failure and stop.

### Step 5: Refresh manage-tink when separately authorized

After any binary update or observed contract mismatch, explain the live skill
may be stale and wait for an explicit refresh grant. Before `destroy`, compare
the active `tink destroy --help` boundary with this skill's ownership
contract; if it is broader, stop and renew approval. When authorized, run
`tink skill refresh manage-tink`.

**Expected:** Missing copies install, current copies report `Unchanged`, and
differing receipt-free copies are atomically replaced; check compares the live payload with the active binary. A same-named skill with remote
provenance is refused.

**On failure:** Stop and report which project, library, and pin states were proven.

### Step 6: Prove the post-state

- After `init`, run `tink skill check`, project/library listings, and
  verify `AGENTS.md` plus the requested optional bundle state.
- After add, promotion, refresh, or sync, run `tink skill check` and verify the
  affected project and library entries. After sync, also run `tink skill verify`.
- After harvest, use its summary and `tink library list`; project check does not
  prove a library-only mutation.
- After `skill lock`, run `tink skill verify`.
- After skillset add or refresh, run `tink skill check`, `tink skillset list`,
  and `tink skillset list --library`.
- After update, resolve the active binary and probe its exact version. After
  refreshing embedded `manage-tink`, run project/library listings plus
  `tink skill check`.
- After `destroy`, confirm `.agents/skills/` is gone and files outside
  `.agents/` (including `AGENTS.md`) are preserved; skip `skill check`.

**Expected:** The proof matching the mutation is reported to the user.

**On failure:** Report the unproven post-state and stop.

## Validation

- [ ] The selected command matches the user's explicit authority.
- [ ] Every refusal was honored.
- [ ] The mutation-specific post-state check passed.
- [ ] Any remaining uncertainty or partial completion is reported.

## Common Pitfalls

- Treating the home library as an agent discovery root.
- Treating a receipt-backed skillset root as a standalone skill.
- Treating an install failure as a no-op without proving untouched state.
- Combining a binary update with project-skill replacement without approval.

## Related Skills

- `skill-scout` — Scout candidate skills with evidence before choosing one to
  add through Tink. When both apply, scout first, then Jev Choice over the
  scouted set (see `references/tink-jev.md`).

## Authority

| User said… | Authorizes… |
|---|---|
| Set up / init Tink (no extras) | `tink init --no-tink-skills` (embeds `manage-tink`) |
| …and tink-skills / skip manage-tink | Only the matching `--with-*` / `--no-*` flags |
| Add / list / read / check / refresh / remove … | The matching `tink skill …` command |
| Harvest harness skills into library | `tink skill harvest` |
| Inspect a public GitHub skill source | `tink inspect GITHUB_URL` (read-only) |
| List project / library skillsets | `tink skillset list` / `tink skillset list --library` |
| Add / refresh / update / remove a canonical skillset | The matching `tink skillset …` command |
| Configure shell completion | Only the matching shell command |
| Lock / verify / sync reproducible state | Only the matching `tink skill …` command |
| Refresh embedded manage-tink | `tink skill refresh manage-tink` |
| Update the Tink binary | `tink update` only; refreshing embedded `manage-tink` needs separate authority |
| Remove managed project skills / destroy Tink setup | `tink destroy` (TTY) or `tink destroy --yes` (scripts) |

"Set up Tink" grants only `tink init --no-tink-skills`. Tink-skills, embedded
refresh, and destroy each need their own ask.

## Ownership

- `.tink-source.json` is the refresh **receipt**; leave it as Tink wrote it.
- `.tink-skillset.json` owns the skillset root even when a root `SKILL.md`
  router also exists; standalone commands leave that root alone. The receipt
  digest ignores root `SKILL.md`. A missing root router fails `skill check`;
  clean add/refresh/update restore it.
- Skillset pins (`skillsets/NAME-skillset.json`) under `$TINK_HOME/skillsets/` are the only externally
  authored Tink-home metadata. URL `skillset add` and `skillset update` write pins;
  hand authoring that exact file still requires explicit authority and
  explicitly authorizes only the file — it does not authorize installing it.
- Authorized deletes are only:
  - `tink skill remove NAME` → `.agents/skills/<name>/` only (library preserved)
  - `tink skillset remove NAME-skillset` → only the receipt-backed project
    skillset tree; preserves its pin file and library copy
  - `tink destroy` → `.agents/skills/`, then `.agents/` only if empty;
    preserves files outside `.agents/` (including `AGENTS.md`)
- Local skills stay non-refreshable unless they carry a valid receipt.
- Library (`~/.tink-library/skills/`) is not an agent discovery root; promote with
  `tink skill add NAME`.
- Skillsets nest at `.agents/skills/NAME-skillset/<member>/SKILL.md`.
- Run one Tink mutation at a time; Tink has no inter-process lock.
