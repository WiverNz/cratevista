//! Prints the canonical JSON Schema for `ExplorerDocument` to stdout.
//!
//! Regenerate the checked-in artifact with:
//!
//! ```bash
//! cargo run -p cratevista-schema --example gen_schema \
//!   > crates/cratevista-schema/schema/cratevista-document.schema.json
//! ```
//!
//! This calls the same functions as `tests/jsonschema_drift.rs`, so they cannot
//! diverge. The output already ends with exactly one newline; use `print!`.

fn main() {
    print!("{}", cratevista_schema::jsonschema::document_schema_json());
}
