//! Generates the checked-in JSON fixtures under `crates/cratevista-schema/fixtures/`.
//!
//! Run with:
//!
//! ```bash
//! cargo run -p cratevista-schema --example gen_schema_fixtures
//! ```
//!
//! Fixtures are written through the canonical serializer, so
//! `tests/roundtrip.rs` (deserialize → canonical serialize → byte-compare) also
//! guards that these builders and the committed files agree.

use std::collections::BTreeMap;
use std::path::PathBuf;

use cratevista_schema::canonical::to_canonical_string;
use cratevista_schema::{
    ArtifactHashes, Counts, DiagnosticsReport, DocBlock, DocumentDiagnostic, Entity, EntityId,
    EntityKind, ExplorerDocument, GenerationReport, Generator, LocalizedText, Project, Relation,
    RelationKind, RepoRelativePath, Severity, SourceLocation, Span, Stage, Timestamp, View, ViewId,
};

fn discovered(id: EntityId, kind: &str, label: &str, qualified: &str) -> Entity {
    Entity::new(
        id,
        EntityKind::new(kind),
        LocalizedText::new(label),
        qualified,
        cratevista_schema::Provenance::Discovered,
    )
}

fn manual(id: EntityId, kind: &str, label: &str, qualified: &str) -> Entity {
    Entity::new(
        id,
        EntityKind::new(kind),
        LocalizedText::new(label),
        qualified,
        cratevista_schema::Provenance::Manual,
    )
}

fn rel(kind: &str, from: &EntityId, to: &EntityId) -> Relation {
    Relation::new(
        RelationKind::new(kind),
        from.clone(),
        to.clone(),
        cratevista_schema::Provenance::Discovered,
    )
}

fn write(name: &str, contents: &str) {
    let mut path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    path.push("fixtures");
    std::fs::create_dir_all(&path).unwrap();
    path.push(name);
    std::fs::write(&path, contents).unwrap();
    println!("wrote {}", path.display());
}

fn project() -> Project {
    Project {
        id: "demo".into(),
        name: "Demo Workspace".into(),
        description: "A sample CrateVista workspace.".into(),
        root: None,
        repository_url: Some("https://example.com/demo".into()),
        default_branch: Some("main".into()),
    }
}

fn minimal() -> ExplorerDocument {
    let ws = EntityId::workspace();
    let pkg = EntityId::package("demo");
    let entities = vec![
        discovered(ws.clone(), EntityKind::WORKSPACE, "Demo", "demo"),
        discovered(pkg.clone(), EntityKind::PACKAGE, "demo", "demo"),
    ];
    let relations = vec![rel(RelationKind::CONTAINS, &ws, &pkg)];
    let views = vec![View {
        id: ViewId::view("overview"),
        title: LocalizedText::new("Workspace overview"),
        description: None,
        entity_kinds: vec![],
        relation_kinds: vec![],
        entity_ids: None,
        stages: vec![],
        default_focus: Some(ws),
        presentation: BTreeMap::new(),
        docs: None,
        examples: Vec::new(),
    }];
    let doc = ExplorerDocument::new(project(), entities, relations, views);
    doc.validate().expect("minimal doc is valid");
    doc
}

fn full_mvp() -> ExplorerDocument {
    let ws = EntityId::workspace();
    let pkg = EntityId::package("demo");
    let ext_pkg = EntityId::external_package("serde", "1.0.0");
    let target = EntityId::target("demo", "lib", "demo");
    let module = EntityId::module("demo", "app");
    let s_struct = EntityId::item("struct", "demo", "app::Thing");
    let s_enum = EntityId::item("enum", "demo", "app::Color");
    let s_union = EntityId::item("union", "demo", "app::Bits");
    let s_trait = EntityId::item("trait", "demo", "app::Greet");
    let s_fn = EntityId::item("function", "demo", "app::run");
    let s_method = EntityId::item("method", "demo", "app::Thing::name");
    let s_impl = EntityId::impl_block("demo", "Greet", "Thing", "impl Greet for Thing");
    let s_alias = EntityId::item("type_alias", "demo", "app::Result");
    let s_const = EntityId::item("constant", "demo", "app::MAX");
    let s_static = EntityId::item("static", "demo", "app::GLOBAL");
    let s_macro = EntityId::item("macro", "demo", "app::make");
    let external = EntityId::manual("postgres");
    let infra = EntityId::manual("redis");
    let stage_entity = EntityId::manual("ingest");
    let manual_block = EntityId::manual("notes");

    let mut struct_entity = discovered(
        s_struct.clone(),
        EntityKind::STRUCT,
        "Thing",
        "demo::app::Thing",
    );
    struct_entity.parent = Some(module.clone());
    struct_entity.source = Some(SourceLocation::new(
        RepoRelativePath::new("src/app.rs").unwrap(),
        Some(Span {
            start_line: 10,
            start_col: 1,
            end_line: 20,
            end_col: 2,
        }),
    ));
    struct_entity.docs = Some(DocBlock {
        markdown: "A thing.".into(),
        summary: Some("A thing.".into()),
        documented: true,
    });
    struct_entity.tags = vec!["public".into()];
    struct_entity
        .attributes
        .insert("visibility".into(), serde_json::json!("pub"));

    let entities = vec![
        discovered(ws.clone(), EntityKind::WORKSPACE, "Demo", "demo"),
        discovered(pkg.clone(), EntityKind::PACKAGE, "demo", "demo"),
        discovered(ext_pkg.clone(), EntityKind::PACKAGE, "serde", "serde"),
        discovered(target.clone(), EntityKind::TARGET, "demo (lib)", "demo"),
        discovered(module.clone(), EntityKind::MODULE, "app", "demo::app"),
        struct_entity,
        discovered(
            s_enum.clone(),
            EntityKind::ENUM,
            "Color",
            "demo::app::Color",
        ),
        discovered(
            s_union.clone(),
            EntityKind::UNION,
            "Bits",
            "demo::app::Bits",
        ),
        discovered(
            s_trait.clone(),
            EntityKind::TRAIT,
            "Greet",
            "demo::app::Greet",
        ),
        discovered(s_fn.clone(), EntityKind::FUNCTION, "run", "demo::app::run"),
        discovered(
            s_method.clone(),
            EntityKind::METHOD,
            "name",
            "demo::app::Thing::name",
        ),
        discovered(
            s_impl.clone(),
            EntityKind::IMPL,
            "impl Greet for Thing",
            "demo::app",
        ),
        discovered(
            s_alias.clone(),
            EntityKind::TYPE_ALIAS,
            "Result",
            "demo::app::Result",
        ),
        discovered(
            s_const.clone(),
            EntityKind::CONSTANT,
            "MAX",
            "demo::app::MAX",
        ),
        discovered(
            s_static.clone(),
            EntityKind::STATIC,
            "GLOBAL",
            "demo::app::GLOBAL",
        ),
        discovered(
            s_macro.clone(),
            EntityKind::MACRO,
            "make",
            "demo::app::make",
        ),
        manual(
            external.clone(),
            EntityKind::EXTERNAL_SYSTEM,
            "PostgreSQL",
            "postgres",
        ),
        manual(infra.clone(), EntityKind::INFRASTRUCTURE, "Redis", "redis"),
        manual(stage_entity.clone(), EntityKind::STAGE, "Ingest", "ingest"),
        manual(
            manual_block.clone(),
            EntityKind::MANUAL_BLOCK,
            "Notes",
            "notes",
        ),
    ];

    let relations = vec![
        rel(RelationKind::CONTAINS, &ws, &pkg),
        rel(RelationKind::DEPENDS_ON, &pkg, &ext_pkg),
        rel(RelationKind::IMPORTS, &module, &s_struct),
        rel(RelationKind::RE_EXPORTS, &module, &s_enum),
        rel(RelationKind::IMPLEMENTS, &s_impl, &s_trait),
        rel(RelationKind::IMPLEMENTED_FOR, &s_impl, &s_struct),
        rel(RelationKind::HAS_FIELD_TYPE, &s_struct, &s_enum),
        rel(RelationKind::ACCEPTS_TYPE, &s_fn, &s_struct),
        rel(RelationKind::RETURNS_TYPE, &s_fn, &s_enum),
        rel(RelationKind::ERROR_TYPE, &s_fn, &s_union),
        rel(RelationKind::REFERENCES_TYPE, &s_method, &s_trait),
        rel(RelationKind::MANUAL, &external, &infra),
    ];

    let views = vec![
        View {
            id: ViewId::view("overview"),
            title: LocalizedText::new("Workspace overview"),
            description: Some(LocalizedText::new("Top-level structure.")),
            entity_kinds: vec![],
            relation_kinds: vec![],
            entity_ids: None,
            stages: vec![
                Stage {
                    id: cratevista_schema::StageId::from_raw("stage:ingest"),
                    title: LocalizedText::new("Ingest"),
                    order: 1,
                },
                Stage {
                    id: cratevista_schema::StageId::from_raw("stage:store"),
                    title: LocalizedText::new("Store"),
                    order: 2,
                },
            ],
            default_focus: Some(ws.clone()),
            presentation: BTreeMap::new(),
            docs: None,
            examples: Vec::new(),
        },
        View {
            id: ViewId::view("types"),
            title: LocalizedText::new("Types"),
            description: None,
            entity_kinds: vec![
                EntityKind::new(EntityKind::STRUCT),
                EntityKind::new(EntityKind::ENUM),
                EntityKind::new(EntityKind::UNION),
            ],
            relation_kinds: vec![RelationKind::new(RelationKind::HAS_FIELD_TYPE)],
            entity_ids: Some(vec![s_struct.clone(), s_enum.clone(), s_union.clone()]),
            stages: vec![],
            default_focus: Some(s_struct.clone()),
            presentation: BTreeMap::new(),
            docs: None,
            examples: Vec::new(),
        },
    ];

    let doc = ExplorerDocument::new(project(), entities, relations, views);
    doc.validate().expect("full_mvp doc is valid");
    doc
}

fn manual_flow() -> ExplorerDocument {
    let client = EntityId::manual("web-client");
    let gateway = EntityId::manual("api-gateway");
    let pkg = EntityId::package("demo");
    let db = EntityId::manual("postgres");

    let entities = vec![
        manual(
            client.clone(),
            EntityKind::EXTERNAL_SYSTEM,
            "Web Client",
            "web-client",
        ),
        manual(
            gateway.clone(),
            EntityKind::INFRASTRUCTURE,
            "API Gateway",
            "api-gateway",
        ),
        discovered(pkg.clone(), EntityKind::PACKAGE, "demo", "demo"),
        manual(
            db.clone(),
            EntityKind::EXTERNAL_SYSTEM,
            "PostgreSQL",
            "postgres",
        ),
    ];

    let mut client_to_gateway = Relation::new(
        RelationKind::new(RelationKind::MANUAL),
        client.clone(),
        gateway.clone(),
        cratevista_schema::Provenance::Manual,
    );
    client_to_gateway.role = Some("http".into());
    client_to_gateway.label = Some(LocalizedText::new("HTTP + WS"));

    let relations = vec![
        client_to_gateway,
        Relation::new(
            RelationKind::new(RelationKind::MANUAL),
            gateway.clone(),
            pkg.clone(),
            cratevista_schema::Provenance::Manual,
        ),
        Relation::new(
            RelationKind::new(RelationKind::MANUAL),
            pkg.clone(),
            db.clone(),
            cratevista_schema::Provenance::Manual,
        ),
    ];

    let views = vec![View {
        id: ViewId::view("clients-to-infra"),
        title: LocalizedText::new("Clients → Gateway → Services → Infrastructure"),
        description: Some(LocalizedText::new("A manually curated runtime flow.")),
        entity_kinds: vec![],
        relation_kinds: vec![],
        entity_ids: Some(vec![
            client.clone(),
            gateway.clone(),
            pkg.clone(),
            db.clone(),
        ]),
        stages: vec![
            Stage {
                id: cratevista_schema::StageId::from_raw("stage:client"),
                title: LocalizedText::new("Client"),
                order: 1,
            },
            Stage {
                id: cratevista_schema::StageId::from_raw("stage:gateway"),
                title: LocalizedText::new("Gateway"),
                order: 2,
            },
        ],
        default_focus: Some(client),
        presentation: BTreeMap::new(),
        docs: None,
        examples: Vec::new(),
    }];

    let doc = ExplorerDocument::new(project(), entities, relations, views);
    doc.validate().expect("manual_flow doc is valid");
    doc
}

fn unknown_kind() -> ExplorerDocument {
    let a = EntityId::from_raw("item:widget:demo::app::Widget");
    let b = EntityId::package("demo");

    let entities = vec![
        Entity::new(
            a.clone(),
            EntityKind::new("widget"), // unknown kind
            LocalizedText::new("Widget"),
            "demo::app::Widget",
            cratevista_schema::Provenance::Discovered,
        ),
        discovered(b.clone(), EntityKind::PACKAGE, "demo", "demo"),
    ];
    // Unknown relation kind between existing entities.
    let relations = vec![rel("talks_to", &b, &a)];
    let doc = ExplorerDocument::new(project(), entities, relations, vec![]);
    doc.validate()
        .expect("unknown_kind doc is valid (unknown kinds are not errors)");
    doc
}

fn generation() -> GenerationReport {
    let mut input_hashes = BTreeMap::new();
    input_hashes.insert("Cargo.lock".into(), "abc123".into());
    let mut durations = BTreeMap::new();
    durations.insert("metadata".into(), 12u64);
    durations.insert("rustdoc".into(), 3400u64);
    GenerationReport {
        generator: Generator {
            name: "cargo-cratevista".into(),
            version: "0.1.0".into(),
        },
        generated_at: Timestamp::new("2026-07-12T00:00:00Z"),
        toolchain: Some("nightly-2026-01-01".into()),
        rustdoc_format_version: Some(45),
        input_hashes,
        counts: Counts {
            entities: 20,
            relations: 12,
            views: 2,
            diagnostics: 1,
        },
        durations_ms: durations,
        artifact_hashes: Some(ArtifactHashes {
            document_blake3: "1111111111111111111111111111111111111111111111111111111111111111"
                .into(),
            diagnostics_blake3: "2222222222222222222222222222222222222222222222222222222222222222"
                .into(),
        }),
        partial: false,
    }
}

fn diagnostics() -> DiagnosticsReport {
    let mut d = DocumentDiagnostic::new(
        Severity::Warning,
        "unresolved_type",
        "could not resolve type `Foo` in a function signature",
    );
    d.entities = vec![EntityId::item("function", "demo", "app::run")];
    DiagnosticsReport::new(vec![d])
}

fn main() {
    write(
        "minimal.document.json",
        &to_canonical_string(&minimal()).unwrap(),
    );
    write(
        "full_mvp.document.json",
        &to_canonical_string(&full_mvp()).unwrap(),
    );
    write(
        "manual_flow.document.json",
        &to_canonical_string(&manual_flow()).unwrap(),
    );
    write(
        "unknown_kind.document.json",
        &to_canonical_string(&unknown_kind()).unwrap(),
    );
    write(
        "full_mvp.generation.json",
        &to_canonical_string(&generation()).unwrap(),
    );
    write(
        "full_mvp.diagnostics.json",
        &to_canonical_string(&diagnostics()).unwrap(),
    );
}
