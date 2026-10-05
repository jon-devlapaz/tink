
$ ["cargo", "check"]
    Checking typenum v1.20.1
    Checking utf8parse v0.2.2
    Checking colorchoice v1.0.5
    Checking anstyle v1.0.14
    Checking is_terminal_polyfill v1.70.2
    Checking anstyle-query v1.1.5
    Checking clap_lex v1.1.0
    Checking strsim v0.11.1
    Checking libc v0.2.189
    Checking serde_core v1.0.229
    Checking equivalent v1.0.2
    Checking cfg-if v1.0.4
    Checking hashbrown v0.17.1
    Checking bitflags v2.13.1
    Checking toml_write v0.1.2
    Checking winnow v0.7.15
    Checking zmij v1.0.23
    Checking memchr v2.8.3
    Checking once_cell v1.21.4
    Checking fastrand v2.5.0
    Checking anstyle-parse v1.0.0
    Checking itoa v1.0.18
    Checking is_executable v1.0.6
    Checking shlex v2.0.1
    Checking semver v1.0.28
    Checking anstream v1.0.0
    Checking clap_builder v4.6.6
    Checking errno v0.3.14
    Checking getrandom v0.4.3
    Checking cpufeatures v0.2.17
    Checking indexmap v2.14.0
    Checking generic-array v0.14.7
    Checking signal-hook-registry v1.4.8
    Checking rustix v1.1.4
    Checking signal-hook v0.3.18
    Checking block-buffer v0.10.4
    Checking crypto-common v0.1.7
    Checking digest v0.10.7
    Checking sha2 v0.10.9
    Checking tempfile v3.27.0
    Checking serde v1.0.229
    Checking serde_json v1.0.151
    Checking clap v4.6.6
    Checking clap_complete v4.6.9
    Checking toml_datetime v0.6.11
    Checking serde_spanned v0.6.9
    Checking toml_edit v0.22.27
    Checking toml v0.8.23
    Checking tink v1.0.49 (/Users/jondev/dev/active/factory/working-copies/tink-patch-guidance)
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 3.03s

$ ["cargo", "test", "--test", "acceptance", "zero_footprint"]
   Compiling autocfg v1.5.1
   Compiling aho-corasick v1.1.5
   Compiling regex-syntax v0.8.11
   Compiling predicates-core v1.0.10
   Compiling normalize-line-endings v0.3.0
   Compiling assert_cmd v2.2.2
   Compiling difflib v0.4.0
   Compiling termtree v0.5.1
   Compiling wait-timeout v0.2.1
   Compiling tink v1.0.49 (/Users/jondev/dev/active/factory/working-copies/tink-patch-guidance)
   Compiling predicates-tree v1.0.13
   Compiling num-traits v0.2.19
   Compiling float-cmp v0.10.0
   Compiling regex-automata v0.4.18
   Compiling regex v1.13.1
   Compiling bstr v1.13.0
   Compiling predicates v3.1.4
    Finished `test` profile [unoptimized + debuginfo] target(s) in 3.57s
     Running tests/acceptance.rs (target/debug/deps/acceptance-e5d732c29e593943)

running 3 tests
test zero_footprint_conflicts_with_bundled_flags ... ok
test zero_footprint_init_leaves_git_clean ... ok
test zero_footprint_init_rerun_reports_ready_not_created ... ok

test result: ok. 3 passed; 0 failed; 0 ignored; 0 measured; 208 filtered out; finished in 0.20s


$ ["cargo", "test", "--test", "acceptance", "mount"]
    Finished `test` profile [unoptimized + debuginfo] target(s) in 0.03s
     Running tests/acceptance.rs (target/debug/deps/acceptance-e5d732c29e593943)

running 4 tests
test mount_path_traversal_aborts ... ok
test mount_missing_skill_fails_cleanly ... ok
test mount_refuses_to_overwrite_real_directory ... ok
test atomic_mount_and_unmount_creates_and_removes_ephemeral_link ... ok

test result: ok. 4 passed; 0 failed; 0 ignored; 0 measured; 207 filtered out; finished in 0.18s


# checklist item mount-guidance
$ ["python3", "tests/e2e/mount_guidance.py"]
PASS help_states_json_requirement
PASS help_has_complete_example
PASS payload_alone_still_refused
PASS unapproved_payload_refused
PASS guidance_has_no_unconditional_active_pointer
PASS generated_read_command_delivers_payload
PASS approved_prompt_payload_without_link
PASS plain_mount_still_creates_link
Evidence: /Users/jondev/dev/active/factory/working-copies/tink-patch-guidance/target/e2e/mount-guidance.json

# checklist item mount-trust
$ ["python3", "tests/e2e/mount_trust.py"]
PASS P1: P1: payload.content == SKILL.md + each references/** file inlined in sorted order; chars counts it; digest matches the bytes.
PASS P2: P2: a skill with scripts/ is linked into .tink/.active (mounted=true); a refs-only skill is not (mounted=false, target=null).
PASS S1: S1: SKILL.md that is a symlink (exfil vector) is refused with code symlink_refused, even after approve attempts.
PASS S2: S2: a symlink anywhere under references/ is refused (symlink_refused).
PASS S3: S3: a library entry that is itself a symlink to a directory is refused (symlink_refused).
PASS I1: I1: frontmatter name != directory name is refused (identity_mismatch).
PASS A1: A1: --payload refuses a never-approved skill (unapproved) without mounting or returning content.
PASS A2: A2: approve, then edit SKILL.md out of band -> digest_mismatch; re-approve -> success.
PASS A3: A3: `library approve --all` approves every clean skill; approvals lists name + digest; all mount with payload.
PASS W1: W1: `tink skill add <local path>` writes the library and records its digest (approve-on-write).
PASS J1: J1: success and error JSON parse and match the contract schema exactly (no extra keys).
PASS B1: B1: plain `tink mount` works without approval, links even refs-only skills, keeps its message; unmount works.
PASS D1: D1: `tink doctor` warns (exit 0) about symlinked library skills and counts unapproved ones.
artifact: target/e2e/mount-trust.json  failures: 0

# checklist item use-skillset
$ ["python3", "tests/e2e/use_skillset.py"]
PASS C1: C1: compile appends an exact managed block after untouched AGENTS.md bytes, in `required` order (non-required members skipped); second run is byte-identical.
PASS C2: C2: an existing block (any skillset/digest) is replaced in place; bytes before and after are preserved exactly.
PASS C3: C3: missing AGENTS.md is refused (never created); a symlinked AGENTS.md is refused and its target untouched.
PASS C4: C4: malformed, duplicated, unbalanced, nested or reversed markers are refused and the file is not modified.
PASS C5: C5: pin without `required`, empty `required`, non-subset, duplicate names, and a missing pin are refused (exit 2) with the fix named; nothing is written.
PASS C6: C6: unapproved, digest mismatch after edit, symlinked SKILL.md, name!=dir, missing skill each refuse the whole command (exit 2) naming skill + code; AGENTS.md and snapshot untouched.
PASS C7: C7: `rule:` frontmatter wins; else the FULL description (whitespace collapsed, so the rule sentence after the `Apply when` trigger is kept); anything over 400 chars is trimmed to 400 with an ellipsis.
PASS C8: C8: a body over the 8192-byte cap is refused with the actual size and the largest skills, nothing modified; --max-bytes raises the cap.
PASS C9: C9: --snapshot writes exactly rules.md (block body) + skills.lock.json (digests, source, revision, required order); creates DIR; refuses a symlinked DIR.
PASS C10: C10: --check exits 0 (no writes) right after compile, with and without --snapshot.
PASS C11: C11: --check exits 1 with a one-line reason per mismatch after: hand-edited block, missing block, edited skill body, revoked approval, deleted snapshot file, tampered lock.
PASS C12: C12: --json success and refusal match the contract exactly (exit 0 / 2), and bad usage is a clap error.
PASS C13: C13: a pin's `required` key survives tink's own pin rewrite (`tink skillset update`) and is still accepted by skillset reads.
PASS C14: C14: existing commands (mount, mount --json --payload, library list, help) behave as before with `use` present.
artifact: target/e2e/use-skillset.json  failures: 0
