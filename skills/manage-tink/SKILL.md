---
name: manage-tink
description: Manage repository-owned Agent Skills with the Tink CLI. Use when asked to set up Tink or a project .agents/skills directory, add a local or public GitHub skill, list or validate project skills, or refresh Tink-imported skills.
---

# Manage Tink

Keep complete skills in the repository's `.agents/skills/` directory so supported agent
harnesses can discover them without global configuration. Treat Tink's conflict refusals
as safety boundaries.

## Operate

1. Run `tink skill check` (or alias `tink check`) to inspect an existing setup. This
   command is read-only and may be used without mutation authority.
2. Run mutating commands only when the user explicitly asks for the corresponding
   change. After a successful mutation, run `tink skill check` and report the result.
3. If `tink` is unavailable, report that it must be installed (for example via
   `cargo install --git https://github.com/jon-devlapaz/tink.git --locked`). Do not
   substitute symlinks, manual copies, or a private registry.

Use these exact operations (canonical form; `tink add` / `tink check` / `tink refresh`
remain aliases):

- Set up safe defaults: `tink init --no-zen --no-twotink`
  (`manage-tink` is included by default; pass `--no-manage-tink` only when the user
  asks to skip it)
- Set up requested options: add only the corresponding `--with-zen` or
  `--with-twotink` flags and explicitly disable the rest with `--no-zen` /
  `--no-twotink` when scripting
- Add one skill: `tink skill add SOURCE`, adding `--skill NAME` only when selecting
  from a multi-skill repository
- List project skills: `tink skill list`
- Check: `tink skill check`
- Refresh all clean imports: `tink skill refresh`
- Refresh one clean import: `tink skill refresh NAME`

"Set up Tink" authorizes safe defaults (skills home + embedded `manage-tink`). Optional
Zen content and the Twotink bundle remain off unless the user requests them.

Live skills are only under `.agents/skills/`. Do not treat `~/.tink` as agent discovery.
Home may list skill names under `skills/by-project/<project>/meta.json` for offline
inventory; that is not a second skill tree.

## Preserve ownership

- Do not edit `.tink-source.json`; it is Tink's three-field update proof.
- Do not bypass a refusal with force, merging, deletion, or replacement.
- Do not claim a local skill is updateable; only imports with valid provenance are.
- Do not run code from acquired skills during add or check.
