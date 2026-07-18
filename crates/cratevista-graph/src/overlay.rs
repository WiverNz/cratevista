//! Applying the `GraphOverlay`: manual additions and presentation-only
//! overrides. Overrides never change a discovered entity's id, kind, structural
//! parent, or source identity. The empty overlay is a normal input.

use std::collections::BTreeMap;

use cratevista_schema::{
    DocBlock, DocumentDiagnostic, Entity, EntityId, Provenance, Relation, View,
};

use crate::diagnostics::{code, warn};
use crate::input::GraphOverlay;
use crate::merge::merge_entity;

/// Applies the overlay, returning any manual flow views to add to the document.
pub fn apply_overlay(
    entities: &mut BTreeMap<EntityId, Entity>,
    relations: &mut Vec<Relation>,
    overlay: GraphOverlay,
    diagnostics: &mut Vec<DocumentDiagnostic>,
) -> Vec<View> {
    let GraphOverlay {
        entities: manual_entities,
        relations: manual_relations,
        overrides,
        views,
    } = overlay;

    // Manual entities/relations are forced to Provenance::Manual.
    for mut entity in manual_entities {
        entity.provenance = Provenance::Manual;
        merge_entity(entities, entity, diagnostics);
    }
    for mut relation in manual_relations {
        relation.provenance = Provenance::Manual;
        relations.push(relation);
    }

    // Presentation-only overrides.
    for (id, entity_override) in overrides {
        let Some(entity) = entities.get_mut(&id) else {
            diagnostics.push(warn(
                code::OVERLAY_TARGET_MISSING,
                format!("overlay override targets missing entity `{id}`"),
                Some(id.clone()),
            ));
            continue;
        };
        if let Some(label) = entity_override.label {
            entity.label = label;
        }
        if let Some(description) = entity_override.description {
            entity.description = Some(description);
        }
        if !entity_override.add_tags.is_empty() {
            entity.tags.extend(entity_override.add_tags);
            entity.tags.sort();
            entity.tags.dedup();
        }
        for (key, value) in entity_override.set_attributes {
            entity.attributes.insert(key, value);
        }
        if let Some(hidden) = entity_override.hidden {
            entity.attributes.insert("hidden".into(), hidden.into());
        }
        if let Some(manual) = entity_override.docs {
            append_docs(&mut entity.docs, manual);
        }
    }

    views
}

/// The exact boundary between discovered and manual Markdown.
///
/// Trims **only newline characters immediately adjoining the junction** — the
/// trailing newlines of the discovered text and the leading newlines of the
/// manual text — then joins with `\n\n`, so the result carries exactly one blank
/// line between the two. Nothing else is touched: indentation, trailing spaces,
/// blank lines *inside* either side, and the final newline of the manual text
/// all survive byte-for-byte. `\r` is trimmed alongside `\n` at the junction so a
/// CRLF-terminated discovered block cannot leave a stray carriage return in the
/// middle of the joined document.
fn join_markdown(discovered: &str, manual: &str) -> String {
    const NEWLINES: [char; 2] = ['\n', '\r'];
    let left = discovered.trim_end_matches(NEWLINES);
    let right = manual.trim_start_matches(NEWLINES);
    if left.is_empty() {
        return right.to_string();
    }
    if right.is_empty() {
        return left.to_string();
    }
    format!("{left}\n\n{right}")
}

/// Appends manual documentation to whatever was discovered.
///
/// Deterministic and coverage-safe:
/// - discovered Markdown comes **first**, manual second;
/// - the discovered `summary` is preserved (a manual block never becomes the
///   summary line);
/// - `documented` is **never changed**. It drives
///   [`crate::coverage::compute_coverage`], which reports *Rust* documentation
///   coverage; letting configuration set it would let a project report coverage
///   it does not have. When nothing was discovered, the entity stays
///   `documented: false` even though it now carries manual prose.
fn append_docs(discovered: &mut Option<DocBlock>, manual: DocBlock) {
    // A manual block with no Markdown is a no-op: leave the discovered docs
    // exactly as they were rather than rewriting them to an identical value.
    if manual.markdown.trim_matches(['\n', '\r']).is_empty() {
        return;
    }
    match discovered {
        Some(existing) => {
            existing.markdown = join_markdown(&existing.markdown, &manual.markdown);
            // `summary` and `documented` deliberately untouched.
        }
        None => {
            *discovered = Some(DocBlock {
                markdown: manual.markdown,
                summary: None,
                documented: false,
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::input::EntityOverride;
    use crate::test_support::entity_with_kind;
    use cratevista_schema::LocalizedText;

    #[test]
    fn manual_additions_are_marked_manual() {
        let mut entities: BTreeMap<EntityId, Entity> = BTreeMap::new();
        let mut relations = Vec::new();
        let mut diags = Vec::new();
        let overlay = GraphOverlay {
            entities: vec![entity_with_kind("manual:m", "manual_block", "m")],
            ..Default::default()
        };
        apply_overlay(&mut entities, &mut relations, overlay, &mut diags);
        assert_eq!(
            entities[&EntityId::from_raw("manual:m")].provenance,
            Provenance::Manual
        );
    }

    #[test]
    fn override_applies_and_missing_target_is_diagnosed() {
        let mut entities: BTreeMap<EntityId, Entity> = BTreeMap::new();
        entities.insert(
            EntityId::from_raw("x"),
            entity_with_kind("x", "struct", "c::X"),
        );
        let mut relations = Vec::new();
        let mut diags = Vec::new();

        let mut overrides = BTreeMap::new();
        overrides.insert(
            EntityId::from_raw("x"),
            EntityOverride {
                label: Some(LocalizedText::new("Renamed")),
                add_tags: vec!["featured".into()],
                ..Default::default()
            },
        );
        overrides.insert(EntityId::from_raw("missing"), EntityOverride::default());
        let overlay = GraphOverlay {
            overrides,
            ..Default::default()
        };
        apply_overlay(&mut entities, &mut relations, overlay, &mut diags);

        let x = &entities[&EntityId::from_raw("x")];
        assert_eq!(x.label.default, "Renamed");
        assert!(x.tags.contains(&"featured".to_string()));
        // Presentation-only: kind unchanged.
        assert_eq!(x.kind.as_str(), "struct");
        assert!(diags.iter().any(|d| d.code == code::OVERLAY_TARGET_MISSING));
    }

    // ---- PRD-08 Amendment B: EntityOverride::docs -------------------------

    fn docs(markdown: &str, summary: Option<&str>, documented: bool) -> DocBlock {
        DocBlock {
            markdown: markdown.into(),
            summary: summary.map(str::to_string),
            documented,
        }
    }

    /// Applies a docs-only override to one entity and returns it.
    fn apply_docs_override(entity: Entity, manual: DocBlock) -> Entity {
        let id = entity.id.clone();
        let mut entities = BTreeMap::from([(id.clone(), entity)]);
        let mut relations = Vec::new();
        let mut diags = Vec::new();
        let mut overrides = BTreeMap::new();
        overrides.insert(
            id.clone(),
            EntityOverride {
                docs: Some(manual),
                ..Default::default()
            },
        );
        apply_overlay(
            &mut entities,
            &mut relations,
            GraphOverlay {
                overrides,
                ..Default::default()
            },
            &mut diags,
        );
        entities.remove(&id).unwrap()
    }

    #[test]
    fn manual_docs_are_appended_after_discovered_docs() {
        let mut entity = entity_with_kind("x", "struct", "c::X");
        entity.docs = Some(docs("Discovered.", Some("Discovered."), true));

        let result = apply_docs_override(entity, docs("Manual.", Some("ignored"), true));
        let result_docs = result.docs.unwrap();

        // Discovered first, manual second, exactly one blank line between.
        assert_eq!(result_docs.markdown, "Discovered.\n\nManual.");
        // The discovered summary survives; the manual block never becomes it.
        assert_eq!(result_docs.summary.as_deref(), Some("Discovered."));
        assert!(result_docs.documented);
    }

    #[test]
    fn exactly_one_blank_line_regardless_of_adjoining_newlines() {
        // Every combination of trailing/leading newlines collapses to one blank
        // line, and nothing else about either side changes.
        for (left, right) in [
            ("A", "B"),
            ("A\n", "B"),
            ("A\n\n", "B"),
            ("A\n\n\n\n", "B"),
            ("A", "\nB"),
            ("A", "\n\n\n B"),
            ("A\n\n", "\n\nB"),
            ("A\r\n", "\r\nB"),
        ] {
            let joined = join_markdown(left, right);
            assert!(
                joined.starts_with('A') && joined.ends_with('B'),
                "{left:?} + {right:?} => {joined:?}"
            );
            assert_eq!(
                joined.matches('\n').count(),
                2,
                "exactly one blank line for {left:?} + {right:?}, got {joined:?}"
            );
            assert!(!joined.contains('\r'), "no stray CR in {joined:?}");
        }
        // The leading space of " B" is content, not a newline: it survives.
        assert_eq!(join_markdown("A", "\n\n\n B"), "A\n\n B");
    }

    #[test]
    fn internal_content_is_never_rewritten() {
        // Blank lines, indentation and trailing spaces INSIDE either side are
        // content — only the junction is normalized.
        let left = "# Title\n\nPara one.\n\n    indented code\n\ntrailing spaces:   \n";
        let right = "\n## Manual\n\n- a\n\n- b\n";
        let joined = join_markdown(left, right);
        assert_eq!(
            joined,
            // The manual side's trailing newline does NOT adjoin the junction,
            // so it survives — only the junction itself is normalized.
            "# Title\n\nPara one.\n\n    indented code\n\ntrailing spaces:   \n\n## Manual\n\n- a\n\n- b\n"
        );
        assert!(joined.contains("    indented code"));
        assert!(joined.contains("trailing spaces:   "));
    }

    #[test]
    fn manual_docs_on_an_undocumented_entity_stay_undocumented() {
        let entity = entity_with_kind("x", "struct", "c::X");
        assert!(entity.docs.is_none());

        // Even though the manual block claims `documented: true`, config must
        // not be able to say an item is documented in Rust.
        let result = apply_docs_override(entity, docs("Manual only.", Some("s"), true));
        let result_docs = result.docs.unwrap();

        assert_eq!(result_docs.markdown, "Manual only.");
        assert_eq!(result_docs.summary, None);
        assert!(
            !result_docs.documented,
            "an override must never mark an entity documented"
        );
    }

    #[test]
    fn an_override_never_flips_documented() {
        for discovered_documented in [true, false] {
            let mut entity = entity_with_kind("x", "struct", "c::X");
            entity.docs = Some(docs("D.", Some("D."), discovered_documented));
            let result = apply_docs_override(entity, docs("M.", None, !discovered_documented));
            assert_eq!(
                result.docs.unwrap().documented,
                discovered_documented,
                "documented must survive the override untouched"
            );
        }
    }

    #[test]
    fn an_empty_manual_block_leaves_discovered_docs_byte_identical() {
        for empty in ["", "\n", "\n\n\n", "\r\n"] {
            let mut entity = entity_with_kind("x", "struct", "c::X");
            entity.docs = Some(docs("Discovered.\n", Some("Discovered."), true));
            let result = apply_docs_override(entity, docs(empty, None, false));
            let result_docs = result.docs.unwrap();
            // Not even the trailing newline is normalized away.
            assert_eq!(result_docs.markdown, "Discovered.\n");
            assert_eq!(result_docs.summary.as_deref(), Some("Discovered."));
            assert!(result_docs.documented);
        }
    }

    #[test]
    fn a_docs_override_leaves_label_and_description_alone() {
        let mut entity = entity_with_kind("x", "struct", "c::X");
        entity.description = Some(LocalizedText::new("Original description"));
        let original_label = entity.label.default.clone();

        let result = apply_docs_override(entity, docs("Manual.", None, false));

        assert_eq!(result.label.default, original_label);
        assert_eq!(
            result.description.unwrap().default,
            "Original description",
            "docs are additive; description keeps its replace semantics"
        );
    }

    #[test]
    fn label_and_description_still_replace() {
        // Amendment B must not have made the other fields additive.
        let mut entity = entity_with_kind("x", "struct", "c::X");
        entity.description = Some(LocalizedText::new("Original"));
        let id = entity.id.clone();
        let mut entities = BTreeMap::from([(id.clone(), entity)]);
        let mut relations = Vec::new();
        let mut diags = Vec::new();
        let mut overrides = BTreeMap::new();
        overrides.insert(
            id.clone(),
            EntityOverride {
                label: Some(LocalizedText::new("New label")),
                description: Some(LocalizedText::new("Replaced")),
                ..Default::default()
            },
        );
        apply_overlay(
            &mut entities,
            &mut relations,
            GraphOverlay {
                overrides,
                ..Default::default()
            },
            &mut diags,
        );
        let result = &entities[&id];
        assert_eq!(result.label.default, "New label");
        assert_eq!(result.description.as_ref().unwrap().default, "Replaced");
    }

    #[test]
    fn appending_docs_is_deterministic() {
        let build = || {
            let mut entity = entity_with_kind("x", "struct", "c::X");
            entity.docs = Some(docs("Discovered.\n", Some("s"), true));
            apply_docs_override(entity, docs("\nManual.\n", None, false))
                .docs
                .unwrap()
                .markdown
        };
        let first = build();
        for _ in 0..8 {
            assert_eq!(build(), first);
        }
        // Junction normalized to one blank line; the manual side's own trailing
        // newline is content and survives.
        assert_eq!(first, "Discovered.\n\nManual.\n");
    }
}
