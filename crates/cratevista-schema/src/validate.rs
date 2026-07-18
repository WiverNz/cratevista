//! Structural validation errors for an [`crate::document::ExplorerDocument`].

/// A single validation problem. [`crate::document::ExplorerDocument::validate`]
/// collects all of these rather than failing on the first.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum SchemaError {
    /// Two entities share an id.
    #[error("duplicate entity id: {0}")]
    DuplicateEntityId(String),
    /// Two relations share an id.
    #[error("duplicate relation id: {0}")]
    DuplicateRelationId(String),
    /// Two views share an id.
    #[error("duplicate view id: {0}")]
    DuplicateViewId(String),
    /// A relation endpoint references a missing entity.
    #[error("relation `{relation}` references missing {end} entity `{entity}`")]
    DanglingRelationEndpoint {
        /// The relation id.
        relation: String,
        /// Which endpoint (`from` or `to`).
        end: &'static str,
        /// The missing entity id.
        entity: String,
    },
    /// An entity's parent references a missing entity.
    #[error("entity `{entity}` references missing parent `{parent}`")]
    DanglingParent {
        /// The entity id.
        entity: String,
        /// The missing parent id.
        parent: String,
    },
    /// A view's explicit membership references a missing entity.
    #[error("view `{view}` references missing entity `{entity}`")]
    DanglingViewEntity {
        /// The view id.
        view: String,
        /// The missing entity id.
        entity: String,
    },
    /// A view's `default_focus` references a missing entity.
    #[error("view `{view}` default_focus references missing entity `{entity}`")]
    DanglingDefaultFocus {
        /// The view id.
        view: String,
        /// The missing entity id.
        entity: String,
    },
}
