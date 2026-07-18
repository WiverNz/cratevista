//! Stable recoverable-diagnostic codes and a small builder.

use cratevista_schema::{DocumentDiagnostic, EntityId, Severity};

/// Recoverable diagnostic codes emitted by metadata ingestion. Fatal problems
/// use [`crate::MetadataError`] instead.
pub mod code {
    /// A target kind CrateVista does not specifically recognize.
    pub const UNSUPPORTED_TARGET: &str = "unsupported_target";
    /// An external identity could not be made portable.
    pub const NON_PORTABLE_PATH_IDENTITY: &str = "non_portable_path_identity";
    /// A source path lies outside the selected workspace root.
    pub const SOURCE_OUTSIDE_WORKSPACE: &str = "source_outside_workspace";
    /// Two entities generated the same id; a deterministic fallback was applied.
    pub const DUPLICATE_GENERATED_ID: &str = "duplicate_generated_id";
    /// A path was not valid UTF-8 / repo-relative and its source was dropped.
    pub const NON_UTF8_PATH: &str = "non_utf8_path";
    /// An external package identity was omitted (e.g. excluded or unresolvable).
    pub const OMITTED_EXTERNAL_IDENTITY: &str = "omitted_external_identity";
    /// Optional metadata was incomplete; ingestion proceeded with less detail.
    pub const INCOMPLETE_OPTIONAL_METADATA: &str = "incomplete_optional_metadata";
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
