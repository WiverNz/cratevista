//! Documentation integrity + overclaim guards (PRD 10, Phase 7 H, controls 5/6).
//!
//! Text/link assertions over the public docs: internal Markdown links resolve, no
//! deleted reference asset is linked, no external reference-project residue, no stale
//! `web/dist` production instruction, no false publication/snippet/reproducibility/
//! `file://`/`default_branch` claim, the stable-vs-nightly distinction is correct, and
//! the Cargo author is `Aleksandr Skibin` for all nine crates.

use std::path::{Path, PathBuf};

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("workspace root")
        .to_path_buf()
}

fn read(rel: &str) -> String {
    std::fs::read_to_string(repo_root().join(rel)).unwrap_or_else(|e| panic!("read {rel}: {e}"))
}

/// The docs authored/expanded by PRD 10 whose internal links must resolve.
const LINK_CHECKED: [&str; 8] = [
    "README.md",
    "SECURITY.md",
    "CONTRIBUTING.md",
    "crates/cargo-cratevista/README.md",
    "docs/hosting.md",
    "docs/launch-checklist.md",
    "docs/launch/announcement-drafts.md",
    "docs/licenses/README.md",
];

/// Extracts `](target)` link targets from Markdown text.
fn markdown_links(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    let bytes = text.as_bytes();
    let mut i = 0;
    while i + 1 < bytes.len() {
        if bytes[i] == b']'
            && bytes[i + 1] == b'('
            && let Some(end) = text[i + 2..].find(')')
        {
            out.push(text[i + 2..i + 2 + end].trim().to_string());
            i += 2 + end;
            continue;
        }
        i += 1;
    }
    out
}

#[test]
fn internal_markdown_links_resolve() {
    let root = repo_root();
    let mut failures = Vec::new();
    for doc in LINK_CHECKED {
        let text = read(doc);
        let doc_dir = root.join(doc).parent().unwrap().to_path_buf();
        for link in markdown_links(&text) {
            // Skip external and in-page links.
            if link.starts_with("http://")
                || link.starts_with("https://")
                || link.starts_with("mailto:")
                || link.starts_with('#')
            {
                continue;
            }
            // Strip an anchor and any query.
            let path_part = link.split('#').next().unwrap().split('?').next().unwrap();
            if path_part.is_empty() {
                continue;
            }
            let target = doc_dir.join(path_part);
            if !target.exists() {
                failures.push(format!("{doc} → {link}"));
            }
        }
    }
    assert!(
        failures.is_empty(),
        "unresolved internal links:\n{}",
        failures.join("\n")
    );
}

#[test]
fn no_deleted_reference_assets_or_external_reference_residue() {
    for doc in LINK_CHECKED {
        let text = read(doc).to_ascii_lowercase();
        assert!(
            !text.contains("gamesrv"),
            "{doc} must not mention the removed external reference project"
        );
        assert!(
            !text.contains("docs/references/"),
            "{doc} must not link a deleted reference screenshot"
        );
    }
}

#[test]
fn readme_has_no_stale_web_dist_production_instruction() {
    // The authoritative bundle lives in crates/cratevista-server/embedded/. No public
    // doc should instruct building/serving from the old web/dist path.
    for doc in [
        "README.md",
        "crates/cargo-cratevista/README.md",
        "docs/hosting.md",
    ] {
        let text = read(doc);
        assert!(
            !text.contains("web/dist"),
            "{doc} must not carry a stale web/dist instruction"
        );
    }
}

/// Control 6: the README documents the crates.io install as a FUTURE step, never as
/// currently available.
#[test]
fn readme_does_not_overclaim_publication() {
    let readme = read("README.md");
    assert!(
        readme.contains("Not published yet"),
        "README must flag crates.io install as future/unavailable"
    );
    for overclaim in [
        "now available on crates.io",
        "is published on crates.io",
        "available on crates.io now",
        "a GitHub Release exists",
    ] {
        assert!(
            !readme
                .to_ascii_lowercase()
                .contains(&overclaim.to_ascii_lowercase()),
            "README must not claim `{overclaim}`"
        );
    }
    // No snippet / binary-reproducibility / file:// overclaims.
    let lower = readme.to_ascii_lowercase();
    assert!(
        !lower.contains("reproducible binaries") && !lower.contains("byte-identical binaries"),
        "README must not claim binary reproducibility"
    );
    assert!(
        readme.contains("No source snippets")
            || readme.contains("no copied source snippets")
            || readme.contains("never copies file contents"),
        "README must state snippets are not written"
    );
    assert!(
        readme.contains("file://") && lower.contains("not supported"),
        "README must state file:// is unsupported"
    );
}

/// Control 5: the README states the stable-vs-nightly distinction correctly and never
/// implies ordinary rustdoc generation works on stable.
#[test]
fn readme_states_stable_vs_nightly_correctly() {
    let readme = read("README.md");
    assert!(
        readme.contains("nightly-2026-07-01"),
        "README must name the pinned nightly"
    );
    assert!(
        readme.contains("Nightly is required only") || readme.contains("nightly is required only"),
        "README must scope nightly to runtime rustdoc generation"
    );
    assert!(
        readme.contains("does **not** work on stable")
            || readme.contains("does not work on stable")
            || readme.contains("never pretends"),
        "README must not imply rustdoc generation works on stable"
    );
    // The exact tuple appears.
    for part in ["60", "0.60.0", "nightly-2026-07-01"] {
        assert!(
            readme.contains(part),
            "README must state tuple part `{part}`"
        );
    }
}

#[test]
fn cargo_author_is_aleksandr_skibin_for_all_nine_crates() {
    let root = read("Cargo.toml");
    assert!(
        root.contains(r#"authors = ["Aleksandr Skibin"]"#),
        "workspace author must be Aleksandr Skibin"
    );
    for crate_name in [
        "cratevista-schema",
        "cratevista-metadata",
        "cratevista-rustdoc",
        "cratevista-graph",
        "cratevista-config",
        "cratevista-server",
        "cratevista-watch",
        "cratevista-core",
        "cargo-cratevista",
    ] {
        let manifest = read(&format!("crates/{crate_name}/Cargo.toml"));
        assert!(
            manifest.contains("authors.workspace = true"),
            "{crate_name} must inherit the workspace author"
        );
    }
}
