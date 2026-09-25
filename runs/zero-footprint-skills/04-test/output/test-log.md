
$ ["cargo", "check"]
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 0.12s

$ ["cargo", "test", "--test", "acceptance", "zero_footprint"]
    Finished `test` profile [unoptimized + debuginfo] target(s) in 0.06s
     Running tests/acceptance.rs (target/debug/deps/acceptance-bf7fc87beefdb18f)

running 3 tests
test zero_footprint_conflicts_with_bundled_flags ... ok
test zero_footprint_init_leaves_git_clean ... ok
test zero_footprint_init_rerun_reports_ready_not_created ... ok

test result: ok. 3 passed; 0 failed; 0 ignored; 0 measured; 205 filtered out; finished in 0.41s


$ ["cargo", "test", "--test", "acceptance", "mount"]
    Finished `test` profile [unoptimized + debuginfo] target(s) in 0.03s
     Running tests/acceptance.rs (target/debug/deps/acceptance-bf7fc87beefdb18f)

running 4 tests
test mount_path_traversal_aborts ... ok
test mount_missing_skill_fails_cleanly ... ok
test mount_refuses_to_overwrite_real_directory ... ok
test atomic_mount_and_unmount_creates_and_removes_ephemeral_link ... ok

test result: ok. 4 passed; 0 failed; 0 ignored; 0 measured; 204 filtered out; finished in 0.32s

