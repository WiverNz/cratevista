//! The **real** recovery builder, against real temporary workspaces.
//!
//! No cargo: that is the entire point — recovery exists for the case where
//! `cargo metadata` cannot run because a declared member's manifest is missing or
//! malformed.

use std::fs;
use std::path::{Path, PathBuf};

use cratevista_core::watch;
use cratevista_watch::WatchInput;
use tempfile::TempDir;

fn root_of(dir: &TempDir) -> PathBuf {
    dir.path().canonicalize().expect("canonical root")
}

fn put(path: &Path, text: &str) {
    fs::create_dir_all(path.parent().expect("parent")).expect("mkdir");
    fs::write(path, text).expect("write");
}

/// The active plan already watches one source root — recovery must not lose it.
fn active(root: &Path) -> Vec<WatchInput> {
    vec![WatchInput::rust_root(root.join("crates/existing/src"))]
}

fn recovery(root: &Path, active: &[WatchInput]) -> cratevista_watch::WatchPlan {
    watch::build_recovery_plan(root, active, false)
        .expect("recovery must be constructible")
        .plan
}

/// A workspace whose root declares an explicit member that does not exist.
fn workspace_with(members: &str) -> (TempDir, PathBuf) {
    let dir = TempDir::new().expect("tempdir");
    let root = root_of(&dir);
    put(&root.join("Cargo.toml"), members);
    put(&root.join("crates/existing/src/lib.rs"), "pub fn e() {}\n");
    (dir, root)
}

#[test]
fn recovery_preserves_every_input_of_the_active_plan() {
    // The superset rule. Trading one blind spot for another would be no fix at all.
    let (_dir, root) = workspace_with("[workspace]\nmembers = [\"crates/new\"]\n");
    let plan = recovery(&root, &active(&root));

    assert!(
        plan.watch_set()
            .is_relevant(&root.join("crates/existing/src/lib.rs")),
        "the pre-existing source root must survive recovery"
    );
}

#[test]
fn an_explicit_member_whose_manifest_is_missing_becomes_watchable() {
    // The scenario the phase exists for: the root declares `crates/new`, its
    // manifest does not exist, and `cargo metadata` would fail. Creating it must
    // be observable.
    let (_dir, root) = workspace_with("[workspace]\nmembers = [\"crates/new\"]\n");
    assert!(!root.join("crates/new").exists());

    let plan = recovery(&root, &active(&root));
    assert!(
        plan.watch_set()
            .is_relevant(&root.join("crates/new/Cargo.toml")),
        "the declared member's manifest must be watched even though it is missing"
    );

    // And something that exists is registered to observe its creation.
    let registered = plan
        .registrations()
        .iter()
        .any(|registration| root.join("crates").starts_with(&registration.path));
    assert!(registered, "a real ancestor must carry the watch");
}

#[test]
fn the_root_manifest_and_lockfile_are_always_recovery_inputs() {
    let (_dir, root) = workspace_with("[workspace]\nmembers = [\"crates/new\"]\n");
    let plan = recovery(&root, &active(&root));
    assert!(plan.watch_set().is_relevant(&root.join("Cargo.toml")));
    assert!(
        plan.watch_set().is_relevant(&root.join("Cargo.lock")),
        "a missing lockfile is still an input"
    );
}

#[test]
fn a_member_glob_covers_existing_matches_and_honors_excludes() {
    let (_dir, root) =
        workspace_with("[workspace]\nmembers = [\"crates/*\"]\nexclude = [\"crates/skipped\"]\n");
    put(
        &root.join("crates/broken/Cargo.toml"),
        "this is not = = toml\n",
    );
    put(&root.join("crates/kept/Cargo.toml"), "[package]\n");
    put(&root.join("crates/skipped/Cargo.toml"), "[package]\n");

    let plan = recovery(&root, &active(&root));
    let set = plan.watch_set();

    assert!(
        set.is_relevant(&root.join("crates/broken/Cargo.toml")),
        "the matching manifest that breaks metadata is exactly what must be watched"
    );
    assert!(set.is_relevant(&root.join("crates/kept/Cargo.toml")));
    assert!(
        !set.is_relevant(&root.join("crates/skipped/Cargo.toml")),
        "an excluded match must never become coverage"
    );
}

#[test]
fn an_unrelated_nested_manifest_is_not_treated_as_a_member() {
    // `crates/*` matches one component. A manifest buried deeper, or outside the
    // declared prefix, is not a member and must not be watched as one.
    let (_dir, root) = workspace_with("[workspace]\nmembers = [\"crates/*\"]\n");
    put(&root.join("crates/kept/Cargo.toml"), "[package]\n");
    put(
        &root.join("crates/kept/vendor/inner/Cargo.toml"),
        "[package]\n",
    );
    put(&root.join("elsewhere/other/Cargo.toml"), "[package]\n");

    let plan = recovery(&root, &active(&root));
    let set = plan.watch_set();

    assert!(set.is_relevant(&root.join("crates/kept/Cargo.toml")));
    assert!(
        !set.is_relevant(&root.join("crates/kept/vendor/inner/Cargo.toml")),
        "a nested manifest is not a member of `crates/*`"
    );
    assert!(
        !set.is_relevant(&root.join("elsewhere/other/Cargo.toml")),
        "a manifest outside the declared prefix is not a member"
    );
}

#[test]
fn absolute_and_traversing_member_entries_never_become_coverage() {
    let (_dir, root) = workspace_with(
        "[workspace]\nmembers = [\"/etc/passwd\", \"../outside\", \"C:/secrets\", \"crates/ok\"]\n",
    );
    let plan = recovery(&root, &active(&root));

    // The legitimate one survives; the dangerous spellings contribute nothing, and
    // no registration points outside the workspace.
    assert!(
        plan.watch_set()
            .is_relevant(&root.join("crates/ok/Cargo.toml"))
    );
    for registration in plan.registrations() {
        assert!(
            registration.path.starts_with(&root),
            "a registration escaped the workspace: {:?}",
            registration.path
        );
    }
}

#[test]
fn a_malformed_root_manifest_is_a_stable_safe_failure() {
    let (_dir, root) = workspace_with("this is not = = toml\n");
    let error = watch::build_recovery_plan(&root, &active(&root), false)
        .expect_err("a malformed root manifest must fail");

    assert_eq!(
        error.code,
        cratevista_core::watch::code::ROOT_MANIFEST_INVALID
    );
    let rendered = error.to_string();
    assert!(
        !rendered.contains(&root.to_string_lossy().to_string()),
        "no absolute path in a browser-safe message: {rendered}"
    );
}

#[test]
fn a_root_manifest_with_no_workspace_section_still_yields_recovery_coverage() {
    // A single-crate project: no `[workspace]`, nothing to expand, and recovery
    // must still watch the root manifest and the active plan's inputs.
    let (_dir, root) = workspace_with("[package]\nname = \"demo\"\n");
    let plan = recovery(&root, &active(&root));
    assert!(plan.watch_set().is_relevant(&root.join("Cargo.toml")));
    assert!(
        plan.watch_set()
            .is_relevant(&root.join("crates/existing/src/lib.rs"))
    );
}

#[test]
fn recovery_is_deterministic_across_repeated_builds() {
    let (_dir, root) = workspace_with("[workspace]\nmembers = [\"crates/*\"]\n");
    put(&root.join("crates/b/Cargo.toml"), "[package]\n");
    put(&root.join("crates/a/Cargo.toml"), "[package]\n");

    let first: Vec<_> = recovery(&root, &active(&root)).registrations().to_vec();
    for _ in 0..5 {
        assert_eq!(recovery(&root, &active(&root)).registrations(), first);
    }
}

#[test]
fn no_config_drops_the_configuration_inputs_from_recovery() {
    let (_dir, root) = workspace_with("[workspace]\nmembers = [\"crates/new\"]\n");
    let plan = watch::build_recovery_plan(&root, &active(&root), true)
        .expect("recovery")
        .plan;
    assert!(
        !plan.watch_set().is_relevant(&root.join("cratevista.toml")),
        "--no-config reads no configuration, so none of it is recovery coverage"
    );
    assert!(plan.watch_set().is_relevant(&root.join("Cargo.toml")));
}

// --- member patterns (Part 3) ---------------------------------------------

#[test]
fn recovery_covers_a_future_glob_member_that_does_not_exist_yet() {
    // The headline: `crates/*` must cover a member created *later*, without the
    // root manifest changing again.
    let (_dir, root) = workspace_with("[workspace]\nmembers = [\"crates/*\"]\n");
    assert!(!root.join("crates/new").exists());

    let plan = recovery(&root, &active(&root));
    assert!(
        plan.watch_set()
            .is_relevant(&root.join("crates/new/Cargo.toml")),
        "a future glob member must be classifiable under recovery coverage"
    );
    // Registered by the pattern's static prefix, recursively.
    assert!(
        plan.registrations().iter().any(|registration| {
            registration.path == root.join("crates")
                && registration.mode == cratevista_watch::RegistrationMode::Recursive
        }),
        "`crates/*` registers `crates` recursively: {:?}",
        plan.registrations()
    );
}

#[test]
fn a_pattern_input_is_narrow_even_though_its_registration_is_broad() {
    // The registration is `crates` recursively — the OS cannot match globs — but
    // relevance is still decided by the pattern, not the prefix.
    let (_dir, root) = workspace_with("[workspace]\nmembers = [\"crates/a*\"]\n");
    let set = recovery(&root, &active(&root));
    let set = set.watch_set();

    assert!(set.is_relevant(&root.join("crates/api/Cargo.toml")));
    assert!(
        !set.is_relevant(&root.join("crates/billing/Cargo.toml")),
        "`crates/a*` must not accept `billing`, however broad the registration is"
    );
    assert!(
        !set.is_relevant(&root.join("crates/api/nested/Cargo.toml")),
        "a nested manifest under a member is not itself a member"
    );
}

#[test]
fn recovery_pattern_excludes_reach_the_classifier() {
    let (_dir, root) =
        workspace_with("[workspace]\nmembers = [\"crates/*\"]\nexclude = [\"crates/skipped\"]\n");
    let plan = recovery(&root, &active(&root));
    let set = plan.watch_set();

    assert!(set.is_relevant(&root.join("crates/kept/Cargo.toml")));
    assert!(
        !set.is_relevant(&root.join("crates/skipped/Cargo.toml")),
        "an excluded FUTURE member must never be relevant either"
    );
}

#[test]
fn an_explicit_member_does_not_become_a_pattern_input() {
    // `crates/new` is exact; only glob entries become pattern inputs.
    let (_dir, root) = workspace_with("[workspace]\nmembers = [\"crates/new\"]\n");
    let plan = recovery(&root, &active(&root));
    let set = plan.watch_set();

    assert!(set.is_relevant(&root.join("crates/new/Cargo.toml")));
    assert!(
        !set.is_relevant(&root.join("crates/other/Cargo.toml")),
        "an explicit member must not behave like a glob"
    );
}

#[test]
fn a_malformed_or_unsafe_pattern_never_broadens_coverage() {
    let (_dir, root) = workspace_with(
        "[workspace]\nmembers = [\"crates/[unterminated\", \"/etc/*\", \"../outside/*\"]\n",
    );
    let plan = recovery(&root, &active(&root));

    assert!(
        !plan
            .watch_set()
            .is_relevant(&root.join("crates/anything/Cargo.toml")),
        "a malformed pattern must match nothing"
    );
    for registration in plan.registrations() {
        assert!(
            registration.path.starts_with(&root),
            "an unsafe entry produced an external registration: {:?}",
            registration.path
        );
    }
}

// --- member symlink containment (Part 4) ----------------------------------

#[cfg(unix)]
#[test]
fn a_member_directory_symlinked_outside_the_workspace_is_rejected() {
    // The check a lexical rule cannot make: `crates/escape` is innocent text, and
    // only resolving it reveals that watching it would watch someone else's disk.
    let outside = TempDir::new().expect("outside");
    let outside_root = root_of(&outside);
    fs::create_dir_all(outside_root.join("elsewhere")).expect("elsewhere");

    let (_dir, root) = workspace_with("[workspace]\nmembers = [\"crates/escape\"]\n");
    std::os::unix::fs::symlink(outside_root.join("elsewhere"), root.join("crates/escape"))
        .expect("symlink");

    let error = watch::build_recovery_plan(&root, &active(&root), false)
        .expect_err("an escaping member must be rejected");

    assert_eq!(error.code, cratevista_core::watch::code::SYMLINK_ESCAPE);
    let rendered = error.to_string();
    assert!(
        !rendered.contains(&outside_root.to_string_lossy().to_string())
            && !rendered.contains(&root.to_string_lossy().to_string()),
        "a browser-safe message must carry no absolute path: {rendered}"
    );
}

#[cfg(unix)]
#[test]
fn a_member_pattern_prefix_symlinked_outside_the_workspace_is_rejected() {
    // The same, through a glob's static prefix: `crates/*` registers `crates`, and
    // if `crates` itself resolves outside, the registration must be refused rather
    // than silently skipped.
    let outside = TempDir::new().expect("outside");
    let outside_root = root_of(&outside);
    fs::create_dir_all(outside_root.join("elsewhere")).expect("elsewhere");

    let dir = TempDir::new().expect("tempdir");
    let root = root_of(&dir);
    put(
        &root.join("Cargo.toml"),
        "[workspace]\nmembers = [\"crates/*\"]\n",
    );
    std::os::unix::fs::symlink(outside_root.join("elsewhere"), root.join("crates"))
        .expect("symlink");

    let error = watch::build_recovery_plan(&root, &[], false)
        .expect_err("an escaping pattern prefix must be rejected");
    assert_eq!(error.code, cratevista_core::watch::code::SYMLINK_ESCAPE);
}

// --- the pattern / exclude / determinism matrix (Part 5) -------------------

/// The whole `CorePlan`, not just its plan: the logical inputs are core's own
/// record, and they are fed back in as the next run's `active`.
fn core(root: &Path, active: &[WatchInput]) -> cratevista_core::watch::CorePlan {
    watch::build_recovery_plan(root, active, false).expect("recovery must be constructible")
}

/// The member-pattern inputs as `(workspace-relative pattern, excludes)`.
fn pattern_inputs(
    plan: &cratevista_core::watch::CorePlan,
    root: &Path,
) -> Vec<(String, Vec<String>)> {
    plan.inputs
        .iter()
        .filter(|input| input.kind == cratevista_watch::InputKind::WorkspaceMemberManifestPattern)
        .map(|input| {
            (
                input
                    .path
                    .strip_prefix(root)
                    .expect("inside the workspace")
                    .to_string_lossy()
                    .replace('\\', "/"),
                input.excludes.clone(),
            )
        })
        .collect()
}

#[test]
fn a_recursive_member_pattern_accepts_a_nested_manifest_and_a_single_star_does_not() {
    // `*` stays within one component; only `**` spans them. Both spellings are
    // legal Cargo, and they must not mean the same thing.
    let (_dir, root) = workspace_with("[workspace]\nmembers = [\"crates/**\"]\n");
    let set = recovery(&root, &active(&root));
    let set = set.watch_set();

    assert!(
        set.is_relevant(&root.join("crates/group/nested/Cargo.toml")),
        "`crates/**` must cover a manifest nested at any depth"
    );
    assert!(set.is_relevant(&root.join("crates/direct/Cargo.toml")));

    let (_dir, root) = workspace_with("[workspace]\nmembers = [\"crates/*\"]\n");
    let set = recovery(&root, &active(&root));
    let set = set.watch_set();

    assert!(set.is_relevant(&root.join("crates/direct/Cargo.toml")));
    assert!(
        !set.is_relevant(&root.join("crates/group/nested/Cargo.toml")),
        "`crates/*` must NOT reach a nested manifest — that is what `**` is for"
    );
}

#[test]
fn windows_and_unix_spellings_of_patterns_and_excludes_behave_identically() {
    // A `Cargo.toml` written on Windows may spell a member `crates\*`. The two
    // spellings name the same members, so they must produce the same coverage.
    let (_dir, unix) =
        workspace_with("[workspace]\nmembers = [\"crates/*\"]\nexclude = [\"crates/skipped\"]\n");
    let (_dir2, windows) =
        workspace_with("[workspace]\nmembers = ['crates\\*']\nexclude = ['crates\\skipped']\n");

    for root in [&unix, &windows] {
        let plan = recovery(root, &active(root));
        let set = plan.watch_set();
        assert!(
            set.is_relevant(&root.join("crates/kept/Cargo.toml")),
            "a member must be covered however the separator was spelled"
        );
        assert!(
            !set.is_relevant(&root.join("crates/skipped/Cargo.toml")),
            "an exclude must apply however the separator was spelled"
        );
    }

    // And the normalized inputs are literally the same, modulo the root.
    assert_eq!(
        pattern_inputs(&core(&unix, &[]), &unix),
        pattern_inputs(&core(&windows, &[]), &windows),
        "both spellings normalize to one canonical pattern input"
    );
}

#[test]
fn duplicate_member_patterns_and_excludes_collapse() {
    let (_dir, root) = workspace_with(
        "[workspace]\nmembers = [\"crates/*\", \"crates/*\"]\n\
         exclude = [\"crates/skipped\", \"crates/skipped\"]\n",
    );
    let plan = core(&root, &[]);

    assert_eq!(
        pattern_inputs(&plan, &root),
        [("crates/*".to_string(), vec!["crates/skipped".to_string()])],
        "one pattern, one exclude — a manifest that says a thing twice means it once"
    );
}

#[test]
fn duplicate_logical_inputs_and_registrations_collapse() {
    // The active plan already contains inputs recovery adds itself. Without
    // deduplication the retained list would grow on every single regeneration.
    let (_dir, root) = workspace_with("[workspace]\nmembers = [\"crates/*\"]\n");
    let already = vec![
        WatchInput::file(root.join("Cargo.toml")),
        WatchInput::file(root.join("Cargo.toml")),
        WatchInput::rust_root(root.join("crates/existing/src")),
    ];
    let plan = core(&root, &already);

    let manifests = plan
        .inputs
        .iter()
        .filter(|input| input.path == root.join("Cargo.toml"))
        .count();
    assert_eq!(manifests, 1, "the root manifest is one input, not three");

    let mut registrations = plan.plan.registrations().to_vec();
    let before = registrations.len();
    registrations.dedup();
    assert_eq!(
        before,
        registrations.len(),
        "registrations are deduplicated"
    );
}

#[test]
fn feeding_recovery_its_own_inputs_back_is_a_fixed_point() {
    // This is what the runtime does: the retained `CorePlan.inputs` become the next
    // run's `active`. If that were not idempotent the input list would grow without
    // bound for as long as watch mode runs.
    let (_dir, root) = workspace_with("[workspace]\nmembers = [\"crates/*\"]\n");
    put(&root.join("crates/a/Cargo.toml"), "[package]\n");

    let first = core(&root, &active(&root));
    let second = core(&root, &first.inputs);
    let third = core(&root, &second.inputs);

    assert_eq!(second.inputs, first.inputs, "recovery is idempotent");
    assert_eq!(third.inputs, second.inputs);
    assert_eq!(second.plan.registrations(), first.plan.registrations());
}

#[test]
fn repeated_recovery_builds_produce_identical_inputs_and_registrations() {
    // Determinism of both patterns and excludes, regardless of the order the
    // filesystem happened to list anything in.
    let (_dir, root) = workspace_with(
        "[workspace]\nmembers = [\"crates/*\", \"tools/*\"]\n\
         exclude = [\"tools/skipped\", \"crates/skipped\"]\n",
    );
    put(&root.join("crates/b/Cargo.toml"), "[package]\n");
    put(&root.join("crates/a/Cargo.toml"), "[package]\n");
    put(&root.join("tools/z/Cargo.toml"), "[package]\n");

    let first = core(&root, &active(&root));
    for _ in 0..5 {
        let again = core(&root, &active(&root));
        assert_eq!(again.inputs, first.inputs);
        assert_eq!(again.plan.registrations(), first.plan.registrations());
    }

    // Excludes are sorted within each pattern input, so a manifest that lists them
    // the other way round yields the same plan.
    let (_dir2, swapped) = workspace_with(
        "[workspace]\nmembers = [\"tools/*\", \"crates/*\"]\n\
         exclude = [\"crates/skipped\", \"tools/skipped\"]\n",
    );
    assert_eq!(
        pattern_inputs(&core(&swapped, &[]), &swapped),
        pattern_inputs(&core(&root, &[]), &root),
        "declaration order must not change the plan"
    );
}

#[test]
fn an_exclude_applies_to_an_existing_member_and_to_a_future_one_alike() {
    let (_dir, root) =
        workspace_with("[workspace]\nmembers = [\"crates/*\"]\nexclude = [\"crates/skipped\"]\n");
    // One excluded member exists already; the other does not exist yet.
    put(&root.join("crates/skipped/Cargo.toml"), "[package]\n");
    assert!(!root.join("crates/later").exists(), "precondition");

    let plan = recovery(&root, &active(&root));
    let set = plan.watch_set();

    assert!(
        !set.is_relevant(&root.join("crates/skipped/Cargo.toml")),
        "an excluded EXISTING member is not coverage"
    );
    assert!(
        set.is_relevant(&root.join("crates/later/Cargo.toml")),
        "a non-excluded future member still is"
    );

    // And an exclude that only names a future member holds too.
    let (_dir2, root) =
        workspace_with("[workspace]\nmembers = [\"crates/*\"]\nexclude = [\"crates/future\"]\n");
    let plan = recovery(&root, &active(&root));
    assert!(
        !plan
            .watch_set()
            .is_relevant(&root.join("crates/future/Cargo.toml")),
        "an excluded FUTURE member must never become relevant when it appears"
    );
}

#[test]
fn an_unrelated_manifest_under_the_registration_prefix_is_not_an_input() {
    // `crates/*` registers `crates` **recursively** — the OS cannot match globs —
    // so vendored manifests genuinely arrive as events. Classification is what
    // rejects them, and it must say `NotAnInput` rather than merely "not relevant":
    // an ignored path and an uninteresting one are different answers.
    let (_dir, root) = workspace_with("[workspace]\nmembers = [\"crates/*\"]\n");
    put(
        &root.join("crates/kept/vendor/dep/Cargo.toml"),
        "[package]\n",
    );

    let plan = recovery(&root, &active(&root));
    let set = plan.watch_set();

    assert_eq!(
        set.classify(&root.join("crates/kept/vendor/dep/Cargo.toml")),
        cratevista_watch::Classification::NotAnInput,
        "a vendored manifest below a member is not a workspace member"
    );
    assert_eq!(
        set.classify(&root.join("crates/kept/README.md")),
        cratevista_watch::Classification::NotAnInput,
        "the broad registration does not make every file under it an input"
    );
}
