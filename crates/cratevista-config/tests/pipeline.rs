//! End-to-end tests over the committed fixtures, through the **real** pipeline:
//!
//! ```text
//! discover → load → validate → build_overlay → embed_files → build_document
//! ```
//!
//! Nothing is stubbed. The last step hands the overlay to the real
//! `cratevista_graph::build_document`, so these prove the seam actually works —
//! manual and discovered entities coexisting in a schema-valid document — rather
//! than that this crate's own types line up with themselves.

use std::path::{Path, PathBuf};

use cratevista_config::error::code;
use cratevista_config::{ConfigDiagnostic, build_overlay, embed_files, load_from, validate};
use cratevista_graph::{GraphBuildOptions, GraphInput, GraphOverlay, build_document};
use cratevista_metadata::{MetadataIngest, MetadataSummary};
use cratevista_schema::{
    Entity, EntityId, EntityKind, ExplorerDocument, LocalizedText, Provenance, Relation,
    RelationKind,
};

fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("fixtures")
        .join(name)
}

/// The whole config pipeline over one fixture.
struct Pipeline {
    overlay: GraphOverlay,
    /// Every diagnostic, in pipeline order: load, then validate, then build,
    /// then embed.
    diagnostics: Vec<ConfigDiagnostic>,
}

fn run(root: &Path) -> Pipeline {
    let config = load_from(root);
    let validation = validate(&config);
    let mut outcome = build_overlay(&config, &validation);
    let embed = embed_files(root, &config, &validation, &mut outcome.overlay);

    let mut diagnostics = config.diagnostics.clone();
    diagnostics.extend(validation.diagnostics);
    diagnostics.extend(outcome.diagnostics);
    diagnostics.extend(embed);
    Pipeline {
        overlay: outcome.overlay,
        diagnostics,
    }
}

fn codes(pipeline: &Pipeline) -> Vec<&str> {
    pipeline.diagnostics.iter().map(|d| d.code).collect()
}

/// A discovered entity, as cargo-metadata ingestion would emit it.
fn discovered(id: &str, kind: &str, name: &str) -> Entity {
    let mut entity = Entity::new(
        EntityId::from_raw(id),
        EntityKind::new(kind),
        LocalizedText::new(name),
        name,
        Provenance::Discovered,
    );
    entity.docs = Some(cratevista_schema::DocBlock {
        markdown: "Rustdoc for the demo service.".into(),
        summary: Some("Rustdoc for the demo service.".into()),
        documented: true,
    });
    entity
}

/// A minimal metadata ingestion: a workspace, one package, one target — the
/// discovered half of the document the flows reference.
fn metadata() -> MetadataIngest {
    let workspace = discovered("workspace", "workspace", "demo-ws");
    let mut package = discovered("package:demo", "package", "demo");
    package.parent = Some(EntityId::from_raw("workspace"));
    let mut target = discovered("target:demo:lib:demo", "target", "demo");
    target.parent = Some(EntityId::from_raw("package:demo"));

    let contains = |from: &str, to: &str| {
        Relation::new(
            RelationKind::new("contains"),
            EntityId::from_raw(from),
            EntityId::from_raw(to),
            Provenance::Discovered,
        )
    };

    MetadataIngest {
        entities: vec![workspace, package, target],
        relations: vec![
            contains("workspace", "package:demo"),
            contains("package:demo", "target:demo:lib:demo"),
        ],
        diagnostics: Vec::new(),
        summary: MetadataSummary {
            workspace_root_repo_relative: Some(".".into()),
            selection: Default::default(),
            external_deps_mode: Default::default(),
            workspace_package_count: 1,
            selected_package_count: 1,
            external_package_count: 0,
            target_count: 1,
            dependency_relation_count: 0,
            recoverable_diagnostic_count: 0,
            cargo_argv: vec!["cargo".into(), "metadata".into()],
        },
    }
}

/// Builds a real document from the overlay + the discovered metadata.
fn document_from(overlay: GraphOverlay) -> cratevista_graph::GraphBuildResult {
    build_document(
        GraphInput {
            metadata: metadata(),
            rustdoc: None,
            overlay,
        },
        &GraphBuildOptions::default(),
    )
    .expect("the document must build")
}

fn entity<'a>(document: &'a ExplorerDocument, id: &str) -> &'a Entity {
    document
        .entities
        .iter()
        .find(|entity| entity.id.as_str() == id)
        .unwrap_or_else(|| panic!("entity `{id}` is missing"))
}

// ---------------------------------------------------------------------------
// clients_gateway_services_infra — the realistic reference flow
// ---------------------------------------------------------------------------

#[test]
fn the_reference_flow_loads_with_no_diagnostics() {
    let pipeline = run(&fixture("clients_gateway_services_infra"));
    assert!(
        pipeline.diagnostics.is_empty(),
        "a correct configuration must be silent: {:?}",
        pipeline.diagnostics
    );
}

#[test]
fn the_reference_flow_mixes_manual_and_discovered_members_with_a_focus() {
    let pipeline = run(&fixture("clients_gateway_services_infra"));
    let view = &pipeline.overlay.views[0];

    assert_eq!(view.id.as_str(), "view:clients-to-infra");
    assert_eq!(
        view.description.as_ref().unwrap().default,
        "How a checkout request reaches storage."
    );

    // Membership is explicit, in the author's order, and mixes provenances.
    let members: Vec<&str> = view
        .entity_ids
        .as_ref()
        .expect("explicit membership")
        .iter()
        .map(|id| id.as_str())
        .collect();
    assert_eq!(
        members,
        [
            "manual:web-client",
            "manual:api-gateway",
            "package:demo", // discovered
            "manual:postgres",
            "manual:redis",
        ]
    );
    assert_eq!(
        view.default_focus.as_ref().unwrap().as_str(),
        "manual:api-gateway"
    );
}

#[test]
fn the_reference_flow_orders_stages_by_order_not_declaration() {
    let pipeline = run(&fixture("clients_gateway_services_infra"));
    let stages = &pipeline.overlay.views[0].stages;
    // Declared infrastructure(4), clients(1), services(3), gateway(2).
    let ids: Vec<&str> = stages.iter().map(|stage| stage.id.as_str()).collect();
    assert_eq!(
        ids,
        [
            "stage:clients",
            "stage:gateway",
            "stage:services",
            "stage:infrastructure"
        ]
    );
    assert_eq!(
        stages.iter().map(|s| s.order).collect::<Vec<_>>(),
        [1, 2, 3, 4]
    );
}

#[test]
fn the_reference_flow_has_labelled_role_distinct_relations() {
    let pipeline = run(&fixture("clients_gateway_services_infra"));
    let relations = &pipeline.overlay.relations;
    assert_eq!(relations.len(), 5);

    // Two edges between the SAME pair survive because their roles differ.
    let same_pair: Vec<&Relation> = relations
        .iter()
        .filter(|r| r.from.as_str() == "manual:web-client" && r.to.as_str() == "manual:api-gateway")
        .collect();
    assert_eq!(same_pair.len(), 2, "role keeps them distinct");
    assert_ne!(same_pair[0].id, same_pair[1].id);
    assert_eq!(same_pair[0].role.as_deref(), Some("http"));
    assert_eq!(same_pair[1].role.as_deref(), Some("ws"));
    // Labels are first-class, not attributes — that is what the UI renders.
    assert_eq!(same_pair[0].label.as_ref().unwrap().default, "HTTPS");
    assert_eq!(same_pair[1].label.as_ref().unwrap().default, "WebSocket");

    for relation in relations {
        assert_eq!(relation.provenance, Provenance::Manual);
    }
}

#[test]
fn the_reference_flow_embeds_its_docs_and_examples() {
    let pipeline = run(&fixture("clients_gateway_services_infra"));
    let view = &pipeline.overlay.views[0];

    // Two doc files, joined in declaration order with one blank line.
    let docs = view.docs.as_ref().expect("docs embedded");
    assert_eq!(
        docs.markdown,
        "# Checkout\n\nClients reach the gateway over HTTPS, which fans out to services.\n\n\
         ## Scaling\n\nThe gateway is stateless and scales horizontally."
    );
    assert_eq!(docs.summary, None);

    // Examples in narrative order, contents embedded verbatim.
    assert_eq!(view.examples.len(), 2);
    assert_eq!(view.examples[0].id, "request");
    assert_eq!(view.examples[0].language.as_deref(), Some("http"));
    assert!(view.examples[0].content.contains("POST /checkout HTTP/1.1"));
    assert!(view.examples[0].content.contains("{\"cart\": 42}"));
    assert_eq!(view.examples[1].id, "response");
    assert!(view.examples[1].content.contains("\"order_id\": 1001"));
}

#[test]
fn the_reference_flow_maps_presentation_overrides() {
    let pipeline = run(&fixture("clients_gateway_services_infra"));
    let entry = &pipeline.overlay.overrides[&EntityId::from_raw("package:demo")];

    assert_eq!(entry.label.as_ref().unwrap().default, "Demo service");
    assert_eq!(entry.add_tags, ["core"]);
    assert_eq!(entry.set_attributes["category"], "service");
    assert_eq!(entry.set_attributes["stage"], "stage:services");
    assert_eq!(entry.set_attributes["promoted"], true);
    assert_eq!(entry.set_attributes["accent"], "orange");
    // Override docs: never a summary, never "documented".
    let docs = entry.docs.as_ref().expect("override docs embedded");
    assert!(docs.markdown.contains("Extra notes about the demo service"));
    assert_eq!(docs.summary, None);
    assert!(!docs.documented);
}

#[test]
fn manual_entities_carry_manual_provenance_and_their_presentation() {
    let pipeline = run(&fixture("clients_gateway_services_infra"));
    let gateway = pipeline
        .overlay
        .entities
        .iter()
        .find(|e| e.id.as_str() == "manual:api-gateway")
        .expect("manual:api-gateway");

    assert_eq!(gateway.provenance, Provenance::Manual);
    assert_eq!(gateway.kind.as_str(), "manual_block");
    assert_eq!(gateway.qualified_name, "api-gateway");
    assert_eq!(gateway.attributes["tier"], "edge");
    assert_eq!(gateway.attributes["replicas"], 3);

    let web = pipeline
        .overlay
        .entities
        .iter()
        .find(|e| e.id.as_str() == "manual:web-client")
        .unwrap();
    assert_eq!(web.label.default, "Web client");
    assert_eq!(web.label.translations["de"], "Web-Client");

    let postgres = pipeline
        .overlay
        .entities
        .iter()
        .find(|e| e.id.as_str() == "manual:postgres")
        .unwrap();
    assert_eq!(postgres.tags, ["storage"], "duplicate tags deduped");
}

// ---------------------------------------------------------------------------
// Through the real graph
// ---------------------------------------------------------------------------

#[test]
fn manual_and_discovered_entities_coexist_in_a_schema_valid_document() {
    let pipeline = run(&fixture("clients_gateway_services_infra"));
    let result = document_from(pipeline.overlay);

    // `build_document` validates before returning, but assert it explicitly:
    // this is the claim the whole fixture exists to support.
    result
        .document
        .validate()
        .expect("the document must be schema-valid");

    // Both provenances, in one document.
    assert_eq!(
        entity(&result.document, "manual:web-client").provenance,
        Provenance::Manual
    );
    assert_eq!(
        entity(&result.document, "package:demo").provenance,
        Provenance::Discovered
    );

    // The manual flow view sits alongside the eight generated views.
    let flow = result
        .document
        .views
        .iter()
        .find(|view| view.id.as_str() == "view:clients-to-infra")
        .expect("the manual flow is a view of the document");
    assert!(flow.docs.is_some(), "its docs survived into the document");
    assert_eq!(flow.examples.len(), 2);
    assert!(
        result.document.views.len() > 1,
        "generated views are not replaced by the manual one"
    );

    // Nothing was dropped as dangling: every flow relation's endpoints exist.
    for relation in &result.document.relations {
        assert!(
            result
                .document
                .entities
                .iter()
                .any(|e| e.id == relation.from),
            "relation `{}` has a live `from`",
            relation.id
        );
    }
    assert!(!result.partial);
}

#[test]
fn the_override_enriches_the_discovered_entity_without_touching_its_identity() {
    let pipeline = run(&fixture("clients_gateway_services_infra"));
    let result = document_from(pipeline.overlay);
    let demo = entity(&result.document, "package:demo");

    // Presentation replaced…
    assert_eq!(demo.label.default, "Demo service");
    assert!(demo.tags.contains(&"core".to_string()));
    assert_eq!(demo.attributes["accent"], "orange");
    // …identity untouched.
    assert_eq!(demo.kind.as_str(), "package");
    assert_eq!(demo.qualified_name, "demo");
    assert_eq!(demo.provenance, Provenance::Discovered);

    // Amendment B: manual docs APPEND to the discovered rustdoc, and never
    // claim the item is documented on their own.
    let docs = demo.docs.as_ref().expect("docs survive");
    assert!(docs.markdown.starts_with("Rustdoc for the demo service."));
    assert!(docs.markdown.contains("Extra notes about the demo service"));
    assert_eq!(
        docs.summary.as_deref(),
        Some("Rustdoc for the demo service."),
        "the discovered summary is preserved"
    );
    assert!(docs.documented, "the discovered `documented` is preserved");
}

#[test]
fn the_document_is_deterministic_across_repeats() {
    let baseline = {
        let pipeline = run(&fixture("clients_gateway_services_infra"));
        cratevista_schema::canonical::to_canonical_string(&document_from(pipeline.overlay).document)
            .unwrap()
    };
    for _ in 0..5 {
        let pipeline = run(&fixture("clients_gateway_services_infra"));
        let json = cratevista_schema::canonical::to_canonical_string(
            &document_from(pipeline.overlay).document,
        )
        .unwrap();
        assert_eq!(
            json, baseline,
            "identical input must produce identical bytes"
        );
    }
}

// ---------------------------------------------------------------------------
// invalid_refs — local degradation, and the PRD-05 boundary
// ---------------------------------------------------------------------------

#[test]
fn a_malformed_file_costs_only_itself() {
    let pipeline = run(&fixture("invalid_refs"));
    // `a_broken.toml` is unparseable…
    let parse_errors: Vec<&ConfigDiagnostic> = pipeline
        .diagnostics
        .iter()
        .filter(|d| d.code == code::PARSE_ERROR)
        .collect();
    assert_eq!(parse_errors.len(), 1);
    assert_eq!(parse_errors[0].file, ".cratevista/flows/a_broken.toml");
    assert!(
        parse_errors[0].position.is_some(),
        "located, not just named"
    );

    // …yet `b_references.toml` still produced its entity and its flow.
    assert!(
        pipeline
            .overlay
            .entities
            .iter()
            .any(|e| e.id.as_str() == "manual:real"),
        "the healthy file still loaded"
    );
    assert_eq!(pipeline.overlay.views.len(), 1);
}

#[test]
fn unknown_manual_references_are_diagnosed_by_config_with_locations() {
    let pipeline = run(&fixture("invalid_refs"));
    let unknown: Vec<&ConfigDiagnostic> = pipeline
        .diagnostics
        .iter()
        .filter(|d| d.code == code::UNKNOWN_MANUAL_REFERENCE)
        .collect();

    // manual:ghost (member), manual:phantom (focus), manual:nowhere (relation).
    assert_eq!(unknown.len(), 3, "{unknown:?}");
    for diagnostic in &unknown {
        assert_eq!(diagnostic.file, ".cratevista/flows/b_references.toml");
        assert!(diagnostic.position.is_some());
    }
    let messages: Vec<&str> = unknown.iter().map(|d| d.message.as_str()).collect();
    assert!(messages.iter().any(|m| m.contains("manual:ghost")));
    assert!(messages.iter().any(|m| m.contains("manual:phantom")));
    assert!(messages.iter().any(|m| m.contains("manual:nowhere")));
}

/// The boundary: config never judges a *discovered* id — PRD 05 does.
#[test]
fn prd_05_owns_unknown_discovered_reference_diagnostics() {
    let pipeline = run(&fixture("invalid_refs"));

    // Config said nothing about the discovered ids…
    for diagnostic in &pipeline.diagnostics {
        assert!(
            !diagnostic.message.contains("item:struct:nope::Gone"),
            "config must not judge a discovered id: {diagnostic}"
        );
        assert!(
            !diagnostic.message.contains("item:struct:totally::made::Up"),
            "config must not resolve an override target: {diagnostic}"
        );
    }

    // …and the graph does, with its own stable codes.
    let result = document_from(pipeline.overlay);
    let graph_codes: Vec<&str> = result.diagnostics.iter().map(|d| d.code.as_str()).collect();
    assert!(
        graph_codes.contains(&"invalid_view_reference"),
        "PRD 05 drops the stale member: {graph_codes:?}"
    );
    assert!(
        graph_codes.contains(&"overlay_target_missing"),
        "PRD 05 reports the missing override target: {graph_codes:?}"
    );
    assert!(
        graph_codes.contains(&"dangling_relation"),
        "PRD 05 drops the relation with a missing endpoint: {graph_codes:?}"
    );

    // And the document is still valid: broken references degrade, never crash.
    result.document.validate().expect("still schema-valid");
}

#[test]
fn missing_docs_and_examples_degrade_locally() {
    let pipeline = run(&fixture("invalid_refs"));
    let missing: Vec<&ConfigDiagnostic> = pipeline
        .diagnostics
        .iter()
        .filter(|d| d.code == code::MISSING_FILE)
        .collect();
    assert_eq!(missing.len(), 2, "one doc + one example");

    // The flow still exists, just without the content that could not be read.
    let view = &pipeline.overlay.views[0];
    assert_eq!(view.id.as_str(), "view:broken-refs");
    assert_eq!(view.docs, None);
    assert!(view.examples.is_empty());
}

#[test]
fn invalid_refs_diagnostics_are_deterministic() {
    let render = || {
        let pipeline = run(&fixture("invalid_refs"));
        pipeline
            .diagnostics
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>()
    };
    let baseline = render();
    assert!(!baseline.is_empty());
    for _ in 0..5 {
        assert_eq!(render(), baseline);
    }
}

// ---------------------------------------------------------------------------
// duplicate_ids — precedence across sorted files
// ---------------------------------------------------------------------------

#[test]
fn duplicate_entity_and_flow_ids_are_reported_and_the_first_wins() {
    let pipeline = run(&fixture("duplicate_ids"));

    assert_eq!(
        codes(&pipeline)
            .iter()
            .filter(|c| **c == code::DUPLICATE_ENTITY_ID)
            .count(),
        1
    );
    assert_eq!(
        codes(&pipeline)
            .iter()
            .filter(|c| **c == code::DUPLICATE_FLOW_ID)
            .count(),
        1
    );

    // The duplicate is reported against the SECOND file and names the first.
    let duplicate = pipeline
        .diagnostics
        .iter()
        .find(|d| d.code == code::DUPLICATE_ENTITY_ID)
        .unwrap();
    assert_eq!(duplicate.file, ".cratevista/flows/b_second.toml");
    assert!(duplicate.message.contains(".cratevista/flows/a_first.toml"));

    // First-wins: exactly one entity and one view, from `a_first.toml`.
    assert_eq!(pipeline.overlay.entities.len(), 1);
    assert_eq!(pipeline.overlay.entities[0].label.default, "Redis (first)");
    assert_eq!(pipeline.overlay.views.len(), 1);
    assert_eq!(pipeline.overlay.views[0].title.default, "Dup (first)");
}

#[test]
fn override_precedence_is_last_loaded_wins_per_field_across_sorted_files() {
    let pipeline = run(&fixture("duplicate_ids"));
    assert_eq!(
        codes(&pipeline)
            .iter()
            .filter(|c| **c == code::DUPLICATE_OVERRIDE)
            .count(),
        1
    );

    let entry = &pipeline.overlay.overrides[&EntityId::from_raw("package:demo")];
    // Conflicting fields: the later file (`z_last.toml`) wins.
    assert_eq!(entry.label.as_ref().unwrap().default, "Last label");
    assert_eq!(entry.set_attributes["category"], "last");
    assert_eq!(entry.hidden, Some(true));
    // A field only the first file set survives — the merge is per field.
    assert_eq!(
        entry.description.as_ref().unwrap().default,
        "Only the first sets this"
    );
    // Tags are additive.
    assert_eq!(entry.add_tags, ["from-first", "from-last"]);
}

#[test]
fn duplicate_ids_still_build_a_valid_document() {
    let pipeline = run(&fixture("duplicate_ids"));
    let result = document_from(pipeline.overlay);
    result
        .document
        .validate()
        .expect("valid despite duplicates");
    // The surviving manual entity is in the document.
    assert_eq!(
        entity(&result.document, "manual:redis").label.default,
        "Redis (first)"
    );
}

// ---------------------------------------------------------------------------
// Pinned current behaviour
// ---------------------------------------------------------------------------

/// Markdown docs are **not** capped; only examples are (64 KiB).
///
/// Pinning today's behaviour rather than endorsing it: the same argument that
/// justifies the example cap — embedded content ships on every `/api/document`
/// fetch — applies to a large doc too. If step 7 decides to cap docs, this test
/// is the one to update, deliberately.
#[test]
fn markdown_docs_are_currently_uncapped() {
    let dir = tempfile::tempdir().unwrap();
    let big = "y".repeat(cratevista_config::MAX_EXAMPLE_BYTES * 2);
    std::fs::create_dir_all(dir.path().join(".cratevista/flows")).unwrap();
    std::fs::create_dir_all(dir.path().join("docs")).unwrap();
    std::fs::write(dir.path().join("docs/big.md"), &big).unwrap();
    std::fs::write(
        dir.path().join(".cratevista/flows/a.toml"),
        "[[flow]]\nid = \"f\"\ntitle = \"F\"\ndocs = [\"docs/big.md\"]\n",
    )
    .unwrap();

    let pipeline = run(dir.path());
    assert!(pipeline.diagnostics.is_empty(), "docs are not capped today");
    assert_eq!(
        pipeline.overlay.views[0]
            .docs
            .as_ref()
            .unwrap()
            .markdown
            .len(),
        big.len()
    );
}

/// Symlink containment, end to end through the real pipeline.
///
/// `#[cfg(unix)]` rather than "attempt and skip": on Unix — which is what CI
/// runs — symlink creation must succeed, so this can never quietly become a
/// no-op on the platform that exercises it. Windows lacks the privilege by
/// default; the containment predicate itself is unit-tested on every platform.
#[cfg(unix)]
#[test]
fn a_symlink_out_of_the_workspace_is_refused_end_to_end() {
    let dir = tempfile::tempdir().unwrap();
    let outside = dir
        .path()
        .parent()
        .unwrap()
        .join("cratevista-e2e-symlink-probe.md");
    std::fs::write(&outside, "secret from outside the workspace").unwrap();

    std::fs::create_dir_all(dir.path().join(".cratevista/flows")).unwrap();
    std::fs::create_dir_all(dir.path().join("docs")).unwrap();
    // Must succeed on Unix — a failure here is a real problem, not a skip.
    std::os::unix::fs::symlink(&outside, dir.path().join("docs/link.md"))
        .expect("unix must support symlinks");
    std::fs::write(
        dir.path().join(".cratevista/flows/a.toml"),
        "[[flow]]\nid = \"f\"\ntitle = \"F\"\ndocs = [\"docs/link.md\"]\n",
    )
    .unwrap();

    let pipeline = run(dir.path());
    // The path text is perfectly repo-relative; only resolving it reveals the
    // escape, which is exactly what `RepoRelativePath` alone cannot catch.
    assert_eq!(codes(&pipeline), [code::PATH_ESCAPES_WORKSPACE]);
    assert_eq!(pipeline.overlay.views[0].docs, None, "no smuggled content");

    let _ = std::fs::remove_file(outside);
}

// ---------------------------------------------------------------------------
// PRD-08 amendment B3 — ConfigOutcome.referenced_files
// ---------------------------------------------------------------------------

/// The realistic fixture, through the real `load_config` entry point.
#[test]
fn referenced_files_lists_every_declared_reference_of_the_reference_fixture() {
    let outcome = cratevista_config::load_config(&fixture("clients_gateway_services_infra"));
    let listed: Vec<(&str, &str)> = outcome
        .referenced_files
        .iter()
        .map(|file| (file.path.as_str(), file.kind.as_str()))
        .collect();

    // Sorted by path, then kind; all three kinds present; nothing invented.
    assert_eq!(
        listed,
        [
            (".cratevista/docs/checkout.md", "flow_doc"),
            (".cratevista/docs/demo-notes.md", "override_doc"),
            (".cratevista/docs/scaling.md", "flow_doc"),
            (".cratevista/examples/request.http", "flow_example"),
            (".cratevista/examples/response.json", "flow_example"),
        ]
    );
}

#[test]
fn referenced_files_never_exposes_an_absolute_or_traversing_path() {
    let root = fixture("clients_gateway_services_infra");
    let outcome = cratevista_config::load_config(&root);
    let root_text = root.to_string_lossy().replace('\\', "/");

    assert!(
        !outcome.referenced_files.is_empty(),
        "fixture has references"
    );
    for file in &outcome.referenced_files {
        let path = file.path.as_str();
        assert!(
            !path.contains(&*root_text),
            "leaked the workspace root: {path}"
        );
        assert!(!path.starts_with('/'), "absolute: {path}");
        assert!(!path.contains(".."), "traversing: {path}");
    }
}

#[test]
fn referenced_files_is_deterministic_across_repeated_loads() {
    let root = fixture("clients_gateway_services_infra");
    let first = cratevista_config::load_config(&root).referenced_files;
    let second = cratevista_config::load_config(&root).referenced_files;
    assert_eq!(first, second);
}

#[test]
fn missing_files_are_listed_even_though_they_are_diagnosed() {
    // The committed `invalid_refs` fixture declares two references to files that
    // do not exist. Both are diagnosed AND both are listed: the next thing that
    // happens to those paths is someone creating them, which must be watchable.
    let outcome = cratevista_config::load_config(&fixture("invalid_refs"));

    let listed: Vec<(&str, &str)> = outcome
        .referenced_files
        .iter()
        .map(|file| (file.path.as_str(), file.kind.as_str()))
        .collect();
    assert_eq!(
        listed,
        [
            (".cratevista/docs/missing.md", "flow_doc"),
            (".cratevista/examples/gone.json", "flow_example"),
        ]
    );

    // And the diagnostics for those very files are still produced — listing a
    // path does not suppress reporting it.
    let missing = outcome
        .diagnostics
        .iter()
        .filter(|d| d.code == code::MISSING_FILE)
        .count();
    assert_eq!(missing, 2, "both missing files still diagnosed");
}

#[test]
fn referenced_files_does_not_change_the_overlay_or_the_diagnostics() {
    // The amendment is read-only: adding it must not perturb what PRD 08 already
    // produced. Compare `load_config` against the pipeline assembled by hand.
    let root = fixture("clients_gateway_services_infra");
    let expected = run(&root);
    let outcome = cratevista_config::load_config(&root);

    // Diagnostics: identical, in the same order.
    let expected_codes: Vec<&str> = expected.diagnostics.iter().map(|d| d.code).collect();
    let actual_codes: Vec<&str> = outcome.diagnostics.iter().map(|d| d.code).collect();
    assert_eq!(actual_codes, expected_codes);
    let expected_text: Vec<String> = expected.diagnostics.iter().map(|d| d.to_string()).collect();
    let actual_text: Vec<String> = outcome.diagnostics.iter().map(|d| d.to_string()).collect();
    assert_eq!(actual_text, expected_text);

    // Overlay: structurally identical.
    assert_eq!(
        outcome.overlay.entities.len(),
        expected.overlay.entities.len()
    );
    assert_eq!(
        outcome.overlay.relations.len(),
        expected.overlay.relations.len()
    );
    assert_eq!(outcome.overlay.views.len(), expected.overlay.views.len());
    assert_eq!(
        outcome.overlay.overrides.len(),
        expected.overlay.overrides.len()
    );
    assert_eq!(
        outcome
            .overlay
            .entities
            .iter()
            .map(|e| e.id.as_str())
            .collect::<Vec<_>>(),
        expected
            .overlay
            .entities
            .iter()
            .map(|e| e.id.as_str())
            .collect::<Vec<_>>()
    );
    assert_eq!(
        outcome
            .overlay
            .views
            .iter()
            .map(|v| v.id.as_str())
            .collect::<Vec<_>>(),
        expected
            .overlay
            .views
            .iter()
            .map(|v| v.id.as_str())
            .collect::<Vec<_>>()
    );
    // The embedded content is untouched too.
    assert_eq!(
        outcome
            .overlay
            .views
            .iter()
            .map(|v| v.examples.len())
            .sum::<usize>(),
        expected
            .overlay
            .views
            .iter()
            .map(|v| v.examples.len())
            .sum::<usize>()
    );
}

#[test]
fn absent_configuration_yields_no_referenced_files() {
    let dir = tempfile::tempdir().expect("tempdir");
    let outcome = cratevista_config::load_config(dir.path());
    assert!(outcome.referenced_files.is_empty());
    assert!(outcome.is_empty(), "no config is still the empty outcome");
}
