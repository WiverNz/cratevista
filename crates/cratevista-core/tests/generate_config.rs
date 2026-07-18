//! Integration tests for project-local configuration in the **real** generate
//! pipeline (PRD-08 step 6).
//!
//! Like `generate.rs`, these need **no nightly and no network**: a bin-only
//! workspace yields an empty default `RustdocPlan`, so the run is a metadata-only
//! success driven by stable `cargo metadata`/`cargo locate-project` alone. That
//! keeps the whole config path — discovery, loading, validation, the overlay,
//! file embedding, diagnostics and the committed artifacts — under test on every
//! platform.

use std::path::Path;

use cratevista_core::clock::FixedClock;
use cratevista_core::exit::ExitCode;
use cratevista_core::generate::{GenerateOptions, run_generate};
use cratevista_schema::{DiagnosticsReport, ExplorerDocument, GenerationReport, Severity};

/// A minimal bin-only crate: no dependencies, so no network.
fn write_bin_crate(dir: &Path, name: &str) {
    std::fs::write(
        dir.join("Cargo.toml"),
        format!("[package]\nname = \"{name}\"\nversion = \"0.0.0\"\nedition = \"2021\"\n"),
    )
    .unwrap();
    std::fs::create_dir_all(dir.join("src")).unwrap();
    std::fs::write(dir.join("src").join("main.rs"), "fn main() {}\n").unwrap();
}

fn write(dir: &Path, relative: &str, contents: &str) {
    let path = dir.join(relative);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, contents).unwrap();
}

fn artifacts(dir: &Path) -> (ExplorerDocument, DiagnosticsReport, GenerationReport) {
    let read = |name: &str| {
        std::fs::read_to_string(dir.join("target/cratevista").join(name))
            .unwrap_or_else(|e| panic!("read {name}: {e}"))
    };
    (
        serde_json::from_str(&read("document.json")).unwrap(),
        serde_json::from_str(&read("diagnostics.json")).unwrap(),
        serde_json::from_str(&read("generation.json")).unwrap(),
    )
}

fn generate(dir: &Path, no_config: bool) -> ExitCode {
    let options = GenerateOptions {
        manifest_path: Some(dir.join("Cargo.toml")),
        no_config,
        ..Default::default()
    };
    run_generate(&options, &FixedClock("2026-07-16T00:00:00Z".into()))
        .expect("generate must succeed even with broken configuration")
}

/// A complete, valid configuration: a manual flow with docs, an example, a
/// manual entity, and an override of a DISCOVERED package.
fn write_valid_config(dir: &Path) {
    write(
        dir,
        ".cratevista/flows/architecture.toml",
        r#"
[[entity]]
id = "postgres"
kind = "infrastructure"
label = "PostgreSQL"

[[flow]]
id = "runtime"
title = "Runtime"
description = "How the binary talks to storage."
members = ["manual:postgres", "package:cfgdemo"]
default_focus = "manual:postgres"
docs = [".cratevista/docs/runtime.md"]

  [[flow.stage]]
  id = "app"
  title = "App"
  order = 1

  [[flow.relation]]
  from = "package:cfgdemo"
  to = "manual:postgres"
  role = "sql"
  label = "SQL"

  [[flow.example]]
  id = "query"
  title = "Example query"
  path = ".cratevista/examples/query.sql"
  language = "sql"
"#,
    );
    write(
        dir,
        ".cratevista/overrides/presentation.toml",
        "[[override]]\ntarget = \"package:cfgdemo\"\nlabel = \"The demo binary\"\ntags = [\"core\"]\ncategory = \"service\"\n",
    );
    write(
        dir,
        ".cratevista/docs/runtime.md",
        "# Runtime\n\nThe binary writes to PostgreSQL.\n",
    );
    write(dir, ".cratevista/examples/query.sql", "SELECT 1;\n");
}

// ---------------------------------------------------------------------------

#[test]
fn valid_config_adds_the_manual_flow_docs_examples_and_overrides() {
    let dir = tempfile::tempdir().unwrap();
    write_bin_crate(dir.path(), "cfgdemo");
    write_valid_config(dir.path());

    assert_eq!(generate(dir.path(), false), ExitCode::SUCCESS);
    let (document, diagnostics, generation) = artifacts(dir.path());

    // The manual entity is in the document, alongside the discovered package.
    let postgres = document
        .entities
        .iter()
        .find(|e| e.id.as_str() == "manual:postgres")
        .expect("the manual entity reached the document");
    assert_eq!(postgres.provenance, cratevista_schema::Provenance::Manual);
    assert!(
        document
            .entities
            .iter()
            .any(|e| e.id.as_str() == "package:cfgdemo"),
        "the discovered package is still there"
    );

    // The flow is a view, with its docs and example embedded.
    let flow = document
        .views
        .iter()
        .find(|v| v.id.as_str() == "view:runtime")
        .expect("the manual flow became a view");
    assert_eq!(flow.title.default, "Runtime");
    assert!(
        flow.docs
            .as_ref()
            .unwrap()
            .markdown
            .contains("writes to PostgreSQL")
    );
    assert_eq!(flow.examples.len(), 1);
    assert_eq!(flow.examples[0].language.as_deref(), Some("sql"));
    assert_eq!(flow.examples[0].content, "SELECT 1;\n");
    assert_eq!(flow.stages.len(), 1);
    assert_eq!(
        flow.default_focus.as_ref().unwrap().as_str(),
        "manual:postgres"
    );
    // The eight generated views are not replaced.
    assert!(document.views.len() > 1);

    // The override enriched the discovered package without changing identity.
    let package = document
        .entities
        .iter()
        .find(|e| e.id.as_str() == "package:cfgdemo")
        .unwrap();
    assert_eq!(package.label.default, "The demo binary");
    assert!(package.tags.contains(&"core".to_string()));
    assert_eq!(package.attributes["category"], "service");
    assert_eq!(package.kind.as_str(), "package");
    assert_eq!(
        package.provenance,
        cratevista_schema::Provenance::Discovered
    );

    // A valid configuration adds no diagnostics of its own.
    assert!(
        !diagnostics
            .diagnostics
            .iter()
            .any(|d| d.code.starts_with("config_")),
        "a valid config is silent: {:?}",
        diagnostics.diagnostics
    );
    // Config never makes a run partial.
    assert!(!generation.partial);
}

#[test]
fn generated_artifacts_pass_schema_hash_and_server_snapshot_loading() {
    let dir = tempfile::tempdir().unwrap();
    write_bin_crate(dir.path(), "cfgdemo");
    write_valid_config(dir.path());
    assert_eq!(generate(dir.path(), false), ExitCode::SUCCESS);

    let (document, _, generation) = artifacts(dir.path());
    document.validate().expect("schema-valid");

    // The embedded hashes cover the exact committed bytes.
    let hashes = generation
        .artifact_hashes
        .as_ref()
        .expect("the writer embeds hashes");
    let digest = |name: &str| {
        let bytes = std::fs::read(dir.path().join("target/cratevista").join(name)).unwrap();
        blake3::hash(&bytes).to_hex().to_string()
    };
    assert_eq!(hashes.document_blake3, digest("document.json"));
    assert_eq!(hashes.diagnostics_blake3, digest("diagnostics.json"));

    // And the REAL server loader accepts the set — the strongest check, since it
    // re-verifies hashes, versions and referential integrity together.
    let snapshot = cratevista_server::load_snapshot(
        &cratevista_server::ArtifactPaths::in_dir(&dir.path().join("target/cratevista")),
        &cratevista_server::SnapshotLoadOptions::default(),
    )
    .expect("the server must load a config-generated snapshot");
    assert!(
        snapshot
            .document
            .views
            .iter()
            .any(|v| v.id.as_str() == "view:runtime"),
        "the manual flow survives into what the server serves"
    );
}

#[test]
fn malformed_config_still_produces_discovered_output_plus_diagnostics() {
    let dir = tempfile::tempdir().unwrap();
    write_bin_crate(dir.path(), "cfgdemo");
    // Unparseable, plus a healthy file alongside it.
    write(
        dir.path(),
        ".cratevista/flows/a_broken.toml",
        "this is not = = toml\n",
    );
    write(
        dir.path(),
        ".cratevista/flows/b_ok.toml",
        "[[entity]]\nid = \"redis\"\nkind = \"infrastructure\"\nlabel = \"Redis\"\n\n[[flow]]\nid = \"ok\"\ntitle = \"Ok\"\nmembers = [\"manual:redis\"]\n",
    );

    // Recoverable: exit 0, artifacts committed.
    assert_eq!(generate(dir.path(), false), ExitCode::SUCCESS);
    let (document, diagnostics, generation) = artifacts(dir.path());
    document.validate().expect("still schema-valid");

    // The discovered output is intact…
    assert!(
        document
            .entities
            .iter()
            .any(|e| e.id.as_str() == "package:cfgdemo")
    );
    // …the healthy config file still contributed…
    assert!(
        document
            .entities
            .iter()
            .any(|e| e.id.as_str() == "manual:redis")
    );
    assert!(document.views.iter().any(|v| v.id.as_str() == "view:ok"));

    // …and the parse error is reported, located, as a WARNING.
    let parse = diagnostics
        .diagnostics
        .iter()
        .find(|d| d.code == "config_parse_error")
        .expect("the parse error reached diagnostics.json");
    assert_eq!(
        parse.severity,
        Severity::Warning,
        "config never fails the run"
    );
    assert!(
        parse
            .message
            .starts_with(".cratevista/flows/a_broken.toml:"),
        "the location is preserved in the message: {}",
        parse.message
    );

    // A config error must not make the run partial — that flag is rustdoc's.
    assert!(!generation.partial);
    // The count includes the config diagnostics.
    assert_eq!(
        generation.counts.diagnostics,
        diagnostics.diagnostics.len() as u64
    );
    assert!(generation.counts.diagnostics > 0);
}

#[test]
fn no_config_ignores_even_deliberately_malformed_configuration() {
    let dir = tempfile::tempdir().unwrap();
    write_bin_crate(dir.path(), "cfgdemo");
    write_valid_config(dir.path());
    // Something that would certainly be diagnosed if it were read.
    write(
        dir.path(),
        ".cratevista/flows/z_broken.toml",
        "= = = not toml\n",
    );

    assert_eq!(generate(dir.path(), true), ExitCode::SUCCESS);
    let (document, diagnostics, _) = artifacts(dir.path());

    // Not one config diagnostic — the broken file was never opened.
    assert!(
        !diagnostics
            .diagnostics
            .iter()
            .any(|d| d.code.starts_with("config_")),
        "--no-config must not even look: {:?}",
        diagnostics.diagnostics
    );
    // And no manual content.
    assert!(
        !document
            .entities
            .iter()
            .any(|e| e.id.as_str().starts_with("manual:"))
    );
    assert!(
        !document
            .views
            .iter()
            .any(|v| v.id.as_str() == "view:runtime")
    );
    // The override was not applied either.
    let package = document
        .entities
        .iter()
        .find(|e| e.id.as_str() == "package:cfgdemo")
        .unwrap();
    assert_ne!(package.label.default, "The demo binary");
}

/// `--no-config` must be byte-identical to having no configuration at all.
///
/// Both runs happen in the **same** directory: the workspace entity's label is
/// derived from the directory name, so comparing across two tempdirs would
/// differ for a reason that has nothing to do with configuration.
#[test]
fn no_config_equals_absent_config_byte_for_byte() {
    let dir = tempfile::tempdir().unwrap();
    write_bin_crate(dir.path(), "cfgdemo");
    write_valid_config(dir.path());
    write(
        dir.path(),
        ".cratevista/flows/z_broken.toml",
        "= = = not toml\n",
    );

    // Config present, but disabled.
    assert_eq!(generate(dir.path(), true), ExitCode::SUCCESS);
    let disabled =
        std::fs::read_to_string(dir.path().join("target/cratevista/document.json")).unwrap();
    let disabled_diagnostics =
        std::fs::read_to_string(dir.path().join("target/cratevista/diagnostics.json")).unwrap();

    // Now genuinely remove the configuration and generate normally.
    std::fs::remove_dir_all(dir.path().join(".cratevista")).unwrap();
    assert_eq!(generate(dir.path(), false), ExitCode::SUCCESS);
    let absent =
        std::fs::read_to_string(dir.path().join("target/cratevista/document.json")).unwrap();
    let absent_diagnostics =
        std::fs::read_to_string(dir.path().join("target/cratevista/diagnostics.json")).unwrap();

    assert_eq!(
        disabled, absent,
        "--no-config must produce exactly the document an unconfigured project does"
    );
    assert_eq!(disabled_diagnostics, absent_diagnostics);
}

#[test]
fn absent_config_behaves_like_an_empty_overlay() {
    let dir = tempfile::tempdir().unwrap();
    write_bin_crate(dir.path(), "cfgdemo");
    // No `.cratevista/` and no `cratevista.toml` at all.
    assert_eq!(generate(dir.path(), false), ExitCode::SUCCESS);

    let (document, diagnostics, _) = artifacts(dir.path());
    document.validate().unwrap();
    assert!(
        !diagnostics
            .diagnostics
            .iter()
            .any(|d| d.code.starts_with("config_"))
    );
    assert!(
        !document
            .entities
            .iter()
            .any(|e| e.id.as_str().starts_with("manual:"))
    );
    // Only the generated views.
    assert!(
        document
            .views
            .iter()
            .all(|v| v.id.as_str() != "view:runtime")
    );
}

/// The PRD-05 boundary, end to end through the real CLI path.
#[test]
fn unknown_discovered_references_are_diagnosed_only_by_prd_05() {
    let dir = tempfile::tempdir().unwrap();
    write_bin_crate(dir.path(), "cfgdemo");
    write(
        dir.path(),
        ".cratevista/flows/a.toml",
        r#"
[[flow]]
id = "stale"
title = "Stale"
members = ["item:struct:nope::Gone"]

  [[flow.relation]]
  from = "package:cfgdemo"
  to = "item:struct:also::Missing"
  role = "x"
"#,
    );
    write(
        dir.path(),
        ".cratevista/overrides/o.toml",
        "[[override]]\ntarget = \"item:struct:totally::made::Up\"\nlabel = \"never\"\n",
    );

    assert_eq!(generate(dir.path(), false), ExitCode::SUCCESS);
    let (document, diagnostics, _) = artifacts(dir.path());
    document
        .validate()
        .expect("broken references degrade, never crash");

    let codes: Vec<&str> = diagnostics
        .diagnostics
        .iter()
        .map(|d| d.code.as_str())
        .collect();
    // PRD 05 owns these…
    assert!(codes.contains(&"invalid_view_reference"), "{codes:?}");
    assert!(codes.contains(&"overlay_target_missing"), "{codes:?}");
    assert!(codes.contains(&"dangling_relation"), "{codes:?}");
    // …and config said nothing about a discovered id.
    for diagnostic in &diagnostics.diagnostics {
        if diagnostic.code.starts_with("config_") {
            assert!(
                !diagnostic.message.contains("item:struct:"),
                "config must not judge discovered ids: {}",
                diagnostic.message
            );
        }
    }
}

/// Same inputs, same bytes — repeated in **one** directory, since the workspace
/// name is itself an input.
#[test]
fn document_and_diagnostics_are_deterministic_under_a_fixed_clock() {
    let dir = tempfile::tempdir().unwrap();
    write_bin_crate(dir.path(), "cfgdemo");
    write_valid_config(dir.path());
    // Content that produces diagnostics from several stages at once, so a
    // non-deterministic merge or sort would surface here.
    write(
        dir.path(),
        ".cratevista/flows/z_more.toml",
        "[[flow]]\nid = \"z\"\ntitle = \"Z\"\nmembers = [\"manual:ghost\"]\ndocs = [\".cratevista/docs/gone.md\"]\n",
    );
    write(dir.path(), ".cratevista/flows/y_broken.toml", "= = bad\n");

    let render = || {
        assert_eq!(generate(dir.path(), false), ExitCode::SUCCESS);
        (
            std::fs::read_to_string(dir.path().join("target/cratevista/document.json")).unwrap(),
            std::fs::read_to_string(dir.path().join("target/cratevista/diagnostics.json")).unwrap(),
        )
    };
    let baseline = render();
    // A config with several diagnostics must still sort identically every time.
    assert!(
        baseline.1.contains("config_"),
        "the run produced config diagnostics"
    );
    for _ in 0..3 {
        assert_eq!(
            render(),
            baseline,
            "identical inputs must produce identical bytes"
        );
    }
}

#[test]
fn no_artifact_or_diagnostic_contains_an_absolute_path() {
    let dir = tempfile::tempdir().unwrap();
    write_bin_crate(dir.path(), "cfgdemo");
    write_valid_config(dir.path());
    // Provoke a config diagnostic too, so the check covers a failure message.
    write(dir.path(), ".cratevista/flows/z_broken.toml", "= = bad\n");
    assert_eq!(generate(dir.path(), false), ExitCode::SUCCESS);

    let root = dir.path().to_string_lossy().to_string();
    // Both spellings: JSON escapes Windows separators.
    let escaped = root.replace('\\', "\\\\");
    let forward = root.replace('\\', "/");

    for name in ["document.json", "diagnostics.json", "generation.json"] {
        let text =
            std::fs::read_to_string(dir.path().join("target/cratevista").join(name)).unwrap();
        for needle in [&root, &escaped, &forward] {
            assert!(
                !text.contains(needle.as_str()),
                "{name} leaked an absolute path"
            );
        }
        // Nor a drive letter / unix root, generally.
        assert!(!text.contains("/home/"), "{name} leaked a unix home");
        assert!(!text.contains("/Users/"), "{name} leaked a unix home");
    }
}
