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
| Add from multi-skill repo | `tink skill add SOURCE --skill NAME_OR_REPOSITORY_PATH` |
| Add from library | `tink skill add NAME` |
| Harvest harness skills into library | `tink skill harvest` |
| Inspect a public GitHub repository or tree | `tink inspect GITHUB_URL` |
| List (this project) | `tink skill list` |
| List (catalog) | `tink skill list --catalog` |
| List (library) | `tink library list` (`tink skill list --library` compatibility alias) |
| Check | `tink skill check` |
| Generate project manifest and lockfile | `tink skill lock --source NAME=PATH` for each local skill; every path must resolve inside the project |
| Verify manifest, lockfile, and installed trees | `tink skill verify` |
| Sync the exact pinned manifest set | `tink skill sync` (preflights expected project/library/catalog refusals, then publishes sequentially; rerun after an operational interruption) |
| Refresh all clean imports | `tink skill refresh` |
| Refresh one | `tink skill refresh NAME` |
| Refresh the active binary's embedded manage-tink | `tink skill refresh manage-tink` (explicitly replaces a differing receipt-free reserved copy; refuses remote provenance) |
| Remove one project skill | `tink skill remove NAME` |
| Add a pinned skillset | `tink skillset add NAME-skillset` |
| List project skillsets | `tink skillset list` |
| List library skillsets | `tink skillset list --library` |
| Refresh a clean pinned skillset | `tink skillset refresh NAME-skillset` |
| Remove one project skillset | `tink skillset remove NAME-skillset` |
| Update the tink CLI binary | `tink update` (newer host asset only; verifies release digest, archive shape, and exact candidate version before replacement) |
| Destroy managed project skills | `tink destroy --yes` (non-TTY/scripts) or `tink destroy` (TTY, confirm `y`); preserves guidance and unrelated `.agents/` siblings |

## Layout facts

- Live skills: `<project>/.agents/skills/<name>/` with `SKILL.md`.
- Live skillsets:
  `<project>/.agents/skills/<name>-skillset/<member>/SKILL.md`. Names must be
  typed canonically with `-skillset`; Tink does not infer the suffix on
  mutating commands.
- Home (`$TINK_HOME` or `~/.tink`) is not an agent discovery root. Installs
  library trees at `skills/<name>/`. List the library with
  `tink library list` (`tink skill list --library` remains a compatibility
  alias); promote into a project with
  `tink skill add NAME` (bare standalone library skill name). Receipt-backed
  roots and receipt-bearing sources (including dangling receipt links) are excluded
  from standalone operations. `tink skill harvest` copies complete trees
  from CLI-owned supported harness roots into the library create-only (never
  overwrites a divergent library entry). Do not duplicate or infer that root
  inventory in agent policy. Matching GitHub tips install into the project from
  that library; divergent library trees are repaired with a warning on `skill
  add`. Names are recorded in
  `catalog/by-project/<bounded-name>-<sha256-identity>/meta.json`. List the catalog with
  `tink skill list --catalog` (always headered three-column TSV; backslash, tab, CR,
  and LF inside fields are escaped as `\\\\`, `\\t`, `\\r`, and `\\n`).
  Do not hand-parse this derived by-project `meta.json` when the CLI is available. `skill remove`
  deletes the project skill directory and drops that name from the by-project
  catalog; it does not prune the library. `destroy` removes
  `.agents/skills/`, removes `.agents/` only when it is then empty, and drops
  this project's catalog entry. It preserves `AGENTS.md`, `ZEN.md`, unrelated
  `.agents/` siblings, library trees, and other projects' catalog rows.
  Project skill overwrites are still refused.
- Project lockfiles use version 2 tree digests with unambiguous entry framing,
  raw Unix path bytes, canonical executable modes, and file contents. A version-1 lock
  is refused until `tink skill lock` explicitly rewrites it. Manifest sync
  prepares every exact source and preflights expected project, library, and
  catalog failures before sequential publication; it does not promise
  cross-skill atomicity. Retry the same sync after an unexpected operational
  interruption.
- `tink inspect GITHUB_URL` is read-only. It recursively discovers valid
  `SKILL.md` folders and reports inferred source skillsets, standalone skills,
  diagnostics, and the immutable inspected revision. It does not install
  anything or write a catalog definition.
- Skillset definitions live at
  `catalog/by-skillset/<name>-skillset/meta.json` and pin an HTTPS Git source,
  full revision, source root, and explicit members. Tink validates and consumes
  this external input but has no command that writes it. Only an explicitly
  authorized authoring step may create or change the exact definition:

  ```json
  {
    "source": "https://github.com/example/agent-skills.git",
    "revision": "0123456789abcdef0123456789abcdef01234567",
    "sourceRoot": "skills/review",
    "members": ["code-review", "security-review"]
  }
  ```

  `tink inspect` may inform a proposal, but never infer or write the pinned
  revision or member list automatically. Definition authoring does not authorize
  `skillset add`. Installed project and
  library trees carry `.tink-skillset.json`. A valid project tree is primary:
  it may repair its library copy, while library state never overwrites a
  divergent project. Receipt presence owns the root even when it also contains
  `SKILL.md`; standalone skill commands never expose, promote, or replace it.
  `skillset remove` deletes only the project tree and keeps
  both the definition and library copy.
  New receipts use `digestVersion: 2`; a clean legacy receipt migrates only via
  `tink skillset refresh NAME-skillset`.
- Tink has no inter-process lock. Do not run concurrent mutations against the
  same project or shared Tink home.
