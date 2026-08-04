---
name: manage-tink
description: "Tink CLI for repository-owned Agent Skills. Use when the user asks to set up or init Tink, add or refresh a project skill, remove a project skill, update the tink CLI, list or check .agents/skills, list the home by-project catalog or home stash, promote a stash skill into the project, or destroy project agent scaffolding."
---

# Manage Tink

**Live skills** live only under the project's `.agents/skills/`. Tink's **refusals**
are hard stops — honor them; do not work around them.

## Steps

1. **Inspect** — If `tink` is missing, report that it must be installed
   (`curl -fsSL https://raw.githubusercontent.com/jon-devlapaz/tink/main/install.sh | sh`)
   and stop. Otherwise: use `tink skill check` for this project's live state; use
   `tink skill list` for live skill names; use `tink skill list --home` when the
   user asks what Tink has recorded across projects; use `tink skill list --stash`
   when the user asks what skill trees sit in the home dump (do not hand-parse
   `~/.tink/catalog/by-project` or `~/.tink/skills`).
   - Done when: either install guidance was given, or the chosen inspect
     command's exit status and output are known.

2. **Mutate only on explicit ask** — Map the user's request to exactly one
   command from [references/commands.md](references/commands.md). Run that
   command; do not invent flags, merges, force-overwrites, or alternate install
   paths (no symlinks, manual copies, or private registries). To promote from
   the home stash into the project, use `tink skill add --stash NAME` only.
   To remove a live project skill, use `tink skill remove NAME` only (does not
   prune home stash or catalog). To update the tink CLI binary, use
   `tink update` only.
   - Done when: the chosen command has finished and its stdout/stderr is known.

3. **Prove** — After every successful mutation except `destroy` and
   `skill remove`, run `tink skill check` and report the result. After
   `skill remove`, run `tink skill list` (or check if other skills remain) and
   confirm the name is gone from the project. After `destroy`, confirm
   `.agents/`, `ZEN.md`, and `AGENTS.md` are gone; do not run check.
   - Done when: proof for that mutation type is reported to the user.

## Authority

| User said… | Authorizes… |
|---|---|
| Set up / init Tink (no extras) | `tink init --no-zen --no-tink-skills` (embeds `manage-tink`) |
| …and ZEN / tink-skills / skip manage-tink | Only the matching `--with-*` / `--no-*` flags |
| Add / list / check / refresh / remove … | The matching `tink skill …` command |
| List home catalog | `tink skill list --home` |
| List home stash / promote from stash | `tink skill list --stash` / `tink skill add --stash NAME` |
| Remove one project skill | `tink skill remove NAME` |
| Update the tink CLI | `tink update` |
| Remove scaffolding / destroy Tink setup | `tink destroy` (TTY) or `tink destroy --yes` (scripts) |

"Set up Tink" does **not** authorize ZEN, tink-skills, or destroy.

## Ownership (always)

- Leave `.tink-source.json` untouched — it is the refresh **receipt**.
- On a refusal, stop and report it. Authorized deletes are only:
  - `tink skill remove NAME` → `.agents/skills/<name>/`
  - `tink destroy` → `.agents/`, `ZEN.md`, and `AGENTS.md`
- Treat local skills as non-refreshable unless they carry a valid receipt.
- Never execute code from a skill while adding, checking, refreshing, removing,
  or destroying.
- Home stash (`~/.tink/skills/`) is **not** an agent discovery root; do not
  symlink or copy from it by hand — use `tink skill add --stash NAME`.
- Do not prune the home stash or by-project catalog unless the user explicitly
  asks and a supporting command exists (prune is out of v1).
  `skill remove` does not prune stash or catalog.
