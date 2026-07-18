//! Views: named projections over the graph (filters + presentation, no
//! coordinates).

use std::collections::BTreeMap;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::docs::DocBlock;
use crate::entity::{AttrValue, LocalizedText};
use crate::ids::{EntityId, StageId, ViewId};
use crate::kind::{EntityKind, RelationKind};

/// An ordered stage/group within a view (e.g. a flow step lane).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct Stage {
    /// Stable identifier.
    pub id: StageId,
    /// Display title (localization-ready).
    pub title: LocalizedText,
    /// Ordering index within the view.
    pub order: u32,
}

/// A worked example attached to a view (e.g. sample output for a manual flow).
///
/// The example's [`content`](ViewExample::content) is **embedded** in the
/// document at generation time rather than referenced by path, so the explorer
/// renders it without the guarded `/api/source` endpoint and a static export is
/// self-contained. Producers embed only files a maintainer named explicitly in
/// configuration.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ViewExample {
    /// Stable identifier, unique within the view.
    pub id: String,
    /// Display title (localization-ready).
    pub title: LocalizedText,
    /// Syntax hint for display only (e.g. `json`, `http`, `sql`). Never
    /// interpreted or executed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub language: Option<String>,
    /// The example content, embedded verbatim (UTF-8).
    pub content: String,
    /// Optional prose about the example (localization-ready).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<LocalizedText>,
}

/// A named projection over the canonical entities/relations.
///
/// Views carry filters and presentation metadata but never UI coordinates
/// (layout is computed client-side).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct View {
    /// Stable identifier.
    pub id: ViewId,
    /// Display title (localization-ready).
    pub title: LocalizedText,
    /// Optional description (localization-ready).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<LocalizedText>,
    /// Entity-kind filter.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub entity_kinds: Vec<EntityKind>,
    /// Relation-kind filter.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub relation_kinds: Vec<RelationKind>,
    /// Explicit membership (else membership is derived from the filters).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub entity_ids: Option<Vec<EntityId>>,
    /// Ordered stages/groups.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub stages: Vec<Stage>,
    /// Optional default focus entity.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_focus: Option<EntityId>,
    /// Presentation hints (legend/grouping); never coordinates.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub presentation: BTreeMap<String, AttrValue>,
    /// View-level documentation (Markdown), e.g. what a manual flow describes.
    ///
    /// Optional and absent on the generated views; added in schema `1.1`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub docs: Option<DocBlock>,
    /// Worked examples whose contents are embedded in the document.
    ///
    /// Optional and empty on the generated views; added in schema `1.1`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub examples: Vec<ViewExample>,
}
