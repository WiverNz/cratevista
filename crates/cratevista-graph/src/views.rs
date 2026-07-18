//! The eight approved default views — filter-based projections over the same
//! canonical entities/relations. No coordinates, viewport, or component names;
//! auto-generated views carry no `Stage`s.

use std::collections::{BTreeMap, BTreeSet};

use cratevista_schema::{Entity, EntityId, EntityKind, LocalizedText, RelationKind, View, ViewId};

struct ViewSpec {
    slug: &'static str,
    title: &'static str,
    entity_kinds: &'static [&'static str],
    relation_kinds: &'static [&'static str],
    public_only: bool,
}

const VIEWS: &[ViewSpec] = &[
    ViewSpec {
        slug: "workspace-overview",
        title: "Workspace overview",
        entity_kinds: &["workspace", "package", "target"],
        relation_kinds: &["contains", "depends_on"],
        public_only: false,
    },
    ViewSpec {
        slug: "crate-dependencies",
        title: "Crate dependencies",
        entity_kinds: &["package"],
        relation_kinds: &["depends_on"],
        public_only: false,
    },
    ViewSpec {
        slug: "module-hierarchy",
        title: "Module hierarchy",
        entity_kinds: &["package", "target", "module"],
        relation_kinds: &["contains"],
        public_only: false,
    },
    ViewSpec {
        slug: "types",
        title: "Types",
        entity_kinds: &[
            "struct",
            "enum",
            "union",
            "type_alias",
            "constant",
            "static",
        ],
        relation_kinds: &["has_field_type", "contains"],
        public_only: false,
    },
    ViewSpec {
        slug: "traits-and-impls",
        title: "Traits and implementations",
        entity_kinds: &["trait", "impl", "struct", "enum", "union"],
        relation_kinds: &["implements", "implemented_for", "contains"],
        public_only: false,
    },
    ViewSpec {
        slug: "type-relationships",
        title: "Type relationships",
        entity_kinds: &[
            "struct",
            "enum",
            "union",
            "trait",
            "function",
            "method",
            "type_alias",
        ],
        relation_kinds: &[
            "has_field_type",
            "accepts_type",
            "returns_type",
            "error_type",
        ],
        public_only: false,
    },
    ViewSpec {
        slug: "public-api",
        title: "Public API",
        entity_kinds: &[
            "module",
            "struct",
            "enum",
            "union",
            "trait",
            "function",
            "method",
            "type_alias",
            "constant",
            "static",
            "macro",
        ],
        relation_kinds: &["contains", "re_exports", "imports"],
        public_only: true,
    },
    ViewSpec {
        slug: "documentation-coverage",
        title: "Documentation coverage",
        entity_kinds: &["package", "module"],
        relation_kinds: &["contains"],
        public_only: false,
    },
];

/// Builds the default views. When `retain_empty` is false, a view whose
/// `entity_kinds` match **no** entity in the document is omitted.
pub fn build_views(entities: &BTreeMap<EntityId, Entity>, retain_empty: bool) -> Vec<View> {
    let present_kinds: BTreeSet<&str> = entities.values().map(|e| e.kind.as_str()).collect();

    VIEWS
        .iter()
        .filter(|spec| retain_empty || spec.entity_kinds.iter().any(|k| present_kinds.contains(k)))
        .map(build_view)
        .collect()
}

fn build_view(spec: &ViewSpec) -> View {
    let mut presentation: BTreeMap<String, cratevista_schema::AttrValue> = BTreeMap::new();
    if spec.public_only {
        presentation.insert("visibility".into(), "public".into());
    }
    View {
        id: ViewId::view(spec.slug),
        title: LocalizedText::new(spec.title),
        description: None,
        entity_kinds: spec
            .entity_kinds
            .iter()
            .map(|k| EntityKind::new(*k))
            .collect(),
        relation_kinds: spec
            .relation_kinds
            .iter()
            .map(|k| RelationKind::new(*k))
            .collect(),
        entity_ids: None,
        stages: Vec::new(),
        default_focus: None,
        presentation,
        // The eight generated views carry no prose or examples; those are for
        // manual flows (issue 08), which arrive via the overlay.
        docs: None,
        examples: Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::{entity_with_kind, item_entity};

    fn map(entities: Vec<Entity>) -> BTreeMap<EntityId, Entity> {
        entities.into_iter().map(|e| (e.id.clone(), e)).collect()
    }

    #[test]
    fn all_eight_views_retained_by_default() {
        let entities = map(vec![entity_with_kind("workspace", "workspace", "ws")]);
        let views = build_views(&entities, true);
        assert_eq!(views.len(), 8);
        // Stable ids, no coordinates/stages.
        assert!(views.iter().any(|v| v.id.as_str() == "view:public-api"));
        assert!(views.iter().all(|v| v.stages.is_empty()));
        // Public API carries a public-only presentation hint.
        let public_api = views
            .iter()
            .find(|v| v.id.as_str() == "view:public-api")
            .unwrap();
        assert_eq!(public_api.presentation["visibility"], "public");
    }

    #[test]
    fn empty_item_views_omitted_when_not_retaining() {
        // Only workspace/package present → item-level views are deterministically empty.
        let entities = map(vec![
            entity_with_kind("workspace", "workspace", "ws"),
            entity_with_kind("package:a", "package", "a"),
        ]);
        let views = build_views(&entities, false);
        let slugs: Vec<&str> = views.iter().map(|v| v.id.as_str()).collect();
        assert!(slugs.contains(&"view:workspace-overview"));
        assert!(slugs.contains(&"view:crate-dependencies"));
        assert!(!slugs.contains(&"view:types"));
        assert!(!slugs.contains(&"view:traits-and-impls"));
        assert!(views.len() < 8);
    }

    #[test]
    fn type_views_present_when_items_exist() {
        let entities = map(vec![
            entity_with_kind("workspace", "workspace", "ws"),
            item_entity("item:struct:a::X", "struct", "a::X"),
        ]);
        let views = build_views(&entities, false);
        assert!(views.iter().any(|v| v.id.as_str() == "view:types"));
    }
}
