//! The checked-in JSON Schema must not drift from the Rust types, and every
//! document fixture must validate against it.
//!
//! The regenerator (`examples/gen_schema.rs`) and this test call the same
//! `document_schema_json()` + canonical serializer, so they cannot diverge.

use std::path::PathBuf;

fn crate_path(parts: &[&str]) -> PathBuf {
    let mut path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    for part in parts {
        path.push(part);
    }
    path
}

#[test]
fn checked_in_schema_matches_rust_types() {
    let generated = cratevista_schema::jsonschema::document_schema_json();
    let committed =
        std::fs::read_to_string(crate_path(&["schema", "cratevista-document.schema.json"]))
            .expect("checked-in schema artifact exists");
    assert_eq!(
        generated, committed,
        "JSON Schema drift. Regenerate with:\n  cargo run -p cratevista-schema --example gen_schema > crates/cratevista-schema/schema/cratevista-document.schema.json"
    );
}

#[test]
fn document_fixtures_validate_against_schema() {
    let schema = cratevista_schema::jsonschema::document_schema();
    let validator = jsonschema::validator_for(&schema).expect("schema compiles");

    for name in [
        "minimal.document.json",
        "full_mvp.document.json",
        "manual_flow.document.json",
        "unknown_kind.document.json",
    ] {
        let text = std::fs::read_to_string(crate_path(&["fixtures", name])).unwrap();
        let instance: serde_json::Value = serde_json::from_str(&text).unwrap();
        assert!(
            validator.is_valid(&instance),
            "{name} does not validate against the generated JSON Schema"
        );
    }
}
