//! Cross-crate reliable-reference resolution using **only** the structured
//! `UnresolvedTypeRef` evidence (crate name, canonical path, item kind, role).
//! Never parses `display`, never fuzzy-matches, never emits `references_type`.

use std::collections::{BTreeMap, BTreeSet};

use cratevista_rustdoc::{CrateSummary, TypeReferenceRole, UnresolvedTypeRef};
use cratevista_schema::{
    DocumentDiagnostic, Entity, EntityId, Provenance, Relation, RelationId, RelationKind,
};

use crate::diagnostics::{code, warn};

/// The outcome of resolving all crates' unresolved references.
pub struct ResolveOutput {
    /// Newly-resolved reliable relations (endpoints guaranteed to exist).
    pub relations: Vec<Relation>,
    /// Diagnostics for unresolved / ambiguous references.
    pub diagnostics: Vec<DocumentDiagnostic>,
    /// Count of references resolved into relations.
    pub resolved: usize,
    /// Count of references left unresolved (preserved as diagnostics).
    pub unresolved: usize,
}

/// Resolvable nominal entity kinds.
fn is_resolvable_kind(kind: &str) -> bool {
    matches!(kind, "struct" | "enum" | "union" | "trait" | "type_alias")
}

/// The reliable relation kind for a reference role, or `None` for roles with no
/// approved relation contract (`AssociatedType` is reserved).
fn relation_kind_for(role: TypeReferenceRole) -> Option<&'static str> {
    match role {
        TypeReferenceRole::Field => Some(RelationKind::HAS_FIELD_TYPE),
        TypeReferenceRole::Parameter => Some(RelationKind::ACCEPTS_TYPE),
        TypeReferenceRole::Return => Some(RelationKind::RETURNS_TYPE),
        TypeReferenceRole::Error => Some(RelationKind::ERROR_TYPE),
        TypeReferenceRole::ImplFor => Some(RelationKind::IMPLEMENTED_FOR),
        TypeReferenceRole::ImplTrait => Some(RelationKind::IMPLEMENTS),
        TypeReferenceRole::AssociatedType => None,
    }
}

/// Resolves cross-crate references across the analyzed workspace crates.
pub fn resolve_cross_crate(
    entities: &BTreeMap<EntityId, Entity>,
    crates: &[CrateSummary],
) -> ResolveOutput {
    // Index resolvable entities by (crate, crate-relative path) → (kind, id).
    let mut by_path: BTreeMap<(String, String), Vec<(String, EntityId)>> = BTreeMap::new();
    for entity in entities.values() {
        if !is_resolvable_kind(entity.kind.as_str()) {
            continue;
        }
        if let Some((crate_name, relative)) = entity.qualified_name.split_once("::") {
            by_path
                .entry((crate_name.to_string(), relative.to_string()))
                .or_default()
                .push((entity.kind.as_str().to_string(), entity.id.clone()));
        }
    }

    let analyzed: BTreeSet<&str> = crates.iter().map(|c| c.crate_name.as_str()).collect();

    let mut relations = Vec::new();
    let mut diagnostics = Vec::new();
    let mut resolved = 0usize;
    let mut unresolved = 0usize;

    for summary in crates {
        for reference in &summary.unresolved_refs {
            let Some(kind) = relation_kind_for(reference.role) else {
                continue; // reserved role (AssociatedType): no approved relation
            };
            match resolve_one(reference, &analyzed, &by_path) {
                Resolution::One(target) => {
                    relations.push(typed(
                        kind,
                        reference.from.clone(),
                        target,
                        reference.role.relation_role(),
                    ));
                    resolved += 1;
                }
                Resolution::Zero => {
                    unresolved += 1;
                    diagnostics.push(warn(
                        code::UNRESOLVED_CROSS_CRATE_REFERENCE,
                        format!(
                            "unresolved cross-crate {} reference `{}`",
                            reference.role.relation_role(),
                            reference.display
                        ),
                        Some(reference.from.clone()),
                    ));
                }
                Resolution::Many => {
                    diagnostics.push(warn(
                        code::AMBIGUOUS_CROSS_CRATE_REFERENCE,
                        format!(
                            "ambiguous cross-crate {} reference `{}`",
                            reference.role.relation_role(),
                            reference.display
                        ),
                        Some(reference.from.clone()),
                    ));
                }
            }
        }
    }

    ResolveOutput {
        relations,
        diagnostics,
        resolved,
        unresolved,
    }
}

enum Resolution {
    One(EntityId),
    Zero,
    Many,
}

fn resolve_one(
    reference: &UnresolvedTypeRef,
    analyzed: &BTreeSet<&str>,
    by_path: &BTreeMap<(String, String), Vec<(String, EntityId)>>,
) -> Resolution {
    // Require structured evidence.
    let (Some(crate_name), Some(path)) = (&reference.crate_name, &reference.canonical_path) else {
        return Resolution::Zero;
    };
    if !analyzed.contains(crate_name.as_str()) {
        return Resolution::Zero; // external / non-analyzed crate
    }
    // Drop the leading crate segment to get the crate-relative path.
    let relative: Vec<&str> = match path.split_first() {
        Some((first, rest)) if first == crate_name => rest.iter().map(String::as_str).collect(),
        _ => path.iter().map(String::as_str).collect(),
    };
    if relative.is_empty() {
        return Resolution::Zero;
    }
    let key = (crate_name.clone(), relative.join("::"));
    let Some(candidates) = by_path.get(&key) else {
        return Resolution::Zero;
    };

    // Constrain by item_kind when available.
    let matching: Vec<&EntityId> = candidates
        .iter()
        .filter(|(kind, _)| {
            reference
                .item_kind
                .as_ref()
                .is_none_or(|wanted| wanted.as_str() == kind)
        })
        .map(|(_, id)| id)
        .collect();

    match matching.as_slice() {
        [one] => Resolution::One((*one).clone()),
        [] => Resolution::Zero,
        _ => Resolution::Many,
    }
}

fn typed(kind: &str, from: EntityId, to: EntityId, role: &str) -> Relation {
    let kind = RelationKind::new(kind);
    let id = RelationId::with_role(&kind, &from, &to, role);
    Relation {
        id,
        kind,
        from,
        to,
        role: Some(role.to_string()),
        label: None,
        provenance: Provenance::Discovered,
        attributes: Default::default(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::{crate_summary, item_entity, unresolved_ref};

    fn index(entities: Vec<Entity>) -> BTreeMap<EntityId, Entity> {
        entities.into_iter().map(|e| (e.id.clone(), e)).collect()
    }

    #[test]
    fn external_crate_reference_stays_unresolved() {
        let entities = index(vec![item_entity("item:struct:a::X", "struct", "a::X")]);
        // Reference to `core::any::Any` — `core` is not an analyzed crate.
        let reference = unresolved_ref(
            "item:struct:a::X",
            TypeReferenceRole::Field,
            Some("core"),
            Some(vec!["core", "any", "Any"]),
            Some("trait"),
            "Any",
        );
        let crates = vec![crate_summary(
            "a",
            "package:a",
            "target:a:lib:a",
            "module:a::a",
            vec![reference],
        )];
        let out = resolve_cross_crate(&entities, &crates);
        assert!(out.relations.is_empty());
        assert_eq!(out.unresolved, 1);
        assert_eq!(
            out.diagnostics[0].code,
            code::UNRESOLVED_CROSS_CRATE_REFERENCE
        );
    }

    #[test]
    fn no_structured_evidence_stays_unresolved() {
        let entities = index(vec![item_entity("item:struct:a::X", "struct", "a::X")]);
        let reference = unresolved_ref(
            "item:struct:a::X",
            TypeReferenceRole::Return,
            None,
            None,
            None,
            "Mystery",
        );
        let crates = vec![crate_summary(
            "a",
            "package:a",
            "target:a:lib:a",
            "module:a::a",
            vec![reference],
        )];
        let out = resolve_cross_crate(&entities, &crates);
        assert_eq!(out.unresolved, 1);
        assert!(out.relations.is_empty());
    }

    #[test]
    fn ambiguous_reference_emits_no_edge() {
        // Two distinct entities share the same (crate, relative path, kind).
        let entities = index(vec![
            item_entity("item:struct:b::Widget#1", "struct", "b::Widget"),
            item_entity("item:struct:b::Widget#2", "struct", "b::Widget"),
        ]);
        let reference = unresolved_ref(
            "item:field:a::F",
            TypeReferenceRole::Field,
            Some("b"),
            Some(vec!["b", "Widget"]),
            Some("struct"),
            "Widget",
        );
        let crates = vec![
            crate_summary(
                "a",
                "package:a",
                "target:a:lib:a",
                "module:a::a",
                vec![reference],
            ),
            crate_summary("b", "package:b", "target:b:lib:b", "module:b::b", vec![]),
        ];
        let out = resolve_cross_crate(&entities, &crates);
        assert!(out.relations.is_empty());
        assert_eq!(
            out.diagnostics[0].code,
            code::AMBIGUOUS_CROSS_CRATE_REFERENCE
        );
    }

    #[test]
    fn never_emits_references_type() {
        // Whatever resolves, the kind is one of the six reliable kinds only.
        let entities = index(vec![item_entity(
            "item:struct:b::Widget",
            "struct",
            "b::Widget",
        )]);
        let reference = unresolved_ref(
            "item:field:a::F",
            TypeReferenceRole::Field,
            Some("b"),
            Some(vec!["b", "Widget"]),
            Some("struct"),
            "Widget",
        );
        let crates = vec![
            crate_summary(
                "a",
                "package:a",
                "target:a:lib:a",
                "module:a::a",
                vec![reference],
            ),
            crate_summary("b", "package:b", "target:b:lib:b", "module:b::b", vec![]),
        ];
        let out = resolve_cross_crate(&entities, &crates);
        assert!(
            out.relations
                .iter()
                .all(|r| r.kind.as_str() != "references_type")
        );
        assert_eq!(out.resolved, 1);
    }
}
