# Reliability

Tink is a short-lived, single-process CLI. Reliability means deterministic command
contracts, refusal before predictable unsafe writes, verifiable persisted state, and a
clear retry path after an operational interruption. It does not mean distributed
transactions or concurrent writers.

## Build and platform gate

`rust-toolchain.toml` pins Rust 1.95.0 with Rustfmt and Clippy. Pull requests and
`main` pushes run formatting, locked workspace check, Clippy with warnings denied,
installer syntax/ShellCheck, documentation tests, and RustSec audit. Native tests and
release builds run on four hosts/targets:

- Apple silicon macOS (`aarch64-apple-darwin`)
- Intel macOS (`x86_64-apple-darwin`)
- arm64 Linux (`aarch64-unknown-linux-gnu`)
- x86_64 Linux (`x86_64-unknown-linux-gnu`)

The automatic bump workflow passes that same four-platform gate before atomically
publishing its release commit and tag. Tag releases and manual dispatches from a
matching `v*` tag repeat the quality gate before building those four artifacts. They
upload an exact asset set to a draft and publish only after GitHub reports every asset
as uploaded with a SHA-256 digest matching the local archive, so a failed or
unverifiable upload remains hidden and retryable. Publication is serialized across
tags and refuses to make an older version the latest release. A branch-based,
version-mismatched, or version-regressing manual dispatch fails before publication.
Windows is not a claimed platform.

## Process output and exits

Application output goes through fallible writers. Success data and summaries use
stdout; warnings and failures use stderr. A closed stdout is normal pipeline control
flow and exits 0 without a Rust printing panic. If the command itself fails, failure
to deliver its stderr diagnostic does not hide that failure: the process still exits
1. Clap owns usage errors and exits 2.

Advisory warnings are best-effort after completed work; a closed warning pipe does not
retroactively turn a successful mutation into failure. Clap parsing occurs before
current-directory resolution, so `--help` and `--version` work even when the cwd has
been removed; project commands fail closed if no cwd can be resolved.

## Install and update supply chain

`install.sh` and `tink update` share the same trust sequence:

1. Accept an HTTPS release-metadata URL without credentials, query, fragment,
   backslash, whitespace, or control characters. Absolute `file://` is allowed only
   as an explicit `TINK_RELEASES_API` test/fixture mode; an HTTPS metadata source may
   not redirect the asset to `file://`.
2. Select exactly the host-specific asset named
   `tink-<semver>-<target>.tar.gz`; require a lowercase `sha256:<64 hex>` digest.
3. Download with curl restricted to the selected protocol, a five-second connect
   timeout, at most 30 seconds for metadata or 300 seconds for the archive, and two
   retries separated by one second. The Rust updater and shell installer run each
   supervised subprocess in a separate process group, enforce the curl, tar, and probe
   limits, terminate remaining group members, and reap the direct child.
4. Verify the archive SHA-256 before extraction. Bounded tar inspection/extraction
   (30 seconds) requires exactly one top-level regular file named `tink`.
5. Run the candidate with a five-second bound and require exact stdout
   `tink <version>\n`, first from extraction and again from destination-adjacent
   staging. Refuse updater downgrades and non-regular replacement targets.
6. Publish only the verified staged file. Both paths keep a same-directory backup,
   probe the published path, and restore the backup on probe failure; if rollback
   itself fails, they report the retained backup path. Success output is emitted only
   after the published path passes its exact probe, and advisory installer output
   cannot turn a completed installation into failure when stdout closes.

The default publisher trust root is the repository's GitHub Release authority over
HTTPS. The asset digest binds transfer bytes to that same release metadata; it is not
an independent signature. Setting `TINK_RELEASES_API` deliberately changes that trust
root, while `file://` remains an explicit local fixture mode. Process-group cleanup
bounds ordinary timeouts and terminal interruption; it is lifecycle control, not a
sandbox, and code trusted as a release candidate can deliberately create a new session
outside that group.

The installer depends on `curl`, `tar`, and `python3`; `tink update` depends on `curl`
and `tar`. Neither runs automatically in the background.

## Filesystem identity and integrity

Skill trees reject symlinks and special files. On Unix, copy and equality preserve
raw path bytes without lossy Unicode conversion. Regular files are canonicalized to
`0o755` when any executable bit is present and `0o644` otherwise, matching the mode
semantics Git can persist while removing umask-only and special-bit variation. Digest
version 2 length-frames raw path bytes, entry kind, that canonical mode, byte length,
and content, making boundaries unambiguous and executable-bit changes visible.

Project lockfiles use version 2. Version 1 is refused with an instruction to rerun
`tink skill lock`; there is no silent reinterpretation of the old digest. Skillset
receipts carry `digestVersion: 2`. A legacy version-1 receipt is accepted only as the
input to `tink skillset refresh`, after its old digest proves the project tree clean;
refresh then installs a version-2 receipt and mirrors the validated project tree.

By-project catalog directories combine a bounded display basename with SHA-256 of the
canonical project path (raw path bytes on Unix). This distinguishes projects sharing a
basename and keeps directory components below common filesystem limits. A legacy
basename-only entry migrates on deposit only when its stored root proves ownership;
ambiguous legacy metadata is refused rather than claimed.

## Publication, interruption, and recovery

Manifest sync resolves and retains exact source bytes for every declaration, validates
every lock digest, and preflights all expected project, library, and catalog refusals
before publishing the first skill. Publication is then sequential. An unexpected
operational error can therefore leave earlier skills complete and later skills
unpublished; cross-skill rollback is not promised. Rerun `tink skill sync` to converge.

The same recovery principle applies to ordinary add/init flows: successful prior
steps remain valid, and retry resumes idempotently. Individual writes commonly use
destination-adjacent staging and rename, and some replacements attempt rollback, but
Tink does not claim atomicity across project trees, the library, catalog metadata, or
manifest/lock pairs.

`tink destroy` preflights catalog cleanup before deleting project state. It removes
only `.agents/skills/`, removes `.agents/` if that directory is then empty, and drops
the owned by-project catalog entry. It preserves `AGENTS.md`, `ZEN.md`, unrelated
`.agents/` siblings, the library, and other projects' catalog entries.

## Explicit operating boundary

Tink has no inter-process lock for a project or shared Tink home. Concurrent mutations
are unsupported and untested. Networked Git operations have low-speed and five-minute
overall bounds. Git, curl, tar, and release-candidate subprocesses run in supervised
process groups: timeout or HUP/INT/TERM directed only at Tink terminates and reaps the
active group, while the default signal action remains in force outside supervision.
The Rust subprocess supervisor and installer candidate probe retain each captured
stream independently up to 16 MiB. Readers continue draining after that point so the
child cannot block on a full pipe, then the command fails explicitly instead of
retaining unbounded output in memory.
No transaction spans separate filesystem owners.
