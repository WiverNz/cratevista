//! Package selection behavior.

mod common;

use cratevista_metadata::{MetadataError, MetadataOptions, PackageSelection, normalize};

#[test]
fn default_includes_all_members() {
    let metadata = common::load("workspace_deps");
    let ingest = normalize(&metadata, &MetadataOptions::default()).unwrap();
    for id in ["package:app", "package:core", "package:mac"] {
        assert!(common::has_entity(&ingest, id));
    }
    assert_eq!(ingest.summary.workspace_package_count, 8);
}

#[test]
fn workspace_selection_matches_default_members() {
    let metadata = common::load("workspace_deps");
    let options = MetadataOptions {
        selection: PackageSelection::Workspace,
        ..Default::default()
    };
    let ingest = normalize(&metadata, &options).unwrap();
    assert!(common::has_entity(&ingest, "package:app"));
}

#[test]
fn packages_selection_restricts_members() {
    let metadata = common::load("workspace_deps");
    let options = MetadataOptions {
        selection: PackageSelection::Packages(vec!["core".to_string()]),
        ..Default::default()
    };
    let ingest = normalize(&metadata, &options).unwrap();
    assert!(common::has_entity(&ingest, "package:core"));
    assert!(!common::has_entity(&ingest, "package:app"));
}

#[test]
fn missing_selected_package_is_fatal() {
    let metadata = common::load("workspace_deps");
    let options = MetadataOptions {
        selection: PackageSelection::Packages(vec!["does-not-exist".to_string()]),
        ..Default::default()
    };
    let error = normalize(&metadata, &options).unwrap_err();
    assert_eq!(error.code(), "package_not_found");
    assert!(matches!(error, MetadataError::PackageNotFound(_)));
}
