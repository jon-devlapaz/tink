# Acceptance boundary (v1)

**Outcome:** A smaller Tink core in Rust that installs complete Agent Skills into
a project's `.agents/skills/`, proves them offline, refreshes clean GitHub
imports, and ensures a home root at `~/.tink` (override: `TINK_HOME`).

**Authority:** This file is the evaluator. Implementation stops when every row
below has an automated test that passes on macOS with `git` on `PATH`.

**Out of v1:** `wipe`, weekly GitHub update workflows, self-update, private
GitHub auth, Windows, Linux CI as a gate.

**Dogfood:** Prefer an installed `tink` from this repo, or `cargo run -q -- …`.

## Commands

| Command | Meaning |
|---|---|
| `tink init` | Create `.agents/skills/`; optional ZEN + Twotink prompts; ensure `~/.tink` |
| `tink add <source> [--skill <name>]` | Install one local or public GitHub skill |
| `tink check` | Validate project skills; no network; no writes |
| `tink refresh [name]` | Refresh clean GitHub imports; refuse local edits |

## On-disk contracts

| Artifact | Contract |
|---|---|
| Live skills | `<project>/.agents/skills/<name>/` with `SKILL.md` |
| Receipt | `.tink-source.json` with exactly `source`, `revision`, `path` (non-empty strings) |
| Home root | `$TINK_HOME` or `~/.tink`, with `layout.json` (`kind`: `tink-skill-inventory`) |

## Rows

Ids are stable. Tests must name or comment the id they prove.

### Bootstrap

| Id | Action | Expect |
|---|---|---|
| I1 | `init` in empty project | Creates `.agents/skills/` as real directories (not symlinks) |
| I2 | `init` when `.agents` is a symlink | Exit ≠ 0; mentions symlink; creates nothing unsafe |
| I3 | `init` (non-interactive / `--no-zen --no-twotink`) | Does **not** write `AGENTS.md`, `ZEN.md`, or `.github/workflows/*` |
| I4 | `init` with `TINK_HOME` set | Creates home root + `layout.json` |
| I5 | `init --with-zen` | Writes `ZEN.md` and an `AGENTS.md` that references it |

### Local add

| Id | Action | Expect |
|---|---|---|
| A1 | `add` valid local skill dir | Installs under `.agents/skills/<name>/` only (no home skill copy) |
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
| P1 | `refresh` clean GitHub-imported skill after upstream change | Updates skill + receipt |
| P2 | `refresh` when installed skill has local modifications | Exit ≠ 0; mentions local modifications; tree unchanged |

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
