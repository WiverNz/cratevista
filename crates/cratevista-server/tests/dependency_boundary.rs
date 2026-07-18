//! The server's dependency boundary, enforced by `cargo test`.
//!
//! PRD 06 makes `cratevista-server` an artifact server: it holds a snapshot and
//! serves it. It must never learn how a document is produced — no metadata, no
//! rustdoc, no graph, no config — and, since PRD 09, it must not depend on
//! `cratevista-watch` either: the server does not watch anything, it publishes
//! events it is handed. Reusing one enum is not worth an edge from the server to
//! the watcher.
//!
//! Reads the manifests rather than shelling out to `cargo tree`, so it works
//! offline and fails with a precise message.

use std::path::{Path, PathBuf};

fn manifest_of(crate_name: &str) -> String {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join(crate_name)
        .join("Cargo.toml");
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()))
}

/// Every dependency section's text only — so a crate named in a doc comment or in
/// `[package] description` never counts as a dependency.
fn dependency_sections(manifest: &str) -> String {
    let mut collected = String::new();
    let mut inside = false;
    for line in manifest.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with('[') {
            inside = trimmed.contains("dependencies");
            continue;
        }
        if inside && !trimmed.starts_with('#') {
            collected.push_str(trimmed);
            collected.push('\n');
        }
    }
    collected
}

#[test]
fn the_server_depends_on_no_pipeline_crate_and_not_on_watch() {
    let deps = dependency_sections(&manifest_of("cratevista-server"));
    for forbidden in [
        "cratevista-watch",
        "cratevista-core",
        "cratevista-config",
        "cratevista-graph",
        "cratevista-metadata",
        "cratevista-rustdoc",
        "cargo-cratevista",
    ] {
        assert!(
            !deps.contains(forbidden),
            "cratevista-server must not depend on {forbidden}: it serves a snapshot \
             and fans out events it is handed; it never produces or watches anything"
        );
    }
    // The one workspace crate it may know: the artifact types it serves.
    assert!(
        deps.contains("cratevista-schema"),
        "the server serves schema types, so this dependency is expected"
    );
}

#[test]
fn the_server_does_not_take_notify() {
    let deps = dependency_sections(&manifest_of("cratevista-server"));
    assert!(
        !deps.contains("notify"),
        "filesystem watching is not the server's job, in any phase"
    );
}

#[test]
fn nothing_but_core_depends_on_the_server() {
    for crate_name in [
        "cratevista-schema",
        "cratevista-metadata",
        "cratevista-rustdoc",
        "cratevista-graph",
        "cratevista-config",
        "cratevista-watch",
    ] {
        let deps = dependency_sections(&manifest_of(crate_name));
        assert!(
            !deps.contains("cratevista-server"),
            "{crate_name} must not depend on cratevista-server; only core composes them"
        );
    }
    let _ = PathBuf::new();
}
