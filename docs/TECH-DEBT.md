# Technical Debt

## Debt Ledger
| Item | Location | Type | Risk | Effort | Priority | Status |
|---|---|---|---|---|---|---|
| 1 | `docs/REMOVE-TECHNICAL-DEBT-PLAN.md` | process | medium | low | high | done |
| 2 | `src/lib.rs` | behavior characterization | medium | medium | high | pinned |
| 3 | `src/add.rs` | behavior characterization | medium | medium | high | pinned |
| 4 | `src/check.rs` | behavior characterization | medium | low | high | pinned |
| 5 | `src/remove.rs` | behavior characterization | medium | low | high | pinned |
| 6 | `src/catalog.rs` | behavior characterization | medium | low | medium | pinned |
| 7 | `src/refresh.rs` | behavior preservation refactor | medium | medium | medium | done |
| 8 | `src/update.rs` | clean-code refactor (`replace_binary` decomposition) | medium | low | medium | done |
| 9 | `src/check.rs` | software-design extraction (`is_ignored_skill_entry`, `read_skill_entry`) | medium | low | medium | done |
| 10 | `src/git.rs` | clean-architecture infrastructure extraction (`run_git`, `git_detail`) | medium | low | medium | done |
| 11 | `src/main.rs` | pragmatic-programmer boundary guard (`current_dir` failure path) | medium | low | high | done |
| 12 | `src/update.rs` | release hardening (timeouts/retries for update outbound calls) | medium | low | high | done |
| 13 | `src/git.rs` | resilience hardening for remote fetch commands (`run_git` transport config) | medium | low | high | done |
| 14 | `src/catalog.rs` | domain-driven-design aggregate extraction (`CatalogMeta`) | medium | low | medium | done |

## Smell Inventory
| Smell | Location | Refactoring | Status |
|---|---|---|---|
| no formal legacy safety net in this repository before this journey | `docs/` | create baseline test-oriented docs and pin top-risk CLI behavior | complete |
| Broader malformed frontmatter field/value cases remain beyond the installed-skill structural sensors | `src/check.rs` and `.agents/skills/*` | C5-C7 now pin name/path mismatch, missing frontmatter, and unclosed frontmatter; add narrower field-shape cases only when a concrete failure appears | backlog |
| `src/update.rs` staging/replace binary logic is separated by responsibility | `src/update.rs` | resolved by phase-3 refactor | complete |
| `src/check.rs` skill-entry parsing is separated from read/validate flow | `src/check.rs` | resolved by phase-4 refactor | complete |
| Catalog metadata modeling and ownership semantics (`name`, `root`, `skills`) | `src/catalog.rs` | move from loose JSON parsing helpers to `CatalogMeta` aggregate | complete |
| Git command execution and exit-code/error parsing duplication | `src/git.rs` | centralize command execution and status/error translation in one helper | complete |
| Outbound calls without bounded failure budget for integration steps | `src/git.rs`, `src/update.rs` | harden with transport timeouts and retry budgets | complete |
| Unhandled startup failure at CLI entry (`current_dir`) | `src/main.rs` | guard process precondition and fail with user-facing message | complete |

## Sprout / Wrap Register
| Host behavior | Why sprouted/wrapped | Rollback plan | Status |
|---|---|---|---|
| N/A (none yet) | N/A | N/A | done |

## Debt Budget & Broken-Windows Policy
- Budget: prioritize production-safety and behavior-risk fixes first; keep each slice single-purpose.
- Broken windows: do not defer observed regressions; if fix is not immediate, record explicitly in Debt Ledger with owner/priority.
- Startup/CLI boundary contract: never panic from process-level assumptions (for example, working directory resolution); convert to explicit errors with exit code + message.
- Integration hardening policy: all external process integrations should include bounded timeouts and explicit failure messages; keep retries bounded and visible.

## Adopted Conventions
- Keep command-path changes anchored to explicit entrypoint modules before touching supporting helpers.
- Prefer extracting behavior-preserving helpers from multi-purpose functions before changing control flow.
- Prefer characterization before structural change.
- Preserve backward-compatible CLI behavior unless intentionally changing acceptance criteria.

- Prefer explicit domain-level boundaries (e.g., directory entry filters before validation) for high-risk I/O flows.
- Prefer crash-loud at source-of-truth boundaries with clear, actionable failures instead of panics/unwraps in normal execution paths.
- Prefer explicit transport hardening (timeouts/retry caps) on outbound commands/tools (`git`, `curl`) and document deviations in RELIABILITY.md.
