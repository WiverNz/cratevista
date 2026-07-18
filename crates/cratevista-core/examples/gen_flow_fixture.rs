//! Generates the `flow` E2E snapshot: a manual architecture flow whose view
//! carries schema-1.1 `docs` and embedded `examples`.
//!
//! **Why this is synthesized rather than generated from a Rust workspace.** The
//! producer of flow docs/examples is `cratevista-config` (PRD 08), which does not
//! exist yet, and `cargo cratevista generate` cannot emit these fields on its
//! own. This fixture is therefore built directly from the schema types — but it
//! is *not* hand-written JSON: it is committed through the **production writer**
//! (`cratevista_core::artifacts::commit_artifacts`), so it is schema-validated
//! and its BLAKE3 `artifact_hashes` are correct by construction, exactly like
//! every other committed snapshot. It models what a `.cratevista/flows/*.toml`
//! file will produce once PRD 08 lands.
//!
//! Its purpose is to let PRD-08 Amendment C prove, in a real browser against the
//! real server and CSP, that view docs/examples render — and that example
//! contents render **without** `/api/source`, because they are embedded.
//!
//! Usage (writes `web/e2e/fixtures/flow/`):
//!
//! ```bash
//! cargo run -p cratevista-core --example gen_flow_fixture
//! ```

use std::path::Path;
use std::process::ExitCode;

use cratevista_core::artifacts::commit_artifacts;
use cratevista_schema::{
    Counts, DiagnosticsReport, DocBlock, Entity, EntityId, EntityKind, ExplorerDocument,
    GenerationReport, Generator, LocalizedText, Project, Provenance, Relation, RelationKind,
    Timestamp, View, ViewExample, ViewId,
};

/// A manual entity, as `cratevista-config` will emit it.
fn manual(id: &str, kind: &str, label: &str) -> Entity {
    Entity::new(
        EntityId::from_raw(id),
        EntityKind::new(kind),
        LocalizedText::new(label),
        label,
        Provenance::Manual,
    )
}

fn relation(from: &str, to: &str, kind: &str, label: &str) -> Relation {
    let mut relation = Relation::new(
        RelationKind::new(kind),
        EntityId::from_raw(from),
        EntityId::from_raw(to),
        Provenance::Manual,
    );
    relation.label = Some(LocalizedText::new(label));
    relation
}

fn main() -> ExitCode {
    let entities = vec![
        manual("manual:web", "external_system", "Web client"),
        manual("manual:gateway", "manual_block", "API gateway"),
        manual("manual:orders", "manual_block", "Orders service"),
        manual("manual:postgres", "infrastructure", "PostgreSQL"),
    ];
    let relations = vec![
        relation("manual:web", "manual:gateway", "manual", "HTTPS"),
        relation("manual:gateway", "manual:orders", "manual", "gRPC"),
        relation("manual:orders", "manual:postgres", "manual", "SQL"),
    ];

    let flow = View {
        id: ViewId::view("flow-checkout"),
        title: LocalizedText::new("Checkout flow"),
        description: Some(LocalizedText::new(
            "How an order travels from the browser to storage.",
        )),
        entity_kinds: Vec::new(),
        relation_kinds: Vec::new(),
        // Explicit membership: a flow selects its participants.
        entity_ids: Some(entities.iter().map(|e| e.id.clone()).collect()),
        stages: Vec::new(),
        default_focus: Some(EntityId::from_raw("manual:gateway")),
        presentation: Default::default(),
        docs: Some(DocBlock {
            markdown: "## Checkout\n\nClients → Gateway → Services → Infrastructure.\n\n\
                       The gateway authenticates, then fans out to the orders service."
                .into(),
            summary: Some("Clients → Gateway → Services → Infrastructure.".into()),
            documented: true,
        }),
        examples: vec![
            ViewExample {
                id: "request".into(),
                title: LocalizedText::new("Example request"),
                language: Some("http".into()),
                content: "POST /checkout HTTP/1.1\nContent-Type: application/json\n\n\
                          {\"cart\": 42}"
                    .into(),
                description: Some(LocalizedText::new("What the web client sends.")),
            },
            ViewExample {
                id: "response".into(),
                title: LocalizedText::new("Example response"),
                language: None,
                content: "{\"order_id\": 1001, \"status\": \"accepted\"}".into(),
                description: None,
            },
        ],
    };

    let project = Project {
        id: "flow-demo".into(),
        name: "Flow demo".into(),
        description: "Fixture exercising schema-1.1 view docs and examples.".into(),
        root: None,
        repository_url: None,
        default_branch: None,
    };
    let document = ExplorerDocument::new(project, entities, relations, vec![flow]);
    if let Err(errors) = document.validate() {
        eprintln!("the generated fixture must be valid: {errors:?}");
        return ExitCode::FAILURE;
    }

    let diagnostics = DiagnosticsReport::new(Vec::new());
    let generation = GenerationReport {
        generator: Generator {
            name: "gen_flow_fixture".into(),
            version: env!("CARGO_PKG_VERSION").into(),
        },
        // Fixed, not the clock: the fixture must be byte-reproducible.
        generated_at: Timestamp::new("2026-07-16T00:00:00Z"),
        toolchain: None,
        rustdoc_format_version: None,
        input_hashes: Default::default(),
        counts: Counts {
            entities: document.entities.len() as u64,
            relations: document.relations.len() as u64,
            views: document.views.len() as u64,
            diagnostics: 0,
        },
        durations_ms: Default::default(),
        artifact_hashes: None, // populated by the writer
        partial: false,
    };

    let dest = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../web/e2e/fixtures/flow");
    match commit_artifacts(&dest, &document, &diagnostics, generation) {
        Ok(paths) => {
            for path in paths {
                println!("wrote {}", path.display());
            }
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("could not commit the fixture: {error}");
            ExitCode::FAILURE
        }
    }
}
