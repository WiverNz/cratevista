//! Validation detects dangling references and duplicate ids, collecting all
//! problems.

use cratevista_schema::{
    Entity, EntityId, EntityKind, ExplorerDocument, LocalizedText, Project, Provenance, Relation,
    RelationKind, SchemaError,
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

fn entity(id: EntityId) -> Entity {
    Entity::new(
        id,
        EntityKind::new(EntityKind::PACKAGE),
        LocalizedText::new("x"),
        "x",
        Provenance::Discovered,
    )
}

#[test]
fn valid_document_passes() {
    let a = EntityId::package("a");
    let b = EntityId::package("b");
    let doc = ExplorerDocument::new(
        project(),
        vec![entity(a.clone()), entity(b.clone())],
        vec![Relation::new(
            RelationKind::new(RelationKind::DEPENDS_ON),
            a,
            b,
            Provenance::Discovered,
        )],
        vec![],
    );
    assert!(doc.validate().is_ok());
}

#[test]
fn dangling_relation_endpoint_is_reported() {
    let a = EntityId::package("a");
    let missing = EntityId::package("missing");
    let doc = ExplorerDocument::new(
        project(),
        vec![entity(a.clone())],
        vec![Relation::new(
            RelationKind::new(RelationKind::DEPENDS_ON),
            a,
            missing,
            Provenance::Discovered,
        )],
        vec![],
    );
    let errors = doc.validate().unwrap_err();
    assert!(
        errors
            .iter()
            .any(|e| matches!(e, SchemaError::DanglingRelationEndpoint { end: "to", .. }))
    );
}

#[test]
fn duplicate_entity_id_is_reported() {
    let a = EntityId::package("a");
    let doc = ExplorerDocument::new(
        project(),
        vec![entity(a.clone()), entity(a)],
        vec![],
        vec![],
    );
    let errors = doc.validate().unwrap_err();
    assert!(
        errors
            .iter()
            .any(|e| matches!(e, SchemaError::DuplicateEntityId(_)))
    );
}

#[test]
fn dangling_parent_is_reported() {
    let a = EntityId::package("a");
    let mut child = entity(EntityId::package("child"));
    child.parent = Some(EntityId::package("nope"));
    let doc = ExplorerDocument::new(project(), vec![entity(a), child], vec![], vec![]);
    let errors = doc.validate().unwrap_err();
    assert!(
        errors
            .iter()
            .any(|e| matches!(e, SchemaError::DanglingParent { .. }))
    );
}
