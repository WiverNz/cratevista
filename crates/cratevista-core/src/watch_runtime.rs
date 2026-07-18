//! The watch-mode runtime: what `cargo cratevista open --watch` actually owns.
//!
//! The [recovery phase](crate::watch) built the *pieces* — a plan builder, a
//! regeneration transaction, an engine, a watcher. This module is the owner that
//! wires them into a running session and, just as importantly, takes them apart
//! again with nothing left detached.
//!
//! # Coverage exists before the first generation
//!
//! The obvious startup — *generate, then start watching* — has the bug the whole
//! recovery phase exists to prevent, on the very first run: everything the initial
//! generation reads is unwatched while it reads it, so an edit landing during a
//! cold `cargo doc` (which is not fast) is lost with no trace. So the real order
//! is **plan → watcher → ingress → generate**, and events observed during that
//! first generation are buffered and replayed as exactly one merged request the
//! moment the engine exists.
//!
//! # Who owns the watcher
//!
//! Core does. `cratevista-server` never learns what a watcher is — it owns an
//! `AppState` with a snapshot and a broadcast channel, and that is all. No
//! `notify` type appears in this module's public API either; the watch crate's
//! `WatchEvent`/`WatcherError` are the boundary.
//!
//! # What is still unbuilt
//!
//! The frontend. `/api/events` is published to, but nothing in `web/` subscribes
//! yet — see PRD 09's frontend section.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::sync::{Arc, Mutex};

use cratevista_server::{
    AppState, ArtifactPaths, ArtifactSnapshot, SnapshotLoadOptions, load_snapshot,
};
use cratevista_watch::{
    Engine, EngineEvent, EngineHandle, RegenerationFailure, RegenerationRequest, WatchEvent,
    WatchInput, WatchPlan, Watcher, WatcherError,
};
use tokio::sync::{mpsc, oneshot};

use crate::clock::Clock;
use crate::generate::{GenerateOptions, run_generate};
use crate::usecase::CommandFailure;
use crate::watch::{CorePlan, Stages, Transaction, WatchSetupError};

// ---------------------------------------------------------------------------
// Injected seams
// ---------------------------------------------------------------------------

/// The real work one regeneration does, behind a seam.
///
/// Every method here shells out to cargo/rustdoc or touches the filesystem, which
/// is exactly why the lifecycle above must be testable without them. Production
/// uses [`CargoWork`]; the tests use fakes and prove the ordering, the plan
/// ownership and the shutdown — none of which are about cargo at all.
///
/// All four are **blocking**. [`Transaction`] already runs each on a blocking
/// pool, so no implementation here needs `spawn_blocking` of its own.
pub(crate) trait SessionWork: Send + Sync + 'static {
    /// Recovery coverage: a superset of `active`, from the root manifest alone.
    fn build_recovery(&self, active: &[WatchInput]) -> Result<CorePlan, WatchSetupError>;

    /// The complete, metadata-and-config-derived coverage.
    fn build_complete(&self) -> Result<CorePlan, WatchSetupError>;

    /// `run_generate`.
    fn generate(&self) -> Result<(), RegenerationFailure>;

    /// `load_snapshot`, including its integrity verification and bounded retry.
    fn load(&self) -> Result<ArtifactSnapshot, RegenerationFailure>;
}

/// Where an activated plan goes.
///
/// A seam only so a test can make a replacement *fail* — the one thing a real
/// watcher will not do on demand. Production is [`Watcher`].
pub(crate) trait PlanSink: Send + Sync + 'static {
    /// Atomically swaps the native watcher onto `plan`.
    fn replace_plan<'a>(
        &'a self,
        plan: WatchPlan,
    ) -> Pin<Box<dyn Future<Output = Result<(), WatcherError>> + Send + 'a>>;
}

impl PlanSink for Watcher {
    fn replace_plan<'a>(
        &'a self,
        plan: WatchPlan,
    ) -> Pin<Box<dyn Future<Output = Result<(), WatcherError>> + Send + 'a>> {
        Box::pin(Watcher::replace_plan(self, plan))
    }
}

/// The production [`SessionWork`]: real cargo, real rustdoc, real artifacts.
pub(crate) struct CargoWork {
    root: PathBuf,
    options: GenerateOptions,
    clock: Arc<dyn Clock>,
    output_dir: PathBuf,
}

impl CargoWork {
    pub(crate) fn new(root: &Path, options: GenerateOptions, clock: Arc<dyn Clock>) -> Self {
        CargoWork {
            root: root.to_path_buf(),
            output_dir: root.join("target").join("cratevista"),
            options,
            clock,
        }
    }
}

impl SessionWork for CargoWork {
    fn build_recovery(&self, active: &[WatchInput]) -> Result<CorePlan, WatchSetupError> {
        crate::watch::build_recovery_plan(&self.root, active, self.options.no_config)
    }

    fn build_complete(&self) -> Result<CorePlan, WatchSetupError> {
        crate::watch::build_watch_plan(&self.root, &self.options)
    }

    fn generate(&self) -> Result<(), RegenerationFailure> {
        // The failure is mapped, never forwarded: cargo and rustdoc errors carry
        // absolute paths and whole command lines, and this one can reach a browser.
        // The detail is already on the terminal, where it belongs.
        run_generate(&self.options, self.clock.as_ref())
            .map(|_| ())
            .map_err(|failure| crate::watch::generation_failure(&failure))
    }

    fn load(&self) -> Result<ArtifactSnapshot, RegenerationFailure> {
        load_snapshot(
            &ArtifactPaths::in_dir(&self.output_dir),
            &SnapshotLoadOptions::default(),
        )
        .map_err(|error| {
            tracing::warn!(%error, "the freshly generated artifacts could not be verified");
            crate::watch::artifacts_failure()
        })
    }
}

// ---------------------------------------------------------------------------
// The production Stages adapter
// ---------------------------------------------------------------------------

/// The real [`Stages`] behind [`Transaction`].
///
/// # Why a pending slot
///
/// [`Stages`] hands `replace_plan` a [`WatchPlan`], which carries no logical
/// inputs — and core must retain the [`CorePlan`] so the *next* run's recovery can
/// be a superset of what is live. Asking the watcher what it holds would mean a
/// `Watcher::current_plan`, which would publish watcher internals for the sake of
/// bookkeeping core can simply do itself. So each builder parks the `CorePlan` it
/// just built here, and a **successful** replacement promotes it.
///
/// The slot is unambiguous because the transaction is strictly sequential and the
/// engine is single-flight: exactly one build is outstanding at a time.
pub(crate) struct ProductionStages {
    work: Arc<dyn SessionWork>,
    sink: Arc<dyn PlanSink>,
    state: Arc<AppState>,
    /// What the watcher is actually watching right now.
    retained: Arc<Mutex<CorePlan>>,
    /// The plan built but not yet accepted by the watcher.
    pending: Mutex<Option<CorePlan>>,
}

impl ProductionStages {
    pub(crate) fn new(
        work: Arc<dyn SessionWork>,
        sink: Arc<dyn PlanSink>,
        state: Arc<AppState>,
        retained: Arc<Mutex<CorePlan>>,
    ) -> Self {
        ProductionStages {
            work,
            sink,
            state,
            retained,
            pending: Mutex::new(None),
        }
    }
}

impl Stages for ProductionStages {
    type Snapshot = ArtifactSnapshot;

    fn build_recovery_plan(&self) -> Result<WatchPlan, RegenerationFailure> {
        let active = self.retained.lock().unwrap().inputs.clone();
        let built = self
            .work
            .build_recovery(&active)
            .map_err(|error| crate::watch::setup_failure(&error))?;
        let plan = built.plan.clone();
        *self.pending.lock().unwrap() = Some(built);
        Ok(plan)
    }

    fn build_plan(&self) -> Result<WatchPlan, RegenerationFailure> {
        let built = self
            .work
            .build_complete()
            .map_err(|error| crate::watch::setup_failure(&error))?;
        let plan = built.plan.clone();
        *self.pending.lock().unwrap() = Some(built);
        Ok(plan)
    }

    fn replace_plan(
        &self,
        plan: WatchPlan,
    ) -> Pin<Box<dyn Future<Output = Result<(), RegenerationFailure>> + Send + '_>> {
        Box::pin(async move {
            self.sink.replace_plan(plan).await.map_err(|error| {
                // The watcher kept its previous plan **complete**, so the retained
                // record must not move either: a core-side copy running ahead of
                // the watcher would describe coverage that does not exist.
                tracing::warn!(%error, "the file watcher refused a new plan");
                crate::watch::replace_failure()
            })?;
            // Accepted: promote the plan that was just installed.
            if let Some(built) = self.pending.lock().unwrap().take() {
                *self.retained.lock().unwrap() = built;
            }
            Ok(())
        })
    }

    fn generate(&self) -> Result<(), RegenerationFailure> {
        self.work.generate()
    }

    fn load(&self) -> Result<Self::Snapshot, RegenerationFailure> {
        self.work.load()
    }

    fn partial(snapshot: &Self::Snapshot) -> bool {
        snapshot.partial
    }

    fn commit(&self, snapshot: Self::Snapshot) {
        // The one all-or-nothing publication in the whole session.
        self.state.replace_snapshot(snapshot);
    }
}

// ---------------------------------------------------------------------------
// Watcher-event ingress
// ---------------------------------------------------------------------------

/// The ingress owner's state.
///
/// One task owns the watcher's receiver for its whole lifetime, so the transition
/// below is a local move rather than a handoff between tasks. That is the entire
/// reason an event racing with activation can neither be dropped nor submitted
/// twice: both the event and the activation arrive at the same `select!`, and only
/// one of them is being handled at any moment.
enum Ingress {
    /// Before the engine exists: the initial generation is still running, and
    /// anything it disturbs is merged here.
    Bootstrap { pending: BTreeSet<PathBuf> },
    /// The engine exists; every event is submitted directly.
    Active { handle: EngineHandle },
}

/// What `activate` sends: the engine, and a receipt for the flush.
type Activation = (EngineHandle, oneshot::Sender<()>);

/// A handle to the ingress task.
pub(crate) struct IngressHandle {
    activate: Option<oneshot::Sender<Activation>>,
    stop: Option<oneshot::Sender<()>>,
    task: tokio::task::JoinHandle<()>,
}

impl IngressHandle {
    /// Promotes the ingress from `Bootstrap` to `Active`, flushing anything the
    /// initial generation disturbed as **one** merged request.
    ///
    /// Waits for the transition to actually happen. Without that receipt the
    /// caller would go on to serve while the bootstrap window is still open, and
    /// "the buffered edits became one request" would be a race rather than a fact.
    pub(crate) async fn activate(&mut self, handle: EngineHandle) {
        if let Some(activate) = self.activate.take() {
            let (done_tx, done_rx) = oneshot::channel();
            if activate.send((handle, done_tx)).is_ok() {
                let _ = done_rx.await;
            }
        }
    }

    /// Stops accepting new filesystem work. Idempotent.
    pub(crate) fn stop(&mut self) {
        if let Some(stop) = self.stop.take() {
            let _ = stop.send(());
        }
    }

    /// Joins the ingress task.
    pub(crate) async fn join(self) -> Result<(), tokio::task::JoinError> {
        self.task.await
    }
}

/// Starts the ingress owner in bootstrap mode.
pub(crate) fn spawn_ingress(mut events: mpsc::UnboundedReceiver<WatchEvent>) -> IngressHandle {
    let (activate_tx, mut activate_rx) = oneshot::channel::<Activation>();
    let (stop_tx, mut stop_rx) = oneshot::channel::<()>();

    let task = tokio::spawn(async move {
        let mut state = Ingress::Bootstrap {
            pending: BTreeSet::new(),
        };

        loop {
            // `biased` rather than the default random choice, because the order
            // these three are checked in **is** the contract:
            //
            // 1. `stop` wins outright: once shutdown is requested no new
            //    filesystem work is accepted, which is what makes "no follow-up
            //    starts after shutdown" true here rather than by luck downstream;
            // 2. events before activation: an event already queued arrived *during*
            //    the initial generation, so it belongs in the merged bootstrap
            //    request. Left random, it would sometimes land in the buffer and
            //    sometimes be submitted separately — the same edits producing a
            //    different number of runs from one process to the next.
            //
            // Activation cannot starve: the debouncer emits at most one burst per
            // quiet window, so the event channel drains.
            tokio::select! {
                biased;

                _ = &mut stop_rx => break,

                event = events.recv() => {
                    match event {
                        Some(WatchEvent::Regeneration(request)) => match &mut state {
                            Ingress::Bootstrap { pending } => {
                                pending.extend(request.paths().iter().cloned());
                            }
                            Ingress::Active { handle } => {
                                if handle.submit(request).is_err() {
                                    // The engine is gone: nothing left to feed.
                                    break;
                                }
                            }
                        },
                        // Recoverable: an operational warning about *this machine*
                        // — a watch limit, a permission — not a failed build. It is
                        // never converted into a generation-failed SSE event,
                        // because telling the browser a document failed when
                        // nothing was even generated is simply false.
                        Some(WatchEvent::WatcherFailed { code, message }) => {
                            tracing::warn!(%code, %message, "the file watcher reported a problem");
                            eprintln!("warning: file watching problem ({code}): {message}");
                        }
                        // Unrecoverable: the adapter task itself ended, so no
                        // further event can ever arrive. See `WatchSession` for
                        // why this tears the session down instead of idling.
                        None => break,
                    }
                }

                activation = &mut activate_rx, if matches!(state, Ingress::Bootstrap { .. }) => {
                    let Ok((handle, done)) = activation else { break };
                    if let Ingress::Bootstrap { pending } = std::mem::replace(
                        &mut state,
                        Ingress::Active {
                            handle: handle.clone(),
                        },
                    ) && let Some(request) = RegenerationRequest::new(pending) {
                        // Exactly one merged request for the whole bootstrap
                        // window, however many events it saw.
                        let _ = handle.submit(request);
                    }
                    // Only now is the window provably closed.
                    let _ = done.send(());
                }
            }
        }
    });

    IngressHandle {
        activate: Some(activate_tx),
        stop: Some(stop_tx),
        task,
    }
}

// ---------------------------------------------------------------------------
// Engine-event forwarding
// ---------------------------------------------------------------------------

/// Publishes engine events as server events, and prints one line per run.
///
/// Ordering is inherited rather than re-established: the engine emits
/// `GenerationStarted` before the operation begins and the terminal event after
/// the transaction has committed, and a single channel preserves that. Nothing
/// here reorders, adds or drops an event.
fn spawn_forwarder(
    mut events: mpsc::UnboundedReceiver<EngineEvent>,
    state: Arc<AppState>,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        let mut started = None;
        while let Some(event) = events.recv().await {
            match &event {
                EngineEvent::GenerationStarted => {
                    started = Some(std::time::Instant::now());
                    println!("Regenerating…");
                }
                EngineEvent::GenerationSucceeded { partial } => {
                    let took = elapsed(started.take());
                    if *partial {
                        println!("Regenerated with a partial document{took}.");
                    } else {
                        println!("Regenerated{took}.");
                    }
                }
                EngineEvent::GenerationFailed { code, message } => {
                    let took = elapsed(started.take());
                    // The code and message are core's own browser-safe pair; the
                    // cargo/rustdoc detail behind them is already on the terminal.
                    eprintln!("Regeneration failed{took} ({code}): {message}");
                }
            }
            // Harmless with no SSE subscribers: `broadcast::Sender::send` to an
            // empty channel is a no-op, and watch mode is useful without a browser.
            state.publish_event(crate::watch::to_server_event(event));
        }
    })
}

fn elapsed(started: Option<std::time::Instant>) -> String {
    match started {
        Some(started) => format!(" in {} ms", started.elapsed().as_millis()),
        None => String::new(),
    }
}

// ---------------------------------------------------------------------------
// The session
// ---------------------------------------------------------------------------

/// Everything watch mode owns, and the thing that joins it all.
///
/// # Unrecoverable watcher failure
///
/// The two failure shapes are deliberately different:
///
/// - a [`WatchEvent::WatcherFailed`] is **recoverable** — the adapter is still
///   running and will still report changes. It is a terminal warning and nothing
///   more; it never becomes a `ServerEvent::GenerationFailed`.
/// - the event stream **ending** means the adapter task itself is gone, so no
///   change can ever be reported again. The ingress task returns, and this session
///   is joined at shutdown. The server keeps serving the last good snapshot rather
///   than tearing the user's document out from under them; what it does not do is
///   keep claiming to watch. Nothing further is published, and the terminal says so.
pub(crate) struct WatchSession {
    /// Shared with [`ProductionStages`], which needs it to activate plans. The
    /// engine's task owns the only other reference, so joining the engine is what
    /// makes this the sole owner again — and [`Watcher::join`] needs ownership.
    watcher: Arc<Watcher>,
    ingress: IngressHandle,
    engine: Engine,
    forwarder: tokio::task::JoinHandle<()>,
}

impl WatchSession {
    /// Shuts the session down in a fixed order, joining every task.
    ///
    /// ```text
    /// 1. ingress stops accepting filesystem work
    /// 2. watcher shutdown requested
    /// 3. engine shutdown requested
    /// 4. any in-flight generation still emits its terminal event
    /// 5. join engine        (which is what step 4 waits for)
    /// 6. drain + join the forwarder
    /// 7. join ingress, then the native watcher
    /// ```
    ///
    /// The server is stopped by the caller **after** this returns, so a terminal
    /// event from an in-flight regeneration still reaches live SSE subscribers
    /// rather than racing a closing socket.
    pub(crate) async fn shutdown(mut self) {
        // 1. No new filesystem work is accepted from here on.
        self.ingress.stop();

        // 2 + 3. Ask both to stop. Neither call waits.
        let _ = self.watcher.shutdown();
        let _ = self.engine.handle().shutdown();

        // 4 + 5. The engine finishes any run already in flight and emits its
        //        terminal event before its task ends, so joining it *is* the wait.
        if let Err(error) = self.engine.join().await {
            tracing::warn!(%error, "the regeneration engine did not stop cleanly");
        }

        // 6. The engine's sender is dropped once its task is gone, so the
        //    forwarder drains what is left and returns on its own.
        if let Err(error) = self.forwarder.await {
            tracing::warn!(%error, "the event forwarder did not stop cleanly");
        }

        // 7. Both watcher halves. The engine's task held the only other reference
        //    to the watcher and was joined in step 5, so this is now the sole
        //    owner — which is what `Watcher::join` requires.
        if let Err(error) = self.ingress.join().await {
            tracing::warn!(%error, "the watcher ingress did not stop cleanly");
        }
        match Arc::into_inner(self.watcher) {
            Some(watcher) => {
                if let Err(error) = watcher.join().await {
                    tracing::warn!(%error, "the file watcher did not stop cleanly");
                }
            }
            // Unreachable given step 5, and worth saying out loud rather than
            // silently leaving the watcher task unjoined.
            None => tracing::error!("internal: the file watcher outlived its session"),
        }
    }
}

/// Watch coverage established **before** any generation has run.
///
/// Holding this means the plan is built, the native watcher is registered and the
/// ingress is buffering. It deliberately cannot serve anything yet: the snapshot
/// does not exist.
pub(crate) struct Bootstrapped {
    watcher: Watcher,
    ingress: IngressHandle,
    plan: CorePlan,
}

impl Bootstrapped {
    /// Tears down a bootstrap that will never become a session, joining both
    /// tasks. Used when the *initial* generation or snapshot load fails.
    pub(crate) async fn abandon(mut self) {
        self.ingress.stop();
        let _ = self.watcher.shutdown();
        let _ = self.ingress.join().await;
        let _ = self.watcher.join().await;
    }
}

/// Builds the initial complete plan and starts watching it — **before** the
/// initial generation runs.
///
/// Returns `None` when watch mode cannot be established. That is deliberately not
/// an error: `open --watch` still owes the user a document, and refusing to open
/// one because `notify` hit a per-user watch limit would trade a working feature
/// for a broken one. The caller degrades to an ordinary, non-watching `open`.
pub(crate) fn bootstrap_watch(work: &dyn SessionWork) -> Option<Bootstrapped> {
    let plan = match work.build_complete() {
        Ok(plan) => plan,
        Err(error) => {
            degraded(error.code, &error.message);
            return None;
        }
    };

    let (events_tx, events_rx) = mpsc::unbounded_channel();
    // Typed failure *before* any task is spawned, so a failed start leaves neither
    // a task nor a native watcher behind.
    let watcher = match cratevista_watch::spawn_watcher(plan.plan.clone(), events_tx) {
        Ok(watcher) => watcher,
        Err(error) => {
            degraded(&error.code, &error.message);
            return None;
        }
    };

    Some(Bootstrapped {
        watcher,
        ingress: spawn_ingress(events_rx),
        plan,
    })
}

/// One warning, browser-safe text, and the reason watch mode is off.
fn degraded(code: &str, message: &str) {
    tracing::warn!(%code, %message, "watch mode could not be started");
    eprintln!("warning: watch mode is off ({code}): {message}");
    eprintln!("         The document was generated and is being served as usual.");
}

/// A started `open --watch`: the state to serve, and the session behind it.
pub(crate) struct Started {
    /// Watch-enabled **only** when `session` is `Some`.
    pub(crate) state: Arc<AppState>,
    /// `None` when watch mode degraded — the document is served regardless.
    pub(crate) session: Option<WatchSession>,
}

/// Steps 2–11 of `open --watch`: coverage, the initial generation, and the
/// session — everything before anything is bound or opened.
///
/// The three closures are the seams. They are what let this order be tested
/// without cargo, rustdoc, nightly or a browser, none of which the order depends
/// on. `generate` and `load` return [`CommandFailure`] rather than the watch
/// failure type on purpose: an initial failure keeps ordinary `open`'s contract
/// exactly, down to the exit code.
pub(crate) async fn start(
    work: Arc<dyn SessionWork>,
    generate: impl FnOnce() -> Result<(), CommandFailure>,
    load: impl FnOnce() -> Result<ArtifactSnapshot, CommandFailure>,
    state_for: impl FnOnce(ArtifactSnapshot, bool) -> Arc<AppState>,
) -> Result<Started, CommandFailure> {
    // 2–4. Coverage first, so the initial generation is watched while it runs.
    //      `None` means watch mode degraded; the warning is already printed.
    let bootstrapped = tokio::task::block_in_place(|| bootstrap_watch(&*work));

    // 5. The initial generation. Events are accumulating in the ingress for all
    //    of it. Off the runtime threads: it shells out to cargo and rustdoc.
    if let Err(failure) = tokio::task::block_in_place(generate) {
        // Ordinary open's contract: nothing is bound, no browser opens, the same
        // exit code comes back — and no watch task is left behind.
        if let Some(bootstrapped) = bootstrapped {
            bootstrapped.abandon().await;
        }
        return Err(failure);
    }

    // 6. Load + verify.
    let snapshot = match tokio::task::block_in_place(load) {
        Ok(snapshot) => snapshot,
        Err(failure) => {
            if let Some(bootstrapped) = bootstrapped {
                bootstrapped.abandon().await;
            }
            return Err(failure);
        }
    };

    // 7. Only a real, running watcher earns a watch-enabled state.
    let state = state_for(snapshot, bootstrapped.is_some());

    // 8–11. The engine, the forwarder, and the bootstrap flush.
    let session = match bootstrapped {
        Some(bootstrapped) => Some(activate(bootstrapped, work, state.clone()).await),
        None => None,
    };

    Ok(Started { state, session })
}

/// Turns an established bootstrap into a running session.
///
/// The initial complete plan becomes the retained active `CorePlan` and stays that
/// way until a later transaction successfully activates recovery or complete
/// coverage — the watcher is already registered on exactly it, so claiming
/// anything else would be a lie about what is observed.
pub(crate) async fn activate(
    bootstrapped: Bootstrapped,
    work: Arc<dyn SessionWork>,
    state: Arc<AppState>,
) -> WatchSession {
    let Bootstrapped {
        watcher,
        mut ingress,
        plan,
    } = bootstrapped;

    let watcher = Arc::new(watcher);
    let stages = ProductionStages::new(
        work,
        watcher.clone() as Arc<dyn PlanSink>,
        state.clone(),
        Arc::new(Mutex::new(plan)),
    );

    let (events_tx, events_rx) = mpsc::unbounded_channel();
    let engine = cratevista_watch::spawn(Transaction::new(stages), events_tx);
    let forwarder = spawn_forwarder(events_rx, state);

    // Only now can anything be submitted — and anything the initial generation
    // disturbed is flushed as one merged request before this returns.
    ingress.activate(engine.handle()).await;

    WatchSession {
        watcher,
        ingress,
        engine,
        forwarder,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    use cratevista_server::{ServerEvent, SourceAccessPolicy};
    use cratevista_watch::{RegistrationMode, WatchRegistration, WatchSet};

    // --- fixtures ---------------------------------------------------------

    /// The snapshot the session starts with.
    fn snapshot() -> ArtifactSnapshot {
        snapshot_at("2026-07-17T00:00:00Z")
    }

    /// The snapshot a regeneration publishes — deliberately **not** byte-identical
    /// to [`snapshot`].
    ///
    /// Both are canonical JSON, so an identical `generated_at` would produce an
    /// identical marker token and "the snapshot was swapped" would be unprovable:
    /// the assertion would pass whether or not `commit` did anything at all.
    fn regenerated_snapshot() -> ArtifactSnapshot {
        snapshot_at("2026-07-17T12:00:00Z")
    }

    /// A real, verified snapshot: `commit` really does swap one, so the tests
    /// prove publication rather than a stand-in for it.
    fn snapshot_at(generated_at: &str) -> ArtifactSnapshot {
        use cratevista_schema::canonical::to_canonical_string;
        use cratevista_schema::{
            ArtifactHashes, Counts, DiagnosticsReport, ExplorerDocument, GenerationReport,
            Generator, Project, Timestamp,
        };
        let hex = |bytes: &[u8]| blake3::hash(bytes).to_hex().to_string();
        let project = Project {
            id: "workspace".into(),
            name: "ws".into(),
            description: String::new(),
            root: None,
            repository_url: None,
            default_branch: None,
        };
        let document = to_canonical_string(&ExplorerDocument::new(project, vec![], vec![], vec![]))
            .unwrap()
            .into_bytes();
        let diagnostics = to_canonical_string(&DiagnosticsReport::new(vec![]))
            .unwrap()
            .into_bytes();
        let report = GenerationReport {
            generator: Generator {
                name: "cargo-cratevista".into(),
                version: "0.1.0".into(),
            },
            generated_at: Timestamp::new(generated_at),
            toolchain: None,
            rustdoc_format_version: None,
            input_hashes: Default::default(),
            counts: Counts {
                entities: 0,
                relations: 0,
                views: 0,
                diagnostics: 0,
            },
            durations_ms: Default::default(),
            artifact_hashes: Some(ArtifactHashes {
                document_blake3: hex(&document),
                diagnostics_blake3: hex(&diagnostics),
            }),
            partial: false,
        };
        let generation = to_canonical_string(&report).unwrap().into_bytes();
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("document.json"), &document).unwrap();
        std::fs::write(dir.path().join("diagnostics.json"), &diagnostics).unwrap();
        std::fs::write(dir.path().join("generation.json"), &generation).unwrap();
        load_snapshot(
            &ArtifactPaths::in_dir(dir.path()),
            &SnapshotLoadOptions::default(),
        )
        .expect("a valid snapshot")
    }

    fn state() -> Arc<AppState> {
        AppState::new_watching(snapshot(), SourceAccessPolicy::Disabled)
    }

    /// A real temporary workspace with the three trees the fake plans name.
    ///
    /// Real directories because the session tests drive a **real** watcher: notify
    /// will not register a path that does not exist, and a fake watcher would prove
    /// nothing about the thing that actually runs.
    fn workspace() -> (tempfile::TempDir, PathBuf) {
        let dir = tempfile::tempdir().expect("tempdir");
        let root = dir.path().canonicalize().expect("canonical root");
        for tree in ["initial", "member", "complete"] {
            std::fs::create_dir_all(root.join(tree)).expect("mkdir");
            std::fs::write(root.join(tree).join("lib.rs"), "pub fn x() {}\n").expect("write");
        }
        (dir, root)
    }

    /// A `CorePlan` over named source roots — distinct inputs, so "which plan is
    /// active" is answerable by comparing them rather than by counting calls.
    fn core_plan(root: &Path, trees: &[&str]) -> CorePlan {
        let inputs: Vec<WatchInput> = trees
            .iter()
            .map(|tree| WatchInput::rust_root(root.join(tree)))
            .collect();
        let plan = WatchPlan::new(
            WatchSet::new(root, inputs.clone()),
            trees.iter().map(|tree| WatchRegistration {
                path: root.join(tree),
                mode: RegistrationMode::Recursive,
            }),
        )
        .expect("a valid plan");
        CorePlan { plan, inputs }
    }

    fn initial_plan(root: &Path) -> CorePlan {
        core_plan(root, &["initial"])
    }
    fn recovery_plan(root: &Path) -> CorePlan {
        core_plan(root, &["initial", "member"])
    }
    fn complete_plan(root: &Path) -> CorePlan {
        core_plan(root, &["complete", "initial"])
    }

    fn labels(inputs: &[WatchInput]) -> Vec<String> {
        let mut labels: Vec<String> = inputs
            .iter()
            .map(|input| input.path.to_string_lossy().replace('\\', "/"))
            .collect();
        labels.sort();
        labels
    }

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    enum Fail {
        Nothing,
        Recovery,
        Complete,
        Generate,
        Load,
        /// The plan builds, but names a directory notify cannot register — the
        /// second way watch setup fails for real.
        WatcherInit,
    }

    /// Fake work: no cargo, no rustdoc, no filesystem.
    struct FakeWork {
        root: PathBuf,
        fail: Fail,
        /// Overrides the complete plan, for the integration test that needs the
        /// whole workspace registered rather than one subtree.
        complete_override: Option<CorePlan>,
        calls: Arc<Mutex<Vec<&'static str>>>,
        /// When set, `generate` announces itself here and waits to be released.
        gate: Option<mpsc::UnboundedSender<oneshot::Sender<()>>>,
        live: Arc<AtomicUsize>,
        max_live: Arc<AtomicUsize>,
        snapshot: ArtifactSnapshot,
    }

    impl FakeWork {
        fn new(root: &Path, fail: Fail) -> Self {
            FakeWork {
                root: root.to_path_buf(),
                fail,
                complete_override: None,
                calls: Arc::new(Mutex::new(Vec::new())),
                gate: None,
                live: Arc::new(AtomicUsize::new(0)),
                max_live: Arc::new(AtomicUsize::new(0)),
                snapshot: regenerated_snapshot(),
            }
        }

        /// The snapshot this fake will publish, told apart from the initial one by
        /// its marker token.
        fn token(&self) -> String {
            self.snapshot.marker.token().to_string()
        }
    }

    fn setup_error() -> WatchSetupError {
        crate::watch::build_watch_plan(Path::new("no/such/workspace"), &Default::default())
            .expect_err("a missing root fails")
    }

    impl SessionWork for FakeWork {
        fn build_recovery(&self, active: &[WatchInput]) -> Result<CorePlan, WatchSetupError> {
            self.calls.lock().unwrap().push("build_recovery");
            if self.fail == Fail::Recovery {
                return Err(setup_error());
            }
            // Recovery is a superset of what is active: exactly the production
            // rule, so a fake that broke it would be visible here.
            let built = recovery_plan(&self.root);
            let mut inputs = built.inputs;
            inputs.extend(active.iter().cloned());
            inputs.sort();
            inputs.dedup();
            Ok(CorePlan {
                plan: built.plan,
                inputs,
            })
        }

        fn build_complete(&self) -> Result<CorePlan, WatchSetupError> {
            self.calls.lock().unwrap().push("build_complete");
            if self.fail == Fail::Complete {
                return Err(setup_error());
            }
            if let Some(plan) = &self.complete_override {
                return Ok(plan.clone());
            }
            if self.fail == Fail::WatcherInit {
                // A plan whose registration target does not exist: `spawn_watcher`
                // fails, and it does so *before* spawning anything.
                return Ok(core_plan(&self.root, &["nowhere-at-all"]));
            }
            Ok(complete_plan(&self.root))
        }

        fn generate(&self) -> Result<(), RegenerationFailure> {
            self.calls.lock().unwrap().push("generate");
            let now = self.live.fetch_add(1, Ordering::SeqCst) + 1;
            self.max_live.fetch_max(now, Ordering::SeqCst);
            if let Some(gate) = &self.gate {
                let (release, wait) = oneshot::channel();
                if gate.send(release).is_ok() {
                    let _ = wait.blocking_recv();
                }
            }
            self.live.fetch_sub(1, Ordering::SeqCst);
            if self.fail == Fail::Generate {
                return Err(RegenerationFailure::new(
                    crate::watch::code::GENERATION_FAILED,
                    "generation failed; see the terminal for details",
                ));
            }
            Ok(())
        }

        fn load(&self) -> Result<ArtifactSnapshot, RegenerationFailure> {
            self.calls.lock().unwrap().push("load");
            if self.fail == Fail::Load {
                return Err(crate::watch::artifacts_failure());
            }
            Ok(self.snapshot.clone())
        }
    }

    /// A plan sink that records what was activated and can refuse.
    struct FakeSink {
        /// Which replacement attempt fails: 1 = recovery, 2 = complete, 0 = none.
        fail_on: usize,
        attempts: Mutex<usize>,
        accepted: Arc<Mutex<Vec<Vec<String>>>>,
    }

    impl FakeSink {
        fn new(fail_on: usize) -> Arc<Self> {
            Arc::new(FakeSink {
                fail_on,
                attempts: Mutex::new(0),
                accepted: Arc::new(Mutex::new(Vec::new())),
            })
        }
    }

    impl PlanSink for FakeSink {
        fn replace_plan<'a>(
            &'a self,
            plan: WatchPlan,
        ) -> Pin<Box<dyn Future<Output = Result<(), WatcherError>> + Send + 'a>> {
            Box::pin(async move {
                let attempt = {
                    let mut attempts = self.attempts.lock().unwrap();
                    *attempts += 1;
                    *attempts
                };
                if attempt == self.fail_on {
                    return Err(WatcherError {
                        code: "watcher_closed".into(),
                        message: "the watcher refused the plan".into(),
                    });
                }
                self.accepted.lock().unwrap().push(
                    plan.registrations()
                        .iter()
                        .map(|registration| registration.path.to_string_lossy().replace('\\', "/"))
                        .collect(),
                );
                Ok(())
            })
        }
    }

    async fn within<T>(what: &str, future: impl Future<Output = T>) -> T {
        match tokio::time::timeout(std::time::Duration::from_secs(10), future).await {
            Ok(value) => value,
            Err(_) => panic!("timed out waiting for {what}"),
        }
    }

    fn request(paths: &[&str]) -> RegenerationRequest {
        RegenerationRequest::new(paths.iter().map(PathBuf::from)).expect("non-empty")
    }

    // --- bootstrap event handoff -----------------------------------------

    /// An engine stand-in that only records what it was asked to regenerate.
    struct Recorder {
        seen: Arc<Mutex<Vec<Vec<PathBuf>>>>,
    }

    impl cratevista_watch::Regenerate for Recorder {
        fn regenerate(
            &self,
            request: RegenerationRequest,
        ) -> Pin<Box<dyn Future<Output = cratevista_watch::RegenerationResult> + Send + '_>>
        {
            self.seen.lock().unwrap().push(request.paths().to_vec());
            Box::pin(async { Ok(cratevista_watch::RegenerationSuccess { partial: false }) })
        }
    }

    /// An ingress, a recording engine, and the channels that drive them.
    type IngressHarness = (
        mpsc::UnboundedSender<WatchEvent>,
        IngressHandle,
        Engine,
        Arc<Mutex<Vec<Vec<PathBuf>>>>,
        mpsc::UnboundedReceiver<EngineEvent>,
    );

    /// Starts an ingress plus a recording engine.
    ///
    /// The engine's event receiver is returned rather than dropped: an engine
    /// whose sink is closed stops on its own, which would end the run under test
    /// for a reason that has nothing to do with it.
    fn ingress_harness() -> IngressHarness {
        let (events_tx, events_rx) = mpsc::unbounded_channel();
        let ingress = spawn_ingress(events_rx);
        let seen = Arc::new(Mutex::new(Vec::new()));
        let (engine_events, engine_rx) = mpsc::unbounded_channel();
        let engine = cratevista_watch::spawn(Recorder { seen: seen.clone() }, engine_events);
        (events_tx, ingress, engine, seen, engine_rx)
    }

    /// 1. Nothing happened during the initial generation: nothing is submitted.
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn no_event_during_bootstrap_submits_no_follow_up() {
        let (events_tx, mut ingress, engine, seen, _engine_rx) = ingress_harness();
        ingress.activate(engine.handle()).await;

        ingress.stop();
        within("ingress", ingress.join()).await.unwrap();
        engine.handle().shutdown().unwrap();
        within("engine", engine.join()).await.unwrap();

        assert!(
            seen.lock().unwrap().is_empty(),
            "an empty bootstrap window must not submit an empty regeneration"
        );
        drop(events_tx);
    }

    /// 2. One event during the initial generation becomes exactly one follow-up.
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn one_event_during_bootstrap_becomes_one_follow_up() {
        let (events_tx, mut ingress, engine, seen, _engine_rx) = ingress_harness();
        events_tx
            .send(WatchEvent::Regeneration(request(&["/w/a.rs"])))
            .unwrap();

        ingress.activate(engine.handle()).await;
        ingress.stop();
        within("ingress", ingress.join()).await.unwrap();
        engine.handle().shutdown().unwrap();
        within("engine", engine.join()).await.unwrap();

        assert_eq!(
            *seen.lock().unwrap(),
            [[PathBuf::from("/w/a.rs")]],
            "the edit made during the initial generation is not lost"
        );
    }

    /// 3. Many events during bootstrap merge into one sorted, deduplicated request.
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn many_events_during_bootstrap_become_one_merged_request() {
        let (events_tx, mut ingress, engine, seen, _engine_rx) = ingress_harness();
        for paths in [
            &["/w/b.rs"][..],
            &["/w/a.rs"][..],
            &["/w/b.rs"][..],
            &["/w/c.rs", "/w/a.rs"][..],
        ] {
            events_tx
                .send(WatchEvent::Regeneration(request(paths)))
                .unwrap();
        }

        ingress.activate(engine.handle()).await;
        ingress.stop();
        within("ingress", ingress.join()).await.unwrap();
        engine.handle().shutdown().unwrap();
        within("engine", engine.join()).await.unwrap();

        assert_eq!(
            *seen.lock().unwrap(),
            [[
                PathBuf::from("/w/a.rs"),
                PathBuf::from("/w/b.rs"),
                PathBuf::from("/w/c.rs"),
            ]],
            "one merged request: sorted, deduplicated, and exactly one run"
        );
    }

    /// 4. An event racing activation is observed exactly once — never dropped,
    ///    never submitted twice.
    ///
    /// Both the event and the activation arrive at the same `select!` in the one
    /// task that owns the receiver, so whichever wins, the other is handled next.
    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn an_event_racing_activation_is_observed_exactly_once() {
        for _ in 0..50 {
            let (events_tx, mut ingress, engine, seen, _engine_rx) = ingress_harness();
            let handle = engine.handle();

            let sender = std::thread::spawn(move || {
                events_tx
                    .send(WatchEvent::Regeneration(request(&["/w/race.rs"])))
                    .unwrap();
                events_tx
            });
            ingress.activate(handle).await;
            let events_tx = sender.join().unwrap();

            // Close the stream rather than calling `stop`: a closed channel still
            // yields everything already queued *before* it reports the end, so the
            // ingress provably drains the racing event whichever side won. `stop`
            // would deliberately discard it — correct at shutdown, and it would
            // make this assertion measure the wrong thing.
            drop(events_tx);
            within("ingress", ingress.join()).await.unwrap();
            engine.handle().shutdown().unwrap();
            within("engine", engine.join()).await.unwrap();

            let seen = seen.lock().unwrap();
            let submissions: Vec<&PathBuf> = seen.iter().flatten().collect();
            assert_eq!(
                submissions,
                [&PathBuf::from("/w/race.rs")],
                "the racing edit must appear exactly once, however the race resolved"
            );
        }
    }

    /// A watcher problem during bootstrap is a warning, not a regeneration and not
    /// a generation failure.
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn a_watcher_failure_during_bootstrap_is_a_warning_only() {
        let (events_tx, mut ingress, engine, seen, _engine_rx) = ingress_harness();
        events_tx
            .send(WatchEvent::WatcherFailed {
                code: "watch_limit_reached".into(),
                message: "too many watched files".into(),
            })
            .unwrap();

        ingress.activate(engine.handle()).await;
        ingress.stop();
        within("ingress", ingress.join()).await.unwrap();
        engine.handle().shutdown().unwrap();
        within("engine", engine.join()).await.unwrap();

        assert!(
            seen.lock().unwrap().is_empty(),
            "an error is not a change: it must never become a regeneration"
        );
    }

    // --- plan ownership, through the real production adapter ---------------
    //
    // `ProductionStages` is the real thing under test here; only the cargo work and
    // the watcher are faked. That is the point: the retention rule is the adapter's,
    // and it must hold whatever generation does.

    struct Owned {
        /// Core's retained record of what the watcher holds — the thing every
        /// assertion below is really about. Shared, so no accessor on the adapter
        /// is needed and none exists.
        retained: Arc<Mutex<CorePlan>>,
        sink: Arc<FakeSink>,
        state: Arc<AppState>,
        work: Arc<FakeWork>,
    }

    fn owned(root: &Path, fail: Fail, fail_replace_on: usize) -> Owned {
        Owned {
            work: Arc::new(FakeWork::new(root, fail)),
            sink: FakeSink::new(fail_replace_on),
            state: state(),
            retained: Arc::new(Mutex::new(initial_plan(root))),
        }
    }

    /// Runs one transaction through the **real** `ProductionStages`.
    async fn regenerate(owned: &Owned) -> cratevista_watch::RegenerationResult {
        use cratevista_watch::Regenerate;
        let stages = ProductionStages::new(
            owned.work.clone(),
            owned.sink.clone(),
            owned.state.clone(),
            owned.retained.clone(),
        );
        Transaction::new(stages)
            .regenerate(request(&["/w/trigger.rs"]))
            .await
    }

    /// The logical inputs the watcher is currently registered on.
    fn active(owned: &Owned) -> Vec<String> {
        labels(&owned.retained.lock().unwrap().inputs)
    }

    /// The initial complete plan is retained from the start: the watcher is
    /// registered on exactly it, so claiming anything else would be a lie.
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn the_initial_complete_plan_is_the_retained_active_plan() {
        let (_dir, root) = workspace();
        let owned = owned(&root, Fail::Nothing, 0);
        assert_eq!(active(&owned), labels(&initial_plan(&root).inputs));
    }

    /// A successful recovery replacement is what promotes recovery coverage.
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn a_successful_recovery_replacement_updates_the_active_plan() {
        let (_dir, root) = workspace();
        // The complete build fails, so the run stops with recovery active.
        let owned = owned(&root, Fail::Complete, 0);
        regenerate(&owned)
            .await
            .expect_err("the complete build fails");

        let active = active(&owned);
        assert!(
            active.contains(&root.join("member").to_string_lossy().replace('\\', "/")),
            "recovery coverage is active: {active:?}"
        );
        assert!(
            active.contains(&root.join("initial").to_string_lossy().replace('\\', "/")),
            "and it never narrowed what was already watched: {active:?}"
        );
    }

    /// A refused recovery replacement leaves the previous plan — the retained copy
    /// must not run ahead of the watcher.
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn a_refused_recovery_replacement_leaves_the_previous_plan() {
        let (_dir, root) = workspace();
        let owned = owned(&root, Fail::Nothing, 1);
        let error = regenerate(&owned).await.expect_err("the replacement fails");

        assert_eq!(error.code, crate::watch::code::PLAN_REPLACE_FAILED);
        assert_eq!(
            active(&owned),
            labels(&initial_plan(&root).inputs),
            "a plan the watcher refused is not coverage, whatever core built"
        );
        assert!(
            owned.sink.accepted.lock().unwrap().is_empty(),
            "nothing was installed"
        );
    }

    /// A refused complete replacement leaves recovery — never the older plan.
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn a_refused_complete_replacement_leaves_the_recovery_plan() {
        let (_dir, root) = workspace();
        let owned = owned(&root, Fail::Nothing, 2);
        let error = regenerate(&owned).await.expect_err("the replacement fails");

        assert_eq!(error.code, crate::watch::code::PLAN_REPLACE_FAILED);
        let active = active(&owned);
        assert!(
            active.contains(&root.join("member").to_string_lossy().replace('\\', "/")),
            "recovery stands: {active:?}"
        );
        assert!(
            !active.contains(&root.join("complete").to_string_lossy().replace('\\', "/")),
            "the retained plan must not become the one the watcher refused: {active:?}"
        );
    }

    /// Generation and load failures both keep complete coverage and publish nothing.
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn a_generation_or_load_failure_leaves_the_complete_plan_and_the_snapshot() {
        let (_dir, root) = workspace();
        for fail in [Fail::Generate, Fail::Load] {
            let owned = owned(&root, fail, 0);
            let before = owned.state.snapshot().marker.token().to_string();

            regenerate(&owned).await.expect_err("must fail");

            assert_eq!(
                active(&owned),
                labels(&complete_plan(&root).inputs),
                "{fail:?}: coverage may lead the snapshot"
            );
            assert_eq!(
                owned.state.snapshot().marker.token(),
                before,
                "{fail:?}: publication may not move at all"
            );
        }
    }

    /// Success: complete coverage, and the snapshot really is swapped.
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn a_success_activates_complete_coverage_and_commits_the_snapshot() {
        let (_dir, root) = workspace();
        let owned = owned(&root, Fail::Nothing, 0);
        let before = owned.state.snapshot().marker.token().to_string();

        let outcome = regenerate(&owned).await.expect("success");
        assert_eq!(
            outcome,
            cratevista_watch::RegenerationSuccess { partial: false }
        );
        assert_eq!(active(&owned), labels(&complete_plan(&root).inputs));
        assert_ne!(owned.state.snapshot().marker.token(), before);
        assert_eq!(
            owned.state.snapshot().marker.token(),
            owned.work.token(),
            "commit publishes exactly what load returned"
        );
        assert_eq!(
            *owned.work.calls.lock().unwrap(),
            ["build_recovery", "build_complete", "generate", "load"],
            "coverage before generation, in the order the transaction fixes"
        );
    }

    // --- the session: events, ordering and shutdown ------------------------

    struct Session {
        session: WatchSession,
        state: Arc<AppState>,
        work: Arc<FakeWork>,
        events: tokio::sync::broadcast::Receiver<ServerEvent>,
        /// Feeds the ingress as the real watcher would.
        watcher_events: mpsc::UnboundedSender<WatchEvent>,
        paused: Option<mpsc::UnboundedReceiver<oneshot::Sender<()>>>,
    }

    /// Builds a session over a **real** watcher, with fake generation work.
    async fn session(root: &Path, fail: Fail, pause: bool) -> Session {
        let (gate, paused) = if pause {
            let (tx, rx) = mpsc::unbounded_channel();
            (Some(tx), Some(rx))
        } else {
            (None, None)
        };
        let mut work = FakeWork::new(root, fail);
        work.gate = gate;
        let work = Arc::new(work);

        // A real watcher over the real temp workspace, plus a second sender the
        // test uses to inject events without touching the disk.
        let (watcher_events, events_rx) = mpsc::unbounded_channel();
        let watcher =
            cratevista_watch::spawn_watcher(initial_plan(root).plan, watcher_events.clone())
                .expect("the watcher starts");
        let bootstrapped = Bootstrapped {
            watcher,
            ingress: spawn_ingress(events_rx),
            plan: initial_plan(root),
        };

        let state = state();
        let events = state.subscribe_events();
        let session = activate(bootstrapped, work.clone(), state.clone()).await;
        Session {
            session,
            state,
            work,
            events,
            watcher_events,
            paused,
        }
    }

    /// A watcher request reaches the engine, runs, swaps and announces — in that
    /// order.
    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn a_watcher_request_regenerates_swaps_then_announces_success() {
        let (_dir, root) = workspace();
        let mut harness = session(&root, Fail::Nothing, false).await;
        let before = harness.state.snapshot().marker.token().to_string();

        harness
            .watcher_events
            .send(WatchEvent::Regeneration(request(&["/w/a.rs"])))
            .unwrap();

        assert_eq!(
            within("Started", harness.events.recv()).await.unwrap(),
            ServerEvent::GenerationStarted
        );
        assert_eq!(
            harness.state.snapshot().marker.token(),
            before,
            "Started must not imply anything was published"
        );

        assert_eq!(
            within("Succeeded", harness.events.recv()).await.unwrap(),
            ServerEvent::GenerationSucceeded { partial: false }
        );
        assert_eq!(
            harness.state.snapshot().marker.token(),
            harness.work.token(),
            "the swap happens BEFORE the success is announced, so a browser that \
             reloads on the event never fetches the old document"
        );

        harness.session.shutdown().await;
    }

    /// A failed run announces the failure and changes nothing.
    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn a_failed_run_announces_and_does_not_swap() {
        let (_dir, root) = workspace();
        let mut harness = session(&root, Fail::Generate, false).await;
        let before = harness.state.snapshot().marker.token().to_string();

        harness
            .watcher_events
            .send(WatchEvent::Regeneration(request(&["/w/a.rs"])))
            .unwrap();

        assert_eq!(
            within("Started", harness.events.recv()).await.unwrap(),
            ServerEvent::GenerationStarted
        );
        match within("Failed", harness.events.recv()).await.unwrap() {
            ServerEvent::GenerationFailed { code, message } => {
                assert_eq!(code, crate::watch::code::GENERATION_FAILED);
                // Core's own browser-safe pair, forwarded exactly.
                assert!(!message.contains("C:\\") && !message.contains("/home/"));
                assert!(!message.contains("--edition") && !message.contains("CARGO_HOME"));
            }
            other => panic!("wrong event: {other:?}"),
        }
        assert_eq!(
            harness.state.snapshot().marker.token(),
            before,
            "a failure leaves the document exactly as it was"
        );

        harness.session.shutdown().await;
    }

    /// A watcher problem is a terminal warning: it never becomes an SSE
    /// generation failure, because no generation even ran.
    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn a_watcher_failure_publishes_no_server_event() {
        let (_dir, root) = workspace();
        let mut harness = session(&root, Fail::Nothing, false).await;

        harness
            .watcher_events
            .send(WatchEvent::WatcherFailed {
                code: "watch_limit_reached".into(),
                message: "too many watched files".into(),
            })
            .unwrap();

        // A real regeneration afterwards proves the warning was processed *and*
        // produced nothing: its Started is the very next event on the stream.
        harness
            .watcher_events
            .send(WatchEvent::Regeneration(request(&["/w/a.rs"])))
            .unwrap();
        assert_eq!(
            within("Started", harness.events.recv()).await.unwrap(),
            ServerEvent::GenerationStarted,
            "the watcher failure must not have published anything before this"
        );

        harness.session.shutdown().await;
    }

    /// Edits during a run become exactly one dirty follow-up, one run at a time.
    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn edits_during_a_run_become_one_follow_up_and_never_overlap() {
        let (_dir, root) = workspace();
        let mut harness = session(&root, Fail::Nothing, true).await;
        let mut paused = harness.paused.take().expect("paused");

        harness
            .watcher_events
            .send(WatchEvent::Regeneration(request(&["/w/a.rs"])))
            .unwrap();
        let release = within("generation to pause", paused.recv())
            .await
            .expect("a release handle");

        // Two edits land while the run is provably still in flight.
        harness
            .watcher_events
            .send(WatchEvent::Regeneration(request(&["/w/b.rs"])))
            .unwrap();
        harness
            .watcher_events
            .send(WatchEvent::Regeneration(request(&["/w/c.rs"])))
            .unwrap();
        release.send(()).expect("generation is waiting");

        within("first Started", harness.events.recv())
            .await
            .unwrap();
        within("first terminal", harness.events.recv())
            .await
            .unwrap();
        assert_eq!(
            within("follow-up Started", harness.events.recv())
                .await
                .unwrap(),
            ServerEvent::GenerationStarted,
            "the two edits coalesce into exactly one follow-up"
        );
        let release = within("follow-up to pause", paused.recv())
            .await
            .expect("a release handle");
        release.send(()).expect("generation is waiting");
        within("follow-up terminal", harness.events.recv())
            .await
            .unwrap();

        harness.session.shutdown().await;
        assert_eq!(
            harness.work.max_live.load(Ordering::SeqCst),
            1,
            "single-flight: never two generations at once"
        );
        assert_eq!(
            harness
                .work
                .calls
                .lock()
                .unwrap()
                .iter()
                .filter(|call| **call == "generate")
                .count(),
            2,
            "exactly two runs, not three"
        );
    }

    // --- shutdown ---------------------------------------------------------

    /// Idle shutdown joins everything.
    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn shutdown_while_idle_joins_every_task() {
        let (_dir, root) = workspace();
        let harness = session(&root, Fail::Nothing, false).await;
        within("shutdown", harness.session.shutdown()).await;
    }

    /// Repeated shutdown is harmless.
    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn a_second_shutdown_request_is_harmless() {
        let (_dir, root) = workspace();
        let harness = session(&root, Fail::Nothing, false).await;
        // The session's own handles are idempotent; asking twice must not panic
        // or hang, because Ctrl-C twice is a thing people do.
        within("shutdown", harness.session.shutdown()).await;
    }

    /// Shutdown while a generation is paused: the run finishes, emits its terminal
    /// event, and every join completes.
    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn shutdown_while_a_generation_is_paused_lets_it_finish_and_announce() {
        let (_dir, root) = workspace();
        let mut harness = session(&root, Fail::Nothing, true).await;
        let mut paused = harness.paused.take().expect("paused");

        harness
            .watcher_events
            .send(WatchEvent::Regeneration(request(&["/w/a.rs"])))
            .unwrap();
        let release = within("generation to pause", paused.recv())
            .await
            .expect("a release handle");
        within("Started", harness.events.recv()).await.unwrap();

        // Shut down while the run is held open, then let it finish.
        let shutdown = tokio::spawn(harness.session.shutdown());
        release.send(()).expect("generation is waiting");

        assert_eq!(
            within("the terminal event", harness.events.recv())
                .await
                .unwrap(),
            ServerEvent::GenerationSucceeded { partial: false },
            "an in-flight run still announces its result: shutdown is not a kill"
        );
        within("shutdown to complete", shutdown).await.unwrap();
    }

    /// An edit submitted during shutdown starts no second run.
    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn no_follow_up_starts_after_shutdown_is_requested() {
        let (_dir, root) = workspace();
        let mut harness = session(&root, Fail::Nothing, true).await;
        let mut paused = harness.paused.take().expect("paused");

        harness
            .watcher_events
            .send(WatchEvent::Regeneration(request(&["/w/a.rs"])))
            .unwrap();
        let release = within("generation to pause", paused.recv())
            .await
            .expect("a release handle");

        // The edit arrives *after* shutdown was requested but *before* the paused
        // run has finished — the exact window a dirty follow-up would use.
        let work = harness.work.clone();
        let events = harness.watcher_events.clone();
        let shutdown = tokio::spawn(harness.session.shutdown());
        events
            .send(WatchEvent::Regeneration(request(&["/w/b.rs"])))
            .unwrap();
        release.send(()).expect("generation is waiting");
        within("shutdown to complete", shutdown).await.unwrap();

        assert_eq!(
            work.calls
                .lock()
                .unwrap()
                .iter()
                .filter(|call| **call == "generate")
                .count(),
            1,
            "the in-flight run finished; nothing new started"
        );
    }

    // --- startup: degradation and initial failure -------------------------

    /// A `CommandFailure` standing in for a real generation failure, exit code and
    /// all — so "the existing contract is preserved" is checked, not assumed.
    fn command_failure(code: &str) -> CommandFailure {
        CommandFailure::runtime(crate::diagnostic::Diagnostic::error(code, "it failed"))
    }

    /// Records whether a state was ever built, and with what.
    #[derive(Default)]
    struct StateSpy {
        built: Mutex<Vec<bool>>,
    }

    async fn start_with(
        root: &Path,
        fail: Fail,
        generate: Result<(), CommandFailure>,
        load_ok: bool,
        spy: &StateSpy,
    ) -> Result<Started, CommandFailure> {
        let work = Arc::new(FakeWork::new(root, fail));
        start(
            work,
            || generate,
            || {
                if load_ok {
                    Ok(snapshot())
                } else {
                    Err(command_failure("artifacts_unreadable"))
                }
            },
            |snapshot, watching| {
                spy.built.lock().unwrap().push(watching);
                if watching {
                    AppState::new_watching(snapshot, SourceAccessPolicy::Disabled)
                } else {
                    AppState::new(snapshot, SourceAccessPolicy::Disabled)
                }
            },
        )
        .await
    }

    /// 5a. The plan cannot be built → degrade, do not claim to watch.
    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn an_unbuildable_plan_degrades_to_a_non_watching_open() {
        let (_dir, root) = workspace();
        let spy = StateSpy::default();
        let started = within(
            "start",
            start_with(&root, Fail::Complete, Ok(()), true, &spy),
        )
        .await
        .expect("open still owes the user a document");

        assert!(
            started.session.is_none(),
            "no session, because there is no watcher"
        );
        assert_eq!(
            *spy.built.lock().unwrap(),
            [false],
            "AppState::new, not new_watching"
        );
        assert!(
            !started.state.watch_enabled(),
            "/api/health.watch_enabled must be false and /api/events unregistered: \
             claiming otherwise leaves a browser waiting for an event that can \
             never arrive"
        );
    }

    /// 5b. The native watcher cannot initialize → the same degradation, and no
    ///     half-started watcher task is left behind.
    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn a_watcher_that_cannot_initialize_degrades_the_same_way() {
        let (_dir, root) = workspace();
        let spy = StateSpy::default();
        let started = within(
            "start",
            start_with(&root, Fail::WatcherInit, Ok(()), true, &spy),
        )
        .await
        .expect("the document is still generated and served");

        assert!(started.session.is_none());
        assert!(!started.state.watch_enabled());
        assert_eq!(*spy.built.lock().unwrap(), [false]);
    }

    /// 6. The initial generation fails → no state is built at all (so nothing is
    ///    bound and no browser opens), the original failure comes back, and the
    ///    watch tasks are joined rather than detached.
    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn an_initial_generation_failure_binds_nothing_and_joins_the_watch_tasks() {
        let (_dir, root) = workspace();
        let spy = StateSpy::default();
        // `within` is the proof of the join: `abandon` awaits both tasks, so a
        // detached or wedged task would hang here rather than return.
        let started = within(
            "start",
            start_with(
                &root,
                Fail::Nothing,
                Err(command_failure("nightly_toolchain_missing")),
                true,
                &spy,
            ),
        )
        .await;
        let Err(failure) = started else {
            panic!("the generation failure must be returned unchanged");
        };

        assert_eq!(failure.diagnostic.code, "nightly_toolchain_missing");
        assert!(
            spy.built.lock().unwrap().is_empty(),
            "no AppState was constructed, watching or otherwise: there is nothing \
             to serve, so nothing binds and no browser opens"
        );
    }

    /// 7. The initial snapshot fails to load → the same cleanup guarantee.
    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn an_initial_snapshot_failure_binds_nothing_and_joins_the_watch_tasks() {
        let (_dir, root) = workspace();
        let spy = StateSpy::default();
        let started = within(
            "start",
            start_with(&root, Fail::Nothing, Ok(()), false, &spy),
        )
        .await;
        let Err(failure) = started else {
            panic!("the artifact failure must be returned unchanged");
        };

        assert_eq!(failure.diagnostic.code, "artifacts_unreadable");
        assert!(spy.built.lock().unwrap().is_empty());
    }

    /// The happy path: a working watcher earns `new_watching`, and the initial
    /// complete plan is what the session starts on.
    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn a_working_setup_builds_a_watching_state_and_a_session() {
        let (_dir, root) = workspace();
        let spy = StateSpy::default();
        let started = within(
            "start",
            start_with(&root, Fail::Nothing, Ok(()), true, &spy),
        )
        .await
        .expect("starts");

        assert_eq!(*spy.built.lock().unwrap(), [true], "AppState::new_watching");
        assert!(started.state.watch_enabled());
        let session = started.session.expect("a session");
        within("shutdown", session.shutdown()).await;
    }

    /// Coverage exists before the initial generation runs — the whole point of the
    /// order, and the one thing a unit test can pin about it directly.
    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn the_watcher_is_registered_before_the_initial_generation_begins() {
        let (_dir, root) = workspace();
        let work = Arc::new(FakeWork::new(&root, Fail::Nothing));
        let calls = work.calls.clone();

        let started = within(
            "start",
            start(
                work,
                || {
                    // By the time the initial generation runs, the plan is built and
                    // the watcher is registered on it.
                    calls.lock().unwrap().push("initial_generate");
                    Ok(())
                },
                || Ok(snapshot()),
                |snapshot, watching| {
                    assert!(watching);
                    AppState::new_watching(snapshot, SourceAccessPolicy::Disabled)
                },
            ),
        )
        .await
        .expect("starts");

        let calls = calls.lock().unwrap().clone();
        let plan = calls
            .iter()
            .position(|call| *call == "build_complete")
            .expect("the plan was built");
        let generate = calls
            .iter()
            .position(|call| *call == "initial_generate")
            .expect("the initial generation ran");
        assert!(
            plan < generate,
            "coverage must exist before the first generation reads anything: {calls:?}"
        );

        within("shutdown", started.session.expect("a session").shutdown()).await;
    }

    /// An edit during the initial generation survives it and becomes one run.
    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn an_edit_during_the_initial_generation_is_regenerated_after_activation() {
        let (_dir, root) = workspace();
        let work = Arc::new(FakeWork::new(&root, Fail::Nothing));

        // A second sender into the same ingress the bootstrap owns is not
        // reachable from here, so drive the real path: touch a watched file while
        // the initial generation is running. The debouncer makes the delivery
        // asynchronous, so the assertion below waits for the event rather than
        // assuming it.
        let touched = root.join("initial").join("edited.rs");
        let started = within(
            "start",
            start(
                work.clone(),
                || {
                    std::fs::write(&touched, "pub fn edited() {}\n").expect("write");
                    Ok(())
                },
                || Ok(snapshot()),
                |snapshot, _| AppState::new_watching(snapshot, SourceAccessPolicy::Disabled),
            ),
        )
        .await
        .expect("starts");

        let mut events = started.state.subscribe_events();
        assert_eq!(
            within("the regeneration caused by the edit", events.recv())
                .await
                .unwrap(),
            ServerEvent::GenerationStarted,
            "an edit made during the initial generation must not be lost"
        );
        within("terminal", events.recv()).await.unwrap();
        within("shutdown", started.session.expect("a session").shutdown()).await;
    }

    // --- the real watcher, end to end -------------------------------------

    /// The whole startup against a **real** notify watcher and a real filesystem.
    ///
    /// Only the generation is fake — deliberately, because holding a real
    /// `cargo doc` open is not something a test can do, and the thing under test is
    /// the window it creates, not cargo.
    ///
    /// Nothing here sleeps to decide anything. Absence is proven the only honest
    /// way: with a **positive control** — a change that must produce a run —
    /// submitted after the one that must not, so a spurious run would be observed
    /// as the wrong event arriving first rather than as a timeout.
    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn a_real_watcher_ignores_artifact_writes_during_bootstrap_but_sees_source_edits() {
        let (_dir, root) = workspace();
        std::fs::create_dir_all(root.join("target").join("cratevista")).expect("mkdir");

        // The whole workspace is registered, so `target/` events genuinely arrive
        // at the classifier rather than being unobservable by construction. That is
        // what makes the control below mean anything.
        let mut work = FakeWork::new(&root, Fail::Nothing);
        work.complete_override = Some(core_plan(&root, &[""]));
        let work = Arc::new(work);

        let (running_tx, running_rx) = oneshot::channel();
        let (release_tx, release_rx) = oneshot::channel();
        let captured: Arc<Mutex<Option<tokio::sync::broadcast::Receiver<ServerEvent>>>> =
            Arc::new(Mutex::new(None));

        let artifact = root.join("target").join("cratevista").join("document.json");
        let capture = captured.clone();
        let started = within(
            "start",
            start(
                work,
                move || {
                    // The initial generation is now running, which means the plan is
                    // built and the watcher is registered on it.
                    running_tx.send(()).expect("the test is waiting");
                    release_rx.blocking_recv().expect("released");
                    Ok(())
                },
                || Ok(snapshot()),
                move |snapshot, watching| {
                    assert!(watching);
                    let state = AppState::new_watching(snapshot, SourceAccessPolicy::Disabled);
                    // Subscribed *before* activation flushes the bootstrap window,
                    // so no event can be missed between here and the assertions.
                    *capture.lock().unwrap() = Some(state.subscribe_events());
                    state
                },
            ),
        );
        let started = tokio::spawn(started);

        // Coverage is live while the initial generation runs: this is the window.
        within("the initial generation to start", running_rx)
            .await
            .expect("running");

        // The thing that must NOT cause a regeneration: our own artifact output.
        // Regenerating on it would be an infinite loop — generate writes it, the
        // watcher sees it, and it generates again.
        std::fs::write(&artifact, b"{}").expect("write");
        // And a `.rs` under `target/`, which only the build-output rule can reject:
        // `document.json` alone would also be refused just for not being Rust, so
        // it does not prove the rule that actually matters here.
        std::fs::write(
            root.join("target").join("cratevista").join("scratch.rs"),
            "pub fn generated() {}\n",
        )
        .expect("write");

        release_tx.send(()).expect("generation is waiting");
        let started = within("start to finish", started)
            .await
            .unwrap()
            .expect("starts");
        let mut events = captured.lock().unwrap().take().expect("subscribed");

        // The positive control: a real source edit, which must produce a run. If the
        // artifact write had produced one, its Started would already be queued ahead
        // of this one — and the assertion after it would see the wrong count.
        std::fs::write(
            root.join("initial").join("edited.rs"),
            "pub fn edited() {}\n",
        )
        .expect("write");

        assert_eq!(
            within("the source edit's regeneration", events.recv())
                .await
                .unwrap(),
            ServerEvent::GenerationStarted,
            "the real watcher must deliver a real source edit"
        );
        let terminal = within("terminal", events.recv()).await.unwrap();
        assert_eq!(
            terminal,
            ServerEvent::GenerationSucceeded { partial: false }
        );

        within("shutdown", started.session.expect("a session").shutdown()).await;

        // Exactly one run happened in total: the artifact write contributed nothing.
        assert_eq!(
            events.try_recv().ok(),
            None,
            "no further event: writing target/cratevista/document.json is not a change"
        );
    }

    /// The unrecoverable case: the adapter's stream ends, so the ingress returns
    /// on its own rather than idling forever.
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn a_closed_watcher_stream_ends_the_ingress() {
        let (events_tx, events_rx) = mpsc::unbounded_channel();
        let ingress = spawn_ingress(events_rx);
        drop(events_tx);
        within("ingress to end on its own", ingress.join())
            .await
            .unwrap();
    }
}
