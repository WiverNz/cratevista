//! Unknown entity/relation kinds deserialize, validate, serialize, and
//! round-trip without loss (no strict mode rejects them).

use std::path::PathBuf;

use cratevista_schema::canonical::to_canonical_string;
use cratevista_schema::{EntityKind, ExplorerDocument, RelationKind};

fn fixture(name: &str) -> String {
    let mut path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    path.push("fixtures");
    path.push(name);
    std::fs::read_to_string(path).unwrap()
}

#[test]
fn unknown_kind_fixture_validates_and_round_trips() {
    let text = fixture("unknown_kind.document.json");
    let doc: ExplorerDocument = serde_json::from_str(&text).unwrap();
    doc.validate()
        .expect("unknown kinds must not be validation errors");
    assert_eq!(to_canonical_string(&doc).unwrap(), text);

    // The document really does contain unknown kinds.
    assert!(doc.entities.iter().any(|e| !e.kind.is_known()));
    assert!(doc.relations.iter().any(|r| !r.kind.is_known()));
}

#[test]
fn unknown_kind_values_preserve_their_string() {
    let entity_kind = EntityKind::new("future_widget");
    assert!(!entity_kind.is_known());
    let relation_kind = RelationKind::new("future_edge");
    assert!(!relation_kind.is_known());

    let ek: EntityKind = serde_json::from_str("\"future_widget\"").unwrap();
    assert_eq!(ek.as_str(), "future_widget");
    let rk: RelationKind = serde_json::from_str("\"future_edge\"").unwrap();
    assert_eq!(rk.as_str(), "future_edge");
}
