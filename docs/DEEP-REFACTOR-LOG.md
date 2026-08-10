# Deep Refactor Log

This is the canonical evidence ledger for `deep-refactor-system-design-improvement`.
Its north star is: **make the next correct change clear and safe for someone new
to the codebase.**

Use one bounded loop at a time:

`Understand -> Improve -> Verify -> Learn`

This ledger owns experiment history and experiment-local findings, learnings, and
antipatterns. Current architecture, test policy, reliability policy, and debt remain
owned by their specialized documents. Promote unresolved actionable findings to
[`TECH-DEBT.md`](TECH-DEBT.md); promote established boundaries or policies to their
current-document owner. Corrections append a superseding entry instead of rewriting
old evidence.

## Evidence rules

- Separate confirmed facts, inferences, and hypotheses.
- Treat build, format, tests, and safety contracts as hard gates, not score inputs.
- Score a fresh-reader probe one point each for: correct owner, correct contract,
  reuse of an existing seam, minimal touch set, preserved invariants, and executable
  verification. Record workspace files inspected as context cost.
- Change one architectural coordinate per iteration. Stop or revert when it does not
  improve the stated maintainability signal.
- Never claim global or perfect architecture; report only the bounded result.

## DRSI-2026-08-09-001 - Skillset receipt classification

- **State:** verified locally; system-comprehension result mixed
- **Scope / acceptance boundary:** Centralize the existing rule that a
  `.tink-skillset.json` filesystem entry classifies an installed root as a skillset,
  including when the entry is a dangling or unsafe symlink. Preserve all CLI behavior.
- **Hard-gate baseline:** `cargo fmt --check` passed. `cargo test -- --nocapture`
  passed 39 unit and 92 acceptance tests (131 total).
- **Fresh-reader baseline:** A general `tink skillset check NAME-skillset` planning
  probe scored 6/6 but inspected 13 workspace files. A focused classification probe
  scored 6/6, found all five repeated production predicates plus two inconsistent
  standalone-library paths, and inspected 11 workspace files.
- **Confirmed finding:** The same `exists() || is_symlink()` receipt predicate appears
  in [`check.rs`](../src/check.rs), [`remove.rs`](../src/remove.rs), and three places in
  [`skillsets.rs`](../src/skillsets.rs).
- **Antipattern:** One domain decision is duplicated across callers. A future caller
  can omit the symlink half of the rule and allow an unsafe skillset root to fall
  through to standalone-skill handling.
- **Confirmed related finding:** [`library.rs`](../src/library.rs) does not apply this
  classification before standalone library reads. Managed skillset installation copies
  only declared member directories and the receipt, not a root `SKILL.md`; behavior for
  a receipt-bearing library root presented to standalone APIs is not explicit. Changing
  that behavior is outside this behavior-preserving iteration.
- **Confirmed documentation finding:** [`ARCHITECTURE.md`](ARCHITECTURE.md) and
  [`TESTING.md`](TESTING.md) omit shipped manifest, inspection, or skillset ownership.
  They are not reliable complete newcomer maps. This is recorded, not repaired in this
  one-coordinate iteration.
- **Hypothesis:** A named helper owned by `skillsets` will reduce information leakage
  from five predicates to one and make the fail-closed symlink rule discoverable.
- **Experiment:** Add the narrow helper, route the five existing predicates through it,
  and add a focused test for both a regular receipt and a dangling receipt symlink.
- **Predicted maintainability effect:** A newcomer searching for receipt classification
  should find one owner and should not need to reconstruct the symlink invariant from
  multiple callers.
- **Verification:** The focused dangling-symlink test passed. `cargo fmt --check` passed.
  The full suite passed 40 unit and 92 acceptance tests (132 total). Clippy was not run
  because `cargo-clippy` is not installed for the active toolchain. A repository-local
  runtime smoke check returned `OK 13 skill(s)`.
- **Fresh-reader result:** The post-change probe inspected 8 workspace files but scored
  3/6: it found the single helper and all five migrated call sites, then incorrectly
  concluded that every command was covered and missed the same two standalone-library
  paths. The probe did not inspect this ledger, so the result was not contaminated by
  the recorded hypothesis.
- **Outcome:** Retain the refactor. It removes five copies of a safety-sensitive rule
  without changing behavior, and its hard gates pass. Do not claim that the broader
  comprehension hypothesis passed.
- **Learning:** Correct planning and low context cost are independent. Centralizing a
  rule reduced search surface but also made a fresh reader more likely to mistake local
  consistency for system-wide completeness. A closed loop must measure semantic recall,
  not only files read or architectural neatness.
- **Next frontier or stop reason:** Decide whether receipt-bearing library roots must be
  categorically excluded from standalone library operations. Treat stale owner maps as
  a separate future coordinate.

## DRSI-2026-08-09-002 - Receipt ownership at standalone library boundaries

- **State:** verified locally within the existing managed-root boundary
- **Scope / acceptance boundary:** When an existing Tink library root contains a
  `.tink-skillset.json` entry, skillset ownership takes precedence over a root
  `SKILL.md`. Standalone list, bare-name add, divergent source add, and exact-cache add
  must not expose, promote, replace, or publish that root. External source discovery
  remains unchanged.
- **Hard-gate baseline:** DRSI-001 ended with 40 unit and 92 acceptance tests (132
  total) passing. Its fresh-reader probe showed that centralizing a predicate had not
  yet made the surrounding ownership contract complete.
- **Decision:** Adopt one owner for this boundary: receipt presence classifies an
  existing managed library root as a skillset before receipt validation or standalone
  interpretation. A root `SKILL.md` does not create a second identity.
- **Red evidence:** H10 first failed because `skill list --library` printed the managed
  root as a standalone skill. H11 first failed because a divergent external standalone
  source exited successfully, replaced the managed library tree, and published a
  project skill. H12 first failed because a byte-identical external source took the
  exact-cache path and published the receipt-bearing library root as a standalone
  project skill.
- **Antipattern:** A domain predicate was centralized without tracing every read,
  write, and cache-reuse boundary that interpreted the classified object. Green local
  tests made an incomplete owner contract look system-wide.
- **Experiment:** Reuse `skillsets::has_receipt_entry` at standalone library
  enumeration, bare-name loading, deposit, and exact-match lookup. Reject collisions
  before mutation or project publication. Add H10-H12 and update the acceptance
  contract, user documentation, and repository-managed skill guidance.
- **Verification:** H10-H12 passed. `cargo fmt --check` and `git diff --check` passed.
  The full suite passed 40 unit and 95 acceptance tests (135 total). A repository-local
  runtime smoke check returned `OK 13 skill(s)`. Clippy was not run because
  `cargo-clippy` is not installed for the active toolchain.
- **Fresh-reader result:** A clean-context boundary probe scored 6/6 and found no
  counterexample across list, bare-name promotion, divergent deposit, exact-cache
  reuse, or remote-tip reuse. It inspected 20 workspace files and deliberately did not
  inspect this ledger. A separate closure review scored 7/8 before this entry existed;
  durable newcomer-visible design history was its only failed diagnostic.
- **Outcome:** Retain the change. The bounded one-owner contract is executable and
  proven for existing receipt-bearing managed library roots. Do not generalize this to
  all receipt-bearing source trees or claim global architecture improvement.
- **Learning:** A classifier becomes an architectural boundary only when every
  interpretation path respects it. For mutable systems, inspect reads, writes, and
  shortcuts such as caches independently; each deserves a red test when it can bypass
  ownership.
- **Next frontier or stop reason:** Stop this coordinate. Separately decide how a
  receipt-bearing arbitrary source should behave when the managed target does not yet
  exist, and whether harvest's create-only deposit should classify or report such
  roots. Repair stale architecture and testing owner maps as another independent
  coordinate.

## DRSI-2026-08-09-003 - Automatic release verification gate

- **State:** verified locally; first GitHub execution pending
- **Scope / acceptance boundary:** Gate the automatic `main` push path before it
  changes Cargo versions, commits, tags, pushes, or dispatches a release. Direct tag
  pushes and manual `release.yml` dispatches remain outside this iteration.
- **Confirmed finding:** `bump-release.yml` automatically shipped every non-release
  push to `main` without running formatting or tests. The release workflow built
  binaries but did not supply a pre-tag behavior gate.
- **Antipattern:** Release safety depended on a contributor remembering local checks;
  the irreversible transition was automated while its evidence was not.
- **Experiment:** Reuse the repository's existing stable Rust action, request
  `rustfmt`, and run only `cargo fmt --check` plus `cargo test --locked` immediately
  after checkout and before the existing mutation step.
- **Verification:** The red characterization found neither command in the bump
  workflow. After the change, the YAML parsed and a focused order assertion proved
  both checks precede the first Cargo mutation. Formatting, diff checks, 40 unit tests,
  and 95 acceptance tests passed (135 total). `actionlint` is not installed locally,
  so GitHub-specific workflow lint was not run.
- **Fresh-reader result:** An independent workflow review scored 6/6 against the
  ledger rubric with two workspace files of context. It confirmed the gate ordering,
  preserved automatic release flow, minimal touch set, and the direct-tag/manual-
  dispatch boundary.
- **Outcome:** Retain the gate. It places executable evidence in the causal path before
  the automatic release becomes difficult to reverse, without introducing a new CI
  job, cache, matrix, or abstraction.
- **Learning:** A local green suite is evidence for one change; a durable safety sensor
  must execute automatically before the transition it protects.
- **Next frontier or stop reason:** Stop this coordinate. Gate `release.yml` separately
  only if every direct-tag and manual release entrypoint must share the same policy.
  Repair the stale architecture and testing maps as an independent comprehension
  coordinate.

## DRSI-2026-08-09-004 - Current system and sensor maps

- **State:** verified locally; documentation-only behavior preservation
- **Scope / acceptance boundary:** Replace historical phase notes in
  `docs/ARCHITECTURE.md` and `docs/TESTING.md` with current owner, authority, command
  flow, sensor, and delivery-boundary maps. Change no production or workflow behavior.
- **Baseline:** A source-restricted reader using only `ZEN.md`, `ARCHITECTURE.md`, and
  `TESTING.md` scored 0/6 supported answers. It could not identify receipt ownership,
  project/library authority, either complete add path, collision/cache sensors, or the
  automated release boundary.
- **Confirmed finding:** The old documents described prior numbered phases and omitted
  shipped modules and state. “Catalog” conflated a derived project-name index with an
  authored skillset definition; “receipt” conflated optional source provenance with
  skillset ownership and digest evidence.
- **Confirmed sensor finding:** Acceptance and executable test identifiers have drifted:
  C4 describes a different sensor than the `c4_*` test; manifest tests have no
  acceptance section; `M2` and `R2` are duplicated; several useful tests lack distinct
  rows; S1/S2 have only partial or unnamed coverage. The acceptance file's historical
  Linux-CI statement also differs from the current automatic release workflow.
- **Antipattern:** Historical refactor narration was serving as a current architecture
  map. A newcomer had to reconstruct live ownership from source while apparently
  authoritative documents concealed ambiguity and sensor gaps.
- **Experiment:** Document present-tense state authority, every source-module group,
  qualified domain vocabulary, standalone and skillset add flows, cross-cutting
  invariants, release predicates, executable sensor topology, and known gaps. Keep
  detailed behavior rows in their existing acceptance owner.
- **Verification:** All local documentation links resolve; historical phase markers are
  absent from the two owner maps; formatting and diff checks passed; the full locked
  suite passed 40 unit and 95 acceptance tests (135 total).
- **Fresh-reader result:** The same three-file source-restricted probe improved from 0/6
  to 6/6 and found no internal contradiction. It identified the classifier, authority
  direction, both add flows, exact collision/cache sensors, and all three automatic
  release-gate bypass classes without reading source.
- **Accuracy result:** A separate code-to-document review found two P1 overclaims and
  five P2 correction categories in the first draft. After narrowing receipt routing,
  separating CLI behavior from delivery authority, and correcting validation, refresh,
  ownership, sensor, and release wording, its final review found no remaining P1/P2 in
  scope.
- **Outcome:** Retain the maps. They reduce required onboarding context from an
  open-ended source search to three named documents while exposing rather than hiding
  the remaining contract and sensor debt.
- **Learning:** Comprehensibility and truth require different feedback loops. A reader
  probe tests whether the map can be followed; an independent implementation audit
  tests whether the map deserves to be followed.
- **Next frontier or stop reason:** Stop this coordinate. Reconcile acceptance rows with
  executable test names and missing sensors as a separate traceability iteration.
  Receipt-bearing arbitrary-source and create-only harvest semantics remain a separate
  product decision.

## Integration note - 2026-08-10

Rebasing onto `origin/main` introduced an upstream H10 contract for rejecting nested
library symlinks. The receipt-ownership acceptance rows and test functions are now
H11-H13. Earlier DRSI-002 references to H10-H12 preserve the identifiers used when
that experiment ran.
