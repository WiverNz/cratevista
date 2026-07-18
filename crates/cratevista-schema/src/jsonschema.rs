//! JSON Schema generation for the explorer document.
//!
//! The Rust types are the single source of truth. [`document_schema`] builds the
//! schema (via `schemars`); the checked-in artifact
//! `crates/cratevista-schema/schema/cratevista-document.schema.json` is produced
//! by `examples/gen_schema.rs` and guarded by `tests/jsonschema_drift.rs` — both
//! call this function and [`crate::canonical::to_canonical_string`].

use serde_json::Value;

use crate::document::ExplorerDocument;

/// Builds the JSON Schema for [`ExplorerDocument`] as a JSON value.
pub fn document_schema() -> Value {
    let schema = schemars::schema_for!(ExplorerDocument);
    serde_json::to_value(schema).expect("schema serializes to a JSON value")
}

/// Builds the canonical JSON Schema string (the checked-in artifact form).
pub fn document_schema_json() -> String {
    crate::canonical::to_canonical_string(&document_schema())
        .expect("schema value serializes canonically")
}
