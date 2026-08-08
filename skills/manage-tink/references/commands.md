# Tink commands

Load when choosing or running a mutation (step 2). Use the `tink skill …`
form for add, list, check, refresh, and remove.

| Intent | Command |
|---|---|
| Safe init | `tink init --no-zen --no-tink-skills` |
| Init + ZEN | `tink init --with-zen --no-tink-skills` |
| Init + tink-skills | `tink init --no-zen --with-tink-skills` |
| Init + both | `tink init --with-zen --with-tink-skills` |
| Init without embedded manage-tink | add `--no-manage-tink` |
| Add one skill | `tink skill add SOURCE` |
| Add from multi-skill repo | `tink skill add SOURCE --skill NAME` |
| Add from library | `tink skill add NAME` |
| Harvest harness skills into library | `tink skill harvest` |
| List (this project) | `tink skill list` |
| List (catalog) | `tink skill list --catalog` |
| List (library) | `tink skill list --library` |
| Check | `tink skill check` |
| Generate project manifest and lockfile | `tink skill lock --source NAME=PATH` for each local skill |
| Verify manifest, lockfile, and installed trees | `tink skill verify` |
| Refresh all clean imports | `tink skill refresh` |
| Refresh one | `tink skill refresh NAME` |
| Remove one project skill | `tink skill remove NAME` |
| Update the tink CLI binary | `tink update` |
| Refresh live manage-tink after major upgrade | `tink skill remove manage-tink` then `tink init --no-zen --no-tink-skills` |
| Destroy scaffolding | `tink destroy --yes` (non-TTY/scripts) or `tink destroy` (TTY, confirm `y`) |

## Layout facts

- Live skills: `<project>/.agents/skills/<name>/` with `SKILL.md`.
- Home (`$TINK_HOME` or `~/.tink`) is not an agent discovery root. Installs
  library trees at `skills/<name>/`. List the library with
  `tink skill list --library`; promote into a project with
  `tink skill add NAME` (bare library skill name). `tink skill harvest` copies complete trees
  from known harness roots into the library create-only (never overwrites a
  divergent library entry). Global roots include `~/.agents/skills`,
  `~/.claude/skills`, `~/.codex/skills`, `~/.cursor/skills`,
  `~/.cursor/skills-cursor`, `~/.copilot/skills`, `~/.github/skills`,
  `~/.codeium/windsurf/skills`, `~/.cline/skills`, `~/.aider/skills`,
  `~/.gemini/skills` (+ Antigravity paths), `~/.roo/skills`,
  `~/.kilocode/skills`, `~/.amazonq/skills`, `~/.augment/skills`,
  `~/.tabnine/skills`, `~/.sourcegraph/skills`, and
  `~/.config/opencode/skills`. Project (cwd) roots include `.agents/skills`,
  `.claude/skills`, `.codex/skills`, `.cursor/skills`, `.github/skills`,
  `.windsurf/skills`, `.cline/skills`, `.clinerules/skills`, `.aider/skills`,
  `.gemini/skills`, `.agent/skills`, `.roo/skills`, `.kilocode/skills`,
  `.amazonq/skills`, `.augment/skills`, `.tabnine/skills`, `.opencode/skills`,
  and `.sourcegraph/skills`. Matching GitHub tips install into the project
  from that library; divergent library trees are repaired with a warning on
  `skill add`. Names are recorded in
  `catalog/by-project/<project>/meta.json`. List the catalog with
  `tink skill list --catalog` (TSV with header `project`, `root`, `skill`).
  Do not hand-parse `meta.json` when the CLI is available. `skill remove`
  deletes the project skill directory and drops that name from the by-project
  catalog; it does not prune the library. `destroy` removes project
  scaffolding and this project's catalog entry; it does not delete library
  trees or other projects' catalog rows.
  Project skill overwrites are still refused.
