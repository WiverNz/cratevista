//! Integration tests for `run_generate` that need **no** nightly and **no**
//! network: a bin-only workspace produces an empty default `RustdocPlan`, so the
//! run is a metadata-only success. Only `cargo metadata`/`cargo locate-project`
//! (stable) are invoked.

use std::path::Path;

use cratevista_core::clock::FixedClock;
use cratevista_core::exit::ExitCode;
use cratevista_core::generate::{GenerateOptions, run_generate};
use cratevista_schema::{DiagnosticsReport, ExplorerDocument, GenerationReport};

/// Writes a minimal bin-only crate (no dependencies → offline).
fn write_bin_crate(dir: &Path, name: &str) {
    std::fs::write(
        dir.join("Cargo.toml"),
        format!("[package]\nname = \"{name}\"\nversion = \"0.0.0\"\nedition = \"2021\"\n"),
    )
    .unwrap();
    std::fs::create_dir_all(dir.join("src")).unwrap();
    std::fs::write(dir.join("src").join("main.rs"), "fn main() {}\n").unwrap();
}

fn read_document(dir: &Path) -> ExplorerDocument {
    let text = std::fs::read_to_string(dir.join("target/cratevista/document.json")).unwrap();
    serde_json::from_str(&text).unwrap()
}

#[test]
fn metadata_only_bin_workspace_succeeds() {
    let dir = tempfile::tempdir().unwrap();
    write_bin_crate(dir.path(), "binonly");

    let options = GenerateOptions {
        manifest_path: Some(dir.path().join("Cargo.toml")),
        ..Default::default()
    };
    let clock = FixedClock("2026-07-14T00:00:00Z".into());
    let outcome = run_generate(&options, &clock).expect("generate succeeds");
    assert_eq!(outcome, ExitCode::SUCCESS);

    let out = dir.path().join("target/cratevista");
    assert!(out.join("document.json").is_file());
    assert!(out.join("generation.json").is_file());
    assert!(out.join("diagnostics.json").is_file());

    // The document validates and is byte-deterministic across two runs.
    let document = read_document(dir.path());
    document.validate().expect("document is schema-valid");
    let first = std::fs::read_to_string(out.join("document.json")).unwrap();
    run_generate(&options, &clock).unwrap();
    let second = std::fs::read_to_string(out.join("document.json")).unwrap();
    assert_eq!(first, second, "document.json is deterministic");

    // generation.json: not partial; no absolute path in the document.
    let generation: GenerationReport =
        serde_json::from_str(&std::fs::read_to_string(out.join("generation.json")).unwrap())
            .unwrap();
    assert!(!generation.partial);
    // The current generator always embeds artifact_hashes over the exact bytes.
    let hashes = generation
        .artifact_hashes
        .expect("generate always emits artifact_hashes");
    let doc_bytes = std::fs::read(out.join("document.json")).unwrap();
    let diag_bytes = std::fs::read(out.join("diagnostics.json")).unwrap();
    assert_eq!(
        hashes.document_blake3,
        cratevista_core::artifacts::blake3_hex(&doc_bytes)
    );
    assert_eq!(
        hashes.diagnostics_blake3,
        cratevista_core::artifacts::blake3_hex(&diag_bytes)
    );
    assert_eq!(hashes.document_blake3.len(), 64);
    assert!(!first.to_lowercase().contains(":\\"));
    assert!(!first.contains(dir.path().to_string_lossy().trim_end_matches(['/', '\\'])));

    // diagnostics.json is a separate artifact carrying the metadata-only note.
    let diagnostics: DiagnosticsReport =
        serde_json::from_str(&std::fs::read_to_string(out.join("diagnostics.json")).unwrap())
            .unwrap();
    assert!(
        diagnostics
            .diagnostics
            .iter()
            .any(|d| d.code == "no_documentable_rustdoc_targets"),
        "metadata-only run records the info diagnostic"
    );
}

#[test]
fn conflicting_feature_flags_exit_2_and_write_nothing() {
    let dir = tempfile::tempdir().unwrap();
    write_bin_crate(dir.path(), "binonly");
    let options = GenerateOptions {
        manifest_path: Some(dir.path().join("Cargo.toml")),
        all_features: true,
        features: vec!["x".into()],
        ..Default::default()
    };
    let failure = run_generate(&options, &FixedClock("t".into())).unwrap_err();
    assert_eq!(failure.exit, ExitCode::USAGE_ERROR);
    // No artifacts written.
    assert!(!dir.path().join("target/cratevista/document.json").exists());
}
