//! The regeneration transaction's order, and what a failure does and does not do.
//!
//! Every stage is injected, so these prove the sequence itself: no cargo, no
//! rustdoc, no filesystem, no native watcher, no sleeps.
//!
//! The invariant under test throughout: **a WatchPlan is liveness coverage, not
//! published state.** It may lead the served snapshot; it may never lag the inputs
//! a regeneration used.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use cratevista_core::watch::{self, CorePlan, Stages, Transaction};
use cratevista_server::ServerEvent;
use cratevista_watch::{
    EngineEvent, Regenerate, RegenerationFailure, RegenerationRequest, RegistrationMode,
    WatchInput, WatchPlan, WatchRegistration, WatchSet,
};

/// A snapshot stand-in: the transaction only ever asks whether it is partial.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct FakeSnapshot {
    partial: bool,
}

/// Which stage should fail.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FailAt {
    Nothing,
    BuildRecovery,
    ReplaceRecovery,
    BuildPlan,
    ReplacePlan,
    Generate,
    Load,
}

/// A plan plus the logical inputs it came from.
///
/// The three plans below are deliberately given **distinct logical inputs**, so
/// "which plan is active" is a question about inputs rather than about a counter
/// the fake incremented.
fn core_plan(trees: &[&str], files: &[&str], patterns: &[&str]) -> CorePlan {
    let root = Path::new("/w");

    let mut inputs: Vec<WatchInput> = trees
        .iter()
        .map(|tree| WatchInput::rust_root(format!("/w/{tree}")))
        .collect();
    inputs.extend(
        files
            .iter()
            .map(|file| WatchInput::file(format!("/w/{file}"))),
    );
    inputs.extend(
        patterns
            .iter()
            .map(|pattern| WatchInput::workspace_member_pattern(format!("/w/{pattern}"), [])),
    );
    inputs.sort();
    inputs.dedup();

    let mut registrations: Vec<WatchRegistration> = trees
        .iter()
        .map(|tree| WatchRegistration {
            path: PathBuf::from(format!("/w/{tree}")),
            mode: RegistrationMode::Recursive,
        })
        .collect();
    registrations.extend(patterns.iter().map(|pattern| WatchRegistration {
        path: PathBuf::from(format!(
            "/w/{}",
            cratevista_watch::pattern::static_prefix(pattern)
        )),
        mode: RegistrationMode::Recursive,
    }));

    let plan =
        WatchPlan::new(WatchSet::new(root, inputs.clone()), registrations).expect("a valid plan");
    CorePlan { plan, inputs }
}

/// What was already active: one source root and nothing else.
fn previous_core() -> CorePlan {
    core_plan(&["old"], &[], &[])
}

/// Recovery: the previous inputs, plus the root manifest, the declared member's
/// tree and the declared `crates/*` pattern. A superset, built without cargo.
fn recovery_core() -> CorePlan {
    core_plan(&["member", "old"], &["Cargo.toml"], &["crates/*"])
}

/// The complete, metadata-derived plan: source roots and config inputs metadata
/// and discovery found — and the declared pattern, which it must never drop.
///
/// It does **not** carry recovery's `member/` tree: metadata resolved the
/// workspace and that concrete guess is obsolete. Coverage may drop an obsolete
/// concrete input; it may never drop a declared pattern.
fn complete_core() -> CorePlan {
    core_plan(&["new", "old"], &["cratevista.toml"], &["crates/*"])
}

/// The logical inputs of whichever known plan this is.
///
/// Deriving the answer from the plan itself — rather than from the order calls
/// happened to arrive in — is what makes the ownership assertions mean something.
/// The `panic` is a real assertion too: the transaction may only ever activate a
/// plan one of the builders produced.
fn inputs_of(plan: &WatchPlan) -> Vec<WatchInput> {
    for known in [previous_core(), recovery_core(), complete_core()] {
        if known.plan.registrations() == plan.registrations() {
            return known.inputs;
        }
    }
    panic!("the transaction activated a plan no builder produced")
}

/// Logical inputs as stable, comparable text.
fn labels(inputs: &[WatchInput]) -> Vec<String> {
    let mut labels: Vec<String> = inputs
        .iter()
        .map(|input| {
            format!(
                "{:?} {}",
                input.kind,
                input.path.to_string_lossy().replace('\\', "/")
            )
        })
        .collect();
    labels.sort();
    labels
}

/// Recovery adds the declared member's tree to whatever was already active. It is
/// coarser than the complete plan (it knows no `src_path`) but it is a superset.
fn recovery_plan() -> WatchPlan {
    recovery_core().plan
}

fn candidate_plan() -> WatchPlan {
    complete_core().plan
}

struct FakeStages {
    fail_at: FailAt,
    partial: bool,
    /// How many replacements have been applied, so the fake can tell recovery from
    /// the complete plan.
    replacements: Arc<Mutex<usize>>,
    /// Every stage entered, in order.
    calls: Arc<Mutex<Vec<&'static str>>>,
    /// Core's retained record of what the watcher currently has. Starts as the
    /// previous plan, and — this is the ownership rule under test — is updated
    /// **only** when a `replace_plan` actually succeeds.
    retained: Arc<Mutex<CorePlan>>,
    /// What was committed, if anything.
    committed: Arc<Mutex<Vec<FakeSnapshot>>>,
}

impl Stages for FakeStages {
    type Snapshot = FakeSnapshot;

    fn generate(&self) -> Result<(), RegenerationFailure> {
        self.calls.lock().unwrap().push("generate");
        if self.fail_at == FailAt::Generate {
            return Err(RegenerationFailure::new(
                cratevista_core::watch::code::GENERATION_FAILED,
                "generation failed; see the terminal for details",
            ));
        }
        Ok(())
    }

    fn load(&self) -> Result<Self::Snapshot, RegenerationFailure> {
        self.calls.lock().unwrap().push("load");
        if self.fail_at == FailAt::Load {
            return Err(watch::artifacts_failure());
        }
        Ok(FakeSnapshot {
            partial: self.partial,
        })
    }

    fn partial(snapshot: &Self::Snapshot) -> bool {
        snapshot.partial
    }

    fn build_recovery_plan(&self) -> Result<WatchPlan, RegenerationFailure> {
        self.calls.lock().unwrap().push("build_recovery");
        if self.fail_at == FailAt::BuildRecovery {
            return Err(watch::setup_failure(&setup_error()));
        }
        Ok(recovery_plan())
    }

    fn build_plan(&self) -> Result<WatchPlan, RegenerationFailure> {
        self.calls.lock().unwrap().push("build_plan");
        if self.fail_at == FailAt::BuildPlan {
            return Err(watch::setup_failure(&setup_error()));
        }
        Ok(candidate_plan())
    }

    fn replace_plan(
        &self,
        plan: WatchPlan,
    ) -> std::pin::Pin<
        Box<dyn std::future::Future<Output = Result<(), RegenerationFailure>> + Send + '_>,
    > {
        Box::pin(async move {
            let index = {
                let mut count = self.replacements.lock().unwrap();
                *count += 1;
                *count
            };
            let first = index == 1;
            self.calls.lock().unwrap().push(if first {
                "replace_recovery"
            } else {
                "replace_plan"
            });

            let fails = if first {
                self.fail_at == FailAt::ReplaceRecovery
            } else {
                self.fail_at == FailAt::ReplacePlan
            };
            if fails {
                // The real watcher keeps the complete previous plan on failure, so
                // the retained record must not move either: a core-side copy that
                // ran ahead of the watcher would describe coverage that does not
                // exist, which is the same lie as lagging, told the other way.
                return Err(watch::replace_failure());
            }
            *self.retained.lock().unwrap() = CorePlan {
                inputs: inputs_of(&plan),
                plan,
            };
            Ok(())
        })
    }

    fn commit(&self, snapshot: Self::Snapshot) {
        self.calls.lock().unwrap().push("commit");
        self.committed.lock().unwrap().push(snapshot);
    }
}

fn setup_error() -> cratevista_core::watch::WatchSetupError {
    watch::build_watch_plan(Path::new("no/such/workspace"), &Default::default())
        .expect_err("a missing root fails")
}

struct Harness {
    transaction: Transaction<FakeStages>,
    calls: Arc<Mutex<Vec<&'static str>>>,
    retained: Arc<Mutex<CorePlan>>,
    replacements: Arc<Mutex<usize>>,
    committed: Arc<Mutex<Vec<FakeSnapshot>>>,
}

fn harness(fail_at: FailAt, partial: bool) -> Harness {
    let calls = Arc::new(Mutex::new(Vec::new()));
    let committed = Arc::new(Mutex::new(Vec::new()));
    let retained = Arc::new(Mutex::new(previous_core()));
    let replacements = Arc::new(Mutex::new(0));
    Harness {
        transaction: Transaction::new(FakeStages {
            fail_at,
            partial,
            replacements: replacements.clone(),
            calls: calls.clone(),
            retained: retained.clone(),
            committed: committed.clone(),
        }),
        calls,
        retained,
        replacements,
        committed,
    }
}

impl Harness {
    fn covers(&self, path: &str) -> bool {
        self.retained
            .lock()
            .unwrap()
            .plan
            .watch_set()
            .is_relevant(Path::new(path))
    }

    /// Whether the currently active plan watches `new/src/lib.rs`.
    fn covers_new_input(&self) -> bool {
        self.covers("/w/new/src/lib.rs")
    }

    /// Whether the active plan covers the recovery-added member.
    fn covers_member(&self) -> bool {
        self.covers("/w/member/src/lib.rs")
    }

    /// Whether the pre-existing source root is still covered.
    fn covers_old_input(&self) -> bool {
        self.covers("/w/old/src/lib.rs")
    }

    /// The **logical inputs** of the plan that is active right now.
    ///
    /// Coverage probes answer "is this one path watched?"; this answers "which
    /// plan is active?", which is the actual claim every failure case makes.
    fn active_inputs(&self) -> Vec<String> {
        labels(&self.retained.lock().unwrap().inputs)
    }
}

fn request() -> RegenerationRequest {
    RegenerationRequest::new([PathBuf::from("/home/someone/secret-project/src/lib.rs")])
        .expect("non-empty")
}

// --- the order ------------------------------------------------------------

#[tokio::test]
async fn a_full_success_builds_and_replaces_the_plan_before_generating() {
    let harness = harness(FailAt::Nothing, false);
    let outcome = harness.transaction.regenerate(request()).await;

    assert_eq!(
        outcome,
        Ok(cratevista_watch::RegenerationSuccess { partial: false })
    );
    assert_eq!(
        *harness.calls.lock().unwrap(),
        [
            "build_recovery",
            "replace_recovery",
            "build_plan",
            "replace_plan",
            "generate",
            "load",
            "commit",
        ],
        "coverage comes first: generation must not read files nobody is watching"
    );
    assert_eq!(harness.committed.lock().unwrap().len(), 1, "committed once");
}

#[tokio::test]
async fn a_partial_success_follows_the_same_order_and_reports_partial() {
    let harness = harness(FailAt::Nothing, true);
    let outcome = harness.transaction.regenerate(request()).await;

    assert_eq!(
        outcome,
        Ok(cratevista_watch::RegenerationSuccess { partial: true })
    );
    assert_eq!(
        *harness.calls.lock().unwrap(),
        [
            "build_recovery",
            "replace_recovery",
            "build_plan",
            "replace_plan",
            "generate",
            "load",
            "commit",
        ]
    );
}

#[tokio::test]
async fn success_is_returned_only_after_the_commit() {
    let harness = harness(FailAt::Nothing, false);
    harness
        .transaction
        .regenerate(request())
        .await
        .expect("success");
    assert_eq!(
        harness.calls.lock().unwrap().last(),
        Some(&"commit"),
        "GenerationSucceeded must mean the new snapshot is already live, so the \
         commit is the last thing the operation does"
    );
}

// --- failures -------------------------------------------------------------

#[tokio::test]
async fn a_plan_build_failure_stops_before_replacement_and_generation() {
    let harness = harness(FailAt::BuildPlan, false);
    let error = harness
        .transaction
        .regenerate(request())
        .await
        .expect_err("must fail");

    assert_eq!(error.code, cratevista_core::watch::code::WORKSPACE_INVALID);
    assert_eq!(
        *harness.calls.lock().unwrap(),
        ["build_recovery", "replace_recovery", "build_plan"],
        "if the workspace cannot be described safely, nothing proceeds — publishing \
         a newer snapshot behind an older plan is exactly the lag this forbids"
    );
    assert!(harness.committed.lock().unwrap().is_empty());
    assert!(
        harness.covers_member(),
        "recovery coverage stays active after a complete-plan failure — it is what \
         observes the fix to whatever made the plan unbuildable"
    );
}

#[tokio::test]
async fn a_plan_replacement_failure_stops_before_generation() {
    let harness = harness(FailAt::ReplacePlan, false);
    let error = harness
        .transaction
        .regenerate(request())
        .await
        .expect_err("must fail");

    assert_eq!(
        error.code,
        cratevista_core::watch::code::PLAN_REPLACE_FAILED
    );
    assert_eq!(
        *harness.calls.lock().unwrap(),
        [
            "build_recovery",
            "replace_recovery",
            "build_plan",
            "replace_plan"
        ],
        "a plan we could not apply must not become a generation whose inputs are \
         unwatched"
    );
    assert!(harness.committed.lock().unwrap().is_empty());
    assert!(
        harness.covers_member(),
        "recovery coverage stands — not the older, narrower plan"
    );
}

#[tokio::test]
async fn a_generation_failure_retains_the_new_plan_and_does_not_load_or_commit() {
    let harness = harness(FailAt::Generate, false);
    let error = harness
        .transaction
        .regenerate(request())
        .await
        .expect_err("must fail");

    assert_eq!(error.code, cratevista_core::watch::code::GENERATION_FAILED);
    assert_eq!(
        *harness.calls.lock().unwrap(),
        [
            "build_recovery",
            "replace_recovery",
            "build_plan",
            "replace_plan",
            "generate"
        ],
        "no load, no commit"
    );
    assert!(harness.committed.lock().unwrap().is_empty());
    assert!(
        harness.covers_new_input(),
        "the newer coverage STAYS: the fix will be an edit to the files this run \
         introduced, and if the plan rolled back nobody would ever see it"
    );
}

#[tokio::test]
async fn a_load_failure_retains_the_new_plan_and_does_not_commit() {
    let harness = harness(FailAt::Load, false);
    let error = harness
        .transaction
        .regenerate(request())
        .await
        .expect_err("must fail");

    assert_eq!(
        error.code,
        cratevista_core::watch::code::ARTIFACTS_UNREADABLE
    );
    assert_eq!(
        *harness.calls.lock().unwrap(),
        [
            "build_recovery",
            "replace_recovery",
            "build_plan",
            "replace_plan",
            "generate",
            "load"
        ],
        "unverifiable artifacts are never committed"
    );
    assert!(harness.committed.lock().unwrap().is_empty());
    assert!(
        harness.covers_new_input(),
        "coverage may lead the served snapshot"
    );
}

#[tokio::test]
async fn the_commit_happens_exactly_once_and_only_after_a_successful_load() {
    let harness = harness(FailAt::Nothing, false);
    harness.transaction.regenerate(request()).await.expect("ok");

    let calls = harness.calls.lock().unwrap();
    let commit = calls
        .iter()
        .position(|call| *call == "commit")
        .expect("committed");
    let load = calls
        .iter()
        .position(|call| *call == "load")
        .expect("loaded");
    assert!(load < commit, "the commit follows a successful load");
    assert_eq!(calls.iter().filter(|call| **call == "commit").count(), 1);
}

// --- recovery coverage ----------------------------------------------------

#[tokio::test]
async fn a_failed_generation_still_leaves_its_new_input_observable() {
    // The scenario the old order got wrong, end to end.
    let harness = harness(FailAt::Generate, false);

    // 1 + 2. The old plan does not cover the new input; the candidate does.
    assert!(
        !harness.covers_new_input(),
        "precondition: the new input is not watched yet"
    );

    let error = harness
        .transaction
        .regenerate(request())
        .await
        .expect_err("generation fails");

    // 3 + 4. The candidate was activated, then generation failed.
    assert_eq!(error.code, cratevista_core::watch::code::GENERATION_FAILED);
    // 5. The old snapshot stands.
    assert!(harness.committed.lock().unwrap().is_empty());
    // 6 + 7. The active plan is still the candidate, so an edit to the new input
    //        classifies as relevant — which is exactly how the user's fix reaches
    //        the engine and triggers the next run.
    assert!(
        harness.covers_new_input(),
        "the fix must be observable, or the user edits the file and nothing happens"
    );
}

#[tokio::test]
async fn a_failed_load_still_leaves_its_new_input_observable() {
    let harness = harness(FailAt::Load, false);
    assert!(!harness.covers_new_input());

    harness
        .transaction
        .regenerate(request())
        .await
        .expect_err("load fails");

    assert!(
        harness.committed.lock().unwrap().is_empty(),
        "old snapshot stands"
    );
    assert!(
        harness.covers_new_input(),
        "coverage leads publication after a load failure too"
    );
}

// --- safety of what escapes -----------------------------------------------

#[tokio::test]
async fn no_changed_path_reaches_a_failure_or_a_server_event() {
    for fail_at in [
        FailAt::BuildRecovery,
        FailAt::ReplaceRecovery,
        FailAt::BuildPlan,
        FailAt::ReplacePlan,
        FailAt::Generate,
        FailAt::Load,
    ] {
        let harness = harness(fail_at, false);
        let error = harness
            .transaction
            .regenerate(request())
            .await
            .expect_err("must fail");

        let rendered = format!("{}|{}", error.code, error.message);
        assert!(
            !rendered.contains("secret-project") && !rendered.contains("/home/"),
            "{fail_at:?} leaked the changed path: {rendered}"
        );

        let event = watch::to_server_event(EngineEvent::GenerationFailed {
            code: error.code.clone(),
            message: error.message.clone(),
        });
        let rendered = format!("{event:?}");
        assert!(
            !rendered.contains("secret-project") && !rendered.contains("/home/"),
            "{fail_at:?} leaked through the event: {rendered}"
        );
    }
}

#[tokio::test]
async fn every_failure_code_is_stable_and_its_message_is_browser_safe() {
    let cases = [
        (FailAt::BuildRecovery, "watch_workspace_invalid"),
        (FailAt::BuildPlan, "watch_workspace_invalid"),
        (FailAt::ReplacePlan, "watch_plan_replace_failed"),
        (FailAt::Generate, "watch_generation_failed"),
        (FailAt::Load, "watch_artifacts_unreadable"),
    ];
    for (fail_at, code) in cases {
        let harness = harness(fail_at, false);
        let error = harness
            .transaction
            .regenerate(request())
            .await
            .expect_err("must fail");
        assert_eq!(error.code, code);
        assert!(!error.message.is_empty(), "a code alone helps nobody");
        assert!(!error.message.contains("/home/"));
        assert!(!error.message.contains("C:\\"));
        assert!(!error.message.contains("--edition"));
        assert!(!error.message.contains("CARGO_HOME"));
        assert!(!error.message.contains("RUSTUP_HOME"));
    }
}

#[tokio::test]
async fn repeated_executions_produce_identical_stage_sequences() {
    let first = {
        let harness = harness(FailAt::Nothing, false);
        harness.transaction.regenerate(request()).await.expect("ok");
        harness.calls.lock().unwrap().clone()
    };
    for _ in 0..5 {
        let harness = harness(FailAt::Nothing, false);
        harness.transaction.regenerate(request()).await.expect("ok");
        assert_eq!(*harness.calls.lock().unwrap(), first);
    }
}

// --- event conversion -----------------------------------------------------

#[test]
fn engine_events_convert_to_server_events_exactly() {
    assert_eq!(
        watch::to_server_event(EngineEvent::GenerationStarted),
        ServerEvent::GenerationStarted
    );
    assert_eq!(
        watch::to_server_event(EngineEvent::GenerationSucceeded { partial: false }),
        ServerEvent::GenerationSucceeded { partial: false }
    );
    assert_eq!(
        watch::to_server_event(EngineEvent::GenerationSucceeded { partial: true }),
        ServerEvent::GenerationSucceeded { partial: true }
    );
    assert_eq!(
        watch::to_server_event(EngineEvent::GenerationFailed {
            code: "watch_generation_failed".into(),
            message: "generation failed; see the terminal for details".into(),
        }),
        ServerEvent::GenerationFailed {
            code: "watch_generation_failed".into(),
            message: "generation failed; see the terminal for details".into(),
        }
    );
}

#[test]
fn the_conversion_adds_nothing_of_its_own() {
    let event = watch::to_server_event(EngineEvent::GenerationFailed {
        code: "c".into(),
        message: "m".into(),
    });
    match event {
        ServerEvent::GenerationFailed { code, message } => {
            assert_eq!(code, "c");
            assert_eq!(message, "m");
        }
        other => panic!("wrong variant: {other:?}"),
    }
}

// --- recovery coverage ----------------------------------------------------

#[tokio::test]
async fn a_recovery_build_failure_retains_the_current_plan_and_stops() {
    // The root manifest itself is unreadable: there is nothing safe to build. The
    // current plan stands — and it already watches the root `Cargo.toml`, which is
    // exactly what observes its repair.
    let harness = harness(FailAt::BuildRecovery, false);
    let error = harness
        .transaction
        .regenerate(request())
        .await
        .expect_err("must fail");

    assert_eq!(
        *harness.calls.lock().unwrap(),
        ["build_recovery"],
        "no replacement, no complete build, no generation, no commit"
    );
    assert!(!error.message.is_empty());
    assert!(harness.committed.lock().unwrap().is_empty());
    assert!(harness.covers_old_input(), "the current plan is retained");
    assert!(!harness.covers_member(), "nothing was activated");
}

#[tokio::test]
async fn a_recovery_replacement_failure_retains_the_previous_plan_and_stops() {
    let harness = harness(FailAt::ReplaceRecovery, false);
    let error = harness
        .transaction
        .regenerate(request())
        .await
        .expect_err("must fail");

    assert_eq!(
        error.code,
        cratevista_core::watch::code::PLAN_REPLACE_FAILED
    );
    assert_eq!(
        *harness.calls.lock().unwrap(),
        ["build_recovery", "replace_recovery"],
        "stop before the complete build and before generation"
    );
    assert!(harness.committed.lock().unwrap().is_empty());
    assert!(harness.covers_old_input(), "the previous plan is retained");
}

#[tokio::test]
async fn recovery_coverage_never_narrows_what_is_already_watched() {
    // The superset rule: an existing source root must survive recovery, and must
    // still be covered after a complete-plan failure.
    let harness = harness(FailAt::BuildPlan, false);
    assert!(harness.covers_old_input(), "precondition");

    harness
        .transaction
        .regenerate(request())
        .await
        .expect_err("the complete plan fails");

    assert!(
        harness.covers_old_input(),
        "recovery added the member without removing the existing source root"
    );
    assert!(harness.covers_member(), "and it added the new member");
}

#[tokio::test]
async fn a_failed_complete_build_leaves_the_new_member_observable() {
    // The scenario the whole phase exists for: metadata cannot run because the new
    // member's manifest is broken, and the fix must still be seen.
    let harness = harness(FailAt::BuildPlan, false);
    assert!(!harness.covers_member(), "precondition: not watched yet");

    harness
        .transaction
        .regenerate(request())
        .await
        .expect_err("metadata fails");

    assert!(
        harness.committed.lock().unwrap().is_empty(),
        "old snapshot stands"
    );
    assert!(
        harness.covers_member(),
        "without this the user fixes the manifest and nothing happens"
    );
}

// --- active plan ownership, at every failure prefix -------------------------
//
// The tests above prove the stage *order*, and probe coverage one path at a time.
// These prove the stronger claim the order exists to support: exactly which plan
// is active after every prefix of the transaction, by comparing the active plan's
// whole logical input set — and that core's retained copy moves only when a
// `replace_plan` actually succeeded.

/// 1. Recovery build failure: nothing was activated, and nothing ran.
#[tokio::test]
async fn a_recovery_build_failure_leaves_the_previous_inputs_active_and_replaces_nothing() {
    let harness = harness(FailAt::BuildRecovery, false);
    harness
        .transaction
        .regenerate(request())
        .await
        .expect_err("must fail");

    assert_eq!(
        harness.active_inputs(),
        labels(&previous_core().inputs),
        "with no safe plan to build, the previous coverage is what stands"
    );
    assert_eq!(
        *harness.replacements.lock().unwrap(),
        0,
        "the native watcher was never asked to accept anything"
    );
    for stage in ["build_plan", "generate", "load", "commit"] {
        assert!(
            !harness.calls.lock().unwrap().contains(&stage),
            "`{stage}` must not run once recovery could not be built"
        );
    }
}

/// 2. Recovery replacement failure: the retained copy did not run ahead.
#[tokio::test]
async fn a_recovery_replacement_failure_leaves_the_previous_inputs_active() {
    let harness = harness(FailAt::ReplaceRecovery, false);
    harness
        .transaction
        .regenerate(request())
        .await
        .expect_err("must fail");

    assert_eq!(
        harness.active_inputs(),
        labels(&previous_core().inputs),
        "a plan the watcher refused is not coverage, whatever core built"
    );
    assert_ne!(
        harness.active_inputs(),
        labels(&recovery_core().inputs),
        "the retained plan must not be updated before the replacement succeeds"
    );
}

/// 3. Complete build failure: recovery coverage is what is active.
#[tokio::test]
async fn a_complete_build_failure_leaves_the_recovery_inputs_active() {
    let harness = harness(FailAt::BuildPlan, false);
    harness
        .transaction
        .regenerate(request())
        .await
        .expect_err("must fail");

    assert_eq!(
        harness.active_inputs(),
        labels(&recovery_core().inputs),
        "metadata failed, so recovery coverage — which needs no metadata — is what \
         observes the fix"
    );
}

/// 4. Complete replacement failure: recovery stands; the retained copy is not
///    quietly promoted to the plan the watcher rejected.
#[tokio::test]
async fn a_complete_replacement_failure_leaves_the_recovery_inputs_active() {
    let harness = harness(FailAt::ReplacePlan, false);
    harness
        .transaction
        .regenerate(request())
        .await
        .expect_err("must fail");

    assert_eq!(
        harness.active_inputs(),
        labels(&recovery_core().inputs),
        "never the older, narrower plan"
    );
    assert_ne!(
        harness.active_inputs(),
        labels(&complete_core().inputs),
        "the retained plan must not become the complete one the watcher refused"
    );
    assert_eq!(
        *harness.replacements.lock().unwrap(),
        2,
        "both replacements were attempted; only the first was accepted"
    );
}

/// 5. Generation failure: complete coverage stays, and nothing was published.
#[tokio::test]
async fn a_generation_failure_leaves_the_complete_inputs_active_and_the_snapshot_untouched() {
    let harness = harness(FailAt::Generate, false);
    harness
        .transaction
        .regenerate(request())
        .await
        .expect_err("must fail");

    assert_eq!(harness.active_inputs(), labels(&complete_core().inputs));
    assert!(
        harness.committed.lock().unwrap().is_empty(),
        "coverage may lead; publication may not move at all"
    );
}

/// 6. Load failure: identical ownership, identical publication.
#[tokio::test]
async fn a_load_failure_leaves_the_complete_inputs_active_and_the_snapshot_untouched() {
    let harness = harness(FailAt::Load, false);
    harness
        .transaction
        .regenerate(request())
        .await
        .expect_err("must fail");

    assert_eq!(harness.active_inputs(), labels(&complete_core().inputs));
    assert!(harness.committed.lock().unwrap().is_empty());
}

/// 7. Success: complete coverage, committed exactly once, last.
#[tokio::test]
async fn a_success_leaves_the_complete_inputs_active_and_commits_exactly_once_last() {
    let harness = harness(FailAt::Nothing, false);
    harness.transaction.regenerate(request()).await.expect("ok");

    assert_eq!(harness.active_inputs(), labels(&complete_core().inputs));
    assert_eq!(harness.committed.lock().unwrap().len(), 1);
    assert_eq!(harness.calls.lock().unwrap().last(), Some(&"commit"));
    assert!(
        harness
            .active_inputs()
            .iter()
            .any(|label| label.starts_with("WorkspaceMemberManifestPattern")),
        "the complete plan keeps the declared pattern: metadata knows only the \
         members that exist now, so dropping it would stop covering `crates/*` the \
         moment it succeeded"
    );
}

/// 8. Partial success: same ownership, and `partial` is reported only after the
///    commit — a caller told `partial: true` can rely on it already being served.
#[tokio::test]
async fn a_partial_success_has_the_same_ownership_and_reports_partial_only_after_the_commit() {
    let harness = harness(FailAt::Nothing, true);
    let outcome = harness
        .transaction
        .regenerate(request())
        .await
        .expect("partial but valid is still a success");

    assert_eq!(
        outcome,
        cratevista_watch::RegenerationSuccess { partial: true }
    );
    assert_eq!(harness.active_inputs(), labels(&complete_core().inputs));
    assert_eq!(harness.committed.lock().unwrap().len(), 1);
    assert_eq!(
        harness.calls.lock().unwrap().last(),
        Some(&"commit"),
        "the commit is the last thing that happens, so `partial: true` is never \
         returned about a snapshot nobody is serving yet"
    );
}

/// The three modelled plans really are distinct.
///
/// Without this, every assertion above could pass against one plan wearing three
/// names.
#[test]
fn the_modelled_plans_have_distinct_logical_inputs() {
    let previous = labels(&previous_core().inputs);
    let recovery = labels(&recovery_core().inputs);
    let complete = labels(&complete_core().inputs);

    assert_ne!(previous, recovery);
    assert_ne!(recovery, complete);
    assert_ne!(previous, complete);

    // And recovery really is a superset of what was active, which is the rule it
    // exists to satisfy.
    for input in &previous {
        assert!(
            recovery.contains(input),
            "recovery narrowed coverage by dropping {input}"
        );
    }
}
