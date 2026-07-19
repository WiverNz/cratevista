//! Stable recoverable-diagnostic codes and a small builder.
//!
//! Fatal problems use [`crate::RustdocError`] instead. These never become a
//! `cratevista_core::Diagnostic` and are never embedded in an `ExplorerDocument`.

use cratevista_schema::{DocumentDiagnostic, EntityId, Severity};

/// Recoverable diagnostic codes emitted by rustdoc normalization.
pub mod code {
    /// A source path lies outside the selected workspace root; source omitted.
    pub const SOURCE_OUTSIDE_WORKSPACE: &str = "source_outside_workspace";
    /// A generated/macro-expanded/synthetic source location was omitted.
    pub const GENERATED_SOURCE_OMITTED: &str = "generated_source_omitted";
    /// No deterministic canonical path could be reconstructed; item skipped.
    pub const MISSING_CANONICAL_PATH: &str = "missing_canonical_path";
    /// A type reference could not be resolved within this crate's index.
    pub const UNRESOLVED_TYPE_REFERENCE: &str = "unresolved_type_reference";
    /// Two items produced the same identity; a fallback was applied.
    pub const DUPLICATE_ITEM_IDENTITY: &str = "duplicate_item_identity";
    /// A rustdoc item form CrateVista does not specifically map.
    pub const UNSUPPORTED_RUSTDOC_ITEM: &str = "unsupported_rustdoc_item";
    /// Optional item metadata was incomplete; normalization proceeded.
    pub const INCOMPLETE_ITEM_METADATA: &str = "incomplete_item_metadata";
    /// A selected target failed under `--keep-going`.
    pub const TARGET_FAILED: &str = "target_failed";
    /// A re-export target was absent from the index.
    pub const REEXPORT_TARGET_MISSING: &str = "reexport_target_missing";
}

/// Builds a warning-level [`DocumentDiagnostic`] optionally referencing an entity.
pub fn warn(
    code: &str,
    message: impl Into<String>,
    entity: Option<EntityId>,
) -> DocumentDiagnostic {
    let mut diagnostic = DocumentDiagnostic::new(Severity::Warning, code, message);
    if let Some(entity) = entity {
        diagnostic.entities.push(entity);
    }
    diagnostic
}

/// Builds an informational [`DocumentDiagnostic`] (e.g. an aggregated summary).
pub fn info(code: &str, message: impl Into<String>) -> DocumentDiagnostic {
    DocumentDiagnostic::new(Severity::Info, code, message)
}
