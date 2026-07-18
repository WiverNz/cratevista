//! The core WatchPlan builder, against real temporary workspaces.
//!
//! No cargo and no nightly: the builder's pure half takes an already-ingested
//! `MetadataIngest`, so these drive it with a synthetic one — the same technique
//! `cratevista-config`'s pipeline tests use. Real directories are still created,
//! because canonicalization and "nearest existing ancestor" are precisely the
//! parts that must touch a real filesystem.

use std::fs;
use std::path::{Path, PathBuf};

use cratevista_core::watch::{self, WatchSetupError};
use cratevista_metadata::{MetadataIngest, MetadataSummary};
use cratevista_schema::{
    Entity, EntityId, EntityKind, LocalizedText, Provenance, RepoRelativePath, SourceLocation,
};
use cratevista_watch::RegistrationMode;
use tempfile::TempDir;

fn root_of(dir: &TempDir) -> PathBuf {
    dir.path().canonicalize().expect("canonical root")
}

fn put(path: &Path, text: &str) {
    fs::create_dir_all(path.parent().expect("parent")).expect("mkdir");
    fs::write(path, text).expect("write");
}

fn entity(id: &str, kind: &str, source: Option<&str>, parent: Option<&str>) -> Entity {
    let mut entity = Entity::new(
        EntityId::from_raw(id),
        EntityKind::new(kind),
        LocalizedText::new(id),
        id,
        Provenance::Discovered,
    );
    if let Some(source) = source {
        entity.source = Some(SourceLocation {
            path: RepoRelativePath::new(source).expect("repo-relative"),
            span: None,
        });
    }
    entity.parent = parent.map(EntityId::from_raw);
    entity
}

fn ingest(entities: Vec<Entity>) -> MetadataIngest {
    MetadataIngest {
        entities,
        relations: Vec::new(),
        diagnostics: Vec::new(),
        summary: MetadataSummary {
            workspace_root_repo_relative: Some(".".into()),
            selection: Default::default(),
            external_deps_mode: Default::default(),
            workspace_package_count: 1,
            selected_package_count: 1,
            external_package_count: 0,
            target_count: 1,
            dependency_relation_count: 0,
            recoverable_diagnostic_count: 0,
            cargo_argv: vec!["cargo".into(), "metadata".into()],
        },
    }
}

fn relative(root: &Path, path: &Path) -> String {
    path.to_string_lossy()
        .replace('\\', "/")
        .strip_prefix(&root.to_string_lossy().replace('\\', "/"))
        .unwrap_or_default()
        .trim_start_matches('/')
        .to_string()
}

/// Registrations as `(relative path, mode)`.
fn registrations_of(
    root: &Path,
    plan: &cratevista_watch::WatchPlan,
) -> Vec<(String, &'static str)> {
    plan.registrations()
        .iter()
        .map(|registration| {
            (
                relative(root, &registration.path),
                match registration.mode {
                    RegistrationMode::Recursive => "recursive",
                    RegistrationMode::NonRecursive => "non-recursive",
                },
            )
        })
        .collect()
}

/// A minimal workspace: root manifest, one member, one lib target.
fn simple_workspace() -> (TempDir, PathBuf, MetadataIngest) {
    let dir = TempDir::new().expect("tempdir");
    let root = root_of(&dir);
    put(&root.join("Cargo.toml"), "[workspace]\n");
    put(&root.join("crates/demo/Cargo.toml"), "[package]\n");
    put(&root.join("crates/demo/src/lib.rs"), "pub fn demo() {}\n");
    let metadata = ingest(vec![
        entity("workspace", EntityKind::WORKSPACE, None, None),
        entity(
            "package:demo",
            EntityKind::PACKAGE,
            Some("crates/demo/Cargo.toml"),
            Some("workspace"),
        ),
        entity(
            "target:demo:lib:demo",
            EntityKind::TARGET,
            Some("crates/demo/src/lib.rs"),
            Some("package:demo"),
        ),
    ]);
    (dir, root, metadata)
}

fn plan(root: &Path, metadata: &MetadataIngest, config: bool) -> cratevista_watch::WatchPlan {
    watch::plan_for_test(root, metadata, config)
        .expect("a valid plan")
        .plan
}

// --- logical inputs -------------------------------------------------------

#[test]
fn the_root_manifest_and_a_missing_lockfile_are_both_inputs() {
    let (_dir, root, metadata) = simple_workspace();
    assert!(
        !root.join("Cargo.lock").exists(),
        "the fixture has no lockfile"
    );

    let plan = plan(&root, &metadata, false);
    let set = plan.watch_set();

    assert!(set.is_relevant(&root.join("Cargo.toml")));
    // The point: a lockfile that does not exist yet is still watched, so `cargo
    // build` creating it repairs the document without a restart.
    assert!(
        set.is_relevant(&root.join("Cargo.lock")),
        "a missing Cargo.lock must still be an input"
    );
}

#[test]
fn every_workspace_member_manifest_is_an_input() {
    let dir = TempDir::new().expect("tempdir");
    let root = root_of(&dir);
    put(&root.join("Cargo.toml"), "[workspace]\n");
    for name in ["alpha", "beta"] {
        put(
            &root.join(format!("crates/{name}/Cargo.toml")),
            "[package]\n",
        );
        put(
            &root.join(format!("crates/{name}/src/lib.rs")),
            "pub fn x() {}\n",
        );
    }
    let metadata = ingest(vec![
        entity(
            "package:alpha",
            EntityKind::PACKAGE,
            Some("crates/alpha/Cargo.toml"),
            Some("workspace"),
        ),
        entity(
            "package:beta",
            EntityKind::PACKAGE,
            Some("crates/beta/Cargo.toml"),
            Some("workspace"),
        ),
    ]);

    let plan = plan(&root, &metadata, false);
    let set = plan.watch_set();
    assert!(set.is_relevant(&root.join("crates/alpha/Cargo.toml")));
    assert!(set.is_relevant(&root.join("crates/beta/Cargo.toml")));
}

#[test]
fn rust_roots_come_from_target_src_path_parents_including_custom_and_nested_ones() {
    let dir = TempDir::new().expect("tempdir");
    let root = root_of(&dir);
    put(&root.join("Cargo.toml"), "[workspace]\n");
    put(&root.join("crates/demo/Cargo.toml"), "[package]\n");
    put(&root.join("crates/demo/src/lib.rs"), "");
    // A custom, nested target path — not `src/`.
    put(&root.join("crates/demo/custom/deep/entry.rs"), "");

    let metadata = ingest(vec![
        entity(
            "package:demo",
            EntityKind::PACKAGE,
            Some("crates/demo/Cargo.toml"),
            Some("workspace"),
        ),
        entity(
            "target:demo:lib:demo",
            EntityKind::TARGET,
            Some("crates/demo/src/lib.rs"),
            Some("package:demo"),
        ),
        entity(
            "target:demo:bin:tool",
            EntityKind::TARGET,
            Some("crates/demo/custom/deep/entry.rs"),
            Some("package:demo"),
        ),
    ]);

    let plan = plan(&root, &metadata, false);
    let set = plan.watch_set();
    // Both roots are watched, recursively.
    assert!(set.is_relevant(&root.join("crates/demo/src/nested/mod.rs")));
    assert!(set.is_relevant(&root.join("crates/demo/custom/deep/more/mod.rs")));
    // And only `.rs` under them.
    assert!(!set.is_relevant(&root.join("crates/demo/src/notes.md")));
}

#[test]
fn two_targets_sharing_a_root_produce_one_sorted_deduplicated_registration() {
    let dir = TempDir::new().expect("tempdir");
    let root = root_of(&dir);
    put(&root.join("Cargo.toml"), "[workspace]\n");
    put(&root.join("crates/demo/Cargo.toml"), "[package]\n");
    put(&root.join("crates/demo/src/lib.rs"), "");
    put(&root.join("crates/demo/src/main.rs"), "");

    let metadata = ingest(vec![
        entity(
            "package:demo",
            EntityKind::PACKAGE,
            Some("crates/demo/Cargo.toml"),
            Some("workspace"),
        ),
        entity(
            "target:demo:lib:demo",
            EntityKind::TARGET,
            Some("crates/demo/src/lib.rs"),
            Some("package:demo"),
        ),
        entity(
            "target:demo:bin:demo",
            EntityKind::TARGET,
            Some("crates/demo/src/main.rs"),
            Some("package:demo"),
        ),
    ]);

    let plan = plan(&root, &metadata, false);
    let registrations = registrations_of(&root, &plan);
    let src_roots = registrations
        .iter()
        .filter(|(path, mode)| path == "crates/demo/src" && *mode == "recursive")
        .count();
    assert_eq!(
        src_roots, 1,
        "one root, however many targets: {registrations:?}"
    );

    // Sorted.
    let paths: Vec<&String> = registrations.iter().map(|(path, _)| path).collect();
    let mut sorted = paths.clone();
    sorted.sort();
    assert_eq!(paths, sorted, "registrations must be sorted");
}

#[test]
fn external_dependency_sources_are_never_watched() {
    let (dir, root, mut metadata) = simple_workspace();
    // An external package, as metadata emits it: `@version` in the id.
    metadata.entities.push(entity(
        "package:serde@1.0.0",
        EntityKind::PACKAGE,
        Some("vendor/serde/Cargo.toml"),
        None,
    ));
    metadata.entities.push(entity(
        "target:serde@1.0.0:lib:serde",
        EntityKind::TARGET,
        Some("vendor/serde/src/lib.rs"),
        Some("package:serde@1.0.0"),
    ));
    put(&root.join("vendor/serde/Cargo.toml"), "[package]\n");
    put(&root.join("vendor/serde/src/lib.rs"), "");

    let plan = plan(&root, &metadata, false);
    let set = plan.watch_set();
    assert!(
        !set.is_relevant(&root.join("vendor/serde/Cargo.toml")),
        "an external manifest is not ours to watch"
    );
    assert!(
        !set.is_relevant(&root.join("vendor/serde/src/lib.rs")),
        "external sources are not ours to watch"
    );
    drop(dir);
}

// --- configuration --------------------------------------------------------

#[test]
fn config_inputs_are_present_when_config_is_enabled() {
    let (_dir, root, metadata) = simple_workspace();
    fs::create_dir_all(root.join(".cratevista/flows")).expect("flows");
    fs::create_dir_all(root.join(".cratevista/overrides")).expect("overrides");

    let plan = plan(&root, &metadata, false);
    let set = plan.watch_set();

    // Absent, and still an input.
    assert!(!root.join("cratevista.toml").exists());
    assert!(
        set.is_relevant(&root.join("cratevista.toml")),
        "an absent cratevista.toml must still be watched"
    );
    assert!(set.is_relevant(&root.join(".cratevista/flows/a.toml")));
    assert!(set.is_relevant(&root.join(".cratevista/overrides/o.toml")));
}

#[test]
fn a_direct_flow_toml_is_an_input_but_a_nested_one_is_not() {
    let (_dir, root, metadata) = simple_workspace();
    fs::create_dir_all(root.join(".cratevista/flows")).expect("flows");

    let plan = plan(&root, &metadata, false);
    let set = plan.watch_set();
    assert!(set.is_relevant(&root.join(".cratevista/flows/architecture.toml")));
    assert!(
        !set.is_relevant(&root.join(".cratevista/flows/nested/deep.toml")),
        "discovery is non-recursive, so a nested TOML is not an input"
    );
    assert!(!set.is_relevant(&root.join(".cratevista/flows/README.md")));
}

#[test]
fn no_config_removes_every_configuration_input() {
    let (_dir, root, metadata) = simple_workspace();
    fs::create_dir_all(root.join(".cratevista/flows")).expect("flows");
    put(&root.join("cratevista.toml"), "version = \"1\"\n");

    let plan = plan(&root, &metadata, true);
    let set = plan.watch_set();

    assert!(
        !set.is_relevant(&root.join("cratevista.toml")),
        "--no-config reads no configuration, so none of it is an input"
    );
    assert!(!set.is_relevant(&root.join(".cratevista/flows/a.toml")));
    assert!(!set.is_relevant(&root.join(".cratevista/overrides/o.toml")));
    assert!(!set.is_relevant(&root.join(".cratevista/docs/checkout.md")));
    // The code half is unaffected.
    assert!(set.is_relevant(&root.join("crates/demo/src/lib.rs")));
    assert!(set.is_relevant(&root.join("Cargo.toml")));
}

#[test]
fn all_three_referenced_file_kinds_become_exact_inputs() {
    let (_dir, root, metadata) = simple_workspace();
    // Real configuration, read by the real loader: flow docs, a flow example and
    // an override doc.
    put(
        &root.join(".cratevista/flows/a.toml"),
        r#"
[[flow]]
id = "one"
title = "One"
docs = [".cratevista/docs/flow.md"]

  [[flow.example]]
  id = "e"
  title = "E"
  path = ".cratevista/examples/req.http"
"#,
    );
    put(
        &root.join(".cratevista/overrides/o.toml"),
        r#"
[[override]]
target = "package:demo"
docs = [".cratevista/docs/override.md"]
"#,
    );
    put(&root.join(".cratevista/docs/flow.md"), "flow\n");
    put(&root.join(".cratevista/docs/override.md"), "override\n");
    put(&root.join(".cratevista/examples/req.http"), "GET /\n");

    let plan = plan(&root, &metadata, false);
    let set = plan.watch_set();
    for referenced in [
        ".cratevista/docs/flow.md",
        ".cratevista/docs/override.md",
        ".cratevista/examples/req.http",
    ] {
        assert!(
            set.is_relevant(&root.join(referenced)),
            "{referenced} is referenced and must be watched"
        );
    }
    // An unreferenced neighbour is not.
    assert!(!set.is_relevant(&root.join(".cratevista/docs/scratch.md")));
}

// --- missing paths --------------------------------------------------------

#[test]
fn a_missing_referenced_file_is_watched_through_its_nearest_existing_parent() {
    let (_dir, root, metadata) = simple_workspace();
    put(
        &root.join(".cratevista/flows/a.toml"),
        r#"
[[flow]]
id = "one"
title = "One"
docs = [".cratevista/docs/not-created-yet.md"]
"#,
    );
    // `.cratevista/docs/` does not exist at all.
    assert!(!root.join(".cratevista/docs").exists());

    let plan = plan(&root, &metadata, false);

    // The intended file is still classifiable: creating it must regenerate.
    assert!(
        plan.watch_set()
            .is_relevant(&root.join(".cratevista/docs/not-created-yet.md")),
        "the missing reference must remain watchable"
    );
    // And something that exists is registered to observe its creation.
    let registrations = registrations_of(&root, &plan);
    assert!(
        registrations
            .iter()
            .any(|(path, mode)| path == ".cratevista" && *mode == "recursive"),
        "the nearest existing ancestor must be registered recursively: {registrations:?}"
    );
}

#[test]
fn several_missing_path_components_are_observed_through_one_recursive_ancestor() {
    let (_dir, root, metadata) = simple_workspace();
    put(
        &root.join(".cratevista/flows/a.toml"),
        r#"
[[flow]]
id = "one"
title = "One"
docs = ["docs/deep/nested/guide.md"]
"#,
    );
    assert!(!root.join("docs").exists(), "three components are missing");

    let plan = plan(&root, &metadata, false);
    assert!(
        plan.watch_set()
            .is_relevant(&root.join("docs/deep/nested/guide.md"))
    );

    let registrations = registrations_of(&root, &plan);
    assert!(
        registrations
            .iter()
            .any(|(path, mode)| path.is_empty() && *mode == "recursive"),
        "the workspace root itself is the nearest existing ancestor, recursively: {registrations:?}"
    );
}

#[test]
fn an_existing_exact_file_is_watched_through_its_containing_directory() {
    let (_dir, root, metadata) = simple_workspace();
    let plan = plan(&root, &metadata, false);
    let registrations = registrations_of(&root, &plan);

    // Not an inode watch on `crates/demo/Cargo.toml`: an editor replaces the file,
    // and the watch would be left following a file nobody will touch again.
    assert!(
        registrations
            .iter()
            .any(|(path, mode)| path == "crates/demo" && *mode == "non-recursive"),
        "an exact file is watched through its directory: {registrations:?}"
    );
    assert!(
        !registrations
            .iter()
            .any(|(path, _)| path == "crates/demo/Cargo.toml"),
        "the file itself must not be registered: {registrations:?}"
    );
}

// --- containment ----------------------------------------------------------

#[cfg(unix)]
#[test]
fn a_symlinked_source_root_escaping_the_workspace_is_rejected() {
    // The check the lexical one cannot make: the path text is innocent, and only
    // resolving it reveals that it leaves the workspace.
    let outside = TempDir::new().expect("outside");
    let outside_root = root_of(&outside);
    fs::create_dir_all(outside_root.join("elsewhere")).expect("elsewhere");

    let dir = TempDir::new().expect("tempdir");
    let root = root_of(&dir);
    put(&root.join("Cargo.toml"), "[workspace]\n");
    put(&root.join("crates/demo/Cargo.toml"), "[package]\n");
    std::os::unix::fs::symlink(outside_root.join("elsewhere"), root.join("crates/demo/src"))
        .expect("symlink");

    let metadata = ingest(vec![
        entity(
            "package:demo",
            EntityKind::PACKAGE,
            Some("crates/demo/Cargo.toml"),
            Some("workspace"),
        ),
        entity(
            "target:demo:lib:demo",
            EntityKind::TARGET,
            Some("crates/demo/src/lib.rs"),
            Some("package:demo"),
        ),
    ]);

    let error = watch::plan_for_test(&root, &metadata, true).expect_err("must be rejected");
    assert_eq!(error.code, cratevista_core::watch::code::SYMLINK_ESCAPE);
}

#[test]
fn the_workspace_root_is_canonical_so_registrations_compare_against_the_truth() {
    let (_dir, root, metadata) = simple_workspace();
    let plan = plan(&root, &metadata, false);
    // Every registration is canonical and under the canonical root, so the
    // backend's canonical event paths match.
    for registration in plan.registrations() {
        assert!(
            registration.path.starts_with(&root),
            "{:?} escaped the canonical root",
            registration.path
        );
        assert_eq!(
            registration.path,
            registration.path.canonicalize().expect("canonical"),
            "registrations are canonical"
        );
    }
}

#[test]
fn an_intended_missing_path_is_never_canonicalized() {
    // `canonicalize` on a missing path fails; if the builder tried, this plan
    // would error instead of watching the parent.
    let (_dir, root, metadata) = simple_workspace();
    assert!(!root.join("Cargo.lock").exists());
    let plan = watch::plan_for_test(&root, &metadata, true)
        .expect("a missing lockfile must not break the plan")
        .plan;
    assert!(plan.watch_set().is_relevant(&root.join("Cargo.lock")));
}

// --- determinism and messages ---------------------------------------------

#[test]
fn the_plan_is_deterministic_across_repeated_builds() {
    let (_dir, root, metadata) = simple_workspace();
    let first = registrations_of(&root, &plan(&root, &metadata, false));
    for _ in 0..5 {
        assert_eq!(
            registrations_of(&root, &plan(&root, &metadata, false)),
            first
        );
    }
}

#[test]
fn a_setup_error_never_contains_an_absolute_path() {
    let missing = Path::new("definitely/not/a/workspace/anywhere");
    let error: WatchSetupError = watch::build_watch_plan(missing, &Default::default())
        .expect_err("a missing root must fail");
    assert_eq!(error.code, cratevista_core::watch::code::WORKSPACE_INVALID);
    let rendered = error.to_string();
    assert!(!rendered.contains('/') || !rendered.contains("definitely"));
    assert!(
        !rendered.contains("definitely"),
        "the message must not echo the path: {rendered}"
    );
}

// --- member patterns retained by the COMPLETE plan (Part 3) ---------------

#[test]
fn the_complete_plan_retains_declared_member_patterns() {
    // Metadata knows only `crates/existing`. If the complete plan were built from
    // metadata alone it would stop covering `crates/*` the moment it succeeded,
    // and creating `crates/new/Cargo.toml` afterwards would trigger nothing —
    // the exact regression this asserts against.
    let dir = TempDir::new().expect("tempdir");
    let root = root_of(&dir);
    put(
        &root.join("Cargo.toml"),
        "[workspace]\nmembers = [\"crates/*\"]\n",
    );
    put(&root.join("crates/existing/Cargo.toml"), "[package]\n");
    put(&root.join("crates/existing/src/lib.rs"), "pub fn e() {}\n");

    let metadata = ingest(vec![
        entity(
            "package:existing",
            EntityKind::PACKAGE,
            Some("crates/existing/Cargo.toml"),
            Some("workspace"),
        ),
        entity(
            "target:existing:lib:existing",
            EntityKind::TARGET,
            Some("crates/existing/src/lib.rs"),
            Some("package:existing"),
        ),
    ]);

    let plan = plan(&root, &metadata, true);
    let set = plan.watch_set();

    assert!(!root.join("crates/new").exists(), "precondition");
    assert!(
        set.is_relevant(&root.join("crates/new/Cargo.toml")),
        "a member created AFTER a successful complete-plan build must still be \
         covered: the declared pattern is the durable statement of intent"
    );
    // And the existing member is still covered by its own concrete inputs.
    assert!(set.is_relevant(&root.join("crates/existing/Cargo.toml")));
    assert!(set.is_relevant(&root.join("crates/existing/src/lib.rs")));
}

#[test]
fn the_complete_plan_keeps_pattern_relevance_narrow() {
    let dir = TempDir::new().expect("tempdir");
    let root = root_of(&dir);
    put(
        &root.join("Cargo.toml"),
        "[workspace]\nmembers = [\"crates/a*\"]\n",
    );
    put(&root.join("crates/api/Cargo.toml"), "[package]\n");

    let metadata = ingest(vec![entity(
        "package:api",
        EntityKind::PACKAGE,
        Some("crates/api/Cargo.toml"),
        Some("workspace"),
    )]);

    let plan = plan(&root, &metadata, true);
    let set = plan.watch_set();
    assert!(set.is_relevant(&root.join("crates/api/Cargo.toml")));
    assert!(
        !set.is_relevant(&root.join("crates/billing/Cargo.toml")),
        "the complete plan must not broaden the declared pattern"
    );
    assert!(
        !set.is_relevant(&root.join("vendor/other/Cargo.toml")),
        "an unrelated manifest is never a member"
    );
}

// --- the COMPLETE plan's pattern matrix (Part 5) ---------------------------
//
// Everything below is asserted against the recovery builder too, in
// `tests/watch_recovery.rs`. Both are asserted deliberately: the two builders
// reach the declared patterns by different routes — recovery from the root
// manifest alone, the complete plan from metadata *plus* the same manifest — and
// a rule that held in only one of them would be a coverage gap on exactly the
// runs that succeeded.

/// The metadata-derived `CorePlan` for a workspace that has one existing member.
fn complete_core(root: &Path) -> cratevista_core::watch::CorePlan {
    put(&root.join("crates/existing/Cargo.toml"), "[package]\n");
    put(&root.join("crates/existing/src/lib.rs"), "pub fn e() {}\n");
    let metadata = ingest(vec![
        entity(
            "package:existing",
            EntityKind::PACKAGE,
            Some("crates/existing/Cargo.toml"),
            Some("workspace"),
        ),
        entity(
            "target:existing:lib:existing",
            EntityKind::TARGET,
            Some("crates/existing/src/lib.rs"),
            Some("package:existing"),
        ),
    ]);
    watch::plan_for_test(root, &metadata, true).expect("a valid plan")
}

/// A workspace root with the given `[workspace]` section and nothing else.
fn workspace_root(section: &str) -> (TempDir, PathBuf) {
    let dir = TempDir::new().expect("tempdir");
    let root = root_of(&dir);
    put(&root.join("Cargo.toml"), section);
    (dir, root)
}

fn pattern_inputs(
    plan: &cratevista_core::watch::CorePlan,
    root: &Path,
) -> Vec<(String, Vec<String>)> {
    plan.inputs
        .iter()
        .filter(|input| input.kind == cratevista_watch::InputKind::WorkspaceMemberManifestPattern)
        .map(|input| (relative(root, &input.path), input.excludes.clone()))
        .collect()
}

#[test]
fn the_complete_plan_distinguishes_recursive_from_single_component_patterns() {
    let (_dir, root) = workspace_root("[workspace]\nmembers = [\"crates/**\"]\n");
    let plan = complete_core(&root);
    assert!(
        plan.plan
            .watch_set()
            .is_relevant(&root.join("crates/group/nested/Cargo.toml")),
        "`crates/**` reaches any depth in the complete plan too"
    );

    let (_dir, root) = workspace_root("[workspace]\nmembers = [\"crates/*\"]\n");
    let plan = complete_core(&root);
    assert!(
        !plan
            .plan
            .watch_set()
            .is_relevant(&root.join("crates/group/nested/Cargo.toml")),
        "`crates/*` must not silently become recursive once metadata succeeds"
    );
}

#[test]
fn the_complete_plan_normalizes_windows_pattern_and_exclude_spellings() {
    let (_dir, unix) =
        workspace_root("[workspace]\nmembers = [\"crates/*\"]\nexclude = [\"crates/skipped\"]\n");
    let (_dir2, windows) =
        workspace_root("[workspace]\nmembers = ['crates\\*']\nexclude = ['crates\\skipped']\n");

    for root in [&unix, &windows] {
        let plan = complete_core(root);
        let set = plan.plan.watch_set();
        assert!(set.is_relevant(&root.join("crates/future/Cargo.toml")));
        assert!(!set.is_relevant(&root.join("crates/skipped/Cargo.toml")));
    }

    assert_eq!(
        pattern_inputs(&complete_core(&unix), &unix),
        pattern_inputs(&complete_core(&windows), &windows),
        "both spellings normalize to one canonical pattern input"
    );
}

#[test]
fn the_complete_plan_collapses_duplicate_patterns_and_excludes() {
    let (_dir, root) = workspace_root(
        "[workspace]\nmembers = [\"crates/*\", \"crates/*\"]\n\
         exclude = [\"crates/skipped\", \"crates/skipped\"]\n",
    );
    assert_eq!(
        pattern_inputs(&complete_core(&root), &root),
        [("crates/*".to_string(), vec!["crates/skipped".to_string()])]
    );
}

#[test]
fn the_complete_plan_applies_excludes_to_existing_and_future_members_alike() {
    let (_dir, root) =
        workspace_root("[workspace]\nmembers = [\"crates/*\"]\nexclude = [\"crates/skipped\"]\n");
    put(&root.join("crates/skipped/Cargo.toml"), "[package]\n");

    let plan = complete_core(&root);
    let set = plan.plan.watch_set();

    assert!(
        !set.is_relevant(&root.join("crates/skipped/Cargo.toml")),
        "an excluded existing member is not coverage"
    );
    assert!(
        !set.is_relevant(&root.join("crates/skipped/later/Cargo.toml")),
        "nor anything beneath it"
    );
    assert!(set.is_relevant(&root.join("crates/future/Cargo.toml")));
}

#[test]
fn repeated_complete_builds_produce_identical_inputs_and_registrations() {
    let (_dir, root) = workspace_root(
        "[workspace]\nmembers = [\"crates/*\", \"tools/*\"]\n\
         exclude = [\"tools/skipped\", \"crates/skipped\"]\n",
    );
    let first = complete_core(&root);
    for _ in 0..5 {
        let again = complete_core(&root);
        assert_eq!(again.inputs, first.inputs);
        assert_eq!(again.plan.registrations(), first.plan.registrations());
    }
}

#[test]
fn the_complete_plans_logical_inputs_are_sorted_and_deduplicated() {
    let (_dir, root) = workspace_root("[workspace]\nmembers = [\"crates/*\"]\n");
    let plan = complete_core(&root);

    let mut expected = plan.inputs.clone();
    expected.sort();
    expected.dedup();
    assert_eq!(
        plan.inputs, expected,
        "a CorePlan's inputs are canonical, so feeding them back in cannot accumulate"
    );
}

#[test]
fn an_unrelated_manifest_under_the_complete_plans_registration_prefix_is_not_an_input() {
    let (_dir, root) = workspace_root("[workspace]\nmembers = [\"crates/*\"]\n");
    put(
        &root.join("crates/existing/vendor/dep/Cargo.toml"),
        "[package]\n",
    );
    let plan = complete_core(&root);

    assert_eq!(
        plan.plan
            .watch_set()
            .classify(&root.join("crates/existing/vendor/dep/Cargo.toml")),
        cratevista_watch::Classification::NotAnInput,
        "a vendored manifest is not a member, however broad the registration is"
    );
}
