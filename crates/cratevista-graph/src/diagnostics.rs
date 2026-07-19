//! Stable graph diagnostic codes and small builders.
//!
//! These are recoverable `DocumentDiagnostic`s serialized only into
//! `diagnostics.json`; they are never embedded in the `ExplorerDocument`.

use cratevista_schema::{DocumentDiagnostic, EntityId, RelationId, Severity};

/// Recoverable/informational diagnostic codes emitted by the graph builder.
pub mod code {
    /// Two entities carried identical or redundant duplicate evidence, or an
    /// attribute value conflicted.
    pub const DUPLICATE_ENTITY_EVIDENCE: &str = "duplicate_entity_evidence";
    /// Two entities sharing an id disagreed on `kind`.
    pub const CONFLICTING_ENTITY_KIND: &str = "conflicting_entity_kind";
    /// Two entities sharing an id disagreed on `parent`.
    pub const CONFLICTING_ENTITY_PARENT: &str = "conflicting_entity_parent";
    /// Two relations sharing an id disagreed on payload.
    pub const CONFLICTING_RELATION_EVIDENCE: &str = "conflicting_relation_evidence";
    /// A relation endpoint referenced a missing entity; the relation was dropped.
    pub const DANGLING_RELATION: &str = "dangling_relation";
    /// A rustdoc crate could not be linked to exactly one metadata target.
    pub const RUSTDOC_TARGET_UNLINKED: &str = "rustdoc_target_unlinked";
    /// A cross-crate type reference into an **analyzed workspace crate** could not
    /// be resolved to an entity — a genuine gap worth surfacing per occurrence.
    pub const UNRESOLVED_CROSS_CRATE_REFERENCE: &str = "unresolved_cross_crate_reference";
    /// One **aggregated, informational** summary (per external crate) of references
    /// to a non-analyzed dependency that are, as expected, not represented as
    /// workspace entities. Replaces the per-occurrence flood for external types.
    /// Only references to a **known** external crate are downgraded to this Info.
    pub const EXTERNAL_CRATE_REFERENCE: &str = "external_crate_reference";
    /// A cross-crate reference with **insufficient crate evidence** — it cannot be
    /// attributed to any crate, so it is neither confirmed external nor a workspace
    /// gap. Kept as a per-occurrence **warning**, never folded into external Info.
    pub const UNRESOLVED_REFERENCE_UNKNOWN_TARGET: &str = "unresolved_reference_unknown_target";
    /// A cross-crate type reference matched more than one candidate.
    pub const AMBIGUOUS_CROSS_CRATE_REFERENCE: &str = "ambiguous_cross_crate_reference";
    /// A view referenced a missing entity (defensive; filter-based views avoid this).
    pub const INVALID_VIEW_REFERENCE: &str = "invalid_view_reference";
    /// An overlay override targeted an entity that does not exist.
    pub const OVERLAY_TARGET_MISSING: &str = "overlay_target_missing";
    /// Rustdoc ingestion was intentionally disabled for this run.
    pub const RUSTDOC_DISABLED: &str = "rustdoc_disabled";
    /// The workspace had no default-documentable library/proc-macro target.
    pub const NO_DOCUMENTABLE_RUSTDOC_TARGETS: &str = "no_documentable_rustdoc_targets";
}

/// Builds a warning-level diagnostic optionally referencing an entity.
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

/// Builds an info-level diagnostic optionally referencing an entity.
pub fn info(
    code: &str,
    message: impl Into<String>,
    entity: Option<EntityId>,
) -> DocumentDiagnostic {
    let mut diagnostic = DocumentDiagnostic::new(Severity::Info, code, message);
    if let Some(entity) = entity {
        diagnostic.entities.push(entity);
    }
    diagnostic
}

/// Builds a warning-level diagnostic referencing a relation.
pub fn warn_relation(
    code: &str,
    message: impl Into<String>,
    relation: RelationId,
) -> DocumentDiagnostic {
    let mut diagnostic = DocumentDiagnostic::new(Severity::Warning, code, message);
    diagnostic.relations.push(relation);
    diagnostic
}
