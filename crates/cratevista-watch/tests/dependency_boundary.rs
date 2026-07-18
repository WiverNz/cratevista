//! The dependency boundary, enforced by `cargo test` rather than by a command
//! someone has to remember to run.
//!
//! PRD 09 keeps the arrow `cratevista-core → cratevista-watch`, never the
//! reverse, and keeps this crate ignorant of the schema, the config, the graph
//! and the server. If it gained any of those, its rules would stop being testable
//! without them — and the classifier's whole value is that it is exact and
//! dependency-free.
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
/// Runtime and dev are separated deliberately: a dev-dependency does not ship, so
/// it cannot violate the architectural boundary. Conflating them would force a
/// choice between a watchdog in the tests and an honest assertion about what the
/// crate actually ships.
///
/// Comments are skipped: this crate's manifest explains the boundary in prose,
/// naming the very crates and features it must not take.
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

#[test]
fn watch_depends_on_no_other_cratevista_crate() {
    let deps = dependency_sections(&manifest_of("cratevista-watch"));
    for forbidden in [
        "cratevista-schema",
        "cratevista-metadata",
        "cratevista-rustdoc",
        "cratevista-graph",
        "cratevista-config",
        "cratevista-server",
        "cratevista-core",
        "cargo-cratevista",
    ] {
        assert!(
            !deps.contains(forbidden),
            "cratevista-watch must not depend on {forbidden}: its data model is \
             PathBuf/Duration/plain values, so no type from another crate is required"
        );
    }
}

#[test]
fn watch_ships_only_notify_and_tokio() {
    // A dependency arrives only with code that uses it. The engine earned `tokio`;
    // the real adapter earned `notify`. Nothing else has.
    let deps = sections(&manifest_of("cratevista-watch"), Kind::Runtime);
    for line in deps.lines().filter(|line| !line.trim().is_empty()) {
        let name = line.split(['=', ' ']).next().unwrap_or_default().trim();
        assert!(
            matches!(name, "notify" | "tokio"),
            "cratevista-watch should ship only notify and tokio, found `{name}`"
        );
    }
    assert!(deps.contains("notify"), "the real adapter uses notify");
    assert!(deps.contains("tokio"), "the engine and adapter use tokio");
}

#[test]
fn watch_takes_notify_with_its_default_native_backend() {
    // The recommended NATIVE watcher. `PollWatcher` is deliberately neither
    // enabled nor used: degrading to polling is a product decision with its own
    // costs, and this PRD does not make it.
    let deps = sections(&manifest_of("cratevista-watch"), Kind::Runtime);
    let notify_line = deps
        .lines()
        .find(|line| line.trim_start().starts_with("notify"))
        .expect("a notify dependency line");
    assert!(
        !notify_line.contains("default-features = false"),
        "the recommended native backend comes from notify's defaults: {notify_line}"
    );
}

#[test]
fn watch_ships_only_the_tokio_features_it_uses() {
    // `rt` for the tasks, `sync` for the channels, `macros` for `select!`, and —
    // since the real adapter landed — `time`, for the one real debounce timer.
    // `time` is now earned: `Debouncer` still reads no clock, but the adapter must
    // sleep until the deadline the debouncer computes.
    let deps = sections(&manifest_of("cratevista-watch"), Kind::Runtime);
    let tokio_line = deps
        .lines()
        .find(|line| line.trim_start().starts_with("tokio"))
        .expect("a tokio dependency line");

    for feature in ["rt", "sync", "macros", "time"] {
        assert!(
            tokio_line.contains(&format!("\"{feature}\"")),
            "the adapter uses tokio/{feature}: {tokio_line}"
        );
    }
    for forbidden in [
        "\"net\"",
        "\"io-util\"",
        "\"fs\"",
        "\"process\"",
        "\"signal\"",
        "\"rt-multi-thread\"",
    ] {
        assert!(
            !tokio_line.contains(forbidden),
            "cratevista-watch must not ship tokio/{forbidden}: nothing here uses it \
             (notify owns the filesystem, and the adapter runs one task)"
        );
    }
}

#[test]
fn only_core_depends_on_watch() {
    // Superseded the "nothing depends on watch yet" rule: the core-foundation
    // phase added `cratevista-core -> cratevista-watch`, which is the one edge
    // this crate is designed for. Everything else must still stay away — in
    // particular the **server**, which would otherwise learn what a watcher is
    // just to reuse an enum. Core owns the `EngineEvent -> ServerEvent`
    // conversion precisely so that edge never has to exist.
    for crate_name in [
        "cratevista-schema",
        "cratevista-metadata",
        "cratevista-rustdoc",
        "cratevista-graph",
        "cratevista-config",
        "cratevista-server",
        "cargo-cratevista",
    ] {
        let deps = dependency_sections(&manifest_of(crate_name));
        assert!(
            !deps.contains("cratevista-watch"),
            "{crate_name} must not depend on cratevista-watch; only core composes it"
        );
    }
    assert!(
        dependency_sections(&manifest_of("cratevista-core")).contains("cratevista-watch"),
        "core builds the WatchPlan and owns the regeneration transaction, so it depends on watch"
    );
}

#[test]
fn watch_is_registered_in_the_workspace() {
    let root = crates_dir().join("..").join("Cargo.toml");
    let manifest = std::fs::read_to_string(&root).expect("read the workspace manifest");
    assert!(
        manifest.contains("crates/cratevista-watch"),
        "cratevista-watch must be a workspace member, or its tests never run in \
         `cargo test --workspace`"
    );
}

#[test]
fn watch_forbids_unsafe_code() {
    let lib = crates_dir()
        .join("cratevista-watch")
        .join("src")
        .join("lib.rs");
    let source = std::fs::read_to_string(&lib).expect("read lib.rs");
    assert!(
        source.contains("#![forbid(unsafe_code)]"),
        "cratevista-watch must forbid unsafe code"
    );
}
