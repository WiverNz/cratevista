//! CrateVista pure graph builder.
//!
//! `cratevista-graph` is the **pure** document-assembly layer. It (a) builds an
//! explicit [`cratevista_rustdoc::RustdocPlan`] from `MetadataIngest`
//! ([`plan::build_rustdoc_plan`]), and (b) merges `MetadataIngest` and
//! `RustdocIngest` — plus an optional in-memory [`GraphOverlay`] — into one
//! deterministic [`cratevista_schema::ExplorerDocument`] with the MVP views as
//! projections ([`build_document`]). It returns pure Rust values only: it never
//! invokes Cargo/rustdoc, reads the clock, touches the filesystem, serializes
//! JSON, or renders CLI output, and it does not depend on `cratevista-core`.
//!
//! See `PRD/issue_05_graph_builder.md`.
#![forbid(unsafe_code)]

pub mod coverage;
pub mod diagnostics;
pub mod error;
pub mod input;
pub mod link;
pub mod merge;
pub mod overlay;
pub mod plan;
pub mod resolve;
pub mod result;
pub mod validate;
pub mod views;

#[cfg(test)]
mod test_support;

pub use error::GraphError;
pub use input::{EntityOverride, GraphBuildOptions, GraphInput, GraphOverlay};
pub use plan::{RustdocPlanOptions, build_rustdoc_plan};
pub use result::{GraphBuildResult, GraphBuildSummary};

use std::collections::{BTreeMap, BTreeSet};

use cratevista_schema::{
    DocumentDiagnostic, Entity, EntityId, EntityKind, ExplorerDocument, Project, Relation,
    RelationId,
};
use serde_json::Value;

/// Assembles one deterministic [`ExplorerDocument`] from the merged inputs.
///
/// Fails with [`GraphError`] only when no trustworthy document can be built
/// (empty input, schema validation failure, or an internal invariant); all
/// recoverable problems are `DocumentDiagnostic`s in the result.
pub fn build_document(
    input: GraphInput,
    options: &GraphBuildOptions,
) -> Result<GraphBuildResult, GraphError> {
    let GraphInput {
        metadata,
        rustdoc,
        overlay,
    } = input;

    if metadata.entities.is_empty() {
        return Err(GraphError::EmptyInput(
            "metadata produced no entities".to_string(),
        ));
    }

    let mut entities: BTreeMap<EntityId, Entity> = BTreeMap::new();
    let mut relations: Vec<Relation> = Vec::new();
    let mut diagnostics: Vec<DocumentDiagnostic> = Vec::new();

    // Metadata owns workspace/package/target structural facts.
    diagnostics.extend(metadata.diagnostics.iter().cloned());
    for mut entity in metadata.entities.iter().cloned() {
        normalize_metadata_entity(&mut entity);
        merge::merge_entity(&mut entities, entity, &mut diagnostics);
    }
    relations.extend(metadata.relations.iter().cloned());

    let mut partial = false;
    let mut documented_crate_count = 0usize;
    let mut resolved_cross_crate_count = 0usize;
    let mut unresolved_reference_count = 0usize;

    if let Some(rustdoc) = rustdoc.as_ref() {
        // Rustdoc owns module/item/impl structure, docs, signatures, typed relations.
        diagnostics.extend(rustdoc.diagnostics.iter().cloned());
        for entity in rustdoc.entities.iter().cloned() {
            merge::merge_entity(&mut entities, entity, &mut diagnostics);
        }
        relations.extend(rustdoc.relations.iter().cloned());
        partial = rustdoc.summary.partial;
        documented_crate_count = rustdoc.crates.len();

        // Cross-source linking via CrateSummary identities (target_id/root_module_id).
        link::link_crates(
            &mut entities,
            &mut relations,
            &rustdoc.crates,
            &mut diagnostics,
        );

        // Cross-crate reliable-reference resolution (structured evidence only).
        let resolved = resolve::resolve_cross_crate(&entities, &rustdoc.crates);
        relations.extend(resolved.relations);
        diagnostics.extend(resolved.diagnostics);
        resolved_cross_crate_count = resolved.resolved;
        unresolved_reference_count = resolved.unresolved;
    }

    // Overlay: manual additions + presentation-only overrides (empty is normal).
    let overlay_views =
        overlay::apply_overlay(&mut entities, &mut relations, overlay, &mut diagnostics);

    // Documentation coverage (after linking, so ancestor chains reach packages).
    let coverage_percent = if options.compute_coverage {
        coverage::compute_coverage(&mut entities)
    } else {
        None
    };

    // Merge/dedup relations by id (distinct ids already encode kind/from/to/role/cfg).
    let mut relation_map: BTreeMap<RelationId, Relation> = BTreeMap::new();
    for relation in relations {
        merge::merge_relation(&mut relation_map, relation, &mut diagnostics);
    }

    // Drop known-dangling relations so schema validation failure is an invariant bug.
    let entity_ids: BTreeSet<EntityId> = entities.keys().cloned().collect();
    let (relations, dangling) =
        validate::drop_dangling_relations(relation_map.into_values().collect(), &entity_ids);
    diagnostics.extend(dangling);

    // Views: the eight defaults + any manual overlay views (sanitized).
    let mut views = views::build_views(&entities, options.retain_empty_views);
    views.extend(overlay_views);
    let (views, view_diags) = validate::sanitize_views(views, &entity_ids);
    diagnostics.extend(view_diags);

    // Assemble (sorts entities/relations/views by id) and validate.
    let project = derive_project(&entities);
    let entities_vec: Vec<Entity> = entities.into_values().collect();
    let document = ExplorerDocument::new(project, entities_vec, relations, views);
    validate::validate_document(&document)?;

    // Deterministic diagnostics.
    diagnostics.sort();
    diagnostics.dedup();

    let summary = GraphBuildSummary {
        entity_count: document.entities.len(),
        relation_count: document.relations.len(),
        view_count: document.views.len(),
        diagnostic_count: diagnostics.len(),
        documented_crate_count,
        unresolved_reference_count,
        resolved_cross_crate_count,
        coverage_percent,
    };

    Ok(GraphBuildResult {
        document,
        diagnostics,
        summary,
        partial,
    })
}

/// Sanitizes a metadata entity for the public artifact. The metadata **workspace**
/// entity carries the absolute workspace path in its `qualified_name`; replace it
/// with the safe workspace label so no absolute path enters `document.json`.
fn normalize_metadata_entity(entity: &mut Entity) {
    if entity.kind.as_str() == "workspace" {
        entity.qualified_name = entity.label.default.clone();
    }
}

/// Derives project metadata from the entities (path-free).
///
/// `repository_url` is the **unanimous** declared repository of the workspace
/// members represented in this document: every non-empty member repository must be
/// identical (compared trailing-slash-insensitively) for it to be adopted;
/// conflicting or absent values yield `None`. The accepted string is kept verbatim
/// — the frontend decides whether it is a safe URL to render.
///
/// `default_branch` has **no authoritative source** in the current pipeline (no
/// config field and no git inspection), so it stays `None`. A source deep link
/// therefore cannot be produced yet; only a repository-root link can. This is a
/// deliberate hold, not a guessed default (never `main`/`master`/current branch).
fn derive_project(entities: &BTreeMap<EntityId, Entity>) -> Project {
    let workspace = entities.get(&EntityId::workspace());
    let name = workspace
        .map(|entity| entity.label.default.clone())
        .unwrap_or_else(|| "workspace".to_string());
    Project {
        id: "workspace".to_string(),
        name,
        description: String::new(),
        root: None,
        repository_url: unanimous_repository(entities),
        default_branch: None,
    }
}

/// The single repository URL shared by every member package that declares one, or
/// `None` when there is disagreement or none is declared.
fn unanimous_repository(entities: &BTreeMap<EntityId, Entity>) -> Option<String> {
    let workspace = EntityId::workspace();
    let mut accepted: Option<String> = None;
    // `entities` is a BTreeMap, so iteration (and the "first accepted" tiebreak
    // among trailing-slash variants) is deterministic by entity id.
    for entity in entities.values() {
        if entity.kind.as_str() != EntityKind::PACKAGE {
            continue;
        }
        // Members only: an external dependency's repository is not the project's.
        if entity.parent.as_ref() != Some(&workspace) {
            continue;
        }
        let Some(value) = entity.attributes.get("repository").and_then(Value::as_str) else {
            continue;
        };
        let value = value.trim();
        if value.is_empty() {
            continue;
        }
        match &accepted {
            None => accepted = Some(value.to_string()),
            Some(existing) if repo_key(existing) == repo_key(value) => {}
            // A genuine disagreement between members: refuse to pick one.
            Some(_) => return None,
        }
    }
    accepted
}

/// The trailing-slash-insensitive comparison key for two repository strings. Used
/// only for equality; the accepted value itself is never rewritten.
fn repo_key(value: &str) -> &str {
    value.trim().trim_end_matches('/')
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::{
        crate_summary, entity_with_kind, item_entity, metadata_ingest, package_entity,
        rustdoc_ingest, target_entity, unresolved_ref, workspace_entity,
    };
    use cratevista_rustdoc::TypeReferenceRole;

    fn metadata_only() -> GraphInput {
        GraphInput {
            metadata: metadata_ingest(vec![
                workspace_entity(),
                package_entity("a", "crates/a/Cargo.toml"),
                target_entity("a", "lib", "a", "crates/a/src/lib.rs"),
            ]),
            rustdoc: None,
            overlay: GraphOverlay::default(),
        }
    }

    #[test]
    fn metadata_only_builds_valid_document() {
        let result = build_document(metadata_only(), &GraphBuildOptions::default()).unwrap();
        assert!(!result.partial);
        assert!(result.document.validate().is_ok());
        // Workspace/package/target present; item-level views retained but empty.
        assert!(
            result
                .document
                .entities
                .iter()
                .any(|e| e.id.as_str() == "workspace")
        );
        assert_eq!(result.document.views.len(), 8);
        // No absolute path in the workspace qualified name.
        let ws = result
            .document
            .entities
            .iter()
            .find(|e| e.id.as_str() == "workspace")
            .unwrap();
        assert!(!ws.qualified_name.contains('/') && !ws.qualified_name.contains('\\'));
    }

    /// Builds a real document from member packages carrying (optional) repository
    /// attributes and returns its **real** `Project` (never a hand-built one).
    fn project_with_repos(repos: &[(&str, Option<&str>)]) -> Project {
        let mut entities = vec![workspace_entity()];
        for (name, repo) in repos {
            let mut package = package_entity(name, &format!("crates/{name}/Cargo.toml"));
            if let Some(url) = repo {
                package
                    .attributes
                    .insert("repository".into(), (*url).into());
            }
            entities.push(package);
            entities.push(target_entity(
                name,
                "lib",
                name,
                &format!("crates/{name}/src/lib.rs"),
            ));
        }
        let input = GraphInput {
            metadata: metadata_ingest(entities),
            rustdoc: None,
            overlay: GraphOverlay::default(),
        };
        build_document(input, &GraphBuildOptions::default())
            .unwrap()
            .document
            .project
    }

    #[test]
    fn one_member_repository_becomes_the_project_repository_url() {
        let project = project_with_repos(&[("a", Some("https://github.com/example/example"))]);
        assert_eq!(
            project.repository_url.as_deref(),
            Some("https://github.com/example/example")
        );
        // No authoritative branch source → no deep link is possible yet.
        assert_eq!(project.default_branch, None);
    }

    #[test]
    fn unanimous_member_repositories_are_adopted_ignoring_trailing_slash() {
        let project = project_with_repos(&[
            ("a", Some("https://github.com/x/y")),
            ("b", Some("https://github.com/x/y/")),
            ("c", None),
        ]);
        // Trailing-slash variants compare equal; the first accepted value is kept.
        assert_eq!(
            project.repository_url.as_deref(),
            Some("https://github.com/x/y")
        );
    }

    #[test]
    fn conflicting_member_repositories_yield_none() {
        let project = project_with_repos(&[
            ("a", Some("https://github.com/x/y")),
            ("b", Some("https://gitlab.com/x/y")),
        ]);
        assert_eq!(project.repository_url, None);
    }

    #[test]
    fn no_member_repository_yields_none() {
        let project = project_with_repos(&[("a", None), ("b", None)]);
        assert_eq!(project.repository_url, None);
    }

    #[test]
    fn an_unsafe_repository_string_is_preserved_as_data() {
        // The graph keeps whatever the manifest declared; the frontend decides it is
        // unsafe to render (proven in web/tests/repository-links.test.ts). The graph
        // never validates or drops it on safety grounds.
        let project = project_with_repos(&[("a", Some("git@github.com:x/y.git"))]);
        assert_eq!(
            project.repository_url.as_deref(),
            Some("git@github.com:x/y.git")
        );
    }

    #[test]
    fn empty_metadata_is_fatal() {
        let input = GraphInput {
            metadata: metadata_ingest(vec![]),
            rustdoc: None,
            overlay: GraphOverlay::default(),
        };
        assert_eq!(
            build_document(input, &GraphBuildOptions::default())
                .unwrap_err()
                .code(),
            "empty_input"
        );
    }

    #[test]
    fn links_target_to_root_module() {
        let metadata = metadata_ingest(vec![
            workspace_entity(),
            package_entity("a", "crates/a/Cargo.toml"),
            target_entity("a", "lib", "a", "crates/a/src/lib.rs"),
        ]);
        let rustdoc = rustdoc_ingest(
            vec![crate_summary(
                "a",
                "package:a",
                "target:a:lib:a",
                "module:a::a",
                vec![],
            )],
            vec![entity_with_kind("module:a::a", "module", "a")],
            vec![],
            false,
        );
        let input = GraphInput {
            metadata,
            rustdoc: Some(rustdoc),
            overlay: GraphOverlay::default(),
        };
        let result = build_document(input, &GraphBuildOptions::default()).unwrap();
        // contains: target:a:lib:a -> module:a::a
        assert!(result.document.relations.iter().any(|r| {
            r.kind.as_str() == "contains"
                && r.from.as_str() == "target:a:lib:a"
                && r.to.as_str() == "module:a::a"
        }));
        // Root module parent set to the target.
        let root = result
            .document
            .entities
            .iter()
            .find(|e| e.id.as_str() == "module:a::a")
            .unwrap();
        assert_eq!(root.parent.as_ref().unwrap().as_str(), "target:a:lib:a");
        result.document.validate().unwrap();
    }

    #[test]
    fn cross_crate_reference_resolves_uniquely() {
        // Crate `b` defines `Widget`; crate `a`'s field references `b::Widget`.
        let metadata = metadata_ingest(vec![
            workspace_entity(),
            package_entity("a", "crates/a/Cargo.toml"),
            target_entity("a", "lib", "a", "crates/a/src/lib.rs"),
            package_entity("b", "crates/b/Cargo.toml"),
            target_entity("b", "lib", "b", "crates/b/src/lib.rs"),
        ]);
        let field = item_entity("item:field:a::Holder::widget", "field", "a::Holder::widget");
        let widget = item_entity("item:struct:b::Widget", "struct", "b::Widget");
        let reference = unresolved_ref(
            "item:field:a::Holder::widget",
            TypeReferenceRole::Field,
            Some("b"),
            Some(vec!["b", "Widget"]),
            Some("struct"),
            "Widget",
        );
        let rustdoc = rustdoc_ingest(
            vec![
                crate_summary(
                    "a",
                    "package:a",
                    "target:a:lib:a",
                    "module:a::a",
                    vec![reference],
                ),
                crate_summary("b", "package:b", "target:b:lib:b", "module:b::b", vec![]),
            ],
            vec![
                entity_with_kind("module:a::a", "module", "a"),
                entity_with_kind("module:b::b", "module", "b"),
                field,
                widget,
            ],
            vec![],
            false,
        );
        let input = GraphInput {
            metadata,
            rustdoc: Some(rustdoc),
            overlay: GraphOverlay::default(),
        };
        let result = build_document(input, &GraphBuildOptions::default()).unwrap();
        assert_eq!(result.summary.resolved_cross_crate_count, 1);
        assert!(result.document.relations.iter().any(|r| {
            r.kind.as_str() == "has_field_type"
                && r.from.as_str() == "item:field:a::Holder::widget"
                && r.to.as_str() == "item:struct:b::Widget"
        }));
    }

    #[test]
    fn partial_propagates_from_rustdoc() {
        let metadata = metadata_ingest(vec![
            workspace_entity(),
            package_entity("a", "crates/a/Cargo.toml"),
            target_entity("a", "lib", "a", "crates/a/src/lib.rs"),
        ]);
        let rustdoc = rustdoc_ingest(
            vec![crate_summary(
                "a",
                "package:a",
                "target:a:lib:a",
                "module:a::a",
                vec![],
            )],
            vec![entity_with_kind("module:a::a", "module", "a")],
            vec![],
            true, // partial
        );
        let result = build_document(
            GraphInput {
                metadata,
                rustdoc: Some(rustdoc),
                overlay: GraphOverlay::default(),
            },
            &GraphBuildOptions::default(),
        )
        .unwrap();
        assert!(result.partial);
    }

    #[test]
    fn dangling_relation_is_dropped_with_diagnostic() {
        use cratevista_schema::{EntityId, Provenance, Relation, RelationKind};
        let metadata = metadata_ingest(vec![
            workspace_entity(),
            package_entity("a", "crates/a/Cargo.toml"),
            target_entity("a", "lib", "a", "crates/a/src/lib.rs"),
        ]);
        let ghost = Relation::new(
            RelationKind::new("contains"),
            EntityId::from_raw("module:a::a"),
            EntityId::from_raw("item:struct:a::Ghost"),
            Provenance::Discovered,
        );
        let ghost_id = ghost.id.clone();
        let rustdoc = rustdoc_ingest(
            vec![crate_summary(
                "a",
                "package:a",
                "target:a:lib:a",
                "module:a::a",
                vec![],
            )],
            vec![entity_with_kind("module:a::a", "module", "a")],
            vec![ghost],
            false,
        );
        let result = build_document(
            GraphInput {
                metadata,
                rustdoc: Some(rustdoc),
                overlay: GraphOverlay::default(),
            },
            &GraphBuildOptions::default(),
        )
        .unwrap();
        assert!(!result.document.relations.iter().any(|r| r.id == ghost_id));
        assert!(
            result
                .diagnostics
                .iter()
                .any(|d| d.code == "dangling_relation")
        );
        result.document.validate().unwrap();
    }

    #[test]
    fn document_has_no_absolute_paths_and_diagnostics_are_separate() {
        let result = build_document(metadata_only(), &GraphBuildOptions::default()).unwrap();
        let json = serde_json::to_string(&result.document).unwrap();
        assert!(!json.contains("/abs"));
        assert!(!json.to_lowercase().contains(":\\"));
        // ExplorerDocument has no diagnostics field; diagnostics live only in the result.
        assert!(!json.contains("\"diagnostics\""));
    }

    #[test]
    fn overlay_manual_entity_is_included() {
        use cratevista_schema::{EntityId, EntityKind, LocalizedText, Provenance};
        let mut overlay = GraphOverlay::default();
        overlay.entities.push(cratevista_schema::Entity::new(
            EntityId::from_raw("manual:note"),
            EntityKind::new("manual_block"),
            LocalizedText::new("Note"),
            "note",
            Provenance::Discovered, // forced to Manual by the overlay
        ));
        let input = GraphInput {
            metadata: metadata_ingest(vec![
                workspace_entity(),
                package_entity("a", "crates/a/Cargo.toml"),
            ]),
            rustdoc: None,
            overlay,
        };
        let result = build_document(input, &GraphBuildOptions::default()).unwrap();
        let note = result
            .document
            .entities
            .iter()
            .find(|e| e.id.as_str() == "manual:note")
            .expect("manual entity present");
        assert_eq!(note.provenance, Provenance::Manual);
    }

    #[test]
    #[ignore = "engineering benchmark (~20k entities / ~50k relations); run with --ignored"]
    fn large_workspace_scales() {
        use cratevista_schema::{EntityId, Provenance, Relation, RelationId, RelationKind};
        use std::time::Instant;

        let n_entities = 20_000usize;
        let n_relations = 50_000usize;

        let mut entities = vec![workspace_entity()];
        for i in 0..n_entities {
            entities.push(entity_with_kind(
                &format!("item:struct:c::T{i}"),
                "struct",
                &format!("c::T{i}"),
            ));
        }
        let mut metadata = metadata_ingest(entities);
        let contains = RelationKind::new(RelationKind::CONTAINS);
        metadata.relations = (0..n_relations)
            .map(|i| {
                let from = EntityId::from_raw(format!("item:struct:c::T{}", i % n_entities));
                let to = EntityId::from_raw(format!("item:struct:c::T{}", (i + 1) % n_entities));
                let role = i.to_string();
                Relation {
                    id: RelationId::with_role(&contains, &from, &to, &role),
                    kind: contains.clone(),
                    from,
                    to,
                    role: Some(role),
                    label: None,
                    provenance: Provenance::Discovered,
                    attributes: Default::default(),
                }
            })
            .collect();

        let input = GraphInput {
            metadata,
            rustdoc: None,
            overlay: GraphOverlay::default(),
        };
        let started = Instant::now();
        let result = build_document(input, &GraphBuildOptions::default()).unwrap();
        let elapsed = started.elapsed();
        // Indexed merge/link/resolve → near-linear; no hard wall-clock contract.
        eprintln!(
            "assembled {} entities / {} relations in {:?}",
            result.summary.entity_count, result.summary.relation_count, elapsed
        );
        assert_eq!(result.summary.entity_count, n_entities + 1);
        assert_eq!(result.summary.relation_count, n_relations);
        result.document.validate().unwrap();
    }

    #[test]
    fn deterministic_under_reordered_inputs() {
        let a = build_document(metadata_only(), &GraphBuildOptions::default()).unwrap();
        // Reverse the metadata entity order → identical document.
        let mut reversed = metadata_ingest(vec![
            workspace_entity(),
            package_entity("a", "crates/a/Cargo.toml"),
            target_entity("a", "lib", "a", "crates/a/src/lib.rs"),
        ]);
        reversed.entities.reverse();
        reversed.relations.reverse();
        let b = build_document(
            GraphInput {
                metadata: reversed,
                rustdoc: None,
                overlay: GraphOverlay::default(),
            },
            &GraphBuildOptions::default(),
        )
        .unwrap();
        assert_eq!(a.document, b.document);
    }
}
