# Skill Read Specification

## Status

Implementation-ready specification for read-only `tink skill read`.

## Purpose

Given a canonical standalone skill name, print its frontmatter description and
lifecycle metadata without parsing `SKILL.md` by hand. The command inspects one
already-installed tree. It does not install, refresh, or change classification.

## Locked decisions

These close the earlier handoff holes:

1. **Names match `skill list` / `library list`.** `tink skill read` addresses
   standalone skill directory names only. Skillset members are out of scope.
   Receipt-backed roots are refused with skillset guidance. Do not glob
   `*-skillset/<name>`.
2. **Compose existing classifiers.** Project lookup uses `.agents/skills/<name>`
   plus `skillsets::has_receipt_entry`. Library lookup uses a create-nothing
   load of `$TINK_HOME/skills/<name>`. Description stays off `Skill`.
3. **Kind is lifecycle; location is `--library`.** Variants are `embedded`,
   `standalone (local)`, and `standalone (remote)`. There is no `library` kind
   and no `tink library read` alias in this version.

## Public interface

```text
tink skill read <NAME> [--library] [--raw]
```

| Argument / flag | Meaning |
|---|---|
| `<NAME>` | Canonical standalone skill name (same grammar as `skill remove`) |
| `--library`, `-l` | Read `$TINK_HOME/skills/<NAME>/` instead of the project |
| `--raw`, `-r` | Print only the description line, unstyled |

`--library` is exclusive: it does not fall back to the project, and project
mode does not fall back to the library except as a missing-name hint.

## Name resolution

1. Reject an invalid skill name before touching the filesystem.
2. If `--library`:
   - Resolve an existing owned inventory root without creating home, `skills/`,
     or catalog state.
   - Load `$TINK_HOME/skills/<NAME>/` with the same standalone/skillset
     refusals as `library::load_at`.
   - Validate the tree (symlinks and special files).
3. If project mode:
   - Require `.agents/skills` as a real directory (same missing/symlink
     refusals as `skill list`).
   - Look only at `.agents/skills/<NAME>/`.
   - A `.tink-skillset.json` entry (including a dangling symlink) is a
     skillset root: refuse and direct the user to `tink skillset list`.
   - Otherwise read and tree-validate that standalone skill.
   - If the directory is missing, fail with `Skill not found`. When that name
     exists as a standalone library skill, mention `--library`.

A nested skillset member such as `.agents/skills/review-skillset/code-review`
is not found by `tink skill read code-review`. That is intentional.

## Kind

After a successful standalone load:

| Condition | Kind label |
|---|---|
| Name is `manage-tink` and there is no source receipt | `embedded` |
| Valid `.tink-source.json` | `standalone (remote)` |
| Otherwise | `standalone (local)` |

Kind does not depend on `--library`. A library copy with a receipt is still
`standalone (remote)`.

Read does not require the embedded payload to match the running binary. That
remains `skill check` / `skill refresh manage-tink`.

## Output

Default output is:

```text
<name>
  Description: <description>
  Path:        <display path>
  Kind:        <kind label>
```

Remote skills also print `Source`, `Revision`, and `Source Path` from the
validated receipt. Project paths are project-relative (`.agents/skills/<name>`).
Library paths are the inventory path. Every untrusted field, including
`--raw` description text, is passed through `output::escape_untrusted`.

`--raw` prints one escaped description line and a newline. Missing or invalid
skills still exit nonzero with empty stdout.

Closed stdout is a normal pipeline termination (exit 0, no panic).

## Out of scope

- Skillset member lookup by bare name or qualified path
- `tink library read`
- Machine-readable JSON
- Re-validating the embedded `manage-tink` payload against this binary
- Writing project, home, catalog, or library state
