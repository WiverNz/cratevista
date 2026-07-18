//! `document.json` is deterministic by default: identical logical documents
//! serialize byte-identically, regardless of the input ordering of entities /
//! relations / views.

use cratevista_schema::canonical::to_canonical_string;
use cratevista_schema::{
    Entity, EntityId, EntityKind, ExplorerDocument, LocalizedText, Project, Provenance, Relation,
    RelationKind,
};

fn project() -> Project {
    Project {
        id: "demo".into(),
        name: "Demo".into(),
        description: "d".into(),
        root: None,
        repository_url: None,
        default_branch: None,
    }
}

fn entity(id: EntityId, kind: &str) -> Entity {
    Entity::new(
        id,
        EntityKind::new(kind),
        LocalizedText::new("x"),
        "x",
        Provenance::Discovered,
    )
}

fn build(order_reversed: bool) -> ExplorerDocument {
    let ws = EntityId::workspace();
    let a = EntityId::package("a");
    let b = EntityId::package("b");
    let mut entities = vec![
        entity(ws.clone(), EntityKind::WORKSPACE),
        entity(a.clone(), EntityKind::PACKAGE),
        entity(b.clone(), EntityKind::PACKAGE),
    ];
    let mut relations = vec![
        Relation::new(
            RelationKind::new(RelationKind::CONTAINS),
            ws.clone(),
            a,
            Provenance::Discovered,
        ),
        Relation::new(
            RelationKind::new(RelationKind::CONTAINS),
            ws,
            b,
            Provenance::Discovered,
        ),
    ];
    if order_reversed {
        entities.reverse();
        relations.reverse();
    }
    ExplorerDocument::new(project(), entities, relations, vec![])
}

#[test]
fn same_inputs_produce_identical_bytes() {
    assert_eq!(
        to_canonical_string(&build(false)).unwrap(),
        to_canonical_string(&build(false)).unwrap(),
    );
}

#[test]
fn input_order_does_not_affect_output() {
    assert_eq!(
        to_canonical_string(&build(false)).unwrap(),
        to_canonical_string(&build(true)).unwrap(),
    );
}

#[test]
fn document_has_no_timestamp_field() {
    let text = to_canonical_string(&build(false)).unwrap();
    assert!(!text.contains("generated_at"));
    assert!(!text.contains("timestamp"));
}
