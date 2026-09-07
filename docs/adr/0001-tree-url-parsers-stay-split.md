# ADR-0001: GitHub tree-URL parsers stay split

Date: 2026-09-07

## Context

Two functions parse the same GitHub `/tree/<ref>/<path>` URL shape:
`parse_url` in `src/inspect.rs` (→ `ParsedUrl`) and
`parse_github_add_source` in `src/sources.rs` (→ `GithubAddSource`).
All validation mechanics are shared (`is_public_github_https`,
`github_part_ok`, `github_tree_segment_ok`,
`git::reject_ambiguous_tree_ref_for`, one `url_lite` parser).

## Decision

Do not merge the two parser shells into one function. Their error strings
are intentionally per-command UX (`inspect` says "Inspection URL …",
`add` says "GitHub URL …" / "Remote sources …"), and acceptance tests pin
those messages.

## Consequences

- Future tree-URL rule changes go in the shared predicates, never in both
  parser shells.
- Re-suggesting a full parser merge requires a UX decision on the error
  strings first; it is not a pure refactor.
