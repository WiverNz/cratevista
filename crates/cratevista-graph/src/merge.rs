//! Deterministic field-level entity and relation merge with semantic ownership.
//!
//! Metadata and rustdoc id spaces are disjoint in practice, so most items pass
//! through unmerged; merge still applies wherever two items share an id (overlay
//! additions, or a future producer). Identical evidence deduplicates;
//! complementary fields enrich; incompatible `kind`/`parent`/attribute conflicts
//! are diagnosed and never silently overwritten.

use std::collections::BTreeMap;

use cratevista_schema::{DocumentDiagnostic, Entity, EntityId, Relation, RelationId};

use crate::diagnostics::{code, warn, warn_relation};

/// Merges `entity` into `map`, emitting diagnostics for conflicts. The
/// already-present entry is treated as the owner (metadata is inserted before
/// rustdoc before overlay, matching semantic ownership of the disjoint id spaces).
pub fn merge_entity(
    map: &mut BTreeMap<EntityId, Entity>,
    entity: Entity,
    diagnostics: &mut Vec<DocumentDiagnostic>,
) {
    match map.get_mut(&entity.id) {
        None => {
            map.insert(entity.id.clone(), entity);
        }
        Some(existing) => {
            if *existing == entity {
                return; // identical evidence → dedup silently
            }
            merge_into(existing, entity, diagnostics);
        }
    }
}

fn merge_into(existing: &mut Entity, new: Entity, diagnostics: &mut Vec<DocumentDiagnostic>) {
    // kind: never silently overwrite a differing kind.
    if existing.kind != new.kind {
        diagnostics.push(warn(
            code::CONFLICTING_ENTITY_KIND,
            format!(
                "entity `{}` has conflicting kinds `{}` vs `{}`; keeping the first",
                existing.id,
                existing.kind.as_str(),
                new.kind.as_str()
            ),
            Some(existing.id.clone()),
        ));
    }

    // label: enrich only when the owner's is empty.
    if existing.label.default.is_empty() && !new.label.default.is_empty() {
        existing.label = new.label;
    }

    // parent: enrich a missing parent; diagnose a real conflict.
    match (&existing.parent, &new.parent) {
        (None, Some(parent)) => existing.parent = Some(parent.clone()),
        (Some(current), Some(incoming)) if current != incoming => {
            diagnostics.push(warn(
                code::CONFLICTING_ENTITY_PARENT,
                format!(
                    "entity `{}` has conflicting parents `{current}` vs `{incoming}`; keeping the first",
                    existing.id
                ),
                Some(existing.id.clone()),
            ));
        }
        _ => {}
    }

    // source / docs / description: enrich when absent.
    if existing.source.is_none() {
        existing.source = new.source;
    }
    if existing.docs.is_none() {
        existing.docs = new.docs;
    }
    if existing.description.is_none() {
        existing.description = new.description;
    }

    // attributes: union; identical dedup; differing → keep owner + diagnose.
    for (key, value) in new.attributes {
        match existing.attributes.get(&key) {
            None => {
                existing.attributes.insert(key, value);
            }
            Some(current) if *current == value => {}
            Some(_) => diagnostics.push(warn(
                code::DUPLICATE_ENTITY_EVIDENCE,
                format!(
                    "entity `{}` has conflicting attribute `{key}`; keeping the first value",
                    existing.id
                ),
                Some(existing.id.clone()),
            )),
        }
    }

    // tags: union, sorted, deduped.
    existing.tags.extend(new.tags);
    existing.tags.sort();
    existing.tags.dedup();
}

/// Merges `relation` into `map`, emitting `conflicting_relation_evidence` on a
/// same-id payload conflict (the first payload is kept). Distinct ids — which
/// already encode kind/from/to/role/cfg — are always kept separately.
pub fn merge_relation(
    map: &mut BTreeMap<RelationId, Relation>,
    relation: Relation,
    diagnostics: &mut Vec<DocumentDiagnostic>,
) {
    match map.get(&relation.id) {
        None => {
            map.insert(relation.id.clone(), relation);
        }
        Some(existing) => {
            if *existing != relation {
                diagnostics.push(warn_relation(
                    code::CONFLICTING_RELATION_EVIDENCE,
                    format!(
                        "relation `{}` has conflicting evidence; keeping the first",
                        relation.id
                    ),
                    relation.id.clone(),
                ));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::entity_with_kind;
    use cratevista_schema::{Provenance, RelationKind};

    #[test]
    fn identical_entities_dedup_silently() {
        let mut map = BTreeMap::new();
        let mut diags = Vec::new();
        merge_entity(
            &mut map,
            entity_with_kind("x", "struct", "c::X"),
            &mut diags,
        );
        merge_entity(
            &mut map,
            entity_with_kind("x", "struct", "c::X"),
            &mut diags,
        );
        assert_eq!(map.len(), 1);
        assert!(diags.is_empty());
    }

    #[test]
    fn complementary_parent_enriches() {
        let mut map = BTreeMap::new();
        let mut diags = Vec::new();
        merge_entity(
            &mut map,
            entity_with_kind("x", "struct", "c::X"),
            &mut diags,
        );
        let mut b = entity_with_kind("x", "struct", "c::X");
        b.parent = Some(EntityId::from_raw("module:c::c"));
        merge_entity(&mut map, b, &mut diags);
        assert_eq!(
            map[&EntityId::from_raw("x")]
                .parent
                .as_ref()
                .unwrap()
                .as_str(),
            "module:c::c"
        );
        assert!(diags.is_empty());
    }

    #[test]
    fn kind_conflict_is_diagnosed_not_overwritten() {
        let mut map = BTreeMap::new();
        let mut diags = Vec::new();
        merge_entity(
            &mut map,
            entity_with_kind("x", "struct", "c::X"),
            &mut diags,
        );
        merge_entity(&mut map, entity_with_kind("x", "enum", "c::X"), &mut diags);
        assert_eq!(map[&EntityId::from_raw("x")].kind.as_str(), "struct");
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].code, code::CONFLICTING_ENTITY_KIND);
    }

    #[test]
    fn parent_conflict_is_diagnosed() {
        let mut map = BTreeMap::new();
        let mut diags = Vec::new();
        let mut a = entity_with_kind("x", "struct", "c::X");
        a.parent = Some(EntityId::from_raw("module:c::a"));
        let mut b = entity_with_kind("x", "struct", "c::X");
        b.parent = Some(EntityId::from_raw("module:c::b"));
        merge_entity(&mut map, a, &mut diags);
        merge_entity(&mut map, b, &mut diags);
        assert_eq!(
            map[&EntityId::from_raw("x")]
                .parent
                .as_ref()
                .unwrap()
                .as_str(),
            "module:c::a"
        );
        assert_eq!(diags[0].code, code::CONFLICTING_ENTITY_PARENT);
    }

    #[test]
    fn attribute_conflict_keeps_first_and_diagnoses() {
        let mut map = BTreeMap::new();
        let mut diags = Vec::new();
        let mut a = entity_with_kind("x", "struct", "c::X");
        a.attributes.insert("visibility".into(), "public".into());
        let mut b = entity_with_kind("x", "struct", "c::X");
        b.attributes.insert("visibility".into(), "crate".into());
        merge_entity(&mut map, a, &mut diags);
        merge_entity(&mut map, b, &mut diags);
        assert_eq!(
            map[&EntityId::from_raw("x")].attributes["visibility"],
            "public"
        );
        assert_eq!(diags[0].code, code::DUPLICATE_ENTITY_EVIDENCE);
    }

    fn relation(from: &str, to: &str) -> Relation {
        Relation::new(
            RelationKind::new("contains"),
            EntityId::from_raw(from),
            EntityId::from_raw(to),
            Provenance::Discovered,
        )
    }

    #[test]
    fn identical_relations_dedup() {
        let mut map = BTreeMap::new();
        let mut diags = Vec::new();
        merge_relation(&mut map, relation("a", "b"), &mut diags);
        merge_relation(&mut map, relation("a", "b"), &mut diags);
        assert_eq!(map.len(), 1);
        assert!(diags.is_empty());
    }

    #[test]
    fn conflicting_relation_payload_is_diagnosed() {
        let mut map = BTreeMap::new();
        let mut diags = Vec::new();
        let mut a = relation("a", "b");
        let mut b = relation("a", "b");
        a.attributes.insert("k".into(), "1".into());
        b.attributes.insert("k".into(), "2".into());
        merge_relation(&mut map, a, &mut diags);
        merge_relation(&mut map, b, &mut diags);
        assert_eq!(map.len(), 1);
        assert_eq!(diags[0].code, code::CONFLICTING_RELATION_EVIDENCE);
    }
}
