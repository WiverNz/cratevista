//! The transaction driven by the **real** engine.
//!
//! The transaction tests prove one run's stage order; these prove what the engine
//! does with it across runs — that expanded coverage from a *failed* run leads to
//! a second run, and that an event arriving during generation is not lost.
//!
//! Channels and barriers only: no cargo, no rustdoc, no watcher, no sleeps.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use cratevista_core::watch::{Stages, Transaction};
use cratevista_watch::{
    EngineEvent, Regenerate, RegenerationFailure, RegenerationRequest, RegenerationResult,
    RegistrationMode, WatchInput, WatchPlan, WatchRegistration, WatchSet,
};
use tokio::sync::{mpsc, oneshot};

fn plan_over(trees: &[&str]) -> WatchPlan {
    let root = Path::new("/w");
    WatchPlan::new(
        WatchSet::new(
            root,
            trees
                .iter()
                .map(|tree| WatchInput::rust_root(format!("/w/{tree}"))),
        ),
        trees.iter().map(|tree| WatchRegistration {
            path: PathBuf::from(format!("/w/{tree}")),
            mode: RegistrationMode::Recursive,
        }),
    )
    .expect("a valid plan")
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct FakeSnapshot;

/// Stages whose first run activates expanded coverage and then fails, and whose
/// later runs succeed. Generation can be paused by the test.
struct ScriptedStages {
    runs: AtomicUsize,
    active_plan: Arc<Mutex<WatchPlan>>,
    committed: Arc<Mutex<usize>>,
    /// Concurrency proof.
    live: Arc<AtomicUsize>,
    max_live: Arc<AtomicUsize>,
    /// When set, `generate` announces itself here and waits to be released.
    gate: Mutex<Option<mpsc::UnboundedSender<oneshot::Sender<()>>>>,
}

impl Stages for ScriptedStages {
    type Snapshot = FakeSnapshot;

    fn generate(&self) -> Result<(), RegenerationFailure> {
        let run = self.runs.fetch_add(1, Ordering::SeqCst) + 1;
        let now = self.live.fetch_add(1, Ordering::SeqCst) + 1;
        self.max_live.fetch_max(now, Ordering::SeqCst);

        // Pause here if the test asked to hold a run open.
        let gate = self.gate.lock().unwrap().clone();
        if let Some(gate) = gate {
            let (release, wait) = oneshot::channel();
            if gate.send(release).is_ok() {
                // Blocking on purpose: this stage already runs on a blocking
                // thread, which is exactly where cargo would be.
                let _ = wait.blocking_recv();
            }
        }

        self.live.fetch_sub(1, Ordering::SeqCst);
        if run == 1 {
            // The first run introduced `new/` — its plan is already active — and
            // then failed to compile.
            return Err(RegenerationFailure::new(
                cratevista_core::watch::code::GENERATION_FAILED,
                "generation failed; see the terminal for details",
            ));
        }
        Ok(())
    }

    fn load(&self) -> Result<Self::Snapshot, RegenerationFailure> {
        Ok(FakeSnapshot)
    }

    fn partial(_snapshot: &Self::Snapshot) -> bool {
        false
    }

    fn build_recovery_plan(&self) -> Result<WatchPlan, RegenerationFailure> {
        // Recovery already covers the new member's tree, from the root manifest
        // alone — which is what makes a fix observable when metadata cannot run.
        Ok(plan_over(&["new", "old"]))
    }

    fn build_plan(&self) -> Result<WatchPlan, RegenerationFailure> {
        // The workspace now has a `new/` member.
        Ok(plan_over(&["new", "old"]))
    }

    fn replace_plan(
        &self,
        plan: WatchPlan,
    ) -> std::pin::Pin<
        Box<dyn std::future::Future<Output = Result<(), RegenerationFailure>> + Send + '_>,
    > {
        Box::pin(async move {
            *self.active_plan.lock().unwrap() = plan;
            Ok(())
        })
    }

    fn commit(&self, _snapshot: Self::Snapshot) {
        *self.committed.lock().unwrap() += 1;
    }
}

struct Harness {
    engine: cratevista_watch::Engine,
    handle: cratevista_watch::EngineHandle,
    events: mpsc::UnboundedReceiver<EngineEvent>,
    active_plan: Arc<Mutex<WatchPlan>>,
    committed: Arc<Mutex<usize>>,
    max_live: Arc<AtomicUsize>,
    /// Receives a release handle each time `generate` pauses.
    paused: Option<mpsc::UnboundedReceiver<oneshot::Sender<()>>>,
}

fn start(pause_generation: bool) -> Harness {
    let (events_tx, events) = mpsc::unbounded_channel();
    let active_plan = Arc::new(Mutex::new(plan_over(&["old"])));
    let committed = Arc::new(Mutex::new(0));
    let max_live = Arc::new(AtomicUsize::new(0));

    let (gate, paused) = if pause_generation {
        let (tx, rx) = mpsc::unbounded_channel();
        (Some(tx), Some(rx))
    } else {
        (None, None)
    };

    let stages = ScriptedStages {
        runs: AtomicUsize::new(0),
        active_plan: active_plan.clone(),
        committed: committed.clone(),
        live: Arc::new(AtomicUsize::new(0)),
        max_live: max_live.clone(),
        gate: Mutex::new(gate),
    };
    let engine = cratevista_watch::spawn(Transaction::new(stages), events_tx);
    let handle = engine.handle();
    Harness {
        engine,
        handle,
        events,
        active_plan,
        committed,
        max_live,
        paused,
    }
}

/// A stand-in so the real engine can be moved out of the harness for `join`,
/// which consumes it, while the harness's other fields stay usable.
fn placeholder_engine() -> cratevista_watch::Engine {
    let (events, _) = mpsc::unbounded_channel();
    let stages = ScriptedStages {
        runs: AtomicUsize::new(0),
        active_plan: Arc::new(Mutex::new(plan_over(&["old"]))),
        committed: Arc::new(Mutex::new(0)),
        live: Arc::new(AtomicUsize::new(0)),
        max_live: Arc::new(AtomicUsize::new(0)),
        gate: Mutex::new(None),
    };
    let engine = cratevista_watch::spawn(Transaction::new(stages), events);
    let _ = engine.handle().shutdown();
    engine
}

async fn within<T>(what: &str, future: impl std::future::Future<Output = T>) -> T {
    match tokio::time::timeout(std::time::Duration::from_secs(10), future).await {
        Ok(value) => value,
        Err(_) => panic!("timed out waiting for {what}"),
    }
}

fn request(path: &str) -> RegenerationRequest {
    RegenerationRequest::new([PathBuf::from(path)]).expect("non-empty")
}

impl Harness {
    fn covers(&self, path: &str) -> bool {
        self.active_plan
            .lock()
            .unwrap()
            .watch_set()
            .is_relevant(Path::new(path))
    }

    async fn drain(&mut self) -> Vec<EngineEvent> {
        let mut collected = Vec::new();
        while let Some(event) = within("the event stream to close", self.events.recv()).await {
            collected.push(event);
        }
        collected
    }
}

/// A failed first run must still leave its new input observable, and an edit to
/// that input must produce a second run.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_failed_run_expands_coverage_and_a_new_only_edit_starts_exactly_one_more_run() {
    let mut harness = start(false);
    assert!(
        !harness.covers("/w/new/src/lib.rs"),
        "precondition: the new input is not watched yet"
    );

    // Run 1: activates `new/`, then fails.
    harness.handle.submit(request("/w/old/src/lib.rs")).unwrap();
    assert_eq!(
        within("Started", harness.events.recv()).await,
        Some(EngineEvent::GenerationStarted)
    );
    let failed = within("Failed", harness.events.recv()).await;
    assert!(
        matches!(failed, Some(EngineEvent::GenerationFailed { .. })),
        "the first run fails: {failed:?}"
    );
    assert_eq!(*harness.committed.lock().unwrap(), 0, "nothing published");
    assert!(
        harness.covers("/w/new/src/lib.rs"),
        "coverage stays after a failed run — this is what makes the fix reachable"
    );

    // The user fixes the new member. Under the OLD order this edit would have been
    // unwatched, so this request could never have existed.
    harness.handle.submit(request("/w/new/src/lib.rs")).unwrap();
    assert_eq!(
        within("the second Started", harness.events.recv()).await,
        Some(EngineEvent::GenerationStarted)
    );
    assert_eq!(
        within("the second terminal", harness.events.recv()).await,
        Some(EngineEvent::GenerationSucceeded { partial: false }),
        "the fix is picked up and published"
    );
    assert_eq!(*harness.committed.lock().unwrap(), 1);

    harness.handle.shutdown().unwrap();
    let engine = std::mem::replace(&mut harness.engine, placeholder_engine());
    within("join", engine.join()).await.unwrap();

    assert_eq!(
        harness.max_live.load(Ordering::SeqCst),
        1,
        "single-flight holds across a failure and its recovery"
    );
    assert_eq!(harness.drain().await, [], "exactly two runs, four events");
}

/// The event order across a failure and its recovery.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_event_order_across_a_failed_run_and_its_recovery_is_started_failed_started_terminal() {
    let mut harness = start(false);
    harness.handle.submit(request("/w/old/src/lib.rs")).unwrap();
    // Wait for the first run to finish before submitting the fix, so the two runs
    // are sequential rather than coalesced.
    within("Started", harness.events.recv()).await;
    within("Failed", harness.events.recv()).await;
    harness.handle.submit(request("/w/new/src/lib.rs")).unwrap();
    within("Started", harness.events.recv()).await;
    within("terminal", harness.events.recv()).await;

    harness.handle.shutdown().unwrap();
    let engine = std::mem::replace(&mut harness.engine, placeholder_engine());
    within("join", engine.join()).await.unwrap();

    // Nothing else was emitted; the four observed above were exactly:
    assert_eq!(harness.drain().await, []);
    assert_eq!(*harness.committed.lock().unwrap(), 1);
}

/// An event for a path only the *candidate* plan covers, arriving while the first
/// run is still generating, must become exactly one dirty follow-up.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn an_event_during_generation_becomes_exactly_one_follow_up() {
    let mut harness = start(true);
    let mut paused = harness.paused.take().expect("paused generation");

    // 1 + 2. The candidate plan is activated, then generation starts and pauses.
    harness.handle.submit(request("/w/old/src/lib.rs")).unwrap();
    let release = within("generation to pause", paused.recv())
        .await
        .expect("a release handle");

    // The plan was activated *before* generation began — the whole point of the
    // order. So the path below is already watched while the run is in flight.
    assert!(
        harness.covers("/w/new/src/lib.rs"),
        "coverage is live before generation, so an edit during it is observable"
    );

    // 3 + 4. An event for a candidate-only path reaches the engine mid-run.
    harness.handle.submit(request("/w/new/src/lib.rs")).unwrap();

    // 5. Complete the first run.
    release.send(()).expect("generation is waiting");

    // 6. Exactly one follow-up: Started/terminal twice, no third run.
    within("the first Started", harness.events.recv()).await;
    within("the first terminal", harness.events.recv()).await;
    let second_start = within("the follow-up Started", harness.events.recv()).await;
    assert_eq!(second_start, Some(EngineEvent::GenerationStarted));

    // Release the follow-up too.
    let release = within("the follow-up to pause", paused.recv())
        .await
        .expect("a release handle");
    release.send(()).expect("generation is waiting");
    within("the follow-up terminal", harness.events.recv()).await;

    harness.handle.shutdown().unwrap();
    let engine = std::mem::replace(&mut harness.engine, placeholder_engine());
    within("join", engine.join()).await.unwrap();
    assert_eq!(
        harness.drain().await,
        [],
        "exactly one follow-up, not two runs"
    );
    assert_eq!(harness.max_live.load(Ordering::SeqCst), 1);
}

// --- a fix arriving while the complete plan is still being built ------------

/// A plan over named source roots plus declared member patterns.
///
/// The pattern is what makes a member that does not exist yet classifiable, so it
/// is the whole reason recovery coverage can see the fix below.
fn plan_with(trees: &[&str], patterns: &[&str]) -> WatchPlan {
    let root = Path::new("/w");

    let mut inputs: Vec<WatchInput> = trees
        .iter()
        .map(|tree| WatchInput::rust_root(format!("/w/{tree}")))
        .collect();
    inputs.extend(
        patterns
            .iter()
            .map(|pattern| WatchInput::workspace_member_pattern(format!("/w/{pattern}"), [])),
    );

    let mut registrations: Vec<WatchRegistration> = trees
        .iter()
        .map(|tree| WatchRegistration::recursive(format!("/w/{tree}")))
        .collect();
    // A pattern is registered by its static prefix, recursively: `crates/*` -> `crates`.
    registrations.extend(patterns.iter().map(|pattern| {
        WatchRegistration::recursive(format!(
            "/w/{}",
            cratevista_watch::pattern::static_prefix(pattern)
        ))
    }));

    WatchPlan::new(WatchSet::new(root, inputs), registrations).expect("a valid plan")
}

/// Stages whose **complete-plan construction** can be held open by the test.
///
/// That is the window this test exists for: recovery coverage is already active,
/// `cargo metadata` is still running, and the user repairs the very manifest that
/// made it fail.
struct BarrierStages {
    runs: AtomicUsize,
    plan_builds: AtomicUsize,
    active_plan: Arc<Mutex<WatchPlan>>,
    committed: Arc<Mutex<usize>>,
    /// The first `build_plan` announces itself here and waits to be released.
    barrier: mpsc::UnboundedSender<oneshot::Sender<()>>,
}

impl Stages for BarrierStages {
    type Snapshot = FakeSnapshot;

    fn build_recovery_plan(&self) -> Result<WatchPlan, RegenerationFailure> {
        // From the root manifest alone: it declares `crates/*`, so a member created
        // later is covered without cargo having run at all.
        Ok(plan_with(&["old"], &["crates/*"]))
    }

    fn build_plan(&self) -> Result<WatchPlan, RegenerationFailure> {
        // Only the first construction pauses; the follow-up run must not block.
        if self.plan_builds.fetch_add(1, Ordering::SeqCst) == 0 {
            let (release, wait) = oneshot::channel();
            if self.barrier.send(release).is_ok() {
                // Blocking on purpose: this stage runs on a blocking thread, which
                // is exactly where `cargo metadata` would be.
                let _ = wait.blocking_recv();
            }
        }
        Ok(plan_with(&["old", "new"], &["crates/*"]))
    }

    fn generate(&self) -> Result<(), RegenerationFailure> {
        let run = self.runs.fetch_add(1, Ordering::SeqCst) + 1;
        if run == 1 {
            return Err(RegenerationFailure::new(
                cratevista_core::watch::code::GENERATION_FAILED,
                "generation failed; see the terminal for details",
            ));
        }
        Ok(())
    }

    fn load(&self) -> Result<Self::Snapshot, RegenerationFailure> {
        Ok(FakeSnapshot)
    }

    fn partial(_snapshot: &Self::Snapshot) -> bool {
        false
    }

    fn replace_plan(
        &self,
        plan: WatchPlan,
    ) -> std::pin::Pin<
        Box<dyn std::future::Future<Output = Result<(), RegenerationFailure>> + Send + '_>,
    > {
        Box::pin(async move {
            *self.active_plan.lock().unwrap() = plan;
            Ok(())
        })
    }

    fn commit(&self, _snapshot: Self::Snapshot) {
        *self.committed.lock().unwrap() += 1;
    }
}

/// Wraps the transaction to record what each run was asked to regenerate, and to
/// measure concurrency across the **whole** regeneration rather than one stage.
///
/// Measuring here is what makes the concurrency claim mean something: run 1 stays
/// live for as long as the barrier holds `build_plan`, so a second run starting
/// while the test is paused would be observed as `max_live == 2`.
struct Recording {
    inner: Transaction<BarrierStages>,
    requests: Arc<Mutex<Vec<Vec<PathBuf>>>>,
    live: Arc<AtomicUsize>,
    max_live: Arc<AtomicUsize>,
}

impl Regenerate for Recording {
    fn regenerate(
        &self,
        request: RegenerationRequest,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = RegenerationResult> + Send + '_>> {
        self.requests.lock().unwrap().push(request.paths().to_vec());
        let now = self.live.fetch_add(1, Ordering::SeqCst) + 1;
        self.max_live.fetch_max(now, Ordering::SeqCst);

        let inner = self.inner.regenerate(request);
        let live = self.live.clone();
        Box::pin(async move {
            let result = inner.await;
            live.fetch_sub(1, Ordering::SeqCst);
            result
        })
    }
}

/// The fix lands *while the complete plan is still being built*, and is observed.
///
/// This is the interleaving the whole recovery phase exists for. Recovery coverage
/// is active before `cargo metadata` is even attempted, so the repair to the
/// manifest that made metadata fail is classified, submitted mid-run, and merged
/// into exactly one follow-up.
///
/// Channels and barriers only: no sleep decides anything here.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_fix_during_complete_plan_construction_is_observed_and_becomes_one_follow_up() {
    let (barrier, mut paused) = mpsc::unbounded_channel();
    let active_plan = Arc::new(Mutex::new(plan_over(&["old"])));
    let committed = Arc::new(Mutex::new(0));
    let requests = Arc::new(Mutex::new(Vec::new()));
    let max_live = Arc::new(AtomicUsize::new(0));

    let new_member = Path::new("/w/crates/new/Cargo.toml");

    // 1. The previous active plan does not cover the new member's manifest.
    assert!(
        !active_plan
            .lock()
            .unwrap()
            .watch_set()
            .is_relevant(new_member),
        "precondition: `crates/new/Cargo.toml` is not watched yet"
    );

    let operation = Recording {
        inner: Transaction::new(BarrierStages {
            runs: AtomicUsize::new(0),
            plan_builds: AtomicUsize::new(0),
            active_plan: active_plan.clone(),
            committed: committed.clone(),
            barrier,
        }),
        requests: requests.clone(),
        live: Arc::new(AtomicUsize::new(0)),
        max_live: max_live.clone(),
    };

    let (events_tx, mut events) = mpsc::unbounded_channel();
    let engine = cratevista_watch::spawn(operation, events_tx);
    let handle = engine.handle();

    // 2 + 3. Run 1 begins; recovery coverage is activated; complete-plan
    //        construction then pauses and stays paused until this test releases it.
    handle.submit(request("/w/old/src/lib.rs")).unwrap();
    assert_eq!(
        within("the first Started", events.recv()).await,
        Some(EngineEvent::GenerationStarted)
    );
    let release = within("complete-plan construction to pause", paused.recv())
        .await
        .expect("a release handle");

    // 5. The event is classified through the *currently active* plan, which is
    //    recovery coverage, built from the root manifest alone. Under the old order
    //    this would still be the previous plan and the fix would be invisible.
    assert_eq!(
        active_plan.lock().unwrap().watch_set().classify(new_member),
        cratevista_watch::Classification::Relevant(
            cratevista_watch::InputKind::WorkspaceMemberManifestPattern
        ),
        "recovery coverage is what classifies the fix, via the declared `crates/*`"
    );

    // 4 + 6. The user repairs the manifest and touches an existing file, and both
    //        are submitted while run 1 is provably still in flight: `build_plan` is
    //        blocked on the barrier this test still holds.
    handle.submit(request("/w/crates/new/Cargo.toml")).unwrap();
    handle.submit(request("/w/old/src/lib.rs")).unwrap();

    // 7. Release complete-plan construction.
    release.send(()).expect("build_plan is waiting");

    // 8. Run 1 reaches its terminal event: it fails, and publishes nothing.
    let terminal = within("the first terminal", events.recv()).await;
    assert!(
        matches!(terminal, Some(EngineEvent::GenerationFailed { .. })),
        "the first run fails: {terminal:?}"
    );
    assert_eq!(*committed.lock().unwrap(), 0, "nothing was published");

    // 9 + 10. Exactly one dirty follow-up, and the event order is
    //         Started -> terminal -> Started -> terminal.
    assert_eq!(
        within("the follow-up Started", events.recv()).await,
        Some(EngineEvent::GenerationStarted)
    );
    assert_eq!(
        within("the follow-up terminal", events.recv()).await,
        Some(EngineEvent::GenerationSucceeded { partial: false }),
        "the fix is picked up and published"
    );

    handle.shutdown().unwrap();
    within("join", engine.join()).await.unwrap();

    let mut drained = Vec::new();
    while let Some(event) = within("the event stream to close", events.recv()).await {
        drained.push(event);
    }
    assert_eq!(
        drained,
        [],
        "exactly two runs and four events, no third run"
    );
    assert_eq!(*committed.lock().unwrap(), 1);

    // 11. Single-flight held across the whole interleaving.
    assert_eq!(
        max_live.load(Ordering::SeqCst),
        1,
        "a submission during a paused run must never start a second regeneration"
    );

    // 12. The two mid-run submissions merged into one follow-up request, and the
    //     new member path is in it.
    let requests = requests.lock().unwrap();
    assert_eq!(requests.len(), 2, "two runs: {requests:?}");
    assert_eq!(
        requests[1],
        [
            PathBuf::from("/w/crates/new/Cargo.toml"),
            PathBuf::from("/w/old/src/lib.rs"),
        ],
        "the follow-up is the merged, sorted set of everything that changed mid-run"
    );
}
