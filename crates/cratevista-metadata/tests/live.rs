//! Gated end-to-end test: runs real `cargo metadata` (offline) on a path-only
//! workspace built in a temporary directory.
//!
//! Ignored by default (needs a live Cargo, but no network). Run with:
//! `cargo test -p cratevista-metadata --test live -- --ignored`.

mod common;

use cratevista_metadata::{MetadataOptions, NetworkMode, ingest};

#[test]
#[ignore = "runs real cargo metadata; run with --ignored"]
fn live_path_only_workspace() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(
        dir.path().join("Cargo.toml"),
        "[package]\nname = \"live\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
    )
    .unwrap();
    std::fs::create_dir_all(dir.path().join("src")).unwrap();
    std::fs::write(dir.path().join("src/lib.rs"), "pub fn x() {}\n").unwrap();

    let options = MetadataOptions {
        manifest_path: Some(dir.path().join("Cargo.toml")),
        network: NetworkMode::Offline,
        ..MetadataOptions::default()
    };

    let result = ingest(&options).expect("live ingest");
    assert!(common::has_entity(&result, "workspace"));
    assert!(common::has_entity(&result, "package:live"));

    // Member source is repo-relative, not the absolute tempdir path.
    let package = common::entity(&result, "package:live");
    assert_eq!(package.source.as_ref().unwrap().path.as_str(), "Cargo.toml");

    let joined = result.summary.cargo_argv.join(" ");
    assert!(joined.contains("--format-version 1"));
    assert!(joined.contains("--offline"));
}
