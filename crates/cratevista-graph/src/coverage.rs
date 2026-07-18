//! Documentation coverage over normalized schema data only (public documentable
//! items + `DocBlock.documented`), aggregated deterministically onto module and
//! package entities.

use std::collections::BTreeMap;

use cratevista_schema::{AttrValue, Entity, EntityId};

/// Item kinds counted for coverage.
fn is_documentable_item(kind: &str) -> bool {
    matches!(
        kind,
        "struct"
            | "enum"
            | "union"
            | "trait"
            | "function"
            | "method"
            | "type_alias"
            | "constant"
            | "static"
            | "macro"
    )
}

/// Container kinds that receive an aggregated `doc_coverage` attribute.
fn is_container(kind: &str) -> bool {
    matches!(kind, "module" | "package")
}

fn is_public(entity: &Entity) -> bool {
    entity
        .attributes
        .get("visibility")
        .and_then(AttrValue::as_str)
        .map(|v| v == "public")
        // Metadata package/target entities carry no visibility attribute; only
        // rustdoc item entities do, and only those are documentable items.
        .unwrap_or(false)
}

/// Attaches `doc_coverage` attributes to module/package entities and returns the
/// workspace-wide public-item coverage percent (`None` when there are no public
/// documentable items).
pub fn compute_coverage(entities: &mut BTreeMap<EntityId, Entity>) -> Option<u8> {
    // Pass 1: tally per-container counters via ancestor walks (immutable).
    let mut counters: BTreeMap<EntityId, (u64, u64)> = BTreeMap::new();
    let mut global_documented = 0u64;
    let mut global_total = 0u64;

    for entity in entities.values() {
        if !is_documentable_item(entity.kind.as_str()) || !is_public(entity) {
            continue;
        }
        let documented = entity.docs.as_ref().map(|d| d.documented).unwrap_or(false);
        global_total += 1;
        if documented {
            global_documented += 1;
        }

        // Walk ancestors, crediting each container.
        let mut current = entity.parent.clone();
        let mut guard = 0usize;
        while let Some(parent_id) = current {
            guard += 1;
            if guard > 1024 {
                break; // defensive against a pathological chain
            }
            let Some(parent) = entities.get(&parent_id) else {
                break;
            };
            if is_container(parent.kind.as_str()) {
                let counter = counters.entry(parent_id.clone()).or_insert((0, 0));
                counter.1 += 1;
                if documented {
                    counter.0 += 1;
                }
            }
            current = parent.parent.clone();
        }
    }

    // Pass 2: attach attributes to module/package entities (mutable).
    for entity in entities.values_mut() {
        if !is_container(entity.kind.as_str()) {
            continue;
        }
        let (documented, total) = counters.get(&entity.id).copied().unwrap_or((0, 0));
        entity
            .attributes
            .insert("doc_coverage".into(), coverage_value(documented, total));
    }

    if global_total == 0 {
        None
    } else {
        Some(percent(global_documented, global_total))
    }
}

fn coverage_value(documented: u64, total: u64) -> AttrValue {
    let mut object = serde_json::Map::new();
    object.insert("documented".into(), AttrValue::from(documented));
    object.insert("total".into(), AttrValue::from(total));
    object.insert(
        "percent".into(),
        AttrValue::from(percent(documented, total)),
    );
    AttrValue::Object(object)
}

fn percent(documented: u64, total: u64) -> u8 {
    if total == 0 {
        0
    } else {
        // Deterministic rounding to nearest integer percent.
        (((documented * 200 + total) / (total * 2)).min(100)) as u8
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::{
        entity_with_kind, item_entity, package_entity, undocumented_item_entity,
    };

    fn parented(mut entity: Entity, parent: &str) -> Entity {
        entity.parent = Some(EntityId::from_raw(parent));
        entity
    }

    /// PRD-08 Amendment B: a docs-only override must never move documentation
    /// coverage. Coverage reports *Rust* documentation, so manual enrichment
    /// from configuration must not be able to inflate it — otherwise a project
    /// could report coverage it does not have by writing prose in a TOML file.
    ///
    /// This runs the real overlay against the real coverage pass in the same
    /// order the builder does (`apply_overlay` → `compute_coverage`).
    #[test]
    fn docs_only_overrides_never_change_coverage() {
        use crate::input::{EntityOverride, GraphOverlay};
        use crate::overlay::apply_overlay;
        use cratevista_schema::DocBlock;

        // One documented and one undocumented public item under a package.
        let build_entities = || {
            let mut map: BTreeMap<EntityId, Entity> = BTreeMap::new();
            let mut package = package_entity("c", "crates/c/Cargo.toml");
            package.attributes.clear(); // packages carry no visibility
            let documented = parented(
                item_entity("item:struct:c::Documented", "struct", "c::Documented"),
                "package:c",
            );
            let bare = parented(
                undocumented_item_entity("item:struct:c::Bare", "struct", "c::Bare"),
                "package:c",
            );
            for entity in [package, documented, bare] {
                map.insert(entity.id.clone(), entity);
            }
            map
        };

        let baseline = compute_coverage(&mut build_entities());
        // Pin the baseline: comparing two `None`s afterwards would prove nothing.
        assert_eq!(baseline, Some(50), "1 of 2 public items is documented");

        // Now add manual docs to BOTH — including the undocumented one, with a
        // manual block that dishonestly claims `documented: true`.
        let mut entities = build_entities();
        let mut overrides = BTreeMap::new();
        for id in ["item:struct:c::Documented", "item:struct:c::Bare"] {
            overrides.insert(
                EntityId::from_raw(id),
                EntityOverride {
                    docs: Some(DocBlock {
                        markdown: "Manual prose from configuration.".into(),
                        summary: Some("manual".into()),
                        documented: true,
                    }),
                    ..Default::default()
                },
            );
        }
        let mut relations = Vec::new();
        let mut diags = Vec::new();
        apply_overlay(
            &mut entities,
            &mut relations,
            GraphOverlay {
                overrides,
                ..Default::default()
            },
            &mut diags,
        );
        let after = compute_coverage(&mut entities);

        assert_eq!(
            after, baseline,
            "a docs-only override must not move coverage"
        );
        // The prose did land — coverage is unchanged because `documented` is
        // preserved, not because the override was ignored.
        let bare = &entities[&EntityId::from_raw("item:struct:c::Bare")];
        let bare_docs = bare.docs.as_ref().expect("manual prose was attached");
        assert!(bare_docs.markdown.contains("Manual prose"));
        assert!(!bare_docs.documented, "still undocumented in Rust terms");
    }

    #[test]
    fn coverage_counts_public_documented_items() {
        let mut map: BTreeMap<EntityId, Entity> = BTreeMap::new();
        let module = parented(
            entity_with_kind("module:c::c", "module", "c::c"),
            "package:c",
        );
        let mut package = package_entity("c", "crates/c/Cargo.toml");
        package.attributes.clear(); // packages carry no visibility
        let documented = parented(
            item_entity("item:struct:c::A", "struct", "c::A"),
            "module:c::c",
        );
        let undocumented = parented(
            undocumented_item_entity("item:struct:c::B", "struct", "c::B"),
            "module:c::c",
        );
        for entity in [module, package, documented, undocumented] {
            map.insert(entity.id.clone(), entity);
        }

        let percent = compute_coverage(&mut map).unwrap();
        assert_eq!(percent, 50);
        let module = &map[&EntityId::from_raw("module:c::c")];
        let coverage = &module.attributes["doc_coverage"];
        assert_eq!(coverage["documented"], 1);
        assert_eq!(coverage["total"], 2);
        assert_eq!(coverage["percent"], 50);
        // The package aggregates its descendants too.
        assert_eq!(
            map[&EntityId::package("c")].attributes["doc_coverage"]["total"],
            2
        );
    }

    #[test]
    fn no_public_items_yields_none() {
        let mut map: BTreeMap<EntityId, Entity> = BTreeMap::new();
        let module = entity_with_kind("module:c::c", "module", "c::c");
        map.insert(module.id.clone(), module);
        assert_eq!(compute_coverage(&mut map), None);
    }
}
