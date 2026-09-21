# Tink commands

Load for Step 2 mutation selection. Prefer the `tink skill …` form for add,
list, read, check, refresh, and remove.

| Intent | Command |
|---|---|
| Safe init | `tink init --no-tink-skills` |
| Init + tink-skills | `tink init --with-tink-skills` |
| Init without embedded manage-tink | add `--no-manage-tink` |
| Add one skill | `tink skill add SOURCE` |
| Add from multi-skill repo | `tink skill add SOURCE --skill NAME_OR_REPOSITORY_PATH` |
| Add from library | `tink skill add NAME` |
| Harvest harness skills into library | `tink skill harvest` |
| Inspect a public GitHub repository or tree | `tink inspect GITHUB_URL` |
| List (this project) | `tink skill list` |
| List (library) | `tink library list` |
| Read one standalone skill | `tink skill read NAME [--library] [--raw]` |
| Check | `tink skill check` (exit ≠ 0 if any root is invalid) |
| Diagnose environment and consistency | `tink doctor` |
| Generate project manifest and lockfile | `tink skill lock --source NAME=PATH` for each local skill; every path must resolve inside the project |
| Verify manifest, lockfile, and installed trees | `tink skill verify` |
| Sync the exact pinned manifest set | `tink skill sync` (rerun after an operational interruption) |
| Refresh all clean imports | `tink skill refresh` |
| Refresh one | `tink skill refresh NAME` |
| List stale imports (read-only) | `tink skill outdated` |
| Preview refresh without writing | `tink skill refresh --dry-run [NAME]` |
| Roll back the last refresh (single use) | `tink skill rollback NAME` |
| Refresh the active binary's embedded manage-tink | `tink skill refresh manage-tink` (replaces a differing receipt-free copy; refuses remote provenance) |
| Remove one project skill | `tink skill remove NAME` |
| Add a skillset from a GitHub tree URL | `tink skillset add <url> [optional-name]` |
| Add a pinned skillset | `tink skillset add NAME-skillset` (or `NAME`) |
| List project / library skillsets | `tink skillset list` / `tink skillset list --library` |
| Refresh a clean pinned skillset | `tink skillset refresh NAME-skillset` (or `NAME`) |
| Update skillset(s) to latest upstream commit | `tink skillset update [NAME[-skillset]]` |
| Remove one project skillset | `tink skillset remove NAME-skillset` (or `NAME`) |
| Update the tink CLI binary | `tink update` |
| Destroy managed project skills | `tink destroy --yes` (scripts) or `tink destroy` (TTY); preserves guidance and unrelated `.agents/` siblings |

## Layout facts

- Live skills: `<project>/.agents/skills/<name>/` with `SKILL.md`. Live
  skillsets: `<project>/.agents/skills/<name>-skillset/<member>/SKILL.md`.
- Home (`$TINK_HOME` or `~/.tink`) is not an agent discovery root. Standalone
  library trees live at `skills/<name>/`, skillset trees at
  `skillsets/<name>-skillset/`. Promote with `tink skill add NAME`.
  Receipt-backed roots are excluded from standalone operations.
- `tink skill harvest` copies complete trees from CLI-owned supported harness roots into the
  library create-only (never overwrites). `skill remove` deletes only the project copy. `destroy` removes
  `.agents/skills/` (then `.agents/` only if empty) and preserves everything
  outside `.agents/`. Project overwrites are refused.
- `tink inspect GITHUB_URL` is read-only: reports skills, inferred skillsets,
  diagnostics, and the inspected revision. It installs nothing and writes no pin.
- Skillset pins live at `skillsets/<name>-skillset.json` (HTTPS source, full
  revision, source root, explicit members). URL `skillset add` create-only authors a pin;
  name-based add reads it. Definition authoring does not authorize install:

  ```json
  {
    "source": "https://github.com/example/agent-skills.git",
    "revision": "0123456789abcdef0123456789abcdef01234567",
    "sourceRoot": "skills/review",
    "members": ["code-review", "security-review"]
  }
  ```
  Installed project and library trees carry `.tink-skillset.json`; the project tree is primary and may repair its
  library copy. The receipt digest ignores root `SKILL.md`; a missing root
  router fails `skill check` and clean add/refresh/update restore it.
  `skillset remove` deletes only the project tree, keeping the pin and library
  copy.
- Tink has no inter-process lock. Do not run concurrent mutations against the
  same project or shared Tink home.
