//! Shared helpers for the integration tests.

#![allow(dead_code)]

use std::path::PathBuf;

use cratevista_metadata::MetadataIngest;

pub fn load(name: &str) -> cargo_metadata::Metadata {
    let mut path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    path.push("fixtures");
    path.push(format!("{name}.metadata.json"));
    let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {path:?}: {e}"));
    serde_json::from_str(&text).unwrap_or_else(|e| panic!("parse {name}: {e}"))
}

pub fn has_entity(ingest: &MetadataIngest, id: &str) -> bool {
    ingest.entities.iter().any(|e| e.id.as_str() == id)
}

pub fn entity<'a>(ingest: &'a MetadataIngest, id: &str) -> &'a cratevista_schema::Entity {
    ingest
        .entities
        .iter()
        .find(|e| e.id.as_str() == id)
        .unwrap_or_else(|| panic!("entity {id} not found"))
}

pub fn dependency_edges<'a>(
    ingest: &'a MetadataIngest,
    from: &str,
    to: &str,
) -> Vec<&'a cratevista_schema::Relation> {
    ingest
        .relations
        .iter()
        .filter(|r| {
            r.kind.as_str() == cratevista_schema::RelationKind::DEPENDS_ON
                && r.from.as_str() == from
                && r.to.as_str() == to
        })
        .collect()
}

/// Asserts that no sanitized absolute-root token (`/w`) leaked into any entity,
/// relation, diagnostic, or the summary's repo-relative fields.
pub fn assert_no_absolute_paths(ingest: &MetadataIngest) {
    let mut blob = String::new();
    for entity in &ingest.entities {
        if let Some(source) = &entity.source {
            blob.push_str(source.path.as_str());
            blob.push('\n');
        }
        blob.push_str(&serde_json::to_string(&entity.attributes).unwrap());
    }
    for relation in &ingest.relations {
        blob.push_str(&serde_json::to_string(&relation.attributes).unwrap());
    }
    if let Some(root) = &ingest.summary.workspace_root_repo_relative {
        blob.push_str(root);
    }
    assert!(
        !blob.contains("/w"),
        "an absolute path token leaked into the ingest output"
    );
}
