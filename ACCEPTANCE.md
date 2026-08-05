# Acceptance boundary (v1)

**Outcome:** A smaller Tink core in Rust that installs complete Agent Skills into
a project's `.agents/skills/`, proves them offline, refreshes clean GitHub
imports, removes a project skill on request, ensures a home root at `~/.tink`
(override: `TINK_HOME`), can list or promote skills from the home stash
(`skills/<name>/`) into a project, can harvest complete skill trees from known
harness locations into that stash (create-only), and can update the CLI binary
from GitHub Releases via `tink update`.

**Authority:** This file is the evaluator. Implementation stops when every row
below has an automated test that passes on macOS with `git` on `PATH`.

**Out of v1:** weekly GitHub update workflows, private GitHub auth, Windows,
Linux CI as a gate, pruning the home stash.

**Dogfood:** Prefer an installed `tink` from this repo, or `cargo run -q -- …`.

## Commands

Skill verbs live only under `tink skill`. There are no top-level `add` /
`check` / `refresh` aliases. CLI binary updates use top-level `tink update`.

| Command | Meaning |
|---|---|
| `tink init` | Create `.agents/skills/`; install `manage-tink` by default; optional ZEN + tink-skills; ensure `~/.tink` |
| `tink skill add <source> [--skill <name>]` | Install one local path, public GitHub skill, or home stash skill by name |
| `tink skill list` | List project skills under `.agents/skills/` (read-only) |
| `tink skill list --home` | List offline by-project catalog (`project`, `root`, `skill` TSV) |
| `tink skill list --stash` | List skill names in the home stash (`skills/<name>/` with `SKILL.md`) |
| `tink skill harvest` | Copy complete skill trees from known harness roots into the home stash (create-only; no project writes) |
| `tink skill check` | Validate project skills; no network; no writes |
| `tink skill refresh [name]` | Refresh clean GitHub imports; refuse local edits |
| `tink skill remove <name>` | Delete one project skill under `.agents/skills/<name>/` and drop that name from the by-project catalog (not home stash) |
| `tink update` | Replace this binary with the latest public GitHub Release (requires `curl` + `tar`) |
| `tink destroy [--yes]` | Remove `.agents/`, `ZEN.md`, and `AGENTS.md`; drop this project's by-project catalog entry (not home stash) |

## On-disk contracts

| Artifact | Contract |
|---|---|
| Live skills | `<project>/.agents/skills/<name>/` with `SKILL.md` |
| Receipt | `.tink-source.json` with exactly `source`, `revision`, `path` (non-empty strings) |
| Home root | `$TINK_HOME` or `~/.tink`, with `layout.json` (`kind`: `tink-skill-inventory`) |
| Home stash | `skills/<name>/` skill trees copied on successful add (rebuildable dump; identical tip may install project from stash; divergent → repair + warn; project overwrite still refused; not an agent discovery root) |
| Offline catalog | `catalog/by-project/<project>/meta.json` with `name`, `root`, `skills` name list (not skill trees); `skill remove` drops a name; `destroy` drops the project entry |

## Rows

Ids are stable. Tests must name or comment the id they prove.

### Bootstrap

| Id | Action | Expect |
|---|---|---|
| I1 | `init` in empty project | Creates `.agents/skills/` as real directories (not symlinks) |
| I2 | `init` when `.agents` is a symlink | Exit ≠ 0; mentions symlink; creates nothing unsafe |
| I3 | `init` (non-interactive / `--no-zen --no-tink-skills`) | Does **not** write `AGENTS.md`, `ZEN.md`, or `.github/workflows/*` (may still install `manage-tink`) |
| I4 | `init` with `TINK_HOME` set | Creates home root + `layout.json` + `catalog/by-project/` + `skills/` |
| I5 | `init --with-zen` | Writes `ZEN.md` and an `AGENTS.md` that references it |
| I6 | `init` (default) | Installs `.agents/skills/manage-tink/`; catalogs `manage-tink`; stashes tree at `skills/manage-tink/` |
| I7 | `init --no-manage-tink` | Does **not** install `manage-tink` |

### Local add

| Id | Action | Expect |
|---|---|---|
| A1 | `skill add` valid local skill dir | Installs under `.agents/skills/<name>/`; records name in catalog; stashes tree at `$TINK_HOME/skills/<name>/` |
| A2 | `skill add` same skill again (byte-identical) | Success noop; project + home stash unchanged |
| A3 | `skill add` when project target exists and differs | Exit ≠ 0; "Refusing to overwrite"; project target unchanged |
| A4 | `skill add` skill tree containing a symlink | Exit ≠ 0; refuse |
| A5 | `skill add` multi-skill source without `--skill` | Exit ≠ 0; lists choices |
| A6 | `skill add` when home stash has same name but different tree (project missing) | Exit 0; installs project; repairs stash; warns on stderr that the home copy was updated |
| A7 | `skill add` skill named `by-project` | Exit ≠ 0; reserved name; no project/stash write |
| A8 | `skill add` same GitHub tip already stashed at home | Exit 0; installs project from stash (no clone); stdout notes stash |

### Remote add

| Id | Action | Expect |
|---|---|---|
| R1 | `skill add owner/repo --skill <name>` (public HTTPS via test redirect) | Installs skill + `.tink-source.json` with canonical `https://github.com/owner/repo.git`, full revision, relative `path` |
| R2 | `skill add` non-GitHub or non-HTTPS remote | Exit ≠ 0; refuse |
| R3 | `skill add ./missing-skill` (path-like, absent) | Exit ≠ 0; "Path does not exist"; **no** GitHub network fetch |
| R4 | `skill add /abs/missing` | Exit ≠ 0; "Path does not exist" |
| R5 | `skill add owner/repo` when `SKILL.md` is at repo root | Receipt `path` is `"."` (non-empty); `skill check` passes; `skill refresh` updates from repo root |

### Check

| Id | Action | Expect |
|---|---|---|
| C1 | `skill check` after valid `init` + `skill add` | Exit 0 |
| C2 | `skill check` without `.agents/skills` | Exit ≠ 0 |
| C3 | `skill check` when `.agents` is a symlink | Exit ≠ 0; refuse |
| C4 | `skill check` | Performs no network I/O and no filesystem writes (manual / future instrumentation; not automated in v1 harness) |

### List

| Id | Action | Expect |
|---|---|---|
| L1 | `skill list` after `init` | Exit 0; stdout includes `manage-tink` |
| L2 | `skill list` without `.agents/skills` | Exit ≠ 0 |
| L3 | `skill list --home` after init+add | Exit 0; header `project\\troot\\tskill` plus TSV rows for cataloged skills |
| L4 | `skill list` when `ZEN.md` exists without `AGENTS.md` reference | Exit 0; lists skills; warns on stderr about ZEN/AGENTS; `skill check` still fails |

### Home stash

| Id | Action | Expect |
|---|---|---|
| H1 | `skill list --stash` after init+add | Exit 0; stdout includes stashed skill names (at least the added skill) |
| H2 | `skill add <name>` when stash has that skill and project lacks it | Exit 0; installs under `.agents/skills/<name>/`; catalogs name; **no** network; stdout notes stash |
| H3 | `skill add <missing-bare-name>` | Exit ≠ 0; mentions stash not found / missing; **no** GitHub network fetch |
| H4 | `skill add <name>` when stash has skill and project skill exists and differs | Exit ≠ 0; "Refusing to overwrite"; project target unchanged |
| H5 | `skill harvest` with fixture `$HOME/.agents/skills` + `$HOME/.claude/skills` + `TINK_HOME` | Copies complete skill trees into `$TINK_HOME/skills/`; no project `.agents` skill writes from harvest |
| H6 | `skill harvest` when stash already identical | Exit 0; summary counts already present; no per-skill already-present lines |
| H7 | `skill harvest` when stash diverges | Exit 0; stash unchanged; stderr skip warn |
| H8 | `skill harvest` skips home stash sources and unsafe (symlink-inside) skill trees; still harvests cwd skills under `TINK_HOME` outside `skills/` | Stash/unsafe omitted; non-stash path under home deposited |

### CLI surface

| Id | Action | Expect |
|---|---|---|
| V1 | `skill add` local skill | Installs under `.agents/skills/<name>/` |
| V2 | `skill check` after valid project | Exit 0; stdout includes `OK` |

### Refresh

| Id | Action | Expect |
|---|---|---|
| P1 | `skill refresh` clean GitHub-imported skill after upstream change | Updates project skill + receipt **and** `$TINK_HOME/skills/<name>/` |
| P2 | `skill refresh` when installed skill has local modifications | Exit ≠ 0; mentions local modifications; tree unchanged |
| P3 | `skill refresh` when upstream unchanged but home stash missing | Exit 0; backfills `$TINK_HOME/skills/<name>/` from project |
| P4 | `skill refresh` when home stash lacks only `.tink-source.json` | Exit 0; updates project + stash |
| P5 | `skill refresh` when home stash body diverges from project | Exit ≠ 0; "stash diverges"; project unchanged |
| P6 | `skill refresh` when upstream revision moves but skill tree bytes match | Exit 0; bumps project + stash receipts |
| P7 | `skill refresh` when project already at HEAD but home stash is stale | Exit 0; repairs stash from project |

### Remove

| Id | Action | Expect |
|---|---|---|
| X1 | `skill remove <name>` after init+add | Exit 0; project `.agents/skills/<name>/` gone; `skill list` omits name; home stash `$TINK_HOME/skills/<name>/` still present; `skill list --home` omits that skill for the project (siblings may remain) |
| X2 | `skill remove <missing>` | Exit ≠ 0; mentions not found / missing; nothing deleted |
| X3 | `skill remove` when `.agents` is a symlink | Exit ≠ 0; mentions symlink; tree unchanged |
| X4 | Successful `skill remove <name>` | Does **not** delete `$TINK_HOME/skills/<name>/` |
| X5 | `init` installs `manage-tink` | Embedded skill documents `tink skill remove NAME` as the only remove path; documents bare `tink skill add NAME` for stash promote (not `add --stash`); states remove drops the catalog name and destroy drops the project catalog entry; home stash remains unpruned |

### Destroy

| Id | Action | Expect |
|---|---|---|
| D1 | `destroy --yes` after `init --with-zen` (extra skill allowed) | Removes `.agents/`, `ZEN.md`, `AGENTS.md`; leaves home stash + `layout.json` intact; drops this project's by-project catalog entry (`skill list --home` has no rows for it) |
| D2 | `destroy` without `--yes` (non-TTY) | Exit ≠ 0; refuses without confirmation; project files unchanged |
| D3 | `destroy --yes` when `.agents` is a symlink | Exit ≠ 0; mentions symlink |

### Update (CLI binary)

| Id | Action | Expect |
|---|---|---|
| U1 | `update` when releases API is unreachable | Exit ≠ 0; clear download/metadata failure; binary unchanged |
| U2 | `update` when latest release version matches this binary | Exit 0; stdout notes up to date; binary unchanged |
| U3 | `update` when a newer release asset exists for this host | Exit 0; replaces the running binary; stdout notes updated version |

### Safety (cross-cutting)

| Id | Action | Expect |
|---|---|---|
| S1 | Any successful command | Does not `git init`, stage, commit, or push |
| S2 | Home root | Never treated as an agent discovery root |

## Proof

```console
cargo test
```

A row is done only when its automated test passes. Passing tests prove only what
they assert.
