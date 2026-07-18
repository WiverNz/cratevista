//! External-dependency mode behavior and external-path portability.

mod common;

use cratevista_metadata::{ExternalDepsMode, MetadataOptions, normalize};

#[test]
fn exclude_omits_externals_and_their_edges() {
    let metadata = common::load("external_path");
    let ingest = normalize(&metadata, &MetadataOptions::default()).unwrap();

    assert!(common::has_entity(&ingest, "package:app"));
    // The external `ext` package is not included.
    assert!(
        !ingest
            .entities
            .iter()
            .any(|e| e.id.as_str().starts_with("package:ext@")),
        "external package excluded by default"
    );
    // No dangling edge to the excluded external.
    assert!(
        !ingest
            .relations
            .iter()
            .any(|r| r.to.as_str().starts_with("package:ext@")),
        "workspace->external edges omitted consistently"
    );
    assert_eq!(ingest.summary.external_package_count, 0);
    common::assert_no_absolute_paths(&ingest);
}

#[test]
fn direct_only_includes_external_without_source() {
    let metadata = common::load("external_path");
    let options = MetadataOptions {
        external_deps: ExternalDepsMode::DirectOnly,
        ..Default::default()
    };
    let ingest = normalize(&metadata, &options).unwrap();

    let ext = common::entity(&ingest, "package:ext@2.0.0");
    assert_eq!(ext.attributes["version"], serde_json::json!("2.0.0"));
    // External path dep outside the workspace root → no source location.
    assert!(
        ext.source.is_none(),
        "external path dep outside root must have no SourceLocation"
    );
    // The dependency edge exists.
    let edges = common::dependency_edges(&ingest, "package:app", "package:ext@2.0.0");
    assert!(edges.iter().any(|r| r.role.as_deref() == Some("normal")));
    assert_eq!(ingest.summary.external_package_count, 1);
    common::assert_no_absolute_paths(&ingest);
}

#[test]
fn full_graph_includes_external() {
    let metadata = common::load("external_path");
    let options = MetadataOptions {
        external_deps: ExternalDepsMode::FullGraph,
        ..Default::default()
    };
    let ingest = normalize(&metadata, &options).unwrap();
    assert!(common::has_entity(&ingest, "package:ext@2.0.0"));
    common::assert_no_absolute_paths(&ingest);
}
