//! Cross-crate reliable-reference resolution using **only** the structured
//! `UnresolvedTypeRef` evidence (crate name, canonical path, item kind, role).
//! Never parses `display`, never fuzzy-matches, never emits `references_type`.

use std::collections::{BTreeMap, BTreeSet};

use cratevista_rustdoc::{CrateSummary, TypeReferenceRole, UnresolvedTypeRef};
use cratevista_schema::{
    DocumentDiagnostic, Entity, EntityId, Provenance, Relation, RelationId, RelationKind,
};

use crate::diagnostics::{code, info, warn};

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
    // References to non-analyzed (external) crates are EXPECTED — they cannot become
    // workspace entities. Rather than one warning per occurrence, they are counted
    // per external crate (empty key = no crate evidence) and summarized once each.
    let mut external: BTreeMap<String, usize> = BTreeMap::new();

    for summary in crates {
        for reference in &summary.unresolved_refs {
            let Some(kind) = relation_kind_for(reference.role) else {
                continue; // reserved role (AssociatedType): no approved relation
            };
            match resolve_one(reference, &analyzed, &by_path) {
                Resolution::ResolvedWorkspace(target) => {
                    relations.push(typed(
                        kind,
                        reference.from.clone(),
                        target,
                        reference.role.relation_role(),
                    ));
                    resolved += 1;
                }
                Resolution::UnresolvedWorkspace => {
                    // The target crate IS analyzed but the item was not found: a real
                    // gap that should resolve. Kept as a per-occurrence warning.
                    unresolved += 1;
                    diagnostics.push(warn(
                        code::UNRESOLVED_CROSS_CRATE_REFERENCE,
                        format!(
                            "unresolved cross-crate {} reference `{}` (target crate is in the workspace but the item was not found)",
                            reference.role.relation_role(),
                            reference.display
                        ),
                        Some(reference.from.clone()),
                    ));
                }
                Resolution::ExternalKnownCrate(crate_name) => {
                    // A reference to a KNOWN external dependency crate: expected, and
                    // aggregated per crate rather than one warning per occurrence.
                    unresolved += 1;
                    *external.entry(crate_name).or_default() += 1;
                }
                Resolution::UnknownTarget => {
                    // Insufficient crate evidence: NOT silently treated as external.
                    // A per-occurrence warning, so an un-attributable reference is
                    // never hidden inside an external-crate summary.
                    unresolved += 1;
                    diagnostics.push(warn(
                        code::UNRESOLVED_REFERENCE_UNKNOWN_TARGET,
                        format!(
                            "unresolved {} reference `{}` with insufficient crate evidence (unknown target crate)",
                            reference.role.relation_role(),
                            reference.display
                        ),
                        Some(reference.from.clone()),
                    ));
                }
                Resolution::Ambiguous => {
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

    // One informational summary per KNOWN external crate, in deterministic (sorted)
    // order. Unknown-target references are warnings above, never summarized here.
    for (crate_name, count) in &external {
        diagnostics.push(info(
            code::EXTERNAL_CRATE_REFERENCE,
            format!(
                "{count} cross-crate type reference(s) to external crate `{crate_name}` were not represented as workspace entities (external dependency)"
            ),
            None,
        ));
    }

    ResolveOutput {
        relations,
        diagnostics,
        resolved,
        unresolved,
    }
}

/// The explicit classification of one cross-crate reference.
enum Resolution {
    /// Resolved to exactly one entity in an analyzed workspace crate.
    ResolvedWorkspace(EntityId),
    /// The target crate is an analyzed workspace crate but no item matched — a real
    /// gap worth a per-occurrence warning.
    UnresolvedWorkspace,
    /// A reference to a KNOWN external (non-analyzed) dependency crate — expected,
    /// aggregated into one Info per crate.
    ExternalKnownCrate(String),
    /// Insufficient crate evidence to attribute the reference to any crate — kept as
    /// a per-occurrence warning, never folded into an external summary.
    UnknownTarget,
    /// The reference matched more than one candidate.
    Ambiguous,
}

fn resolve_one(
    reference: &UnresolvedTypeRef,
    analyzed: &BTreeSet<&str>,
    by_path: &BTreeMap<(String, String), Vec<(String, EntityId)>>,
) -> Resolution {
    // Without a crate, the reference cannot be attributed to anything: it is neither
    // confirmed external nor a workspace gap. It stays a warning (UnknownTarget) —
    // never silently downgraded to an external-dependency Info.
    let Some(crate_name) = &reference.crate_name else {
        return Resolution::UnknownTarget;
    };
    if !analyzed.contains(crate_name.as_str()) {
        // A known external dependency crate (outside the analyzed workspace).
        return Resolution::ExternalKnownCrate(crate_name.clone());
    }
    // The target crate IS an analyzed workspace crate from here on, so any miss is a
    // real workspace gap (a warning), not an external reference.
    let Some(path) = &reference.canonical_path else {
        return Resolution::UnresolvedWorkspace;
    };
    // Drop the leading crate segment to get the crate-relative path.
    let relative: Vec<&str> = match path.split_first() {
        Some((first, rest)) if first == crate_name => rest.iter().map(String::as_str).collect(),
        _ => path.iter().map(String::as_str).collect(),
    };
    if relative.is_empty() {
        return Resolution::UnresolvedWorkspace;
    }
    let key = (crate_name.clone(), relative.join("::"));
    let Some(candidates) = by_path.get(&key) else {
        return Resolution::UnresolvedWorkspace;
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
        [one] => Resolution::ResolvedWorkspace((*one).clone()),
        [] => Resolution::UnresolvedWorkspace,
        _ => Resolution::Ambiguous,
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
    fn external_crate_reference_is_aggregated_not_flooded() {
        use cratevista_schema::Severity;
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
        // One aggregated Info summary, not a per-occurrence warning.
        assert_eq!(out.diagnostics.len(), 1);
        assert_eq!(out.diagnostics[0].code, code::EXTERNAL_CRATE_REFERENCE);
        assert_eq!(out.diagnostics[0].severity, Severity::Info);
        assert!(out.diagnostics[0].message.contains("core"));
    }

    #[test]
    fn many_external_refs_collapse_to_one_summary_per_crate() {
        let entities = index(vec![item_entity("item:struct:a::X", "struct", "a::X")]);
        // Three refs to `serde`, two to `sqlx` — five occurrences, two summaries.
        let mut refs = Vec::new();
        for _ in 0..3 {
            refs.push(unresolved_ref(
                "item:struct:a::X",
                TypeReferenceRole::Field,
                Some("serde"),
                Some(vec!["serde", "Error"]),
                Some("struct"),
                "Error",
            ));
        }
        for _ in 0..2 {
            refs.push(unresolved_ref(
                "item:struct:a::X",
                TypeReferenceRole::Return,
                Some("sqlx"),
                Some(vec!["sqlx", "Pool"]),
                Some("struct"),
                "Pool",
            ));
        }
        let crates = vec![crate_summary(
            "a",
            "package:a",
            "target:a:lib:a",
            "module:a::a",
            refs,
        )];
        let out = resolve_cross_crate(&entities, &crates);
        let external: Vec<&_> = out
            .diagnostics
            .iter()
            .filter(|d| d.code == code::EXTERNAL_CRATE_REFERENCE)
            .collect();
        assert_eq!(external.len(), 2, "one summary per external crate");
        // Deterministic sorted order (serde before sqlx), with the right counts.
        assert!(external[0].message.contains("serde") && external[0].message.contains('3'));
        assert!(external[1].message.contains("sqlx") && external[1].message.contains('2'));
        assert_eq!(out.unresolved, 5);
    }

    #[test]
    fn unresolved_reference_into_a_workspace_crate_stays_a_warning() {
        // The target crate `a` IS analyzed, but no entity lives at `a::Missing`:
        // a genuine gap that should resolve — kept as a per-occurrence warning.
        let entities = index(vec![item_entity("item:struct:a::X", "struct", "a::X")]);
        let reference = unresolved_ref(
            "item:struct:a::X",
            TypeReferenceRole::Field,
            Some("a"),
            Some(vec!["a", "Missing"]),
            Some("struct"),
            "Missing",
        );
        let crates = vec![crate_summary(
            "a",
            "package:a",
            "target:a:lib:a",
            "module:a::a",
            vec![reference],
        )];
        let out = resolve_cross_crate(&entities, &crates);
        assert_eq!(out.diagnostics.len(), 1);
        assert_eq!(
            out.diagnostics[0].code,
            code::UNRESOLVED_CROSS_CRATE_REFERENCE
        );
    }

    /// A reference with NO crate evidence must NOT be silently classified as an
    /// external dependency — it stays a per-occurrence `unresolved_reference_unknown_target`
    /// warning, never an `external_crate_reference` Info summary.
    #[test]
    fn a_reference_with_no_crate_evidence_is_a_warning_not_external_info() {
        use cratevista_schema::Severity;
        let entities = index(vec![item_entity("item:struct:a::X", "struct", "a::X")]);
        let reference = unresolved_ref(
            "item:struct:a::X",
            TypeReferenceRole::Return,
            None, // no crate evidence
            None, // no canonical path
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
        // Exactly one diagnostic, and it is the unknown-target WARNING.
        assert_eq!(out.diagnostics.len(), 1);
        assert_eq!(
            out.diagnostics[0].code,
            code::UNRESOLVED_REFERENCE_UNKNOWN_TARGET
        );
        assert_eq!(out.diagnostics[0].severity, Severity::Warning);
        // It must NOT be downgraded to an external-crate Info summary.
        assert!(
            !out.diagnostics
                .iter()
                .any(|d| d.code == code::EXTERNAL_CRATE_REFERENCE),
            "an unattributed reference must never be folded into an external summary"
        );
    }

    /// A reference whose crate IS an analyzed workspace crate but the *path* is
    /// missing is a workspace gap (warning), not an unknown target or external.
    #[test]
    fn a_workspace_crate_reference_without_a_path_stays_a_workspace_warning() {
        let entities = index(vec![item_entity("item:struct:a::X", "struct", "a::X")]);
        let reference = unresolved_ref(
            "item:struct:a::X",
            TypeReferenceRole::Field,
            Some("a"), // the analyzed workspace crate
            None,      // but no canonical path
            None,
            "Something",
        );
        let crates = vec![crate_summary(
            "a",
            "package:a",
            "target:a:lib:a",
            "module:a::a",
            vec![reference],
        )];
        let out = resolve_cross_crate(&entities, &crates);
        assert_eq!(out.diagnostics.len(), 1);
        assert_eq!(
            out.diagnostics[0].code,
            code::UNRESOLVED_CROSS_CRATE_REFERENCE
        );
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
