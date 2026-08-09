# GitHub Source Inspection Specification

## Status

Implementation-ready specification for the first read-only `tink inspect`
tracer bullet.

## Purpose

Given a supported GitHub repository, folder, or skill URL, report the skills
and source-defined skillsets visible inside that URL boundary. Inspection must
not modify the current project, Tink home, catalog, or library.

This command discovers source structure. It does not install, register, or name
new project content.

## Domain language

- **Inspection boundary**: the repository subtree selected by the input URL.
- **Discovered skill**: a directory inside the boundary containing a regular,
  valid `SKILL.md`.
- **Skill collection root**: the directory whose immediate child directories
  represent source-defined skillset groups.
- **Source skillset**: an immediate child directory of the skill collection
  root. Its proposed canonical Tink name is `<folder>-skillset` when the folder
  is already a valid Tink name.
- **Structural wrapper**: a directory between the inspection boundary and the
  skill collection root that contains one skill-bearing child and no direct
  discovered skills.
- **Diagnostic**: an ambiguity or invalid source condition that prevents Tink
  from presenting an item as an unqualified skill or skillset.

Skills are marker-defined by `SKILL.md`. Skillsets are boundary-defined by the
source directory structure.

## Public interface

```text
tink inspect <GITHUB_URL>
```

The MVP accepts canonical public GitHub HTTPS URLs in these forms:

```text
https://github.com/<owner>/<repository>
https://github.com/<owner>/<repository>.git
https://github.com/<owner>/<repository>/tree/<ref>/<path>
```

For a repository URL, the inspection boundary is the repository root at the
remote default branch. For a tree URL, the inspection boundary is `<path>` at
`<ref>`.

Refs containing `/` are out of scope for this tracer bullet because the GitHub
URL does not unambiguously separate such a ref from the path without additional
remote resolution. Tink must refuse them with an actionable error rather than
guessing.

The command has no flags in the MVP. Machine-readable output, local paths,
GitLab, SSH URLs, private repositories, installation, and catalog writes are
out of scope.

## Source resolution

1. Parse the URL into canonical clone URL, requested ref, and boundary path.
2. Clone into a temporary directory without writing Tink or project state.
3. Resolve the inspected checkout to a full immutable Git commit ID.
4. Validate that the boundary exists as a regular directory inside the
   checkout.
5. Never follow symbolic links while discovering skills.
6. Delete temporary checkout state when the command exits.

The reported source must contain:

```text
Repository: https://github.com/<owner>/<repository>.git
Revision:   <full commit ID>
Boundary:   <repository-relative POSIX path or .>
```

## Skill discovery

Walk the inspection boundary recursively in deterministic lexical path order.
A candidate is a directory containing a regular `SKILL.md`.

For every candidate:

1. Reuse Tink's existing `SKILL.md` validation.
2. Read its canonical name from frontmatter.
3. Record its repository-relative POSIX directory path.
4. Do not require the directory basename to equal the frontmatter name during
   inspection; report a mismatch as a diagnostic.
5. Report duplicate canonical names as diagnostics while retaining every
   candidate path in the output.
6. Report nested/overlapping skill roots as diagnostics rather than silently
   discarding either candidate.

Invalid `SKILL.md` candidates are diagnostics and are not counted as discovered
skills. A valid source with zero discovered skills is a successful inspection.

## Skillset inference

Inference operates only inside the inspection boundary and never changes the
discovered skill list.

### Boundary is a skill

If the boundary itself contains `SKILL.md`, report that one skill and no
skillsets. Do not infer a one-member skillset.

### Boundary directly contains skills

If one or more immediate child directories of the boundary are discovered
skills and no other immediate child contains deeper discovered skills, treat the
boundary as one source skillset. Use the boundary folder name to propose
`<folder>-skillset`. A repository-root boundary has no folder name; in that case
report an unnamed boundary proposal rather than inventing a name.

The exact directory basename `skills` is reserved as a structural collection
root and is never itself a source skillset. If `skills/` directly contains
discovered skills, report those skills and zero skillsets. If it contains
grouped skills, infer meaningful child groups normally. This rule applies
equally when `skills/` is reached through a repository URL or selected directly
by a tree URL. A repository whose own name is `skills` is unaffected because
its repository-root boundary is `.`.

If direct skills coexist with one or more nested skill-bearing collections,
report the skills but infer no skillsets. Emit a diagnostic explaining that the
mixed root is ambiguous and that inspecting a narrower tree URL can select the
intended collection. Do not collapse unrelated levels into one skillset.

### Boundary contains grouped skills

If the boundary has no direct skill children and at least two immediate child
directories contain descendant discovered skills, treat the boundary as the
skill collection root. Every regular, non-hidden immediate child directory is a
source skillset, including empty peers. Its member count is the number of
discovered skills beneath that child.

This rule intentionally reports an empty structural peer such as
`skills/deprecated/` alongside populated groups. Display zero-member groups as
empty structural candidates so users can distinguish structural inference from
a skill-backed group.

### Single structural wrapper

If the boundary has no direct skill children and exactly one immediate child
directory that is non-hidden, regular, and contains descendant discovered
skills, descend through that child and evaluate again. Stop at the first
directory satisfying one of the rules above. Do not descend through symlinks.

This allows a repository root containing one `skills/` wrapper to expose the
groups beneath `skills/` without encoding the literal directory name `skills`.

### Ambiguous structure

If no valid skills are discovered, report zero skills and a diagnostic that no
valid skills were found in the boundary. Otherwise, if no rule yields one
collection root, report discovered skills and a diagnostic that skillsets could
not be inferred. Do not manufacture groupings from every ancestor directory.

### Canonical skillset names

When a source skillset folder is a valid Tink name, append `-skillset` in the
displayed proposal. If it already ends in `-skillset`, use it verbatim rather
than doubling the suffix. The resulting canonical name must also pass Tink's
name validation, including the length limit. Inspection does not create or
accept that name on the user's behalf. If no valid canonical name can be formed,
report a naming diagnostic and display no canonical proposal.

## Human-readable output

Output has four sections in this order:

1. `Source`
2. `Skillsets (<count>, <member-count> member skills)`; each inferred skillset
   owns its nested member list and reports its source path once
3. `Standalone skills (<count>)`; discovered skills outside every inferred
   skillset retain their individual source paths
4. `Diagnostics (<count>)`, omitted when empty

Within each section, entries are sorted lexically by repository-relative path.
Do not use color when stdout is not a terminal, consistent with existing Tink
style behavior.

For `https://github.com/mattpocock/skills` at commit
`84fdeffd12f2ee307994d1eb6feb48173b6e0502`, the semantic output is:

```text
Source
  Repository: https://github.com/mattpocock/skills.git
  Revision:   84fdeffd12f2ee307994d1eb6feb48173b6e0502
  Boundary:   .

Skillsets (5, 35 member skills)
  deprecated-skillset (0 skills)  skills/deprecated/
    (empty structural candidate)

  engineering-skillset (18 skills)  skills/engineering/
    ask-matt
    ...

  productivity-skillset (7 skills)  skills/productivity/
    grill-me
    ...
    writing-for-agents

Standalone skills (0)
```

For a boundary at `skills/productivity`, report one
`productivity-skillset` proposal and its seven skills. For a boundary at
`skills/productivity/grill-me`, report zero skillsets and the single
`grill-me` skill.

Exact spacing is not contractual. Section names, counts, canonical names,
membership, skill names, source paths, and deterministic ordering are
contractual.

When stdout is an interactive terminal, color reinforces—but never replaces—the
text hierarchy:

- skillset names use Tink's blue grouped-identity style;
- skill names use Tink's magenta identity style;
- repository metadata values and source paths use Tink's cyan accent style;
- the `Diagnostics` heading uses Tink's yellow warning style while diagnostic
  messages remain in the terminal's default foreground;
- headings, counts, and empty structural annotations use the terminal's default
  foreground.

Apply padding before ANSI styling so escape sequences cannot disturb column
alignment. Non-terminal output and `NO_COLOR` output remain plain and
byte-stable.

## Side-effect contract

Inspection must not create or change:

- `.agents/`
- `.tink/` inside the project
- `$TINK_HOME`
- skillset catalog definitions
- home-library trees
- Git state in the current project

Network access and temporary checkout files are the only expected effects.

## Failure behavior

Exit nonzero with an actionable message for:

- unsupported or malformed URL;
- non-GitHub host;
- unavailable repository or ref;
- ambiguous slash-containing tree ref;
- missing or non-directory boundary;
- inability to create or cleanly use temporary checkout state.

Content diagnostics inside a successfully resolved boundary do not by
themselves make inspection fail.

## Implementation ownership

- `src/lib.rs`: CLI declaration, dispatch, and presentation only.
- New `src/inspect.rs`: GitHub URL parsing, source inspection orchestration,
  discovery result model, skillset inference, and deterministic report data.
- `src/git.rs`: only the smallest reusable checkout primitive required to
  inspect a requested ref.
- `src/skills.rs`: reuse existing skill validation; do not add GitHub policy.
- `tests/acceptance.rs`: end-to-end command behavior and side-effect proof.

Do not route inspection through `skill add` or `skillset add`; those commands
own mutation and stronger installation policy.

## Acceptance criteria

Add stable acceptance rows and tests proving:

1. Repository URL for `mattpocock/skills` shape reports five source skillsets,
   including empty `deprecated`, and 35 skills in deterministic order.
2. Group tree URL reports one skillset and only skills beneath that boundary.
3. Skill tree URL reports one skill and zero skillsets.
4. A repository with one structural wrapper whose name is not `skills` is
   inferred identically, proving no literal `skills/` convention.
5. Duplicate skill names and invalid `SKILL.md` files are visible diagnostics.
6. Empty valid boundaries succeed with zero skills and appropriate structural
   output.
7. Unsupported URLs, missing refs, and missing boundaries fail clearly.
8. Inspection leaves the project and `$TINK_HOME` absent or byte-for-byte
   unchanged.

All existing unit and acceptance tests, formatting, and `git diff --check`
must remain green.

## Out of scope and next decision

This tracer bullet does not decide how discovered skills become catalog entries
or installed skillsets. After inspecting real repository shapes, the next human
decision is whether `tink skillset add` accepts every discovered member,
requires explicit confirmation, or accepts an explicit subset.
