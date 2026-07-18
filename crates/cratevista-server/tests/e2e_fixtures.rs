//! Validates the committed Playwright E2E snapshots against the **real** server
//! loader.
//!
//! The E2E suite serves `web/e2e/fixtures/{normal,partial}` — and the benchmark
//! serves `bench-{near,at,large}` — through the actual
//! `cargo cratevista serve` binary. If a snapshot ever drifts out of integrity —
//! a regenerated `document.json` committed without its matching
//! `generation.json` hashes, say — the browser tests would fail with a confusing
//! startup error. These tests catch that here, in `cargo test`, with a precise
//! message. They need no nightly toolchain: the snapshots are committed.

use std::path::{Path, PathBuf};

use cratevista_server::{ArtifactPaths, SnapshotLoadOptions, load_snapshot};

/// `web/e2e/fixtures/<name>`, resolved from this crate's manifest directory.
fn fixture_dir(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../web/e2e/fixtures")
        .join(name)
}

/// The exact committed bytes of a fixture artifact, as text.
fn read_fixture(name: &str, artifact: &str) -> String {
    let path = fixture_dir(name).join(artifact);
    std::fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("cannot read {}: {error}", path.display()))
}

fn load(name: &str) -> cratevista_server::ArtifactSnapshot {
    let dir = fixture_dir(name);
    assert!(
        dir.is_dir(),
        "missing E2E fixture directory: {}",
        dir.display()
    );
    load_snapshot(
        &ArtifactPaths::in_dir(&dir),
        &SnapshotLoadOptions::default(),
    )
    .unwrap_or_else(|error| panic!("E2E snapshot `{name}` must load: {}", error.code()))
}

#[test]
fn normal_e2e_snapshot_loads_and_is_complete() {
    let snapshot = load("normal");
    assert!(
        !snapshot.generation.partial,
        "the `normal` snapshot must not be partial"
    );
    // The E2E smoke suite asserts against these; keep them non-trivial.
    assert!(
        !snapshot.document.entities.is_empty(),
        "the `normal` snapshot must have entities"
    );
    assert_eq!(
        snapshot.document.views.len(),
        8,
        "the `normal` snapshot must carry all eight generated views"
    );
}

#[test]
fn partial_e2e_snapshot_loads_and_is_marked_partial() {
    let snapshot = load("partial");
    assert!(
        snapshot.generation.partial,
        "the `partial` snapshot must be marked partial — it drives the partial banner"
    );
    // A partial document must still be usable: the graph and inspector render.
    assert!(
        !snapshot.document.entities.is_empty(),
        "the `partial` snapshot must still have a usable graph"
    );
}

/// PRD-08 Amendment A back-compatibility, proven on **real committed artifacts**
/// rather than synthetic ones.
///
/// Every committed snapshot was generated before the additive `SchemaVersion`
/// 1.0 → 1.1 bump, so they are all `1.0`. They must keep loading through the
/// real loader unchanged — that is what makes the bump additive rather than
/// breaking. This test deliberately asserts the OLD version: if a future fixture
/// refresh regenerates them at the current version, update this test
/// consciously, and keep some 1.0 coverage (see `snapshot.rs`'s
/// `older_matching_minor_version_snapshot_still_loads`).
#[test]
fn committed_1_0_snapshots_still_load_after_the_1_1_bump() {
    assert_eq!(
        cratevista_schema::SchemaVersion::CURRENT,
        "1.1",
        "this test exists because CURRENT moved past the fixtures' version"
    );
    // `flow` is deliberately excluded: it is a 1.1 fixture, generated after the
    // bump to exercise the schema-1.1 view docs/examples (PRD-08 Amendment C).
    for name in ALL_FIXTURES.iter().filter(|name| **name != "flow") {
        let snapshot = load(name);
        let document_version = snapshot.document.schema_version.as_str().to_string();
        assert_eq!(
            document_version, "1.0",
            "fixture `{name}` is expected to still be a pre-bump 1.0 artifact"
        );
        // The loader requires document and diagnostics to agree exactly; an
        // older snapshot is self-consistent, which is why it still loads.
        assert_eq!(
            snapshot.diagnostics.schema_version.as_str(),
            document_version,
            "fixture `{name}` must be self-consistent"
        );
    }
}

/// PRD-08 Amendment C: the `flow` fixture is the only committed artifact that
/// exercises the schema-1.1 `View::docs` / `View::examples` fields, and the
/// browser test (`web/e2e/tests/view-docs.spec.ts`) renders it.
///
/// It is synthesized by `cargo run -p cratevista-core --example gen_flow_fixture`
/// rather than generated from a Rust workspace, because its producer
/// (`cratevista-config`, PRD 08) does not exist yet — but it is committed through
/// the production writer, so it is schema-valid with correct hashes like every
/// other snapshot.
#[test]
fn flow_fixture_carries_schema_1_1_view_docs_and_examples() {
    let snapshot = load("flow");
    assert_eq!(snapshot.document.schema_version.as_str(), "1.1");

    let view = snapshot
        .document
        .views
        .iter()
        .find(|view| view.id.as_str() == "view:flow-checkout")
        .expect("the flow view is present");

    assert!(view.description.is_some(), "the flow states what it shows");
    let docs = view.docs.as_ref().expect("the flow carries documentation");
    assert!(docs.markdown.contains("Clients → Gateway → Services"));

    assert_eq!(view.examples.len(), 2, "request + response examples");
    let request = &view.examples[0];
    assert_eq!(request.id, "request");
    assert_eq!(request.language.as_deref(), Some("http"));
    // Embedded verbatim: the explorer renders this without /api/source.
    assert!(request.content.contains("POST /checkout"));
    assert!(!request.content.is_empty());
    // Explicit membership + a manual focus, as a real flow would have.
    assert!(view.entity_ids.as_ref().is_some_and(|ids| ids.len() == 4));
    assert!(view.default_focus.is_some());
}

/// The partial fixture is generated and then deterministically path-normalized,
/// so its genuine content must survive normalization.
#[test]
fn partial_fixture_keeps_its_real_target_failed_diagnostic() {
    let text = read_fixture("partial", "diagnostics.json");
    assert!(
        text.contains("target_failed"),
        "the `partial` fixture must keep the real rustdoc failure diagnostic"
    );
    assert!(
        text.contains("cvbroken"),
        "the diagnostic must still name the crate that failed to document"
    );
    // The normalized stand-in replaces only the machine-specific path prefix.
    assert!(
        text.contains("<fixture-workspace>"),
        "the fixture-workspace path must be normalized to the stable token"
    );
}

/// Every committed snapshot, including the benchmark fixtures.
const ALL_FIXTURES: [&str; 6] = [
    "normal",
    "partial",
    "flow",
    "bench-near",
    "bench-at",
    "bench-large",
];

/// The large-graph benchmark fixtures must load through the real loader too —
/// the benchmark serves them with the real binary, and a hash mismatch there
/// would surface as an opaque startup failure.
#[test]
fn benchmark_fixtures_load_and_are_above_or_near_the_budget() {
    // `traits-and-impls` is the widest projection these documents produce; the
    // budget applies to projected (visible) entities, not the document total.
    for (name, minimum_entities) in [
        ("bench-near", 1600),
        ("bench-at", 2100),
        ("bench-large", 4400),
    ] {
        let snapshot = load(name);
        assert!(
            !snapshot.generation.partial,
            "benchmark fixture `{name}` must be a complete generation"
        );
        assert!(
            snapshot.document.entities.len() >= minimum_entities,
            "benchmark fixture `{name}` has {} entities, expected >= {minimum_entities}",
            snapshot.document.entities.len()
        );
        assert_eq!(
            snapshot.document.views.len(),
            8,
            "benchmark fixture `{name}` must carry all eight views"
        );
    }
}

/// A realistic Rust shape — not isolated synthetic nodes.
#[test]
fn benchmark_large_fixture_has_a_realistic_rust_shape() {
    let snapshot = load("bench-large");
    let kinds: std::collections::BTreeMap<&str, usize> =
        snapshot
            .document
            .entities
            .iter()
            .fold(Default::default(), |mut acc, entity| {
                *acc.entry(entity.kind.as_str()).or_default() += 1;
                acc
            });
    for required in [
        "workspace",
        "package",
        "target",
        "module",
        "struct",
        "enum",
        "trait",
        "impl",
        "method",
        "function",
        "field",
        "variant",
    ] {
        assert!(
            kinds.get(required).copied().unwrap_or(0) > 0,
            "the benchmark fixture must contain `{required}` entities, got {kinds:?}"
        );
    }

    let relations: std::collections::BTreeSet<&str> = snapshot
        .document
        .relations
        .iter()
        .map(|relation| relation.kind.as_str())
        .collect();
    for required in [
        "contains",
        "depends_on",
        "implements",
        "implemented_for",
        "returns_type",
    ] {
        assert!(
            relations.contains(required),
            "the benchmark fixture must contain `{required}` relations, got {relations:?}"
        );
    }
}

/// No committed fixture may carry the filesystem layout of whoever refreshed it.
#[test]
fn fixtures_contain_no_developer_specific_absolute_paths() {
    for name in ALL_FIXTURES {
        for artifact in ["document.json", "diagnostics.json", "generation.json"] {
            let text = read_fixture(name, artifact);
            let where_ = format!("{name}/{artifact}");

            // A Windows drive-letter path, escaped (`D:\\...`) or plain (`D:/...`).
            for (index, _) in text.match_indices(":\\\\").chain(text.match_indices(":/")) {
                let start = index.saturating_sub(1);
                let drive = text[start..index].chars().next().unwrap_or(' ');
                assert!(
                    !drive.is_ascii_alphabetic(),
                    "{where_} contains a Windows drive path near byte {index}"
                );
            }
            // A Unix home or common absolute prefix.
            for needle in ["/home/", "/Users/", "/root/"] {
                assert!(
                    !text.contains(needle),
                    "{where_} contains an absolute Unix path (`{needle}`)"
                );
            }
        }
    }
}

/// The integrity contract: hashes are present, well-formed, and verified over
/// the exact stored bytes. `load_snapshot` enforces this, so a successful load
/// is the proof; this pins the shape so a fixture refresh cannot silently drop
/// `artifact_hashes` and fall back to a weaker check.
#[test]
fn both_snapshots_carry_wellformed_artifact_hashes() {
    for name in ALL_FIXTURES {
        let snapshot = load(name);
        let hashes = snapshot
            .generation
            .artifact_hashes
            .as_ref()
            .unwrap_or_else(|| panic!("snapshot `{name}` must embed artifact_hashes"));
        for (label, digest) in [
            ("document_blake3", &hashes.document_blake3),
            ("diagnostics_blake3", &hashes.diagnostics_blake3),
        ] {
            assert_eq!(digest.len(), 64, "{name}.{label} must be 64 hex chars");
            assert!(
                digest
                    .bytes()
                    .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase()),
                "{name}.{label} must be lowercase hex with no 0x prefix"
            );
        }
    }
}
