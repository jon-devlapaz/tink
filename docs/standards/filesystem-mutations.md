# Filesystem mutations

Applies to every code path that writes under a project, `TINK_HOME`, the library, or the install location. Each rule below came from a reviewer finding on a real diff; apply all of them to the whole diff, then to the callers.

## Writing

**Stage, then rename.** Write the new tree or file beside the target and rename it over the directory entry. `fs::write` on an existing path follows hard links and truncates the shared inode; this includes `.tink-source.json` receipts.

**Every ancestor.** Refuse symlinks at each component between the project root and the target (`.agents`, `.agents/skills`, each `sourceRoot` segment), not only at the endpoint. Canonicalize each member and require it to stay beneath the root.

**Any entry shape.** A receipt path may be a file, a symlink, a hard link, or an empty directory. State which shapes are replaceable drift and which refuse, and test each.

**Revalidate at the write.** Fingerprint the installed tree when planning; re-check it immediately before the first write of a batch. Long network work between plan and apply opens a window for local edits.

**Bounded disk.** Hold one checkout at a time. Keep a staged copy of each candidate skill and release the clone after preflight.

**Read-only means no writes.** `check`, `list`, and `verify` create no files, including temp directories. Compare against an in-memory value or a precomputed digest.

**Recoverable.** A failure after the old tree is removed (`fs::remove_dir_all`) leaves the previous state restorable. Reuse `replace_verified` for refresh writes.

## Reading remote and user input

**Revisions are objects.** A tree URL revision can be a branch, a tag, or a 40-hex commit. Clone, fetch the revision (tags included for tag-only commits), then check it out. `git clone --branch <sha>` fails.

**Decode URLs.** Percent-decode path segments before resolving a boundary (`skill%20collections`).

**Root skill naming.** A skill at the repository root compares its canonical name to the repository basename or skips the comparison; the temp checkout directory name is an implementation detail.

**Boundary skill wins.** When the selected boundary is itself a skill, report exactly that skill and zero skillsets; discovery stops at it.

**Terminal-safe errors.** Paths in errors and output pass through `output::display_path`.

**Advisory output.** Text written after a completed mutation (`Next:` lines) is best-effort; a closed stdout cannot change the exit status.
