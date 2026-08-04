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
| Add from home stash | `tink skill add --stash NAME` |
| Harvest harness skills into home stash | `tink skill harvest` |
| List (this project) | `tink skill list` |
| List (home catalog) | `tink skill list --home` |
| List (home stash) | `tink skill list --stash` |
| Check | `tink skill check` |
| Refresh all clean imports | `tink skill refresh` |
| Refresh one | `tink skill refresh NAME` |
| Remove one project skill | `tink skill remove NAME` |
| Update the tink CLI binary | `tink update` |
| Destroy scaffolding | `tink destroy --yes` (non-TTY/scripts) or `tink destroy` (TTY, confirm `y`) |

## Layout facts

- Live skills: `<project>/.agents/skills/<name>/` with `SKILL.md`.
- Home (`$TINK_HOME` or `~/.tink`) is not an agent discovery root. Installs
  stash trees at `skills/<name>/`. List the stash with
  `tink skill list --stash`; promote into a project with
  `tink skill add --stash NAME`. `tink skill harvest` copies complete trees
  from known harness roots into the stash create-only (never overwrites a
  divergent stash entry). Global roots include `~/.agents/skills`,
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
  from that stash; divergent stash trees are repaired with a warning on
  `skill add`. Names are recorded in
  `catalog/by-project/<project>/meta.json`. List the catalog with
  `tink skill list --home` (TSV with header `project`, `root`, `skill`).
  Do not hand-parse `meta.json` when the CLI is available. Destroy does not
  delete home or prune that catalog or stash. `skill remove` deletes only
  the project skill directory; it does not prune stash or catalog.
  Project skill overwrites are still refused.
