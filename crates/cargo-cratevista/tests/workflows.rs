//! Workflow-boundary and release/PRD bookkeeping guards (PRD 10, Phase 6/7).
//!
//! These are text assertions over the committed workflows and bookkeeping documents.
//! They encode the invariants a reviewer would otherwise have to re-check by hand,
//! and they are the discriminating half of the Phase-6/7 negative controls: mutate
//! the workflow or the status line and the matching test fails.

use std::path::{Path, PathBuf};

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("workspace root")
        .to_path_buf()
}

fn read(rel: &str) -> String {
    let path = repo_root().join(rel);
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()))
}

/// Byte offset of `needle` in `haystack`, or a panic naming the missing text.
fn index_of(haystack: &str, needle: &str, what: &str) -> usize {
    haystack
        .find(needle)
        .unwrap_or_else(|| panic!("{what}: missing `{needle}`"))
}

// ---------------------------------------------------------------------------
// release.yml
// ---------------------------------------------------------------------------

#[test]
fn release_triggers_only_on_a_semver_tag() {
    let yaml = read(".github/workflows/release.yml");
    assert!(
        yaml.contains("tags:") && yaml.contains("v[0-9]+.[0-9]+.[0-9]+"),
        "release.yml must trigger on a SemVer tag"
    );
    // It must NOT fire on ordinary branch pushes or pull requests.
    assert!(
        !yaml.contains("pull_request"),
        "release.yml must not trigger on pull requests"
    );
    assert!(
        !yaml.contains("branches:"),
        "release.yml must not trigger on branch pushes"
    );
}

#[test]
fn release_requests_only_contents_write_and_no_broader_scope() {
    let yaml = read(".github/workflows/release.yml");
    assert!(
        yaml.contains("contents: write"),
        "the upload job needs contents: write"
    );
    // Negative control 9-adjacent: no over-broad permission is requested anywhere.
    for forbidden in [
        "packages: write",
        "id-token: write",
        "actions: write",
        "deployments: write",
        "pull-requests: write",
    ] {
        assert!(
            !yaml.contains(forbidden),
            "release.yml must not request `{forbidden}`"
        );
    }
    // No signing/provenance in the first release.
    for deferred in ["cosign", "sigstore", "slsa", "attestation", "provenance"] {
        assert!(
            !yaml.to_ascii_lowercase().contains(deferred),
            "release.yml must not add {deferred} (deferred)"
        );
    }
}

#[test]
fn release_uses_pinned_stable_and_never_nightly() {
    let yaml = read(".github/workflows/release.yml");
    assert!(yaml.contains("1.97.1"), "release.yml pins stable 1.97.1");
    assert!(
        !yaml.to_ascii_lowercase().contains("nightly"),
        "release.yml must never install or use nightly"
    );
}

// ---------------------------------------------------------------------------
// publish.yml
// ---------------------------------------------------------------------------

#[test]
fn publish_is_manual_only_and_cannot_be_tag_triggered() {
    let yaml = read(".github/workflows/publish.yml");
    assert!(
        yaml.contains("workflow_dispatch"),
        "publish.yml must be workflow_dispatch"
    );
    // Negative control 10: a tag trigger must NOT exist on the publish workflow.
    assert!(
        !yaml.contains("tags:"),
        "publish.yml must not be triggered by a tag"
    );
    assert!(
        !yaml.contains("\n  push:") && !yaml.contains("on: push"),
        "publish.yml must not run on push"
    );
}

#[test]
fn publish_uses_the_protected_release_environment_and_confirmation() {
    let yaml = read(".github/workflows/publish.yml");
    // Negative control 11: losing the protected environment fails this test.
    assert!(
        yaml.contains("environment: release"),
        "publish.yml must use the protected `release` environment"
    );
    assert!(
        yaml.contains("PUBLISH"),
        "publish.yml must require an explicit confirmation input"
    );
    assert!(
        yaml.contains("CARGO_REGISTRY_TOKEN") && yaml.contains("secrets.CARGO_REGISTRY_TOKEN"),
        "publish.yml must take the crates.io token from a secret"
    );
    // No unnecessary GitHub write permission.
    for forbidden in ["contents: write", "packages: write", "id-token: write"] {
        assert!(
            !yaml.contains(forbidden),
            "publish.yml must not request `{forbidden}`"
        );
    }
}

#[test]
fn publish_order_matches_the_dependency_dag() {
    let yaml = read(".github/workflows/publish.yml");
    let order = [
        "cratevista-schema",
        "cratevista-metadata",
        "cratevista-rustdoc",
        "cratevista-graph",
        "cratevista-config",
        "cratevista-server",
        "cratevista-watch",
        "cratevista-core",
        "cargo-cratevista",
    ];
    let mut last = 0usize;
    for crate_name in order {
        // Search from `last` so the crates appear strictly in dependency order.
        let rel = yaml[last..].find(crate_name).unwrap_or_else(|| {
            panic!("publish.yml must list {crate_name} after the previous crate")
        });
        last += rel + crate_name.len();
    }
    // Sanity: `cargo publish` is invoked, not `--dry-run` automation.
    assert!(
        yaml.contains("cargo publish"),
        "publish.yml runs cargo publish"
    );
    assert!(
        !yaml.contains("--dry-run"),
        "publish.yml must not add automated --dry-run (manual launch gate)"
    );
}

// ---------------------------------------------------------------------------
// Bookkeeping: statuses stay pre-closure until hosted evidence lands.
// ---------------------------------------------------------------------------

#[test]
fn adr_0009_remains_proposed() {
    let adr = read("docs/adr/0009-static-build-and-release.md");
    // Negative control 13: flipping to Accepted before hosted closure fails here.
    let status_line = adr
        .lines()
        .find(|l| l.contains("**Status**"))
        .expect("ADR-0009 has a Status line");
    assert!(
        status_line.contains("Proposed"),
        "ADR-0009 must remain Proposed until PRD 10 is Implemented / Verified: {status_line}"
    );
    assert!(
        !status_line.contains("Accepted"),
        "ADR-0009 must not be Accepted yet"
    );
}

#[test]
fn prd_10_remains_approved_not_verified() {
    let prd = read("PRD/issue_10_static_build_and_release.md");
    let status = index_of(&prd, "## Status", "PRD 10");
    let window = &prd[status..status + 600];
    // The bolded status declaration is what matters; the plain phrase
    // "Implemented / Verified" legitimately appears in the explanatory prose (ADR-0009
    // "becomes Accepted only when PRD 10 is Implemented / Verified"), so match the
    // bold form. Negative control 14: flipping the declaration fails here.
    assert!(
        window.contains("**Approved — safe to implement**"),
        "PRD 10 must stay Approved — safe to implement"
    );
    assert!(
        !window.contains("**Implemented / Verified**"),
        "PRD 10 must not be marked Implemented / Verified before hosted closure"
    );
}

#[test]
fn index_lists_prd_10_as_approved_only() {
    let index = read("PRD/INDEX.md");
    let row = index
        .lines()
        .find(|l| l.contains("| 10 ") || l.contains("issue_10"))
        .expect("INDEX has a PRD-10 row");
    assert!(
        row.contains("Approved"),
        "INDEX PRD-10 row must read Approved: {row}"
    );
    assert!(
        !row.contains("Implemented"),
        "INDEX PRD-10 row must not read Implemented before hosted closure: {row}"
    );
}

#[test]
fn issue_13_is_a_specification_shell_not_an_approved_prd() {
    // The shell exists.
    let issue = read("ISSUES/issue_13_static_source_snippets.md");
    assert!(
        issue.to_ascii_lowercase().contains("specification"),
        "issue 13 must describe itself as a specification shell"
    );
    // It states the current release writes no snippets.
    assert!(
        issue.to_ascii_lowercase().contains("no snippet") || issue.contains("writes no snippet"),
        "issue 13 must state the current release writes no snippets"
    );
    // It is NOT listed in the PRD index as an approved PRD.
    let index = read("PRD/INDEX.md");
    assert!(
        !index.contains("issue_13") && !index.contains("| 13 "),
        "INDEX must not list issue 13 as an (approved) PRD row"
    );
}
