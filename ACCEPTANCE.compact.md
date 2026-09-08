# Acceptance boundary — compact (v1)

> Derivative of `ACCEPTANCE.md`. **Not authoritative.** The 400-line
> `ACCEPTANCE.md` is the evaluator; row Ids are stable there and tests must
> name the Id they prove. Use this file for orientation only.

## Core model

- Rust CLI manages standalone skills + pinned skillsets under `<project>/.agents/skills/`; validates offline; manifest + lockfile record and restore standalone intent; reusable home inventory at `~/.tink` (`TINK_HOME` override); read-only GitHub inspection; scoped removal; verified binary updates from GitHub Releases.
- Gate: PRs/`main` run pinned Rust 1.95 fmt, check, Clippy `-D warnings`, docs, audit, test, release build. Native macOS + Linux, x86_64 + arm64. Tag releases repeat the gate before building all four artifacts.
- Out of v1: weekly update workflows, private GitHub auth, Windows, library pruning, concurrent mutations in one project/home, cross-filesystem rollback after unexpected I/O failure. Recovery model: rerun the idempotent command.
- Process: stdout = data/summaries; stderr = warnings/errors. Exits: 1 = failure, 2 = Clap usage. Closed stdout = normal exit 0, no panic.
- Verbs: mutations under `tink skill` / `tink skillset`, plus top-level `tink update`. `tink library list` is canonical read-only; `tink skill list --library` is a compatibility alias. No top-level `add`/`check`/`refresh`.

## Commands (condensed)

- `init`: create `.agents/skills/`, write `AGENTS.md` if missing, install `manage-tink` by default, ensure `~/.tink`.
- `skill add <source> [--skill <name-or-path>]`: one local path, public GitHub skill, GitHub tree URL, or library name. Remote selectors may be unique names or repo-relative paths.
- `skill list` / `--catalog` / `--library`: project skills (read-only); offline by-project TSV (`project\troot\tskill`); library names.
- `skill read <name> [--library] [--raw]`: description + lifecycle metadata.
- `skill harvest`: harness roots into library (create-only, no project writes).
- `skill promote <name> [--replace]`: validated project skill to library as receipt-free payload; divergence requires `--replace`.
- `skill check`: validate project, no network, no writes.
- `skill lock [--source <name=source>]` / `sync` / `verify`: record, restore, verify manifest (`.tink/skills.toml` v1) + lockfile (`.tink/skills.lock` v2).
- `skill refresh [name]` / `--dry-run` / `outdated` / `rollback`: clean GitHub imports only; refuse local edits; preview; staleness; one-generation undo.
- `skill remove <name>`: delete project skill + drop catalog name (not library).
- `skillset add <url> [name]` / `add <name>-skillset`: from tree URL (create-only definition) or pinned catalog definition; nested project tree + baseline router + library mirror.
- `skillset list [--library]` / `refresh` / `update` / `remove`: grouped read-only view; clean replace; advance pinned revision; delete project tree only (definition + library preserved).
- `inspect <GITHUB_URL>`: skills + inferred skillsets, no project/home writes.
- `update`: replace binary with newer verified release (`curl` + `tar`).
- `destroy [--yes]`: remove `.agents/skills/` (+ empty `.agents/`); preserve `AGENTS.md`, unrelated siblings, library; drop catalog entry.

## On-disk (condensed)

- Live skills: `<project>/.agents/skills/<name>/` with `SKILL.md`.
- Live skillsets: `<project>/.agents/skills/<name>-skillset/<member>/`, one `SKILL.md` per explicit member.
- Receipt `.tink-source.json`: exactly `source`, `revision`, `path` (non-empty strings).
- Home: `$TINK_HOME` or `~/.tink` (relative absolutized vs cwd) with `layout.json` (`kind: tink-skill-inventory`).
- Library `skills/<name>/`: rebuildable copies on successful add; identical tip may seed project; divergence repairs + warns; project overwrite still refused; never a discovery root.
- Catalog `catalog/by-project/<bounded-name>-<sha256(raw-root)>/meta.json`: display `name`, `root`, raw `identity`, `skills` list.
- Skillset definition `catalog/by-skillset/<name>-skillset/meta.json`: create-only, `source`, immutable `revision`, repo-relative `sourceRoot`, explicit `members`.
- Manifest `.tink/skills.toml` v1: `name`, typed `source`, optional rel `path`.
- Lock `.tink/skills.lock` v2: domain-separated length-framed SHA-256 over path bytes, entry kind, canonical 0o755/0o644 mode, contents (receipt excluded). v1 must be relocked.
- Skillset receipt `.tink-skillset.json`: digest v2, same tree semantics; legacy migrates only via clean `skillset refresh`.

## Legend

- `OK` = exit 0; `FAIL` = exit nonzero (usually 1; promote-conflict = 3; Clap usage = 2). `unchanged` = relevant trees byte-identical (plus modes where stated). Unless noted, FAIL leaves named trees unchanged; symlink / special-file refusals never follow or replace the link.

## Rows

### Bootstrap

- I1 init empty: creates `.agents/skills/` as real dirs, not symlinks.
- I2 `.agents` symlink: FAIL mentions symlink, nothing unsafe created.
- I3 non-interactive / `--no-tink-skills`: no `ZEN.md`, no `.github/workflows/*` (manage-tink + AGENTS.md allowed).
- I4 `TINK_HOME` set: creates home root + `layout.json` + `catalog/by-project/` + `skills/`.
- I5 `AGENTS.md` absent: writes Tink-manages-skills note; later init leaves existing file byte-identical.
- I6 default: installs + catalogs `manage-tink`, copies to library.
- I7 `--no-manage-tink`: no manage-tink.
- I8 relative `TINK_HOME` (e.g. `../home`): OK, absolutized sibling home, stdout shows absolute path.
- I9 init twice unchanged: 2nd OK (`Ready`/`Already present`), files identical.
- I10 `--with-tink-skills` vs incomplete bundle: 1st FAILS preserving setup; after repair 2nd OK with manage-tink + skill-scout + triangulate-me.
- I11 `TINK_HOME` = non-empty project dir: FAIL, project identical.
- I12 marker-only partial home: OK, rebuilds library/catalog, valid inventory.

### Local add

- A1 valid local dir: installs, catalogs, copies to library.
- A2 identical re-add: OK noop, project + library unchanged.
- A3 target exists and differs: FAIL "Refusing to overwrite", target unchanged.
- A3B only divergence is stale `.tink-source.json`: OK, removes sidecar.
- A4 tree has symlink: FAIL.
- A5 multi-skill source without `--skill`: FAIL, lists choices.
- A6 library same name, different tree, project missing: OK, installs project, repairs library, stderr warn.
- A6B stderr closed during A6 warn: OK, mutation complete, no retry ambiguity.
- A7 name `by-project`: FAIL reserved, no writes.
- A8 same GitHub tip in library: OK from library, no clone, stdout notes it.
- A9 malformed catalog: 1st FAILS preserving project/library; after repair 2nd OK, catalogs, copies identical.
- A10 direct symlink / symlinked child: FAIL mentions symlink, no project/library/catalog entry.
- A11 project target is symlink: FAIL, symlink untouched, no library/catalog.
- A12 `TINK_HOME` = unrelated non-empty dir: FAIL, dir identical.
- A13 `owner/repo` with one non-root remote skill at same tip in library: OK no clone, installs from library.
- A14 exec / non-exec files: portable 0o755/0o644 in project + library, strips special/umask-only bits.
- A15 two distinct non-UTF-8 filenames: preserves both names + contents where FS admits (macOS/APFS may reject fixture first).
- A16 standalone-looking source with regular/dangling `.tink-skillset.json`: FAIL before any writes; point to `skillset add NAME`.

### Promotion

- H15 promote valid standalone: receipt-free library payload + digest, preserves modes; later `skill add NAME` uses managed path.
- H16 library differs: exit 3 with both digests + `--replace` opt-in; no changes; `--replace` updates only that library entry.
- H17 malformed project receipt: FAIL before staging/publication; project + library unchanged.

### Remote add

- R1 `owner/repo --skill <name>` (HTTPS): installs + receipt (canonical `https://github.com/owner/repo.git`, full rev, rel path).
- R2 non-GitHub / non-HTTPS: FAIL.
- R3 `./missing-skill` absent: FAIL "Path does not exist", no network fetch.
- R4 `/abs/missing`: FAIL "Path does not exist".
- R5 `SKILL.md` at repo root: receipt path `"."`, check passes, refresh tracks root.
- R6 unique nested match under wrapper: installs match, receipt records exact rel path; catalog/library/check valid.
- R7 duplicate name matches: FAIL before writes, lists all rel paths.
- R8 `--skill <rel-path>`: installs exactly that dir; refresh follows default branch there.
- R9 cache holds one of several same-name remotes: name-only add still checks repo, refuses ambiguity.
- R10 canonical + malformed same-name candidate: installs valid tree only.
- R11 root + nested share name, `--skill .`: installs root, path `"."`.
- R12 `tink:embedded/manage-tink` as add source: FAIL, embedded not accepted.
- R13 tree URL that is a skill: installs dir; receipt source = canonical repo URL, path = tree path; refresh follows default branch.
- R14 tree URL not a skill: FAIL before writes, lists skill paths below.
- R15 ref contains `/`: FAIL ambiguous URL, no writes.

### Skillsets

- K1 add pinned definition: installs members under `.agents/skills/<name>-skillset/`, digest-v2 receipt, validates, mirrors exact tree to library; re-add noop; library drift repaired from project.
- K1B root router added later: ignored by digest, check clean, re-add mirrors router, refresh preserves router while updating members.
- K2 remove after K1: deletes project tree only; keeps definition + library; `skill remove` refuses skillset root. Sensor: K1.
- K3 list after K1: groups receipt-backed skillsets + members, no network/writes. Sensor: K1.
- K3B two skillsets, drift one: list exits 0 showing both; check reports valid counts then exits ≠ 0; clean refresh still works.
- K4 invalid skillset name: FAIL, no tree written.
- K5 ordinary/unowned library entry at canonical name: FAIL before network/project publication, library preserved.
- K6 remove with missing/invalid receipt: FAIL, project dir preserved.
- K7 before project/catalog setup: list explains init; missing catalog leaves project untouched.
- K8 re-add unchanged while remote down: OK offline, syncs library from project.
- K9 only grouped members: check reports standalone + skillset + member counts; list says no standalone, points to `skillset list`.
- K10 refresh after definition change: stages + rename-replaces clean tree (best-effort rollback), mirrors to library; refuses local mods.
- K11 member folder vs `SKILL.md` name differ: FAIL before publication.
- K12 add `<url> [name]`: create-only `meta.json` with immutable SHA; discovery skips non-skills; aborts on corrupt frontmatter; baseline router generated; mirrors to library; re-add = Unchanged; `inspect` preview is read-only.
- K13 update: queries upstream, advances `meta.json` to new tip SHA, discovers members, preserves router, replaces project tree, mirrors to library; Unchanged if at tip; refuses on local mods.

### Inspection (`inspect`, read-only, no project/home writes)

- G1 repo URL: source metadata + inferred skillsets (incl. empty peers) + all skills, deterministic order.
- G2 group tree URL: only that skillset + skills beneath.
- G3 skill tree URL: one skill, zero skillsets.
- G4 one non-`skills` wrapper: infers groups without literal `skills/` dir.
- G4B flat repo, multiple root skills: one unnamed skillset, no temp-dir leak.
- G4C mixed root (root skill + `skills/` collection): no collapse; reports standalones, asks narrower URL.
- G4D rooted at literal `skills/`: collection root, not `skills-skillset`.
- G4E canonical name over limit: unnamed proposal + explanation.
- G5 dup names / invalid `SKILL.md`: OK with diagnostics, invalid excluded.
- G6 empty valid dir: OK, zero skills + structural diagnostic.
- G7 unsupported URLs / missing / ambiguous slash-refs / missing boundaries: FAIL actionable.
- G8 with existing state: project + `TINK_HOME` absent-or-identical.
- G9 SIGTERM during git: FAIL, kills/reaps git process group, no delayed side effects.
- G10 control chars in dirname: OK, no raw controls emitted, backslash escapes.

### Check

- C1 valid init + add: OK.
- C2 without `.agents/skills`: FAIL.
- C3 `.agents` symlink: FAIL.
- C4: project + home identical incl. modes; needs no external commands.
- C5 corrupted frontmatter name: FAIL reports mismatch.
- C6 lost frontmatter: FAIL requires YAML frontmatter.
- C7 unclosed frontmatter: FAIL not closed.
- C8 embedded manage-tink drifted: check/lock FAIL with drift + exact `skill refresh manage-tink` repair; no lockfiles written.

### Manifest (`.tink/skills.toml` + `.tink/skills.lock`)

- M1 verify empty manifest + lock in empty project: OK, zero verified.
- M2 lock `--source reviewer=fixture/reviewer`: writes both; verify OK.
- M3 sync after deleting locked local skill: restores; verify OK.
- M4 sync after deleting locked embedded manage-tink: restores; verify OK.
- M5 sync after slash-containing local source disappears: FAIL missing path, no GitHub-shorthand reinterpretation.
- M6 verify without manifest: FAIL missing manifest.
- M7 bad hash on later skill: FAIL before publishing any earlier skill.
- M8 symlink/unsafe library target for later skill: FAIL before publishing any earlier entry.
- M9 v1 lockfile: verify refuses legacy digest with relock instruction; lock rewrites v2, verify OK.

### List

- L1 after init: OK includes manage-tink.
- L2 without `.agents/skills`: FAIL.
- L3 `--catalog` after init + add: OK, `project\troot\tskill` header + 3-col TSV.
- L5 `--stash` / `--home`: FAIL, stderr names flag (removed 0.3.0; use `--library` / `--catalog`).
- L6 `--catalog` with valid + malformed metadata: OK, valid rows only.
- L7 nested symlink in standalone: list + check FAIL mention symlink, tree untouched.
- L8 `--library` / `--catalog` with non-empty unmarked home: FAIL, dir identical.
- L9 home owner (`skills/` or `catalog/`) is symlink: FAIL, no follow/replace.
- L10 two projects, same basename, one home: distinct identities, own roots + skills.
- L11 project name starts with `.`: hashed identity still visible; not a staging entry.
- L12 catalog name/root has tab/CR/LF/backslash/control: visible escapes (`\t` `\r` `\n` `\\` `\x1b`), 3 TSV cols, no raw controls.
- L13 `--catalog` empty: OK header only.
- L14 `library list` vs `skill list --library`: identical stdout + stderr.

### Read

- RD1 read name: OK, name + description + `.agents/skills/<name>` + `Kind: standalone (local)`.
- RD2 `--raw`: OK, exactly description line + newline.
- RD3 manage-tink: OK `Kind: embedded`.
- RD4 valid `.tink-source.json`: OK `Kind: standalone (remote)` + Source / Revision / Source Path.
- RD5 `--library`: OK, description matches, path is library tree.
- RD6 missing: FAIL not found; if library-only, mentions `--library`.
- RD7 lost frontmatter: FAIL requires frontmatter.
- RD8 read `<name>-skillset` with receipt: FAIL mentions skillset + `skillset list`, no member description.
- RD9 `--library` with skillset receipt: FAIL, directs to `skillset add <name>`.
- RD10 nested symlink: FAIL mentions symlink, tree untouched.
- RD11: project + home identical incl. modes; needs no external commands.
- RD12 stdout closed before write: OK, no panic / exit 101.
- RD13 without `.agents/skills`: FAIL.
- RD14 name only nested under skillset: FAIL not found.

### Library

- H1 after init + add: OK includes added skill.
- H2 add `<name>` library-hit, project missing: OK installs + catalogs, no network, stdout notes library.
- H3 missing bare name: FAIL library-not-found, no network.
- H4 library-hit but project exists and differs: FAIL "Refusing to overwrite", target unchanged.
- H5 harvest (`~/.agents/skills` + `~/.claude/skills` + home): copies complete trees to library; no project writes.
- H6 harvest identical: OK, summary counts present, no per-skill lines.
- H7 harvest diverges: OK, library unchanged, stderr skip warn.
- H8 harvest skips library sources + symlink-inside trees; still harvests cwd skills under home outside `skills/`.
- H9 completion `skill add <prefix>`: offers library matches, creates no home/library.
- H10 `--library` with nested symlink: FAIL mentions symlink, advertises nothing, alters neither side.
- H11 library root has `SKILL.md` + receipt: list excludes it; add FAILs toward `skillset add <name>`.
- H12 add would deposit standalone over receipt-bearing library root: FAIL before project publication, library preserved, names collision.
- H13 add exactly matches receipt-bearing root: no standalone cache-hit reuse; FAIL, library identical, no project tree.
- H14 harvest finds standalone-looking source with regular/dangling receipt: skip + skillset guidance, no publication.

### CLI surface

- V1 add local: installs under `.agents/skills/<name>/`.
- V2 check valid: OK includes `OK`.
- V3 `--help` with cwd deleted: OK without resolving project; project command fails closed with cwd error.
- V4 stdout closed before listing write: OK, no panic/101.
- V5 stderr closed while command fails: exit 1 preserved.
- V6 `install.sh` stdout closed after verified install: OK, destination intact.
- V7 catalog failure from control-char path: FAIL, stderr escaped, one row.

### Refresh / outdated / preview / rollback / doctor

- P1 clean import, upstream changed: updates project + receipt and library.
- P2 local mods: FAIL mentions mods, tree unchanged.
- P3 upstream unchanged, library missing: OK backfills library from project.
- P4 library lacks only receipt: OK updates project + library.
- P5 library body diverges: FAIL "library diverges", project unchanged.
- P6 rev moves, tree bytes same: OK bumps receipts.
- P7 project at HEAD, library stale: OK repairs library from project.
- P8 refresh-all with later skill modified: FAIL, no skill updated.
- P9 refresh manage-tink missing: installs active binary copy, reconciles, check passes.
- P10 already matches: OK Unchanged, reconciles, tree identical.
- P11 receipt-free reserved copy differs: atomically replaces with active copy, reconciles, check passes.
- P12 same-named skill has remote provenance: FAIL collision, user tree identical.
- P13 project missing + library same-name remote provenance: FAIL before publication; project missing; library + receipt identical.
- P14 receipt-free/stale project copy + library remote provenance: FAIL before publication; all trees + receipts identical.
- O1 upstream moved: names skill behind; tree + receipt identical after.
- O2 caught up: `Current (no stale imports)`.
- O3 local mods: names skill modified.
- O4 rev moved, tree same: behind + `(tree unchanged)`.
- Y1 `--dry-run NAME` with change: lists added/modified files; tree + receipt identical.
- Y2 dry-run then real refresh: preview names files, refresh applies them.
- Y3 all current: `Unchanged (nothing would update)`.
- B1 refresh then rollback: restores pre-refresh bytes; 2nd rollback = no snapshot.
- B2 rollback after post-refresh edits: FAIL names change, tree untouched.
- B3 no snapshot: FAIL clean error.
- B4 two refreshes then rollback: restores 2nd pre-image (one generation).
- T1 doctor healthy: OK names git, home, skills rows.
- T2 invalid tree: FAIL names failing skills probe.
- T3 reachable remote: reports network reachable.

### Remove / destroy

- X1 remove after init + add: OK; project dir gone; list omits; library kept; `--catalog` omits for project.
- X2 missing: FAIL not-found; nothing deleted.
- X3 `.agents` symlink: FAIL mentions symlink; tree unchanged.
- X4 successful remove: never deletes library copy.
- X5 manage-tink content: covers lifecycle, proof/partial-state reporting, lock sources, session-vs-persistent completion, refresh warning, library/catalog effects, skillsets, update, destroy.
- X6 malformed catalog metadata: FAIL; project + library intact.
- X7 catalog symlink: FAIL mentions symlink; project + external target identical.
- D1 `destroy --yes` after init: removes `.agents/skills/` + empty `.agents/`; preserves `AGENTS.md`; library + `layout.json` intact; drops catalog entry.
- D2 no `--yes` non-TTY: FAIL, files unchanged.
- D3 `.agents` symlink: FAIL mentions symlink.
- D4 catalog symlink: FAIL mentions symlink; scaffolding + target identical.
- D5 unrelated sibling in `.agents/`: removes `skills/`, preserves sibling, keeps `.agents/`.

### Update (binary) / install.sh

- U1 releases API unreachable: FAIL, binary unchanged.
- U2 latest == current: OK up-to-date, binary unchanged.
- U3 newer host asset: OK replaces binary, notes version + `skill refresh manage-tink` next step; no project mutation.
- U4 valid-digest archive fails version probe: FAIL, binary identical.
- U5 older SemVer metadata: FAIL downgrade refused.
- U6 invalid/non-executable payload, binary exists: FAIL, existing identical.
- U7 valid payload: atomic replace, probes published path, OK + exact version.
- U8 malformed JSON / non-SemVer tag: FAIL concise error, no traceback.
- U9 credential/query-bearing API URL: FAIL before mutation; stderr hides secrets.
- U10 probe hangs: bounded kill, FAIL, binary preserved.
- U11 non-ASCII digits in SemVer core: FAIL SemVer error.
- U12 descendant holds pipes: 5s bound kills group, reaps child, no side effect.
- U13 passes staging, fails at published path: FAIL, restores prior bytes + mode, no success.
- U14 interrupt install.sh during candidate: FAIL no traceback, kills group, prior binary preserved.
- U15 interrupt `tink update` during candidate: FAIL, kills group, running binary preserved.
- U16 control-char path, up-to-date: OK, stdout escaped, one stable row.
- U17 probe output over capture limit: reject before publication in budget; at most 16 MiB/stream retained.
- U18 signals supervisor while spawning: FAIL no traceback, kills/reaps group.
- U19 SHA-256 algo/hex in mixed/upper case: update + install.sh accept, verify exact bytes, publish.
- U20 non-ASCII confusable in algo name: both reject, binaries identical.

### Safety

- S1 representative init/remote-add: central git boundary refuses root init/add/commit/push; no project git state created.
- S2 skill only in library: project list/check ignore it; explicit add promotes, then both observe it.
- S3 remove/destroy with unresolvable `TINK_HOME`/`HOME`: OK local cleanup, no inventory mutation.

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

Row done only when its named sensor passes. Manual/partial markers disclose
gaps. Passing tests prove only what they assert.
