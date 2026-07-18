//! The pure build result and its path-free summary.
//!
//! Nothing here contains serialized JSON, a `GenerationReport`, a
//! `DiagnosticsReport`, a `cratevista_core::Diagnostic`, timestamps, durations,
//! filesystem paths, CLI output, or UI coordinates.

use cratevista_schema::{DocumentDiagnostic, ExplorerDocument};

/// The pure output of `build_document`.
#[derive(Debug, Clone, PartialEq)]
pub struct GraphBuildResult {
    /// The schema-valid, deterministic document (no timestamps).
    pub document: ExplorerDocument,
    /// The sorted union of input + graph-produced diagnostics.
    pub diagnostics: Vec<DocumentDiagnostic>,
    /// Path-free counts for the CLI / `GenerationReport`.
    pub summary: GraphBuildSummary,
    /// `true` when the underlying rustdoc result was partial.
    pub partial: bool,
}

/// Path-free counts describing a build.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GraphBuildSummary {
    /// Number of entities in the document.
    pub entity_count: usize,
    /// Number of relations in the document.
    pub relation_count: usize,
    /// Number of views in the document.
    pub view_count: usize,
    /// Number of diagnostics.
    pub diagnostic_count: usize,
    /// Number of rustdoc crates documented.
    pub documented_crate_count: usize,
    /// Number of preserved unresolved cross-crate references.
    pub unresolved_reference_count: usize,
    /// Number of cross-crate references resolved into relations.
    pub resolved_cross_crate_count: usize,
    /// Workspace-wide public-item documentation coverage percent, if computed.
    pub coverage_percent: Option<u8>,
}
