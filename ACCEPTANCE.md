# Acceptance boundary (v1)

**Outcome:** A smaller Tink core in Rust that installs complete Agent Skills into
a project's `.agents/skills/`, proves them offline, refreshes clean GitHub
imports, and maintains a home inventory at `~/.tink` (override: `TINK_HOME`).

**Authority:** This file is the evaluator. Implementation stops when every row
below has an automated test that passes on macOS with `git` on `PATH`.

**Out of v1:** `wipe`, setup option bundles (ZEN/Twotink/workflows/manage-tink),
self-update, private GitHub auth, Windows, Linux CI as a gate.

**Dogfood:** Prefer an installed `tink` from this repo, or `cargo run -q -- …`.
If an older Python `tink` is still on `PATH`, put this binary ahead of it.

## Commands

| Command | Meaning |
|---|---|
| `tink init` | Create `.agents/skills/` only; ensure inventory root exists |
| `tink add <source> [--skill <name>]` | Install one local or public GitHub skill |
| `tink check` | Validate project skills; no network; no writes |
| `tink refresh [name]` | Refresh clean GitHub imports; refuse local edits |
| `tink inventory list` | List inventory skills for this project |

## On-disk contracts (compatible with Python tink-agents)

| Artifact | Contract |
|---|---|
| Live skills | `<project>/.agents/skills/<name>/` with `SKILL.md` |
| Receipt | `.tink-source.json` with exactly `source`, `revision`, `path` (non-empty strings) |
| Inventory root | `$TINK_HOME` or `~/.tink`, with `layout.json` (`kind`: `tink-skill-inventory`) and `skills/by-project/` |
| Deposit | On successful `add` / `refresh`: copy into `skills/by-project/<project>/skills/<name>/` |

## Rows

Ids are stable. Tests must name or comment the id they prove.

### Bootstrap

| Id | Action | Expect |
|---|---|---|
| I1 | `init` in empty project | Creates `.agents/skills/` as real directories (not symlinks) |
| I2 | `init` when `.agents` is a symlink | Exit ≠ 0; mentions symlink; creates nothing unsafe |
| I3 | `init` | Does **not** write `AGENTS.md`, `ZEN.md`, or `.github/workflows/*` |
| I4 | `init` with `TINK_HOME` set | Creates inventory root + `layout.json` + `skills/by-project/` |

### Local add

| Id | Action | Expect |
|---|---|---|
| A1 | `add` valid local skill dir | Installs under `.agents/skills/<name>/`; deposits inventory copy |
| A2 | `add` same skill again (byte-identical) | Success noop; tree unchanged |
| A3 | `add` when target exists and differs | Exit ≠ 0; "Refusing to overwrite"; target unchanged |
| A4 | `add` skill tree containing a symlink | Exit ≠ 0; refuse |
| A5 | `add` multi-skill source without `--skill` | Exit ≠ 0; lists choices |

### Remote add

| Id | Action | Expect |
|---|---|---|
| R1 | `add owner/repo --skill <name>` (public HTTPS via test redirect) | Installs skill + `.tink-source.json` with canonical `https://github.com/owner/repo.git`, full revision, relative `path` |
| R2 | `add` non-GitHub or non-HTTPS remote | Exit ≠ 0; refuse |
| R3 | `add ./missing-skill` (path-like, absent) | Exit ≠ 0; "Path does not exist"; **no** GitHub network fetch |
| R4 | `add /abs/missing` | Exit ≠ 0; "Path does not exist" |

### Check

| Id | Action | Expect |
|---|---|---|
| C1 | `check` after valid `init` + `add` | Exit 0 |
| C2 | `check` without `.agents/skills` | Exit ≠ 0 |
| C3 | `check` when `.agents` is a symlink | Exit ≠ 0; refuse |
| C4 | `check` | Performs no network I/O and no filesystem writes (manual / future instrumentation; not automated in v1 harness) |

### Refresh

| Id | Action | Expect |
|---|---|---|
| P1 | `refresh` clean GitHub-imported skill after upstream change | Updates skill + receipt; deposits inventory |
| P2 | `refresh` when installed skill has local modifications | Exit ≠ 0; mentions local modifications; tree unchanged |

### Inventory

| Id | Action | Expect |
|---|---|---|
| V1 | `inventory list` with no deposits | Exit 0; empty listing for this project |
| V2 | `inventory list` after `add` | Lists the deposited skill name for this project partition |

### Safety (cross-cutting)

| Id | Action | Expect |
|---|---|---|
| S1 | Any successful command | Does not `git init`, stage, commit, or push |
| S2 | Inventory paths | Never treated as agent discovery roots in docs or layout README |

## Proof

```console
cargo test
```

A row is done only when its automated test passes. Passing tests prove only what
they assert.
