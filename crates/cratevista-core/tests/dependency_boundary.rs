//! The core-side dependency direction, enforced by `cargo test`.
//!
//! Core composes; it is the only crate allowed to know about all of them. What
//! must stay true is that nothing points back: the watcher must be testable
//! without cargo or a server, and the server must not learn what a watcher is.

use std::path::Path;

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
fn core_now_depends_on_watch() {
    let deps = dependency_sections(&manifest_of("cratevista-core"));
    assert!(
        deps.contains("cratevista-watch"),
        "core builds the WatchPlan and owns the regeneration transaction"
    );
}

#[test]
fn core_keeps_its_existing_direction() {
    let deps = dependency_sections(&manifest_of("cratevista-core"));
    for expected in [
        "cratevista-schema",
        "cratevista-metadata",
        "cratevista-rustdoc",
        "cratevista-graph",
        "cratevista-config",
        "cratevista-server",
    ] {
        assert!(deps.contains(expected), "core still composes {expected}");
    }
}

#[test]
fn watch_has_no_reverse_dependency_on_anything_core_composes() {
    let deps = dependency_sections(&manifest_of("cratevista-watch"));
    for forbidden in [
        "cratevista-core",
        "cratevista-server",
        "cratevista-config",
        "cratevista-metadata",
        "cratevista-schema",
        "cratevista-graph",
        "cratevista-rustdoc",
    ] {
        assert!(
            !deps.contains(forbidden),
            "cratevista-watch must not depend on {forbidden}: the arrow is core -> watch, \
             and a reverse edge would make the watcher untestable without cargo"
        );
    }
}

#[test]
fn the_server_still_depends_on_neither_watch_nor_core() {
    let deps = dependency_sections(&manifest_of("cratevista-server"));
    for forbidden in ["cratevista-watch", "cratevista-core"] {
        assert!(
            !deps.contains(forbidden),
            "cratevista-server must not depend on {forbidden}: it serves a snapshot and \
             fans out events it is handed. Core owns the EngineEvent -> ServerEvent \
             conversion precisely so this edge never has to exist"
        );
    }
}

#[test]
fn no_cycle_exists_between_core_watch_and_server() {
    // A cycle is a hard cargo error, so this is belt-and-braces: it names the
    // shape rather than waiting for a confusing resolver message.
    let core = dependency_sections(&manifest_of("cratevista-core"));
    let watch = dependency_sections(&manifest_of("cratevista-watch"));
    let server = dependency_sections(&manifest_of("cratevista-server"));

    assert!(core.contains("cratevista-watch") && core.contains("cratevista-server"));
    assert!(!watch.contains("cratevista-core") && !watch.contains("cratevista-server"));
    assert!(!server.contains("cratevista-core") && !server.contains("cratevista-watch"));
}
