# Tink commands

Load when choosing or running a mutation (step 2). Prefer the `tink skill …`
form; `tink add`, `tink check`, and `tink refresh` are aliases of the matching
skill subcommands.

| Intent | Command |
|---|---|
| Safe init | `tink init --no-zen --no-twotink` |
| Init + ZEN | `tink init --with-zen --no-twotink` |
| Init + Twotink | `tink init --no-zen --with-twotink` |
| Init + both | `tink init --with-zen --with-twotink` |
| Init without embedded manage-tink | add `--no-manage-tink` |
| Add one skill | `tink skill add SOURCE` |
| Add from multi-skill repo | `tink skill add SOURCE --skill NAME` |
| List | `tink skill list` |
| Check | `tink skill check` |
| Refresh all clean imports | `tink skill refresh` |
| Refresh one | `tink skill refresh NAME` |
| Destroy scaffolding | `tink destroy --yes` (non-TTY/scripts) or `tink destroy` (TTY, confirm `y`) |

## Layout facts

- Live skills: `<project>/.agents/skills/<name>/` with `SKILL.md`.
- Home (`$TINK_HOME` or `~/.tink`) is not an agent discovery root. Installs may
  record skill **names** in `skills/by-project/<project>/meta.json` — a name
  catalog, not a skill-tree mirror. Destroy does not delete home or prune that
  catalog.
