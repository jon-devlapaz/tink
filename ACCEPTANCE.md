# Acceptance boundary (v1)

**Outcome:** A smaller Tink core in Rust that installs complete Agent Skills into
a project's `.agents/skills/`, proves them offline, refreshes clean GitHub
imports, removes a project skill on request, ensures a home root at `~/.tink`
(override: `TINK_HOME`), can list or promote skills from the library
(`skills/<name>/`) into a project, can harvest complete skill trees from known
harness locations into that library (create-only), and can update the CLI binary
from GitHub Releases via `tink update`.

**Authority:** This file is the evaluator. A row is complete when its named automated
sensor passes on a supported CI host with `git` on `PATH`. Rows explicitly marked
`Sensor: manual` remain known gaps rather than implied automated proof.

**Delivery gate:** Pull requests and `main` run the pinned Rust 1.95.0 formatting,
check, Clippy (`-D warnings`), documentation, audit, test, and release-build gates.
Native tests and release builds cover macOS and Linux on x86_64 and arm64. The
tag release workflow (including a manual dispatch from the matching `v*` tag)
repeats the quality gate before building all four artifacts.

**Out of v1:** weekly GitHub update workflows, private GitHub auth, Windows,
pruning the library, concurrent Tink mutations in one project/home, and
cross-filesystem rollback after an unexpected I/O failure. Expected validation
and ownership failures are preflighted before multi-skill publication; rerunning
an interrupted idempotent command is the recovery model.

**Process contract:** Success data and summaries go to stdout; warnings and
errors go to stderr. Command failures exit 1, Clap usage errors exit 2, and a
closed stdout is a normal pipeline termination that exits 0 without panic.

**Dogfood:** Prefer an installed `tink` from this repo, or `cargo run -q -- …`.

## Commands

Skill verbs live only under `tink skill`. There are no top-level `add` /
`check` / `refresh` aliases. CLI binary updates use top-level `tink update`.

| Command | Meaning |
|---|---|
| `tink init` | Create `.agents/skills/`; install `manage-tink` by default; optional ZEN + tink-skills; ensure `~/.tink` |
| `tink skill add <source> [--skill <name-or-path>]` | Install one local path, public GitHub skill, or library skill by name; remote selectors may be unique names or repository-relative paths |
| `tink skill list` | List project skills under `.agents/skills/` (read-only) |
| `tink skill list --catalog` | List offline by-project catalog (`project`, `root`, `skill` TSV) |
| `tink skill list --library` | List standalone skill names in the library; receipt-backed skillset roots are excluded |
| `tink skill harvest` | Copy complete skill trees from known harness roots into the library (create-only; no project writes) |
| `tink skill check` | Validate project skills; no network; no writes |
| `tink skill lock [--source <name=source>]` | Record the installed project skills in `.tink/skills.toml` and `.tink/skills.lock` |
| `tink skill sync` | Restore the locked project skills from their typed sources |
| `tink skill verify` | Verify installed project skills against the manifest and lockfile |
| `tink skill refresh [name]` | Refresh clean GitHub imports; refuse local edits |
| `tink skill remove <name>` | Delete one project skill under `.agents/skills/<name>/` and drop that name from the by-project catalog (not library) |
| `tink inspect <GITHUB_URL>` | Inspect skills and source-defined skillsets in a public GitHub URL without writing project or home state |
| `tink update` | Replace this binary with a newer verified public GitHub Release (requires `curl` + `tar`) |
| `tink destroy [--yes]` | Remove `.agents/skills/` and an empty `.agents/`; preserve `AGENTS.md`, `ZEN.md`, unrelated `.agents/` siblings, and the library; drop this project's catalog entry |

## On-disk contracts

| Artifact | Contract |
|---|---|
| Live skills | `<project>/.agents/skills/<name>/` with `SKILL.md` |
| Receipt | `.tink-source.json` with exactly `source`, `revision`, `path` (non-empty strings) |
| Home root | `$TINK_HOME` or `~/.tink` (relative `$TINK_HOME` absolutized against cwd), with `layout.json` (`kind`: `tink-skill-inventory`) |
| Library | `skills/<name>/` skill trees copied on successful add (rebuildable collection; identical tip may install project from library; divergent → repair + warn; project overwrite still refused; not an agent discovery root) |
| Offline catalog | `catalog/by-project/<bounded-name>-<sha256(raw-canonical-root)>/meta.json` with display `name`, `root`, raw-path `identity`, and `skills` name list; owned basename-only entries migrate on the next deposit |
| Project lock | `.tink/skills.lock` version 2; a domain-separated, length-framed SHA-256 pins path bytes, entry kind, canonical executable/non-executable mode, and contents (receipt excluded). Version 1 must be regenerated with `skill lock`. |
| Skillset receipt | `.tink-skillset.json` digest version 2 pins the same tree semantics; `skillset refresh` is the migration path for a legacy receipt. |

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
| I6 | `init` (default) | Installs `.agents/skills/manage-tink/`; catalogs `manage-tink`; copies tree into library at `skills/manage-tink/` |
| I7 | `init --no-manage-tink` | Does **not** install `manage-tink` |
| I8 | `init` with relative `TINK_HOME` (e.g. `../home`) from project cwd | Exit 0; home is the absolutized sibling path (not nested under the project); stdout shows an absolute home path |
| I9 | Run non-interactive `init` twice with an unchanged project | Second run exits 0, reports `Ready` / `Already present`, and leaves project/home contract files byte-identical |
| I10 | Run `init --with-tink-skills` against an incomplete optional bundle, repair the bundle, then rerun the same command | First run fails but preserves completed setup; second run exits 0 and converges to `manage-tink` plus both bundle skills |
| I11 | Run `init` with `TINK_HOME` pointed at the non-empty project directory | Exit ≠ 0; refuses to claim the directory and leaves the project byte-identical |
| I12 | Run `init` with a marker-only partial Tink home left by interrupted initialization | Exit 0; recreates the owned library/catalog directories and leaves a valid inventory |

### Local add

| Id | Action | Expect |
|---|---|---|
| A1 | `skill add` valid local skill dir | Installs under `.agents/skills/<name>/`; records name in catalog; copies tree into library at `$TINK_HOME/skills/<name>/` |
| A2 | `skill add` same skill again (byte-identical) | Success noop; project + library unchanged |
| A3 | `skill add` when project target exists and differs | Exit ≠ 0; "Refusing to overwrite"; project target unchanged |
| A3B | Re-add an unchanged local skill whose stale `.tink-source.json` is its only divergence | Exit 0; removes the inapplicable remote-source sidecar |
| A4 | `skill add` skill tree containing a symlink | Exit ≠ 0; refuse |
| A5 | `skill add` multi-skill source without `--skill` | Exit ≠ 0; lists choices |
| A6 | `skill add` when library has same name but different tree (project missing) | Exit 0; installs project; repairs library; warns on stderr that the home copy was updated |
| A6B | Close stderr while `skill add` emits its post-repair warning | Exit 0 after the completed project/library/catalog mutation; advisory output failure cannot create retry ambiguity |
| A7 | `skill add` skill named `by-project` | Exit ≠ 0; reserved name; no project/library write |
| A8 | `skill add` same GitHub tip already in library | Exit 0; installs project from library (no clone); stdout notes library |
| A9 | `skill add` when catalog metadata is malformed, repair the catalog, then rerun the same command | First run fails after preserving valid project/library copies; second run exits 0, catalogs the skill, and leaves those copies byte-identical |
| A10 | `skill add` from a direct symlink or a symlinked child under `skills/` | Exit ≠ 0; mentions symlink; creates no project, library, or catalog entry for that skill |
| A11 | `skill add` when the matching project target is a symlink | Exit ≠ 0; leaves the symlink untouched; creates no library or catalog entry |
| A12 | `skill add` with `TINK_HOME` pointed at an unrelated non-empty directory | Exit ≠ 0; refuses to claim the directory and leaves its existing files byte-identical |
| A13 | `skill add owner/repo` when one non-root remote skill at the same tip is already in the library | Exit 0 without cloning; installs from library and reports that source |
| A14 | `skill add` regular executable and non-executable files | Preserves executable semantics in project and library using portable `0o755`/`0o644` modes while stripping special bits and umask-only variation |
| A15 | `skill add` a tree with two distinct non-UTF-8 Unix filenames | On filesystems that admit opaque names, preserves both names and their distinct contents in project and library; macOS/APFS may reject the fixture before Tink writes |
| A16 | `skill add` a standalone-looking source with a regular or dangling `.tink-skillset.json` entry | Exit ≠ 0 before project, home, library, or catalog creation; direct the user to `tink skillset add NAME` |

### Remote add

| Id | Action | Expect |
|---|---|---|
| R1 | `skill add owner/repo --skill <name>` (public HTTPS via test redirect) | Installs skill + `.tink-source.json` with canonical `https://github.com/owner/repo.git`, full revision, relative `path` |
| R2 | `skill add` non-GitHub or non-HTTPS remote | Exit ≠ 0; refuse |
| R3 | `skill add ./missing-skill` (path-like, absent) | Exit ≠ 0; "Path does not exist"; **no** GitHub network fetch |
| R4 | `skill add /abs/missing` | Exit ≠ 0; "Path does not exist" |
| R5 | `skill add owner/repo` when `SKILL.md` is at repo root | Receipt `path` is `"."` (non-empty); `skill check` passes; `skill refresh` updates from repo root |
| R6 | `skill add owner/repo --skill <unique-name>` when the skill is nested below a nonstandard wrapper | Installs the unique recursive match; receipt records its exact repository-relative path; catalog, library, and `skill check` are valid |
| R7 | `skill add owner/repo --skill <duplicate-name>` with multiple recursive matches | Exit ≠ 0 before project/library/catalog writes; lists every matching repository-relative path |
| R8 | `skill add owner/repo --skill <repository-relative-path>` | Installs exactly that nested skill; receipt preserves the path and `skill refresh` follows it on the remote default branch |
| R9 | A matching library copy exists for one of several same-name remote skills | Name-only add still checks the repository and refuses ambiguity; the cache cannot choose a path implicitly |
| R10 | A canonical nested skill and a directory/name-mismatched `SKILL.md` declare the same name | Name selection ignores the malformed candidate and installs the one valid tree; inspection may still diagnose both |
| R11 | Root and nested skills share a name, then `--skill .` selects the listed root path | Installs the root skill and records receipt path `"."` |
| R12 | `skill add tink:embedded/manage-tink` through the free-form add boundary | Exit ≠ 0; embedded lock sources are not accepted as remote add sources |

### Skillsets

| Id | Action | Expect |
|---|---|---|
| K1 | `skillset add <name>-skillset` with `$TINK_HOME/catalog/by-skillset/<name>-skillset/meta.json` | Installs the pinned members under `.agents/skills/<name>-skillset/`, writes a mode-aware digest-version-2 receipt, validates the project, then mirrors that exact tree to `$TINK_HOME/skills/<name>-skillset/`; matching re-add is a no-op; library drift is repaired from the valid project |
| K2 | `skillset remove <name>-skillset` after K1 | Removes only the project skillset tree; preserves the shared catalog definition and home library copy; `skill remove` refuses the skillset root. Sensor: K1. |
| K3 | `skillset list [--library]` after K1 | Groups each receipt-backed project or library skillset with its member skill names without network or writes. Sensor: K1. |
| K4 | Any skillset command receives a name without `-skillset` | Exit ≠ 0; clear canonical-name error; no skillset tree written |
| K5 | `skillset add` finds an ordinary or unowned library entry at the canonical name | Exit ≠ 0 before network/project publication; preserve the library entry |
| K6 | `skillset remove` finds a missing or invalid receipt | Exit ≠ 0; preserve the complete project directory |
| K7 | `skillset list` or `add` runs before project/catalog setup | List explains how to initialize; missing catalog leaves the project untouched |
| K8 | Re-add a valid unchanged project skillset while its remote is unavailable | Succeeds offline as unchanged and synchronizes the library from the project |
| K9 | `skill check` / `skill list` with grouped members only | Check reports standalone, skillset, and member counts; list says there are no standalone skills and points to `skillset list` |
| K10 | `skillset refresh <name>-skillset` after the pinned catalog definition changes | Stages and rename-replaces the clean project tree with best-effort rollback, then mirrors the validated result to the library; refuses local project modifications |
| K11 | A declared member folder and its `SKILL.md` name differ | Exit ≠ 0 before project or library publication; explain the name mismatch |

### GitHub inspection

| Id | Action | Expect |
|---|---|---|
| G1 | `inspect` a repository URL | Reports source metadata, inferred source skillsets including empty peers, and all discovered skills in deterministic order |
| G2 | `inspect` a group tree URL | Reports only the one inferred skillset and skills beneath that boundary |
| G3 | `inspect` a skill tree URL | Reports one skill and zero skillsets |
| G4 | `inspect` a repository with one non-`skills` structural wrapper | Infers grouped skillsets without relying on a literal `skills/` directory |
| G4B | `inspect` a flat repository containing multiple root skills | Proposes one unnamed skillset without exposing the temporary checkout directory name |
| G4C | `inspect` a mixed root with one root skill and a separate `skills/` collection | Refuses to collapse unrelated levels; reports standalone skills and requests a narrower URL |
| G4D | `inspect` a repository or tree rooted at a literal `skills/` collection | Treats `skills` as a collection root rather than proposing `skills-skillset` |
| G4E | `inspect` a grouped repository whose derived canonical name would exceed the name limit | Leaves that proposal unnamed and explains that no valid canonical name is available |
| G5 | `inspect` duplicate names and invalid `SKILL.md` files | Succeeds with visible diagnostics and excludes invalid candidates from the skill count |
| G6 | `inspect` an empty valid directory | Succeeds with zero skills and a structural diagnostic |
| G7 | `inspect` unsupported URLs, missing or ambiguous slash-containing refs, and missing boundaries | Exits nonzero with actionable errors |
| G8 | `inspect` with existing project and home state | Leaves both project and `$TINK_HOME` absent or byte-for-byte unchanged |
| G9 | Send SIGTERM only to `tink inspect` while its Git subprocess is running | Exits nonzero; terminates and reaps the Git process group; no delayed child side effect |
| G10 | `inspect` a repository with control characters in a directory name | Succeeds without emitting raw terminal controls; path fields use visible backslash escapes |

### Check

| Id | Action | Expect |
|---|---|---|
| C1 | `skill check` after valid `init` + `skill add` | Exit 0 |
| C2 | `skill check` without `.agents/skills` | Exit ≠ 0 |
| C3 | `skill check` when `.agents` is a symlink | Exit ≠ 0; refuse |
| C4 | `skill check` | Performs no network I/O and no filesystem writes. Sensor: manual. |
| C5 | `skill check` after an installed skill's frontmatter name is corrupted | Exit ≠ 0; reports the skill-name mismatch |

### Project manifest

| Id | Action | Expect |
|---|---|---|
| M1 | `skill verify` with matching empty manifest and lockfile in an empty project | Exit 0; reports zero verified manifest skills |
| M2 | `skill lock --source reviewer=fixture/reviewer` after adding that local skill | Writes manifest and lockfile; subsequent `skill verify` succeeds |
| M3 | `skill sync` after deleting an installed locked local skill whose source remains | Restores the skill; subsequent `skill verify` succeeds |
| M4 | `skill sync` after deleting locked embedded `manage-tink` | Restores the embedded skill; subsequent `skill verify` succeeds |
| M5 | `skill sync` after a slash-containing local source disappears | Exit ≠ 0 as a missing local path; does not reinterpret it as GitHub shorthand |
| M6 | `skill verify` without a project manifest | Exit ≠ 0; reports the missing manifest |
| M7 | `skill sync` with a bad hash on a later locked skill | Exit ≠ 0 before any project, library, or catalog publication for earlier skills |
| M8 | `skill sync` with a symlink or unsafe library target for a later locked skill | Exit ≠ 0 before publishing any earlier project, library, or catalog entry |
| M9 | `skill verify` with a version-1 lockfile, followed by `skill lock` | Verify refuses the legacy ambiguous digest with an actionable relock instruction; lock rewrites version 2 and verify succeeds |

### List

| Id | Action | Expect |
|---|---|---|
| L1 | `skill list` after `init` | Exit 0; stdout includes `manage-tink` |
| L2 | `skill list` without `.agents/skills` | Exit ≠ 0 |
| L3 | `skill list --catalog` after init+add | Exit 0; header `project\\troot\\tskill` plus three-column TSV rows for cataloged skills |
| L4 | `skill list` when `ZEN.md` exists without `AGENTS.md` reference | Exit 0; lists skills; warns on stderr about ZEN/AGENTS; `skill check` still fails |
| L5 | `skill list --stash` or `skill list --home` | Exit ≠ 0; stderr mentions the flag or unexpected argument (removed in 0.3.0; use `--library` / `--catalog`) |
| L6 | `skill list --catalog` with valid and malformed project metadata | Exit 0; lists valid rows and omits malformed entries |
| L7 | Insert a nested symlink into an installed standalone skill, then run `skill list` and `skill check` | Both exit ≠ 0 and mention the symlink; the installed tree is untouched |
| L8 | `skill list --library` or `--catalog` with an existing non-empty unmarked `TINK_HOME` | Exit ≠ 0; refuses the unrelated directory and leaves it byte-identical |
| L9 | `skill list --library` or `--catalog` when the corresponding direct home owner (`skills/` or `catalog/`) is a symlink | Exit ≠ 0; refuses the symlink without following or replacing it |
| L10 | Two projects share a basename and use one Tink home | Both retain distinct catalog identities and appear with their own canonical root and skills |
| L11 | A project directory name begins with `.` | Its hashed catalog identity remains visible in `skill list --catalog`; hidden project names are not mistaken for staging entries |
| L12 | A cataloged project name or root contains tab, CR, LF, backslash, or another terminal control | `skill list --catalog` emits visible backslash escapes (including `\\t`, `\\r`, `\\n`, `\\\\`, and `\\x1b`); every data row remains exactly three TSV columns and contains no raw terminal controls |
| L13 | `skill list --catalog` with no catalog entries | Exit 0; emits the TSV header and no data rows |

### Library

| Id | Action | Expect |
|---|---|---|
| H1 | `skill list --library` after init+add | Exit 0; stdout includes library skill names (at least the added skill) |
| H2 | `skill add <name>` when library has that skill and project lacks it | Exit 0; installs under `.agents/skills/<name>/`; catalogs name; **no** network; stdout notes library |
| H3 | `skill add <missing-bare-name>` | Exit ≠ 0; mentions library not found / missing; **no** GitHub network fetch |
| H4 | `skill add <name>` when library has skill and project skill exists and differs | Exit ≠ 0; "Refusing to overwrite"; project target unchanged |
| H5 | `skill harvest` with fixture `$HOME/.agents/skills` + `$HOME/.claude/skills` + `TINK_HOME` | Copies complete skill trees into `$TINK_HOME/skills/`; no project `.agents` skill writes from harvest |
| H6 | `skill harvest` when library already identical | Exit 0; summary counts already present; no per-skill already-present lines |
| H7 | `skill harvest` when library diverges | Exit 0; library unchanged; stderr skip warn |
| H8 | `skill harvest` skips library sources and unsafe (symlink-inside) skill trees; still harvests cwd skills under `TINK_HOME` outside `skills/` | Library/unsafe omitted; non-library path under home deposited |
| H9 | Shell completion for `tink skill add <prefix>` | Offers matching names from the current library without creating the home or library |
| H10 | `skill list --library` when a valid library skill contains a nested symlink | Exit ≠ 0; mentions the symlink; does not advertise the unsafe skill or alter either side of the link |
| H11 | A library root contains both `SKILL.md` and `.tink-skillset.json` | `skill list --library` excludes it; `skill add <name>` exits nonzero and directs the user to `tink skillset add <name>` |
| H12 | `skill add <source>` would deposit a standalone skill over a receipt-bearing library root with the same name | Exit nonzero before project publication; preserve every existing library file and identify the standalone/skillset collision |
| H13 | `skill add <source>` exactly matches a receipt-bearing library root with the same name | Do not reuse the managed root as a standalone cache hit; exit nonzero, preserve the library bytes, and publish no project tree |
| H14 | `skill harvest` discovers a standalone-looking source with a regular or dangling `.tink-skillset.json` entry | Skip with actionable `skillset add` guidance; do not publish it into the standalone library or create project state |

### CLI surface

| Id | Action | Expect |
|---|---|---|
| V1 | `skill add` local skill | Installs under `.agents/skills/<name>/` |
| V2 | `skill check` after valid project | Exit 0; stdout includes `OK` |
| V3 | Run top-level `--help`, then a project command, after each command's current directory is removed | Help succeeds without resolving a project; the project command fails closed with a current-directory error |
| V4 | Close stdout before a successful listing command writes | Exit 0; no panic and no exit 101 |
| V5 | Close stderr while an underlying command fails | Exit 1; diagnostic delivery failure does not hide the command failure |
| V6 | Close `install.sh` stdout after a verified installation reaches advisory reporting | Exit 0 with the verified destination intact; broken advisory output cannot turn completed installation into retry ambiguity |

### Refresh

| Id | Action | Expect |
|---|---|---|
| P1 | `skill refresh` clean GitHub-imported skill after upstream change | Updates project skill + receipt **and** `$TINK_HOME/skills/<name>/` |
| P2 | `skill refresh` when installed skill has local modifications | Exit ≠ 0; mentions local modifications; tree unchanged |
| P3 | `skill refresh` when upstream unchanged but library missing | Exit 0; backfills `$TINK_HOME/skills/<name>/` from project |
| P4 | `skill refresh` when library lacks only `.tink-source.json` | Exit 0; updates project + library |
| P5 | `skill refresh` when library body diverges from project | Exit ≠ 0; "library diverges"; project unchanged |
| P6 | `skill refresh` when upstream revision moves but skill tree bytes match | Exit 0; bumps project + library receipts |
| P7 | `skill refresh` when project already at HEAD but library is stale | Exit 0; repairs library from project |
| P8 | `skill refresh` for all when a later imported skill has local modifications | Exit ≠ 0; no project skill is updated |

### Remove

| Id | Action | Expect |
|---|---|---|
| X1 | `skill remove <name>` after init+add | Exit 0; project `.agents/skills/<name>/` gone; `skill list` omits name; library `$TINK_HOME/skills/<name>/` still present; `skill list --catalog` omits that skill for the project (siblings may remain) |
| X2 | `skill remove <missing>` | Exit ≠ 0; mentions not found / missing; nothing deleted |
| X3 | `skill remove` when `.agents` is a symlink | Exit ≠ 0; mentions symlink; tree unchanged |
| X4 | Successful `skill remove <name>` | Does **not** delete `$TINK_HOME/skills/<name>/` |
| X5 | `init` installs `manage-tink` | Embedded skill covers standalone skill lifecycle, manifests, library promotion/harvest, read-only GitHub inspection, canonical nested skillsets, project-first library authority, catalog effects, CLI update, and destroy; removals preserve library state |
| X6 | `skill remove <name>` when that project's catalog metadata is malformed | Exit ≠ 0; project and library skill trees remain intact |
| X7 | `skill remove <name>` when `$TINK_HOME/catalog` is a symlink | Exit ≠ 0; mentions the symlink; project skill and external catalog target remain byte-identical |

### Destroy

| Id | Action | Expect |
|---|---|---|
| D1 | `destroy --yes` after `init --with-zen` (extra skill allowed) | Removes `.agents/skills/` and the now-empty `.agents/`; preserves `ZEN.md` and `AGENTS.md` byte-for-byte; leaves library + `layout.json` intact; drops this project's catalog entry |
| D2 | `destroy` without `--yes` (non-TTY) | Exit ≠ 0; refuses without confirmation; project files unchanged |
| D3 | `destroy --yes` when `.agents` is a symlink | Exit ≠ 0; mentions symlink |
| D4 | `destroy --yes` when `$TINK_HOME/catalog` is a symlink | Exit ≠ 0; mentions the symlink; project scaffolding and external catalog target remain byte-identical |
| D5 | `destroy --yes` when `.agents/` contains an unrelated sibling | Removes `.agents/skills/`, preserves the sibling byte-for-byte, and leaves `.agents/` in place |

### Update (CLI binary)

| Id | Action | Expect |
|---|---|---|
| U1 | `update` when releases API is unreachable | Exit ≠ 0; clear download/metadata failure; binary unchanged |
| U2 | `update` when latest release version matches this binary | Exit 0; stdout notes up to date; binary unchanged |
| U3 | `update` when a newer release asset exists for this host | Exit 0; replaces the running binary; stdout notes updated version |
| U4 | `update` receives a valid-digest archive whose payload fails the exact version probe | Exit ≠ 0; running binary remains byte-identical; no success output |
| U5 | `update` metadata names an older semantic version | Exit ≠ 0; refuses downgrade before publication; running binary remains unchanged |
| U6 | `install.sh` receives an invalid or non-executable verified payload while a binary exists | Exit ≠ 0; existing binary remains byte-identical; no success output |
| U7 | `install.sh` receives a valid verified payload | Publishes with atomic replace semantics, probes the published path, exits 0, and prints the exact installed version |
| U8 | `install.sh` receives malformed JSON or a non-SemVer tag | Exit ≠ 0 with a concise metadata error and no Python traceback; existing binary remains unchanged |
| U9 | `install.sh` receives a credential-bearing or query-bearing release API URL | Exit ≠ 0 before network/filesystem mutation; stderr does not disclose credentials or query secrets |
| U10 | `install.sh` receives a verified candidate whose version probe hangs | Terminates within the bounded probe budget, exits nonzero, and preserves the existing binary |
| U11 | `install.sh` metadata uses non-ASCII digits in the semantic-version core | Exit ≠ 0 with a clean semantic-version error; existing binary remains unchanged |
| U12 | `install.sh` candidate exits but leaves a descendant holding probe pipes | The five-second bound kills the process group, reaps the direct child, leaves no descendant side effect, and preserves the existing binary |
| U13 | `install.sh` candidate passes staging probes but fails only at the published path | Exit ≠ 0; restore the prior binary bytes and executable mode; print no install success |
| U14 | Interrupt `install.sh` while its release candidate is running | Exit ≠ 0 without a Python traceback; terminate the candidate process group; preserve the prior binary |
| U15 | Interrupt `tink update` while its release candidate is running | Exit ≠ 0; terminate the candidate process group; preserve the running binary |

### Safety (cross-cutting)

| Id | Action | Expect |
|---|---|---|
| S1 | Any successful command | Does not `git init`, stage, commit, or push. Sensor: S1 (partial: `init` only). |
| S2 | Home root | Never treated as an agent discovery root. Sensor: manual. |
| S3 | `skill remove` or `destroy --yes` when neither `TINK_HOME` nor `HOME` can resolve an inventory | Exit 0; completes local cleanup without creating or mutating inventory state |

## Proof

```console
cargo fmt --all -- --check
cargo check --workspace --all-targets --locked
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --all-targets --locked
cargo test --workspace --locked --doc
cargo build --workspace --release --locked
cargo audit --file Cargo.lock
```

A row is done only when its named automated sensor passes. Explicit manual or partial
markers disclose remaining proof gaps. Passing tests prove only what they assert.
