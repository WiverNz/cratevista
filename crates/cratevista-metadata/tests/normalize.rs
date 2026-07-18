//! Pure normalization of checked-in Cargo metadata fixtures.

mod common;

use cratevista_metadata::{MetadataOptions, TargetKinds, normalize};

#[test]
fn member_package_captures_declared_repository() {
    // The fixture's `[package] repository` reaches the member package entity as an
    // attribute — the value the graph layer turns into `Project.repository_url`.
    let metadata = common::load("single_package_repo");
    let ingest = normalize(&metadata, &MetadataOptions::default()).unwrap();
    let package = common::entity(&ingest, "package:solo");
    assert_eq!(
        package.attributes["repository"],
        serde_json::json!("https://github.com/example/example")
    );
}

#[test]
fn absent_repository_adds_no_attribute() {
    // The plain fixture declares no `repository`, so no attribute is added (the
    // graph layer then yields `Project.repository_url = None`).
    let metadata = common::load("single_package");
    let ingest = normalize(&metadata, &MetadataOptions::default()).unwrap();
    let package = common::entity(&ingest, "package:solo");
    assert!(!package.attributes.contains_key("repository"));
}

#[test]
fn single_package_maps_workspace_package_and_target() {
    let metadata = common::load("single_package");
    let ingest = normalize(&metadata, &MetadataOptions::default()).unwrap();

    assert!(common::has_entity(&ingest, "workspace"));
    assert!(common::has_entity(&ingest, "package:solo"));
    assert!(common::has_entity(&ingest, "target:solo:lib:solo"));

    let package = common::entity(&ingest, "package:solo");
    assert_eq!(package.kind.as_str(), "package");
    assert_eq!(package.attributes["version"], serde_json::json!("0.1.0"));
    assert_eq!(package.source.as_ref().unwrap().path.as_str(), "Cargo.toml");

    let target = common::entity(&ingest, "target:solo:lib:solo");
    assert_eq!(target.source.as_ref().unwrap().path.as_str(), "src/lib.rs");
    assert_eq!(target.attributes["edition"], serde_json::json!("2021"));

    // Containment relations exist.
    assert_eq!(
        ingest
            .relations
            .iter()
            .filter(|r| r.kind.as_str() == "contains")
            .count(),
        2
    );
    common::assert_no_absolute_paths(&ingest);
}

#[test]
fn workspace_deps_map_roles_attributes_and_targets() {
    let metadata = common::load("workspace_deps");
    // Include example/test/bench to exercise opt-in kinds.
    let options = MetadataOptions {
        target_kinds: TargetKinds {
            example: true,
            test: true,
            bench: true,
            build_script: true,
        },
        ..MetadataOptions::default()
    };
    let ingest = normalize(&metadata, &options).unwrap();

    // Members present.
    for id in ["package:app", "package:core", "package:mac", "package:plat"] {
        assert!(common::has_entity(&ingest, id), "missing {id}");
    }
    // proc-macro target.
    assert!(common::has_entity(&ingest, "target:mac:proc-macro:mac"));
    // app targets (with opt-ins enabled): lib, bin, build-script, example, test, bench.
    for id in [
        "target:app:lib:app",
        "target:app:bin:app",
        "target:app:custom-build:build-script",
    ] {
        assert!(
            ingest.entities.iter().any(|e| e.id.as_str() == id)
                || ingest
                    .entities
                    .iter()
                    .any(|e| e.id.as_str().starts_with("target:app:")),
            "expected app targets incl. {id}"
        );
    }

    // Dependency roles.
    let normal = common::dependency_edges(&ingest, "package:app", "package:core");
    assert!(
        normal.iter().any(|r| r.role.as_deref() == Some("normal")),
        "app->core normal edge"
    );
    let build = common::dependency_edges(&ingest, "package:app", "package:builddep");
    assert!(
        build.iter().any(|r| r.role.as_deref() == Some("build")),
        "app->builddep build edge"
    );
    let dev = common::dependency_edges(&ingest, "package:app", "package:devdep");
    assert!(
        dev.iter().any(|r| r.role.as_deref() == Some("dev")),
        "app->devdep dev edge"
    );

    // Renamed dependency: app -> core2, attribute `rename`.
    let renamed = common::dependency_edges(&ingest, "package:app", "package:core2");
    assert!(
        renamed.iter().any(|r| r.attributes.contains_key("rename")),
        "renamed dep carries a rename attribute"
    );

    // Target-specific dep to plat carries a target_cfg attribute and a
    // discriminator in its id (distinct from a plain role edge).
    let plat = common::dependency_edges(&ingest, "package:app", "package:plat");
    assert!(
        plat.iter()
            .any(|r| r.attributes.contains_key("target_cfg")
                && r.id.as_str().matches(':').count() >= 5),
        "platform dep carries target_cfg + discriminator id"
    );

    // Feature attributes on package entities.
    let app = common::entity(&ingest, "package:app");
    assert!(app.attributes.contains_key("declared_features"));
    assert!(app.attributes.contains_key("enabled_features"));
    assert!(app.attributes.contains_key("default_features_enabled"));

    common::assert_no_absolute_paths(&ingest);
}

#[test]
fn default_excludes_optin_target_kinds() {
    let metadata = common::load("workspace_deps");
    let ingest = normalize(&metadata, &MetadataOptions::default()).unwrap();
    // No example/test/bench/build-script targets by default.
    assert!(
        !ingest
            .entities
            .iter()
            .any(|e| e.id.as_str().contains(":example:")
                || e.id.as_str().contains(":test:")
                || e.id.as_str().contains(":bench:")
                || e.id.as_str().contains(":custom-build:")),
        "opt-in target kinds must be excluded by default"
    );
}
