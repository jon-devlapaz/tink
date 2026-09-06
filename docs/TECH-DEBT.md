# Technical Debt

## Debt Ledger
| Item | Location | Type | Risk | Effort | Priority | Status |
|---|---|---|---|---|---|---|
| 15 | `src/check.rs` and `.agents/skills/*` | frontmatter field-shape beyond C5–C7 | low | low | low | backlog |

## Smell Inventory
| Smell | Location | Refactoring | Status |
|---|---|---|---|
| Broader malformed frontmatter field/value cases remain beyond the installed-skill structural sensors | `src/check.rs` and `.agents/skills/*` | C5-C7 now pin name/path mismatch, missing frontmatter, and unclosed frontmatter; add narrower field-shape cases only when a concrete failure appears | backlog |

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
