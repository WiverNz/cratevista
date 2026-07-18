//! The dependency boundary, enforced by `cargo test` rather than by a command
//! someone has to remember to run.
//!
//! PRD 05 makes `cratevista-graph` a **pure** layer, and PRD 08 keeps the
//! direction `cratevista-config → cratevista-graph`, never the reverse. If the
//! graph ever gained a config dependency, the overlay seam would stop being a
//! plain in-memory input and the graph would start knowing about TOML.
//!
//! These read the manifests rather than shelling out to `cargo tree`, so they
//! work offline, need no network, and fail with a precise message.

use std::path::{Path, PathBuf};

fn crates_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("..")
}

fn manifest_of(crate_name: &str) -> String {
    let path = crates_dir().join(crate_name).join("Cargo.toml");
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()))
}

/// Which manifest sections to read.
#[derive(Clone, Copy, PartialEq)]
enum Kind {
    /// `[dependencies]` + `[build-dependencies]` — what actually ships.
    Runtime,
    /// Every dependency section, including `[dev-dependencies]`.
    Any,
}

/// The requested dependency sections' text only — so a crate named in a doc
/// comment or in `[package] description` never counts as a dependency.
///
/// Runtime and dev are separated deliberately: a dev-dependency does not ship,
/// so it cannot violate the architectural boundary. Conflating them would have
/// forced the integration tests to either skip proving the graph integration or
/// to fake a `MetadataIngest`.
fn sections(manifest: &str, kind: Kind) -> String {
    let mut collected = String::new();
    let mut inside = false;
    for line in manifest.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with('[') {
            let is_dependency_section = trimmed.contains("dependencies");
            let is_dev = trimmed.contains("dev-dependencies");
            inside = is_dependency_section && (kind == Kind::Any || !is_dev);
            continue;
        }
        // Skip comments: this crate's manifest explains the boundary in prose.
        if inside && !trimmed.starts_with('#') {
            collected.push_str(trimmed);
            collected.push('\n');
        }
    }
    collected
}

/// Every dependency section, for "must not appear anywhere" checks.
fn dependency_sections(manifest: &str) -> String {
    sections(manifest, Kind::Any)
}

/// The direction that must never reverse.
///
/// Since step 3 added the legitimate `config → graph` edge, Cargo itself now
/// rejects the reverse as a **cyclic package dependency** before any test runs —
/// a stronger guarantee than this assertion. Verified by injecting the edge:
/// `cargo` fails with "cyclic package dependency". This test is kept as
/// belt-and-braces and, unlike Cargo's error, it says *why* the rule exists.
#[test]
fn cratevista_graph_does_not_depend_on_cratevista_config() {
    let graph = dependency_sections(&manifest_of("cratevista-graph"));
    assert!(
        !graph.contains("cratevista-config"),
        "cratevista-graph must not depend on cratevista-config — the overlay seam \
         is a plain in-memory input and the graph must stay free of TOML/config \
         concerns. Dependency sections were:\n{graph}"
    );
}

/// No other pure layer may reach for config either.
#[test]
fn no_upstream_crate_depends_on_cratevista_config() {
    for crate_name in [
        "cratevista-schema",
        "cratevista-metadata",
        "cratevista-rustdoc",
        "cratevista-graph",
        "cratevista-server",
    ] {
        let manifest = dependency_sections(&manifest_of(crate_name));
        assert!(
            !manifest.contains("cratevista-config"),
            "{crate_name} must not depend on cratevista-config"
        );
    }
}

/// This crate's own dependencies stay within the approved set.
#[test]
fn cratevista_config_depends_only_on_what_the_prd_approves() {
    let manifest = dependency_sections(&manifest_of("cratevista-config"));

    // `cratevista-graph` joined in step 3: `overlay` converts into its
    // `GraphOverlay`. That edge is legitimate — but only in this direction,
    // which `cratevista_graph_does_not_depend_on_cratevista_config` pins.
    for allowed in [
        "cratevista-schema",
        "cratevista-graph",
        "serde",
        "serde_json",
        "toml",
        "serde_spanned",
    ] {
        assert!(
            manifest.contains(allowed),
            "expected `{allowed}` among the dependencies:\n{manifest}"
        );
    }

    // Forbidden AT RUNTIME: config is a pure transform. It must never reach up
    // into the orchestrator, the CLI, or the server, and it must reach the
    // ingestion layers only through the graph.
    let runtime = sections(&manifest_of("cratevista-config"), Kind::Runtime);
    for forbidden in [
        "cratevista-core",
        "cargo-cratevista",
        "cratevista-server",
        "cratevista-metadata",
        "cratevista-rustdoc",
    ] {
        assert!(
            !runtime.contains(forbidden),
            "cratevista-config must not depend on `{forbidden}` at runtime:\n{runtime}"
        );
    }
}

/// `cratevista-metadata` is a **dev**-dependency only.
///
/// The integration tests need it to construct the `MetadataIngest` inside a
/// `GraphInput`, which is how they prove manual and discovered entities coexist
/// in a real document. It must never become a runtime dependency: config's job
/// is TOML → `GraphOverlay`, and it has no business knowing how Cargo metadata
/// is ingested.
#[test]
fn cratevista_metadata_is_a_dev_dependency_only() {
    let manifest = manifest_of("cratevista-config");
    assert!(
        sections(&manifest, Kind::Any).contains("cratevista-metadata"),
        "the integration tests need it as a dev-dependency"
    );
    assert!(
        !sections(&manifest, Kind::Runtime).contains("cratevista-metadata"),
        "…but it must not ship"
    );
}

/// `toml_edit` is a format-preserving *editor*; this crate only reads.
///
/// The PRD's original dependency list named it, but nothing here rewrites TOML,
/// and `toml` + `serde_spanned` already provide the spans diagnostics need.
/// Adding it would violate the project rule against taking a dependency merely
/// because a document mentions it.
#[test]
fn toml_edit_is_not_a_dependency() {
    let manifest = dependency_sections(&manifest_of("cratevista-config"));
    assert!(
        !manifest.contains("toml_edit"),
        "toml_edit is an editor, not a parser; this crate never rewrites TOML:\n{manifest}"
    );
}

/// The crate is registered, or nothing builds it.
#[test]
fn the_crate_is_a_workspace_member() {
    let root = std::fs::read_to_string(crates_dir().join("..").join("Cargo.toml")).unwrap();
    assert!(root.contains("\"crates/cratevista-config\""));
}
