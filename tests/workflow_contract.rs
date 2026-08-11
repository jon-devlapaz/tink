//! Static delivery-workflow invariants. GitHub's native runs remain the
//! authoritative evidence that the platform matrix executes successfully.

const CI: &str = include_str!("../.github/workflows/ci.yml");
const BUMP: &str = include_str!("../.github/workflows/bump-release.yml");
const RELEASE: &str = include_str!("../.github/workflows/release.yml");

fn position(haystack: &str, needle: &str) -> usize {
    haystack
        .find(needle)
        .unwrap_or_else(|| panic!("workflow is missing {needle:?}"))
}

fn between<'a>(text: &'a str, start: &str, end: &str) -> &'a str {
    let start_at = position(text, start);
    let tail = &text[start_at..];
    let after_header = &tail[start.len()..];
    let end = after_header
        .find(end)
        .map(|offset| start.len() + offset)
        .unwrap_or_else(|| panic!("workflow section is missing {end:?}"));
    &tail[..end]
}

#[test]
fn bump_gates_the_tag_on_every_platform_and_publishes_refs_atomically() {
    for pair in [
        "target: aarch64-apple-darwin\n            os: macos-15",
        "target: x86_64-apple-darwin\n            os: macos-15-intel",
        "target: x86_64-unknown-linux-gnu\n            os: ubuntu-22.04",
        "target: aarch64-unknown-linux-gnu\n            os: ubuntu-22.04-arm",
    ] {
        assert!(BUMP.contains(pair), "missing native release gate {pair:?}");
    }
    assert!(BUMP.contains("needs: [verify, audit, platform]"));
    assert!(BUMP.contains("git push --atomic origin HEAD:main"));
    assert!(!BUMP.contains("git push origin HEAD:main\n"));
    assert!(!BUMP.contains("git push origin \"${tag}\""));

    let current_tag = position(BUMP, "current_tag=\"v${current}\"");
    let current_release = position(BUMP, "gh release view \"${current_tag}\" --json isDraft");
    let reconcile = position(BUMP, "gh workflow run release.yml --ref \"${current_tag}\"");
    let next_version = position(BUMP, "new=\"${major}.${minor}.$((patch + 1))\"");
    assert!(current_tag < current_release && current_release < reconcile);
    assert!(
        reconcile < next_version,
        "release gaps must stop the next bump"
    );

    let existing_tag = position(
        BUMP,
        "if git rev-parse --verify --quiet \"refs/tags/${tag}\"",
    );
    let release_check = position(BUMP, "gh release view \"${tag}\" --json isDraft");
    let dispatch = position(BUMP, "gh workflow run release.yml --ref \"${tag}\"");
    assert!(existing_tag < release_check && release_check < dispatch);
}

#[test]
fn release_uploads_an_exact_asset_set_to_a_draft_before_publication() {
    let create = position(RELEASE, "gh release create \"${tag}\"");
    let draft = position(RELEASE, "--verify-tag --draft");
    let upload = position(RELEASE, "gh release upload \"${tag}\"");
    let clobber = position(RELEASE, "--clobber");
    let local_digest = position(RELEASE, "sha256sum \"${asset}\"");
    let remote_digest = upload + position(&RELEASE[upload..], ".digest // \"\"");
    let validation = position(
        RELEASE,
        "Release asset state or digest does not match the local artifact",
    );
    let publish = position(RELEASE, "gh release edit \"${tag}\" --draft=false");

    assert!(create < draft && draft < upload);
    assert!(local_digest < upload);
    assert!(upload < clobber && clobber < remote_digest);
    assert!(remote_digest < validation && validation < publish);
    assert!(RELEASE.contains("expected_assets=("));
    assert!(RELEASE.contains("actual_assets=("));
}

#[test]
fn release_retries_require_exact_published_digests_and_never_regress_latest() {
    let published = between(
        RELEASE,
        "if [ \"${release_draft}\" = \"false\" ]; then",
        "if [ -z \"${release_draft}\" ]; then",
    );
    assert!(
        published.contains("${published_assets[$index]}\" != \"${expected_uploads[$index]}"),
        "published retries must compare GitHub digests with the local artifacts"
    );

    let latest_lookup = position(RELEASE, "latest_tag=\"");
    let monotonic_refusal = position(RELEASE, "Refusing to publish older release");
    let create = position(RELEASE, "gh release create \"${tag}\"");
    assert!(latest_lookup < monotonic_refusal && monotonic_refusal < create);
    assert!(RELEASE.contains("group: tink-release-publication"));
}

#[test]
fn dependency_audit_token_is_isolated_from_repository_code_execution() {
    for (workflow, quality_name, next_job, after_audit) in [
        (BUMP, "\n  verify:\n", "\n  audit:\n", "\n  platform:\n"),
        (RELEASE, "\n  quality:\n", "\n  audit:\n", "\n  build:\n"),
        (CI, "\n  quality:\n", "\n  audit:\n", "\n  platform:\n"),
    ] {
        let quality = between(workflow, quality_name, next_job);
        assert!(!quality.contains("checks: write"));
        assert!(!quality.contains("rustsec/audit-check"));

        let audit = between(workflow, "\n  audit:\n", after_audit);
        assert!(audit.contains("checks: write"));
        assert!(audit.contains("rustsec/audit-check"));
        assert!(!audit.contains("run: cargo"));
    }
}
