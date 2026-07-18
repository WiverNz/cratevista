//! Inputs to the pure builder: the ingested data, build options, and the
//! issue-08 overlay seam.

use std::collections::BTreeMap;

use cratevista_metadata::MetadataIngest;
use cratevista_rustdoc::RustdocIngest;
use cratevista_schema::{AttrValue, DocBlock, Entity, EntityId, LocalizedText, Relation, View};

/// Everything the pure builder needs. Assembled by `cratevista-core`.
///
/// No separate plan-link collection is required: `RustdocIngest`'s
/// `CrateSummary` already carries `package_id`/`target_id`/`root_module_id`, so
/// the graph links each rustdoc crate to its Cargo target directly.
pub struct GraphInput {
    /// Cargo metadata ingestion result (PRD 03).
    pub metadata: MetadataIngest,
    /// Normalized rustdoc ingestion result (PRD 04); `None` = metadata-only.
    pub rustdoc: Option<RustdocIngest>,
    /// The issue-08 overlay seam; `GraphOverlay::default()` (empty) is a normal input.
    pub overlay: GraphOverlay,
}

/// Options for a single build. External-dependency selection is **not** here:
/// which external package entities / `depends_on` relations exist is owned by
/// metadata ingestion (`MetadataOptions.external_deps`). The graph never applies
/// a second external-dependency filter.
#[derive(Debug, Clone)]
pub struct GraphBuildOptions {
    /// Attach documentation-coverage attributes + the coverage view.
    pub compute_coverage: bool,
    /// Retain the stable view set even when a view is empty.
    pub retain_empty_views: bool,
}

impl Default for GraphBuildOptions {
    fn default() -> Self {
        GraphBuildOptions {
            compute_coverage: true,
            retain_empty_views: true,
        }
    }
}

/// The issue-08 overlay seam: a plain, dependency-free struct of schema types.
/// The empty overlay is a fully supported normal input.
#[derive(Debug, Clone, Default)]
pub struct GraphOverlay {
    /// Manual entity additions (forced to `Provenance::Manual`).
    pub entities: Vec<Entity>,
    /// Manual relation additions (forced to `Provenance::Manual`).
    pub relations: Vec<Relation>,
    /// Presentation-only overrides keyed by discovered entity id.
    pub overrides: BTreeMap<EntityId, EntityOverride>,
    /// Manual flow views (may carry stages).
    pub views: Vec<View>,
}

/// A presentation-only override. It never changes a discovered entity's id,
/// kind, structural parent, or source identity — those fields are absent.
#[derive(Debug, Clone, Default)]
pub struct EntityOverride {
    /// Replace the display label.
    pub label: Option<LocalizedText>,
    /// Replace/set the description.
    pub description: Option<LocalizedText>,
    /// Tags to add (unioned, sorted, deduped).
    pub add_tags: Vec<String>,
    /// Presentation attributes to set (e.g. `category`, `hidden`).
    pub set_attributes: BTreeMap<String, AttrValue>,
    /// Mark the entity hidden in default views.
    pub hidden: Option<bool>,
    /// Manual documentation **appended after** whatever was discovered.
    ///
    /// Unlike `label`/`description`, which replace, this is additive
    /// enrichment: the discovered Markdown is kept and the manual Markdown
    /// follows it. The discovered `summary` and `documented` flag always
    /// survive — in particular an override can never flip `documented`, because
    /// it feeds documentation-**coverage**, which measures Rust documentation
    /// and must not be movable from configuration.
    pub docs: Option<DocBlock>,
}
