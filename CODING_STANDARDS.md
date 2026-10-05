# Coding standards

Read during review. A reviewer applies every rule to the diff and reports each violation with file and line. Mechanical rules live in CI (`cargo fmt`, `clippy -D warnings`, tests), not here.

Disclosed standards, read when the diff touches the named area:

- Anything that creates, replaces, or removes files (skills, skillsets, receipts, manifests, library, binary): [`docs/standards/filesystem-mutations.md`](docs/standards/filesystem-mutations.md)

## Structure

**One owner.** Each act and fact has one owning module. Count the edit sites for a plausible change; more than one is a finding. Entry points that install from the library share one placement function. The library write functions (`deposit_at`, `deposit_create_only_at`, `deposit_refresh_at`, `sync_from_installed_at`) move toward one write policy; a new promote path does not add another.

**Typed origin.** A closed enum (such as `AddOrigin { Source, Library }`) replaces mutually exclusive booleans, and every `match` over it is exhaustive. Reports and exit messages read the enum.

**Whole rename.** A rename lands in one change across module names, APIs, user strings, and docs. Two words for one thing block approval (the retired `stash` beside `library`); keep the on-disk layout stable and rename the code.

**Distinct outcomes.** Each observable result has its own variant. A repaired install, an unchanged no-op, and a created install print three different reports and carry three different values.

**Shared predicate.** One function decides "hidden entry", "receipt-backed", and "canonical name". Every scan of the same tree calls it, so `skill check`, `skill list`, and `load_project_skills` agree.

**Scoped gate.** A drift or currentness check runs in the commands that name it (`check`, `lock`). Placing it in the shared loader blocks unrelated commands.

**Round trip.** Every file tink writes is read back by tink. `lock` output passes `verify` and `sync`; an empty project emits `skills = []`; a pinned source restores the locked bytes, not the current binary's embedded copy.

**Idempotent publish.** CI release steps rerun cleanly: a completed older tag succeeds before any monotonic-version guard fires, and digest comparison uses artifacts that are stable across runs.

**Honest copy.** README, SKILL.md, ACCEPTANCE.md, and help text state what the code does. When a contract changes, search for each restatement (flag names, the manage-tink workflow, acceptance rows) and fix them together. An acceptance row stays `partial` until its sensor compares content before and after, not mere existence.

**Earned layer.** Add a wrapper, trait, or module split for a second caller.

## Judgement over the diff

Prefer deleting a layer to rearranging it. The question for each finding: "what is the smallest change that makes this concept disappear?"
