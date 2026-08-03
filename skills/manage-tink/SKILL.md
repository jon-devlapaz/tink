---
name: manage-tink
description: "Tink CLI for repository-owned Agent Skills. Use when the user asks to set up or init Tink, add or refresh a project skill, list or check .agents/skills, list the home by-project catalog, or destroy project agent scaffolding."
---

# Manage Tink

**Live skills** live only under the project's `.agents/skills/`. Tink's **refusals**
are hard stops — honor them; do not work around them.

## Steps

1. **Inspect** — If `tink` is missing, report that it must be installed
   (`cargo install --git https://github.com/jon-devlapaz/tink.git --locked`) and
   stop. Otherwise: use `tink skill check` for this project's live state; use
   `tink skill list` for live skill names; use `tink skill list --home` when the
   user asks what Tink has recorded across projects (do not hand-parse
   `~/.tink/skills/by-project`).
   - Done when: either install guidance was given, or the chosen inspect
     command's exit status and output are known.

2. **Mutate only on explicit ask** — Map the user's request to exactly one
   command from [references/commands.md](references/commands.md). Run that
   command; do not invent flags, merges, force-overwrites, or alternate install
   paths (no symlinks, manual copies, or private registries).
   - Done when: the chosen command has finished and its stdout/stderr is known.

3. **Prove** — After every successful mutation except `destroy`, run
   `tink skill check` and report the result. After `destroy`, confirm
   `.agents/`, `ZEN.md`, and `AGENTS.md` are gone; do not run check.
   - Done when: proof for that mutation type is reported to the user.

## Authority

| User said… | Authorizes… |
|---|---|
| Set up / init Tink (no extras) | `tink init --no-zen --no-tink-skills` (embeds `manage-tink`) |
| …and ZEN / tink-skills / skip manage-tink | Only the matching `--with-*` / `--no-*` flags |
| Add / list / check / refresh … | The matching `tink skill …` command (`list --home` for the offline catalog) |
| Remove scaffolding / destroy Tink setup | `tink destroy` (TTY) or `tink destroy --yes` (scripts) |

"Set up Tink" does **not** authorize ZEN, tink-skills, or destroy.

## Ownership (always)

- Leave `.tink-source.json` untouched — it is the refresh **receipt**.
- On a refusal, stop and report it; only authorized `destroy` may delete, and
  only `.agents/`, `ZEN.md`, and `AGENTS.md`.
- Treat local skills as non-refreshable unless they carry a valid receipt.
- Never execute code from a skill while adding, checking, refreshing, or destroying.
