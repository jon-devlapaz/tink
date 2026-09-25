# Code review policy

Review logic and edge cases, security boundaries, and acceptance criteria for every
change. A single review may cover these areas for routine work. Use separate focused
passes for consequential changes when justified by risk; record that choice and any
remaining uncertainty in the review findings.

Compare the candidate with the approved run's `brief.md` for light runs, or its
`02-design/output/spec.md` and `03-build/output/plan.md` for full runs. Resolve all
Important findings and refresh verification after implementation changes.

- **Important:** functional regressions, security flaws, broken contracts, or
  unhandled exceptions; block release review completion until resolved.
- **Nit:** optional readability or style suggestions; report at most five.

Automated review is advisory. Independent code-owner approval and current required
CI results are enforced by the project's forge; local files do not authenticate them.
