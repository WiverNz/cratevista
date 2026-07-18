//! Typed, directed relations between entities.

use std::collections::BTreeMap;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::entity::{AttrValue, LocalizedText, Provenance};
use crate::ids::{EntityId, RelationId};
use crate::kind::RelationKind;

/// A typed, directed edge between two entities.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Relation {
    /// Stable identifier.
    pub id: RelationId,
    /// Open, string-backed kind.
    pub kind: RelationKind,
    /// Source entity id.
    pub from: EntityId,
    /// Target entity id.
    pub to: EntityId,
    /// Optional semantic role, disambiguating multiple same-kind relations
    /// between the same endpoints.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub role: Option<String>,
    /// Optional edge label (localization-ready).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<LocalizedText>,
    /// Discovered vs manual provenance.
    pub provenance: Provenance,
    /// Freeform attributes (e.g. protocol labels such as HTTP/SQL).
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub attributes: BTreeMap<String, AttrValue>,
}

impl Relation {
    /// Builds a minimal relation with the basic id derived from its endpoints.
    pub fn new(kind: RelationKind, from: EntityId, to: EntityId, provenance: Provenance) -> Self {
        let id = RelationId::basic(&kind, &from, &to);
        Relation {
            id,
            kind,
            from,
            to,
            role: None,
            label: None,
            provenance,
            attributes: BTreeMap::new(),
        }
    }
}
