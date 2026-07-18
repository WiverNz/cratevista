//! Canonical round-trip: every committed fixture must deserialize and
//! re-serialize (through the canonical serializer) to byte-identical output.
//! This also guards that the fixture builders and the committed files agree.

use std::path::PathBuf;

use cratevista_schema::canonical::to_canonical_string;
use cratevista_schema::{DiagnosticsReport, ExplorerDocument, GenerationReport};

fn fixture(name: &str) -> String {
    let mut path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    path.push("fixtures");
    path.push(name);
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()))
}

const DOCUMENT_FIXTURES: &[&str] = &[
    "minimal.document.json",
    "full_mvp.document.json",
    "manual_flow.document.json",
    "unknown_kind.document.json",
];

#[test]
fn documents_round_trip_byte_identical() {
    for name in DOCUMENT_FIXTURES {
        let text = fixture(name);
        let doc: ExplorerDocument =
            serde_json::from_str(&text).unwrap_or_else(|e| panic!("deserialize {name}: {e}"));
        let out = to_canonical_string(&doc).unwrap();
        assert_eq!(out, text, "canonical round-trip mismatch for {name}");
        doc.validate()
            .unwrap_or_else(|errors| panic!("{name} failed validation: {errors:?}"));
    }
}

#[test]
fn generation_report_round_trips() {
    let text = fixture("full_mvp.generation.json");
    let report: GenerationReport = serde_json::from_str(&text).unwrap();
    assert_eq!(to_canonical_string(&report).unwrap(), text);
}

#[test]
fn diagnostics_report_round_trips() {
    let text = fixture("full_mvp.diagnostics.json");
    let report: DiagnosticsReport = serde_json::from_str(&text).unwrap();
    assert_eq!(to_canonical_string(&report).unwrap(), text);
}
