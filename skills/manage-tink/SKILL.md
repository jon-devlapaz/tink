---
name: manage-tink
description: "Tink CLI for repository-owned Agent Skills. Use when the user asks to initialize Tink; inspect GitHub skill sources; add, list, check, lock, verify, sync, refresh, or remove project skills; manage grouped skillsets; use the home library or catalog; harvest harness skills; update Tink; or destroy project agent scaffolding."
---

# Manage Tink

**Live skills and skillsets** live only under the project's `.agents/skills/`.
The project is authoritative; the home library conforms to it. Tink's
**refusals** are hard stops — honor them; do not work around them.

## Steps

1. **Inspect** — If `tink` is missing, report that it must be installed
   (`curl -fsSL https://raw.githubusercontent.com/jon-devlapaz/tink/main/install.sh | sh`)
   and stop. Otherwise: use `tink skill check` for this project's live state; use
   `tink skill list` for live skill names; use `tink skill list --catalog` when the
   user asks what Tink has recorded across projects; use `tink skill list --library`
   when the user asks what skill trees sit in the home library (do not hand-parse
   `~/.tink/catalog/by-project` or `~/.tink/skills`).
   Use `tink skillset list` for receipt-backed project skillsets and
   `tink skillset list --library` for library skillsets. Use
   `tink inspect GITHUB_URL` to discover valid skills and inferred source
   skillsets without writing project or home state.
   - Done when: either install guidance was given, or the chosen inspect
     command's exit status and output are known.

2. **Mutate only on explicit ask** — Map the user's request to exactly one
   command from [references/commands.md](references/commands.md). Run that
   command; do not invent flags, merges, force-overwrites, or alternate install
   paths (no symlinks, manual copies, or private registries). To promote from
   the library into the project, use `tink skill add NAME` only (bare library
   skill name; not a path and not `owner/repo`).
   To fill the library from known harness skill locations, use
   `tink skill harvest` only (create-only; does not write project skills).
   To remove a live project skill, use `tink skill remove NAME` only (drops the
   name from the by-project catalog; does not prune the library). To update
   the tink CLI binary, use `tink update` only. After a **major** binary
   upgrade (new major version / removed flags), re-embed this skill:
   `tink skill remove manage-tink && tink init --no-zen --no-tink-skills`
   (init installs the manage-tink copy shipped with the binary; it does not
   overwrite a divergent live skill otherwise).
   Shell Tab completion setup:
   - Zsh (`~/.zshrc`, after `autoload -Uz compinit` and `compinit`):
     `source <(COMPLETE=zsh tink)`
   - Bash (`~/.bashrc`): `source <(COMPLETE=bash tink)`
   - Fish (`~/.config/fish/completions/tink.fish`):
     `COMPLETE=fish tink | source`
   - Done when: the chosen command has finished and its stdout/stderr is known.
   - After adding or changing project skills, if `.tink/skills.toml` and `.tink/skills.lock` do not exist, ask the user whether they want Tink to record a reproducible project skill manifest. If they agree, run `tink skill lock`; provide `--source NAME=PATH` mappings for local skills. Do not create the files without user approval.
   - When a project has a manifest and lockfile, use `tink skill sync` to restore missing pinned skills; stop on divergent content rather than overwriting it.
   - Skillset names are explicit: require the canonical name ending in
     `-skillset`; never append or remove the suffix for the user. `skillset add`
     reads its pinned definition from
     `$TINK_HOME/catalog/by-skillset/NAME-skillset/meta.json`; inspection does
     not create that definition. Use only `tink skillset add`, `refresh`, or
     `remove` for the corresponding project mutation.

3. **Prove** — After every successful mutation except `destroy`, `skill remove`,
   and `skillset remove`, run `tink skill check` and report the result. After
   `skill remove`, run `tink skill list` and `tink skill list --catalog` and
   confirm the name is gone from the project and the catalog. After
   `skillset remove`, run `tink skillset list` and confirm the skillset is gone
   from the project; its catalog definition and library copy should remain.
   After `destroy`,
   confirm `.agents/`, `ZEN.md`, and `AGENTS.md` are gone and
   `tink skill list --catalog` has no rows for this project; do not run check.
   - Done when: proof for that mutation type is reported to the user.
   - After `skill lock`, run `tink skill verify` and report that the manifest and lockfile match the installed trees. For projects that already have these files, recommend `tink skill verify` after skill mutations.

## Authority

| User said… | Authorizes… |
|---|---|
| Set up / init Tink (no extras) | `tink init --no-zen --no-tink-skills` (embeds `manage-tink`) |
| …and ZEN / tink-skills / skip manage-tink | Only the matching `--with-*` / `--no-*` flags |
| Add / list / check / refresh / remove … | The matching `tink skill …` command |
| List catalog | `tink skill list --catalog` |
| List library / promote from library | `tink skill list --library` / `tink skill add NAME` |
| Harvest harness skills into library | `tink skill harvest` |
| Inspect a public GitHub skill source | `tink inspect GITHUB_URL` (read-only) |
| List project / library skillsets | `tink skillset list` / `tink skillset list --library` |
| Add / refresh / remove a canonical skillset | The matching `tink skillset … NAME-skillset` command |
| Remove one project skill | `tink skill remove NAME` |
| Update the tink CLI | `tink update` |
| Remove scaffolding / destroy Tink setup | `tink destroy` (TTY) or `tink destroy --yes` (scripts) |

"Set up Tink" does **not** authorize ZEN, tink-skills, or destroy.

## Ownership (always)

- Leave `.tink-source.json` untouched — it is the refresh **receipt**.
- Leave `.tink-skillset.json` untouched — it is skillset ownership and digest
  evidence.
- On a refusal, stop and report it. Authorized deletes are only:
  - `tink skill remove NAME` → `.agents/skills/<name>/` and drops that name
    from the by-project catalog
  - `tink skillset remove NAME-skillset` → only the receipt-backed project
    skillset tree; preserves its catalog definition and library copy
  - `tink destroy` → `.agents/`, `ZEN.md`, and `AGENTS.md`, and drops this
    project's by-project catalog entry
- Treat local skills as non-refreshable unless they carry a valid receipt.
- Never execute code from a skill while adding, checking, refreshing, removing,
  or destroying.
- Library (`~/.tink/skills/`) is **not** an agent discovery root; do not
  symlink or copy from it by hand — use `tink skill add NAME`.
- Skillsets are nested at
  `.agents/skills/NAME-skillset/<member>/SKILL.md`; each member remains a valid
  named skill. Never flatten, merge, or manually copy members.
- Do not prune the library by hand. `skill remove` and `destroy` sync the
  by-project catalog; they do not delete library trees.
