# Tink commands

Load for Step 2 mutation selection. Prefer the `tink skill …` form for add,
list, read, check, refresh, and remove.

| Intent | Command |
|---|---|
| Safe init | `tink init --no-tink-skills` |
| Init + tink-skills | `tink init --with-tink-skills` |
| Init without embedded manage-tink | add `--no-manage-tink` |
| Add one skill | `tink skill add SOURCE` |
| Add from a GitHub skill tree URL | `tink skill add https://github.com/OWNER/REPO/tree/REF/PATH` |
| Add from multi-skill repo | `tink skill add SOURCE --skill NAME_OR_REPOSITORY_PATH` |
| Add from library | `tink skill add NAME` |
| Harvest harness skills into library | `tink skill harvest` |
| Inspect a public GitHub repository or tree | `tink inspect GITHUB_URL` |
| List (this project) | `tink skill list` |
| List (library) | `tink library list` (`tink skill list --library` compatibility alias) |
| Read one standalone skill | `tink skill read NAME` |
| Read from the library | `tink skill read NAME --library` |
| Raw description | `tink skill read NAME --raw` |
| Check | `tink skill check` (exit ≠ 0 if any root is invalid; prints valid counts first) |
| Diagnose environment and consistency | `tink doctor` |
| Generate project manifest and lockfile | `tink skill lock --source NAME=PATH` for each local skill; every path must resolve inside the project |
| Verify manifest, lockfile, and installed trees | `tink skill verify` |
| Sync the exact pinned manifest set | `tink skill sync` (preflights expected project/library refusals, then publishes sequentially; rerun after an operational interruption) |
| Refresh all clean imports | `tink skill refresh` |
| Refresh one | `tink skill refresh NAME` |
| List stale imports (read-only) | `tink skill outdated` |
| Preview refresh without writing | `tink skill refresh --dry-run [NAME]` |
| Roll back the last refresh (single use) | `tink skill rollback NAME` |
| Refresh the active binary's embedded manage-tink | `tink skill refresh manage-tink` (explicitly replaces a differing receipt-free reserved copy; refuses remote provenance) |
| Remove one project skill | `tink skill remove NAME` |
| Add a skillset from a GitHub tree URL | `tink skillset add <url> [optional-name]` |
| Add a pinned skillset | `tink skillset add NAME-skillset` (or `NAME`) |
| List project skillsets | `tink skillset list` (divergent trees stay visible; list exits 0) |
| List library skillsets | `tink skillset list --library` |
| Refresh a clean pinned skillset | `tink skillset refresh NAME-skillset` (or `NAME`) |
| Update skillset(s) to latest upstream commit | `tink skillset update [NAME[-skillset]]` |
| Remove one project skillset | `tink skillset remove NAME-skillset` (or `NAME`) |
| Update the tink CLI binary | `tink update` (newer host asset only; verifies release digest, archive shape, and exact candidate version before replacement) |
| Destroy managed project skills | `tink destroy --yes` (non-TTY/scripts) or `tink destroy` (TTY, confirm `y`); preserves guidance and unrelated `.agents/` siblings |

## Layout facts

- Live skills: `<project>/.agents/skills/<name>/` with `SKILL.md`.
- Live skillsets:
  `<project>/.agents/skills/<name>-skillset/<member>/SKILL.md`. Skillset directories
  end canonically with `-skillset`; CLI mutating commands auto-append `-skillset` if omitted.
- Home (`$TINK_HOME` or `~/.tink`) is not an agent discovery root. Installs
  standalone library trees at `skills/<name>/` and derived skillset trees at
  `skillsets/<name>-skillset/`. List standalone skills with
  `tink library list` (`tink skill list --library` remains a compatibility
  alias); list skillsets with `tink skillset list --library`. Promote a
  standalone skill into a project with
  `tink skill add NAME` (bare standalone library skill name). `tink skill read
  NAME` prints one standalone skill's description (`--library` for the home
  copy; `--raw` for the description line). Receipt-backed
  roots and receipt-bearing sources (including dangling receipt links) are excluded
  from standalone operations. `tink skill harvest` copies complete trees
  from CLI-owned supported harness roots into the library create-only (never
  overwrites a divergent library entry). Do not duplicate or infer that root
  inventory in agent policy. Matching GitHub tips install into the project from
  that library; divergent library trees are repaired with a warning on `skill
  add`. `skill remove` deletes the project skill directory; it does not prune
  the library. `destroy` removes `.agents/skills/` and removes `.agents/` only
  when it is then empty. It preserves files outside `.agents/` (including
  `AGENTS.md`), unrelated `.agents/` siblings, and library trees. Project skill
  overwrites are still refused.
- Project lockfiles use version 2 tree digests with unambiguous entry framing,
  raw Unix path bytes, canonical executable modes, and file contents. A version-1 lock
  is refused until `tink skill lock` explicitly rewrites it. Manifest sync
  prepares every exact source and preflights expected project and library
  failures before sequential publication; it does not promise
  cross-skill atomicity. Retry the same sync after an unexpected operational
  interruption.
- `tink inspect GITHUB_URL` is read-only. It recursively discovers valid
  `SKILL.md` folders and reports inferred source skillsets, standalone skills,
  diagnostics, and the immutable inspected revision. It does not install
  anything or write a skillset pin.
- Skillset pins live at `skillsets/<name>-skillset.json` and pin an HTTPS Git
  source, full revision, source root, and explicit members. `tink skillset add
  <url>` create-only authors a pin; `tink skillset update` advances its revision.
  Name-based `skillset add` reads an existing pin. Hand authoring of that exact
  file still requires explicit authority and does not authorize install:

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
  skillsets-library trees carry `.tink-skillset.json`. A valid project tree is primary:
  it may repair its `$TINK_HOME/skillsets/` copy, while skillsets-library state never
  overwrites a
  divergent project. Receipt presence owns the root even when it also contains
  `SKILL.md`; standalone skill commands never expose, promote, or replace it.
  The receipt digest ignores root `SKILL.md` so manage-tink can author the
  required router without dirtying the install; refresh preserves that router.
  A missing root router fails `skill check`; clean `skillset add`/`refresh`/`update`
  restore it from `$TINK_HOME/skillsets/` when present, otherwise regenerate a
  baseline.
  `skillset remove` deletes only the project tree and keeps
  both the definition and skillsets-library copy.
  New receipts use `digestVersion: 2`; a clean legacy receipt migrates only via
  `tink skillset refresh NAME-skillset`.
- Tink has no inter-process lock. Do not run concurrent mutations against the
  same project or shared Tink home.
