//! The root explorer document (`document.json`) and its assembly/validation.

use std::collections::{BTreeMap, BTreeSet};

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::entity::Entity;
use crate::ids::EntityId;
use crate::relation::Relation;
use crate::source::SourceLocation;
use crate::validate::SchemaError;
use crate::version::SchemaVersion;
use crate::view::View;

/// Project/workspace metadata.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct Project {
    /// Stable project id.
    pub id: String,
    /// Human-readable project name.
    pub name: String,
    /// Short description.
    pub description: String,
    /// Optional repository-relative root location.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub root: Option<SourceLocation>,
    /// Optional repository URL.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub repository_url: Option<String>,
    /// Optional default branch.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_branch: Option<String>,
}

/// The canonical explorer document serialized to `document.json`.
///
/// Deterministic: it carries no timestamps, no runtime metadata, and no
/// diagnostics (those are separate `generation.json` / `diagnostics.json`
/// artifacts). Entities, relations, and views are sorted by id.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ExplorerDocument {
    /// Schema version of this artifact.
    pub schema_version: SchemaVersion,
    /// Project/workspace metadata.
    pub project: Project,
    /// Entities, sorted by id.
    pub entities: Vec<Entity>,
    /// Relations, sorted by id.
    pub relations: Vec<Relation>,
    /// Views, sorted by id.
    pub views: Vec<View>,
}

impl ExplorerDocument {
    /// Assembles a document from parts, sorting entities/relations/views by id
    /// for deterministic output. Duplicate ids are reported by [`Self::validate`].
    pub fn new(
        project: Project,
        mut entities: Vec<Entity>,
        mut relations: Vec<Relation>,
        mut views: Vec<View>,
    ) -> Self {
        entities.sort_by(|a, b| a.id.cmp(&b.id));
        relations.sort_by(|a, b| a.id.cmp(&b.id));
        views.sort_by(|a, b| a.id.cmp(&b.id));
        ExplorerDocument {
            schema_version: SchemaVersion::current(),
            project,
            entities,
            relations,
            views,
        }
    }

    /// Returns an id → entity index (not serialized).
    pub fn index(&self) -> BTreeMap<&EntityId, &Entity> {
        self.entities.iter().map(|e| (&e.id, e)).collect()
    }

    /// Validates referential integrity, collecting **all** problems.
    ///
    /// Checks duplicate ids and dangling references (relation endpoints, entity
    /// parents, view membership, and view `default_focus`). Unknown kinds are
    /// not errors.
    pub fn validate(&self) -> Result<(), Vec<SchemaError>> {
        let mut errors = Vec::new();

        // Duplicate ids + the set of known entity ids.
        let mut entity_ids: BTreeSet<&EntityId> = BTreeSet::new();
        for entity in &self.entities {
            if !entity_ids.insert(&entity.id) {
                errors.push(SchemaError::DuplicateEntityId(entity.id.to_string()));
            }
        }
        let mut relation_ids = BTreeSet::new();
        for relation in &self.relations {
            if !relation_ids.insert(&relation.id) {
                errors.push(SchemaError::DuplicateRelationId(relation.id.to_string()));
            }
        }
        let mut view_ids = BTreeSet::new();
        for view in &self.views {
            if !view_ids.insert(&view.id) {
                errors.push(SchemaError::DuplicateViewId(view.id.to_string()));
            }
        }

        // Dangling references.
        for relation in &self.relations {
            if !entity_ids.contains(&relation.from) {
                errors.push(SchemaError::DanglingRelationEndpoint {
                    relation: relation.id.to_string(),
                    end: "from",
                    entity: relation.from.to_string(),
                });
            }
            if !entity_ids.contains(&relation.to) {
                errors.push(SchemaError::DanglingRelationEndpoint {
                    relation: relation.id.to_string(),
                    end: "to",
                    entity: relation.to.to_string(),
                });
            }
        }
        for entity in &self.entities {
            if let Some(parent) = &entity.parent
                && !entity_ids.contains(parent)
            {
                errors.push(SchemaError::DanglingParent {
                    entity: entity.id.to_string(),
                    parent: parent.to_string(),
                });
            }
        }
        for view in &self.views {
            if let Some(members) = &view.entity_ids {
                for member in members {
                    if !entity_ids.contains(member) {
                        errors.push(SchemaError::DanglingViewEntity {
                            view: view.id.to_string(),
                            entity: member.to_string(),
                        });
                    }
                }
            }
            if let Some(focus) = &view.default_focus
                && !entity_ids.contains(focus)
            {
                errors.push(SchemaError::DanglingDefaultFocus {
                    view: view.id.to_string(),
                    entity: focus.to_string(),
                });
            }
        }

        if errors.is_empty() {
            Ok(())
        } else {
            Err(errors)
        }
    }
}
