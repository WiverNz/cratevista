//! Gated end-to-end `generate` test: runs the full metadata → plan → rustdoc →
//! graph → artifacts pipeline on a tiny path-only library crate using the pinned
//! nightly (`nightly-2026-07-01`). Ignored by default; **no network**.
//!
//! ```text
//! cargo test -p cratevista-core --test generate_live -- --ignored
//! ```

use std::path::Path;

use cratevista_core::clock::FixedClock;
use cratevista_core::exit::ExitCode;
use cratevista_core::generate::{GenerateOptions, run_generate};
use cratevista_schema::{ExplorerDocument, GenerationReport};

fn write_lib_crate(dir: &Path, name: &str) {
    std::fs::write(
        dir.join("Cargo.toml"),
        format!(
            "[package]\nname = \"{name}\"\nversion = \"0.0.0\"\nedition = \"2021\"\n\n[lib]\npath = \"src/lib.rs\"\n"
        ),
    )
    .unwrap();
    std::fs::create_dir_all(dir.join("src")).unwrap();
    std::fs::write(
        dir.join("src").join("lib.rs"),
        "//! A tiny crate.\n\n/// A documented struct.\npub struct Widget {\n    /// The size.\n    pub size: u32,\n}\n\n/// A documented function.\npub fn make() -> Widget { Widget { size: 0 } }\n",
    )
    .unwrap();
}

#[test]
#[ignore = "requires the pinned nightly toolchain; run with --ignored"]
fn live_generate_produces_a_valid_document() {
    let dir = tempfile::tempdir().unwrap();
    write_lib_crate(dir.path(), "livelib");

    let options = GenerateOptions {
        manifest_path: Some(dir.path().join("Cargo.toml")),
        ..Default::default()
    };
    let clock = FixedClock("2026-07-14T00:00:00Z".into());
    let outcome = run_generate(&options, &clock).expect("live generate succeeds");
    assert_eq!(outcome, ExitCode::SUCCESS);

    let out = dir.path().join("target/cratevista");
    let document: ExplorerDocument =
        serde_json::from_str(&std::fs::read_to_string(out.join("document.json")).unwrap()).unwrap();
    document.validate().expect("document is schema-valid");

    // The lib's own items were documented and linked to the target.
    assert!(
        document
            .entities
            .iter()
            .any(|e| e.id.as_str() == "item:struct:livelib::Widget")
    );
    assert!(document.relations.iter().any(|r| {
        r.kind.as_str() == "contains"
            && r.from.as_str() == "target:livelib:lib:livelib"
            && r.to.as_str() == "module:livelib::livelib"
    }));

    // Complete (not partial); generation.json records the verified tuple.
    let generation: GenerationReport =
        serde_json::from_str(&std::fs::read_to_string(out.join("generation.json")).unwrap())
            .unwrap();
    assert!(!generation.partial);
    assert_eq!(generation.rustdoc_format_version, Some(60));
    assert_eq!(generation.toolchain.as_deref(), Some("nightly-2026-07-01"));

    // No absolute path in the document.
    let document_text = std::fs::read_to_string(out.join("document.json")).unwrap();
    assert!(!document_text.to_lowercase().contains(":\\"));
}
