//! Shared helpers for the rustdoc-adapter integration tests.

#![allow(dead_code)]

use std::path::PathBuf;

use cratevista_rustdoc::{CrateIngest, NormalizeContext, RustdocTargetKind};
use cratevista_schema::EntityId;

/// The path to a checked-in `*.rustdoc.json` fixture.
pub fn fixture_path(name: &str) -> PathBuf {
    let mut path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    path.push("tests");
    path.push("fixtures");
    path.push(format!("{name}.rustdoc.json"));
    path
}

/// Reads a checked-in `*.rustdoc.json` fixture as a string.
pub fn fixture(name: &str) -> String {
    let path = fixture_path(name);
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {path:?}: {e}"))
}

/// A normalize context matching how the `sample_lib` fixture was generated:
/// workspace root `/w`, package under it, no absolute path leaks.
pub fn sample_context() -> NormalizeContext {
    NormalizeContext {
        workspace_root: PathBuf::from("/w"),
        package_root: PathBuf::from("/w/crates/cratevista-rustdoc/tests/fixtures/sample_lib"),
        package_id: EntityId::package("sample_lib"),
        target_id: EntityId::target("sample_lib", "lib", "sample_lib"),
        package_name: "sample_lib".into(),
        crate_name: "sample_lib".into(),
        target_name: "sample_lib".into(),
        target_kind: RustdocTargetKind::Library,
        toolchain: "nightly-2026-07-01".into(),
    }
}

pub fn has_entity(ingest: &CrateIngest, id: &str) -> bool {
    ingest.entities.iter().any(|e| e.id.as_str() == id)
}

pub fn entity<'a>(ingest: &'a CrateIngest, id: &str) -> &'a cratevista_schema::Entity {
    ingest
        .entities
        .iter()
        .find(|e| e.id.as_str() == id)
        .unwrap_or_else(|| panic!("entity {id} not found; have: {:?}", ids(ingest)))
}

pub fn ids(ingest: &CrateIngest) -> Vec<String> {
    ingest
        .entities
        .iter()
        .map(|e| e.id.as_str().to_string())
        .collect()
}

pub fn relations_of_kind<'a>(
    ingest: &'a CrateIngest,
    kind: &str,
) -> Vec<&'a cratevista_schema::Relation> {
    ingest
        .relations
        .iter()
        .filter(|r| r.kind.as_str() == kind)
        .collect()
}

/// Asserts no sanitized workspace-root token (`/w`) leaks into any entity path,
/// attribute, relation, diagnostic, or summary field.
pub fn assert_no_absolute_paths(ingest: &CrateIngest) {
    let mut blob = String::new();
    for entity in &ingest.entities {
        if let Some(source) = &entity.source {
            blob.push_str(source.path.as_str());
            blob.push('\n');
        }
        blob.push_str(&serde_json::to_string(&entity.attributes).unwrap());
        blob.push_str(entity.id.as_str());
        blob.push_str(&entity.qualified_name);
    }
    for relation in &ingest.relations {
        blob.push_str(relation.id.as_str());
    }
    for reference in &ingest.summary.unresolved_refs {
        blob.push_str(&reference.display);
        if let Some(name) = &reference.crate_name {
            blob.push_str(name);
        }
        if let Some(path) = &reference.canonical_path {
            blob.push_str(&path.join("::"));
        }
    }
    // The crate-identity fields must not carry a path either.
    blob.push_str(ingest.summary.package_id.as_str());
    blob.push_str(ingest.summary.target_id.as_str());
    blob.push_str(ingest.summary.root_module_id.as_str());
    assert!(
        !blob.contains("/w"),
        "a workspace-root token leaked into the output"
    );
    assert!(
        !blob.to_lowercase().contains(":\\"),
        "a Windows absolute path leaked"
    );
}
