//! PRD-08 Amendment A: optional `View::docs` / `View::examples` (+ `ViewExample`)
//! and the additive `SchemaVersion` 1.0 → 1.1 bump.
//!
//! The properties that make the amendment *additive* are asserted here, so a
//! later change cannot quietly turn a minor bump into a breaking one: a `1.0`
//! document (which has neither field) must still deserialize and validate, and a
//! view without docs/examples must serialize without the keys at all.

use cratevista_schema::canonical::to_canonical_string;
use cratevista_schema::{
    DocBlock, EntityId, ExplorerDocument, LocalizedText, Project, SchemaVersion, View, ViewExample,
    ViewId,
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

/// A view carrying both new fields.
fn documented_view() -> View {
    View {
        id: ViewId::view("flow-checkout"),
        title: LocalizedText::new("Checkout flow"),
        description: Some(LocalizedText::new("How an order is placed.")),
        entity_kinds: Vec::new(),
        relation_kinds: Vec::new(),
        entity_ids: Some(vec![EntityId::from_raw("manual:gateway")]),
        stages: Vec::new(),
        default_focus: None,
        presentation: Default::default(),
        docs: Some(DocBlock {
            markdown: "# Checkout\n\nClients → Gateway → Services.".into(),
            summary: Some("Checkout".into()),
            documented: true,
        }),
        examples: vec![
            ViewExample {
                id: "request".into(),
                title: LocalizedText::new("Request"),
                language: Some("http".into()),
                content: "POST /checkout HTTP/1.1\n\n{\"id\":1}".into(),
                description: Some(LocalizedText::new("A sample request.")),
            },
            ViewExample {
                id: "response".into(),
                title: LocalizedText::new("Response"),
                language: None,
                content: "{\"ok\":true}".into(),
                description: None,
            },
        ],
    }
}

fn plain_view() -> View {
    View {
        id: ViewId::view("types"),
        title: LocalizedText::new("Types"),
        description: None,
        entity_kinds: Vec::new(),
        relation_kinds: Vec::new(),
        entity_ids: None,
        stages: Vec::new(),
        default_focus: None,
        presentation: Default::default(),
        docs: None,
        examples: Vec::new(),
    }
}

fn document(views: Vec<View>) -> ExplorerDocument {
    ExplorerDocument::new(project(), Vec::new(), Vec::new(), views)
}

#[test]
fn current_schema_version_is_1_1_and_still_major_1() {
    assert_eq!(SchemaVersion::CURRENT, "1.1");
    assert_eq!(SchemaVersion::current().as_str(), "1.1");
    // The major is what every consumer gates on; a minor bump must not touch it.
    assert!(SchemaVersion::CURRENT.starts_with("1."));
}

#[test]
fn view_with_docs_and_examples_round_trips() {
    let original = document(vec![documented_view()]);
    let json = to_canonical_string(&original).unwrap();
    let parsed: ExplorerDocument = serde_json::from_str(&json).unwrap();
    assert_eq!(parsed, original);

    let view = &parsed.views[0];
    let docs = view.docs.as_ref().expect("docs survive the round trip");
    assert!(docs.markdown.contains("Clients → Gateway → Services."));
    assert_eq!(docs.summary.as_deref(), Some("Checkout"));
    assert_eq!(view.examples.len(), 2);
    assert_eq!(view.examples[0].id, "request");
    assert_eq!(view.examples[0].language.as_deref(), Some("http"));
    // The content is embedded verbatim — no path, no /api/source needed.
    assert!(view.examples[0].content.contains("POST /checkout"));
    assert_eq!(view.examples[1].language, None);
    assert_eq!(view.examples[1].description, None);
}

#[test]
fn view_example_round_trips_on_its_own() {
    let example = ViewExample {
        id: "e1".into(),
        title: LocalizedText::new("E1"),
        language: Some("json".into()),
        content: "{}".into(),
        description: None,
    };
    let json = serde_json::to_string(&example).unwrap();
    assert_eq!(serde_json::from_str::<ViewExample>(&json).unwrap(), example);
}

#[test]
fn absent_docs_and_examples_are_omitted_from_the_json() {
    let json = to_canonical_string(&document(vec![plain_view()])).unwrap();
    // `skip_serializing_if` keeps the generated views byte-identical in shape to
    // schema 1.0 apart from the version marker itself.
    assert!(!json.contains("\"docs\""), "absent docs must not serialize");
    assert!(
        !json.contains("\"examples\""),
        "empty examples must not serialize"
    );
}

/// The core backward-compatibility property: a document written against schema
/// 1.0 has no `docs`/`examples` keys anywhere, and must still parse and validate.
#[test]
fn a_schema_1_0_document_still_deserializes_and_validates() {
    let json = r#"{
      "schema_version": "1.0",
      "project": { "id": "demo", "name": "Demo", "description": "d" },
      "entities": [],
      "relations": [],
      "views": [
        {
          "id": "view:types",
          "title": { "default": "Types" },
          "entity_kinds": ["struct"]
        }
      ]
    }"#;
    let parsed: ExplorerDocument = serde_json::from_str(json).expect("a 1.0 document must parse");
    assert_eq!(parsed.schema_version.as_str(), "1.0");
    parsed.validate().expect("a 1.0 document must stay valid");

    // The new fields default to absent rather than failing.
    let view = &parsed.views[0];
    assert_eq!(view.docs, None);
    assert!(view.examples.is_empty());
}

#[test]
fn canonical_serialization_of_docs_and_examples_is_deterministic() {
    let json = to_canonical_string(&document(vec![documented_view()])).unwrap();
    for _ in 0..8 {
        assert_eq!(
            to_canonical_string(&document(vec![documented_view()])).unwrap(),
            json
        );
    }
    // And a parse → re-serialize cycle reproduces the exact bytes.
    let parsed: ExplorerDocument = serde_json::from_str(&json).unwrap();
    assert_eq!(to_canonical_string(&parsed).unwrap(), json);
}

#[test]
fn example_order_is_preserved_not_sorted() {
    // Examples are an author-ordered narrative (request, then response), so the
    // canonical serializer must not reorder them the way it sorts entities.
    let json = to_canonical_string(&document(vec![documented_view()])).unwrap();
    let request = json.find("\"request\"").expect("request example present");
    let response = json.find("\"response\"").expect("response example present");
    assert!(
        request < response,
        "author order must survive serialization"
    );
}
