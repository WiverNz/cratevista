//! The single-flight regeneration engine.
//!
//! # One task, one call site
//!
//! Exactly one task owns the state machine
//! `Idle → Running → Running+Dirty → Running → Idle`, and there is exactly **one**
//! place in this file where the regeneration operation is started. Concurrency is
//! therefore bounded at one **by construction**: there is no lock to forget to
//! take, and no second call site that could race the first. A test asserts the
//! observed maximum anyway, because "by construction" should still be falsifiable.
//!
//! # What the engine does not know
//!
//! It has never heard of `CommandFailure`, `ArtifactSnapshot`, `AppState`,
//! `ServerEvent`, cargo, rustdoc or HTTP. Regeneration is an injected
//! [`Regenerate`] operation returning
//! `Result<`[`RegenerationSuccess`]`, `[`RegenerationFailure`]`>`, so every rule
//! below is testable with a fake that completes when a test says so — no cargo, no
//! nightly, no filesystem, no sleeps.
//!
//! The later `cratevista-core` adapter is what gives that operation meaning: it
//! wraps the **synchronous, blocking** `run_generate` → `load_snapshot` →
//! `replace_snapshot` → WatchSet-rebuild work in `tokio::task::spawn_blocking`
//! and completes the future when that finishes. **This crate never calls
//! `spawn_blocking` itself** — it has no blocking work to do, and doing so merely
//! to imitate core would put a fake seam in the real code.
//!
//! # Debouncing lives elsewhere
//!
//! The engine consumes requests that are **already debounced**
//! ([`crate::Debouncer`]). It has no timers and no quiet window; it schedules.

use std::collections::BTreeSet;
use std::future::Future;
use std::path::PathBuf;
use std::pin::Pin;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use tokio::sync::mpsc::error::TryRecvError;
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender, unbounded_channel};
use tokio::task::{JoinError, JoinHandle};

use crate::event::EngineEvent;

/// The set of changed paths that justifies one regeneration.
///
/// **Never empty**: [`RegenerationRequest::new`] returns `None` for an empty
/// input, so the operation can never be entered with nothing to do. That is a
/// type-level guarantee rather than a check the engine has to remember.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RegenerationRequest {
    paths: Vec<PathBuf>,
}

impl RegenerationRequest {
    /// Builds a request, **sorting and deduplicating** the paths.
    ///
    /// Returns `None` when nothing changed. Sorting here means the operation sees
    /// one canonical description of a change set regardless of the order the OS
    /// reported it in.
    pub fn new(paths: impl IntoIterator<Item = PathBuf>) -> Option<Self> {
        let unique: BTreeSet<PathBuf> = paths.into_iter().collect();
        if unique.is_empty() {
            return None;
        }
        Some(RegenerationRequest {
            paths: unique.into_iter().collect(),
        })
    }

    /// The changed paths: sorted, deduplicated, non-empty.
    pub fn paths(&self) -> &[PathBuf] {
        &self.paths
    }

    /// How many distinct paths changed.
    pub fn len(&self) -> usize {
        self.paths.len()
    }

    /// Always `false` — a request cannot be empty. Present because clippy asks
    /// for it next to `len`, and because it documents the invariant.
    pub fn is_empty(&self) -> bool {
        false
    }

    /// Merges another request into this one, keeping the result sorted and
    /// deduplicated.
    ///
    /// This is how N requests arriving mid-run become **one** follow-up: the dirty
    /// set is a set, so ten events for one file schedule one run over one path.
    pub fn merge(&mut self, other: RegenerationRequest) {
        let mut unique: BTreeSet<PathBuf> = std::mem::take(&mut self.paths).into_iter().collect();
        unique.extend(other.paths);
        self.paths = unique.into_iter().collect();
    }
}

/// A regeneration finished successfully.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct RegenerationSuccess {
    /// Whether the resulting artifacts are partial but valid.
    pub partial: bool,
}

/// A regeneration failed. The previous state stands.
///
/// # Sanitization is the caller's job
///
/// The engine transports `code` and `message` **unchanged** into
/// [`EngineEvent::GenerationFailed`]. It does not inspect, rewrite, redact or
/// truncate them, and it cannot: it has no idea what a workspace root looks like,
/// which is exactly why it must not try.
///
/// **The later `cratevista-core` adapter is responsible for supplying
/// browser-safe values** — a stable code plus a message carrying no absolute
/// path, no `CARGO_HOME`, no username and no raw child-process command line —
/// because cargo and rustdoc failures arrive full of all four. Anything put in
/// here should be assumed to reach a browser.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RegenerationFailure {
    /// A stable, machine-matchable code.
    pub code: String,
    /// A message the caller has already made safe to publish.
    pub message: String,
}

impl RegenerationFailure {
    /// Builds a failure from a caller-supplied code and message.
    pub fn new(code: impl Into<String>, message: impl Into<String>) -> Self {
        RegenerationFailure {
            code: code.into(),
            message: message.into(),
        }
    }
}

/// What one regeneration returns.
pub type RegenerationResult = Result<RegenerationSuccess, RegenerationFailure>;

/// The injected regeneration operation.
///
/// Boxed rather than `async fn` in a trait so the trait stays object-safe and the
/// engine can hold one without a generic parameter leaking into every type.
pub trait Regenerate: Send + Sync + 'static {
    /// Runs one regeneration over the changed paths.
    fn regenerate(
        &self,
        request: RegenerationRequest,
    ) -> Pin<Box<dyn Future<Output = RegenerationResult> + Send + '_>>;
}

/// Injected pause points, used to force an interleaving a test could not
/// otherwise observe.
///
/// The field — and the `await` that consults it — exist **only under
/// `cfg(test)`**, so production compiles to exactly the code it did before:
/// there is no branch, no allocation and no behavior to change. It is needed
/// because between emitting a terminal event and deciding on a follow-up the
/// engine has no `await` point at all, so no other task can interleave there by
/// scheduling alone.
#[derive(Default, Clone)]
struct Hooks {
    /// Awaited immediately before the dirty-follow-up decision.
    #[cfg(test)]
    #[allow(clippy::type_complexity)]
    before_follow_up_decision:
        Option<Arc<dyn Fn() -> Pin<Box<dyn Future<Output = ()> + Send>> + Send + Sync>>,
}

/// What a handle sends to the engine task.
#[derive(Debug)]
enum Command {
    Submit(RegenerationRequest),
    Shutdown,
}

/// The engine is no longer accepting requests.
///
/// Returned instead of panicking: submitting after shutdown is a normal race (a
/// change can land while the process is winding down), not a bug to crash on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EngineClosed;

impl std::fmt::Display for EngineClosed {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("the regeneration engine is shutting down or has stopped")
    }
}

impl std::error::Error for EngineClosed {}

/// A cloneable handle for submitting requests and asking the engine to stop.
#[derive(Debug, Clone)]
pub struct EngineHandle {
    commands: UnboundedSender<Command>,
    /// Set the instant shutdown is requested, so a submit racing the task's exit
    /// gets a deterministic `Err` rather than being silently dropped.
    closed: Arc<AtomicBool>,
}

impl EngineHandle {
    /// Submits a debounced change set.
    ///
    /// Returns [`EngineClosed`] once shutdown has been requested or the engine has
    /// stopped. The request is **not** rejected merely because a run is in
    /// progress — it becomes the dirty follow-up.
    pub fn submit(&self, request: RegenerationRequest) -> Result<(), EngineClosed> {
        if self.closed.load(Ordering::SeqCst) {
            return Err(EngineClosed);
        }
        self.commands
            .send(Command::Submit(request))
            .map_err(|_| EngineClosed)
    }

    /// Asks the engine to stop.
    ///
    /// An in-flight regeneration is **allowed to finish** and still emits its
    /// terminal event; any dirty follow-up is discarded. Idempotent.
    pub fn shutdown(&self) -> Result<(), EngineClosed> {
        // Flip first: a submit that races this must lose, not slip through.
        let already = self.closed.swap(true, Ordering::SeqCst);
        if already {
            return Ok(());
        }
        self.commands
            .send(Command::Shutdown)
            .map_err(|_| EngineClosed)
    }

    /// Whether shutdown has been requested.
    pub fn is_closed(&self) -> bool {
        self.closed.load(Ordering::SeqCst)
    }
}

/// A running engine: a handle plus the task to join.
#[derive(Debug)]
pub struct Engine {
    handle: EngineHandle,
    task: JoinHandle<()>,
}

impl Engine {
    /// A cloneable handle to this engine.
    pub fn handle(&self) -> EngineHandle {
        self.handle.clone()
    }

    /// Waits for the engine task to finish.
    ///
    /// Resolves once the in-flight regeneration (if any) has completed and its
    /// terminal event has been emitted. Consumes the engine, so the last handle it
    /// holds is dropped and the event sink closes.
    pub async fn join(self) -> Result<(), JoinError> {
        drop(self.handle);
        self.task.await
    }
}

/// Starts the engine task.
///
/// `events` is the caller's sink. The engine **moves** it, so when the task exits
/// the sender drops and the receiver observes the stream closing — that is the
/// "sink closed, no leaked task" signal, with no extra channel to wire.
pub fn spawn(operation: impl Regenerate, events: UnboundedSender<EngineEvent>) -> Engine {
    spawn_inner(operation, events, Hooks::default())
}

/// [`spawn`] with injected pause points.
#[cfg(test)]
fn spawn_with_hooks(
    operation: impl Regenerate,
    events: UnboundedSender<EngineEvent>,
    hooks: Hooks,
) -> Engine {
    spawn_inner(operation, events, hooks)
}

fn spawn_inner(
    operation: impl Regenerate,
    events: UnboundedSender<EngineEvent>,
    hooks: Hooks,
) -> Engine {
    let (commands_tx, commands_rx) = unbounded_channel();
    let closed = Arc::new(AtomicBool::new(false));
    // The task gets the same flag the handle sets. `shutdown()` flips it
    // **before** it sends the command, so the flag is visible to the engine even
    // when the command itself is still sitting unread in the channel — which is
    // the only way to close the window between the bounded drain and the
    // follow-up decision.
    let task = tokio::spawn(run(commands_rx, operation, events, closed.clone(), hooks));
    Engine {
        handle: EngineHandle {
            commands: commands_tx,
            closed,
        },
        task,
    }
}

/// The single task that owns the state machine.
async fn run(
    mut commands: UnboundedReceiver<Command>,
    operation: impl Regenerate,
    events: UnboundedSender<EngineEvent>,
    closed: Arc<AtomicBool>,
    // Underscored because the hook — and every use of it — is `cfg(test)` only,
    // so in a production build this parameter is genuinely unused.
    _hooks: Hooks,
) {
    let mut stopping = false;

    // --- Idle ---------------------------------------------------------------
    'idle: while let Some(mut current) = next_request(&mut commands, &mut stopping).await {
        // --- Running (and any dirty follow-ups) -----------------------------
        loop {
            // Started is emitted BEFORE the operation is entered, so an observer
            // never sees a run's terminal event without its start.
            if events.send(EngineEvent::GenerationStarted).is_err() {
                // Nobody is listening; there is no point regenerating for them.
                return;
            }

            // A set, not `Option<RegenerationRequest>`: the drain folds in one
            // command at a time, and `merge` rebuilds its whole set per call — so
            // merging N queued commands one by one would be quadratic and would
            // stall the terminal event behind a deep backlog. Extending a set is
            // O(log n) per path.
            let mut dirty: BTreeSet<PathBuf> = BTreeSet::new();
            let mut disconnected = false;

            // THE ONE CALL SITE. Awaited to completion inside this single task,
            // which is what bounds concurrency at one.
            let outcome = {
                let operation_future = operation.regenerate(current.clone());
                let mut operation_future = std::pin::pin!(operation_future);
                loop {
                    tokio::select! {
                        // `biased` polls in written order, and **completion comes
                        // first**. That is the whole scheduling rule:
                        //
                        // - While the operation is pending its branch yields, so
                        //   the command branch runs and folds requests into
                        //   `dirty` exactly as before.
                        // - The instant it is ready it is taken, **whatever is
                        //   queued behind it**. A command-first bias would let a
                        //   continuously-ready channel starve a finished
                        //   regeneration indefinitely: the run would be over, and
                        //   its terminal event would never be emitted, for as long
                        //   as edits kept arriving.
                        //
                        // Nothing is lost by preferring completion, because the
                        // commands are still in the channel and are drained below
                        // before any follow-up is decided.
                        biased;

                        result = &mut operation_future => break result,

                        command = commands.recv(), if !disconnected => {
                            apply(command, &mut dirty, &mut stopping, &mut disconnected);
                        }
                    }
                }
            };

            // The completion drain. Everything queued before (or during) the
            // switch is folded in with **non-blocking** receives, so a request
            // submitted before completion still lands in this run's follow-up
            // rather than waiting for the next one — and `Shutdown` found here is
            // honored *before* we decide whether to start that follow-up at all.
            //
            // A request that arrives after this drain is not lost either: it stays
            // queued, and the follow-up's own select — or the idle wait — picks it
            // up next.
            if !disconnected {
                // BOUNDED by the queue depth at this instant. Draining "until
                // empty" would never finish against a channel being refilled
                // faster than it is read — the very starvation this step exists to
                // remove, merely moved from the `select!` into the drain. Taking a
                // snapshot of the depth means the drain always terminates, however
                // hard the producer pushes.
                //
                // Commands that arrive after this snapshot are not lost: they stay
                // queued for the follow-up's own `select!`, or for the idle wait.
                for _ in 0..commands.len() {
                    match commands.try_recv() {
                        Ok(command) => {
                            apply(Some(command), &mut dirty, &mut stopping, &mut disconnected)
                        }
                        Err(TryRecvError::Empty) => break,
                        // Every handle is gone. Only `stopping` matters from here:
                        // the terminal event is still emitted below, and then this
                        // run is the last one.
                        Err(TryRecvError::Disconnected) => {
                            stopping = true;
                            break;
                        }
                    }
                }
            }

            // Exactly one terminal event per run, emitted BEFORE any follow-up
            // starts — so the sequence is always Started → terminal → Started → …
            let terminal = match outcome {
                Ok(RegenerationSuccess { partial }) => EngineEvent::GenerationSucceeded { partial },
                Err(RegenerationFailure { code, message }) => {
                    EngineEvent::GenerationFailed { code, message }
                }
            };
            if events.send(terminal).is_err() {
                return;
            }

            // Shutdown discards the follow-up: we promised to stop, and the
            // in-flight run has now had its say.
            // A pause point that exists only in tests — see [`Hooks`].
            #[cfg(test)]
            if let Some(hook) = &_hooks.before_follow_up_decision {
                hook().await;
            }

            // Consult the **shared flag**, not only the commands we happened to
            // drain. The drain is bounded by a queue length sampled at completion,
            // so a `shutdown()` landing after that sample leaves its `Shutdown`
            // command unread — and `stopping` false — while the caller has already
            // been told the engine is closing. `shutdown()` sets this flag before
            // sending, so checking it here is race-free: any shutdown requested
            // before this instant is visible, and one requested after it is, by
            // definition, after the decision.
            if stopping || closed.load(Ordering::SeqCst) {
                return;
            }

            match RegenerationRequest::new(dirty) {
                // Exactly one follow-up, however many requests arrived.
                Some(next) => current = next,
                // Nothing new: back to Idle. Success and failure take this same
                // path — a failed run must not discard a pending follow-up, since
                // that follow-up may be the very fix.
                None => continue 'idle,
            }
        }
    }
}

/// Folds one received command into the running state.
///
/// Shared by the `select!` branch and the completion drain so the two cannot
/// drift: a command must mean the same thing whether it was noticed while the
/// operation was still running or while draining after it finished.
fn apply(
    command: Option<Command>,
    dirty: &mut BTreeSet<PathBuf>,
    stopping: &mut bool,
    disconnected: &mut bool,
) {
    match command {
        // Every handle was dropped: finish what is running, then stop. The same
        // graceful path as an explicit shutdown.
        None => {
            *disconnected = true;
            *stopping = true;
        }
        Some(Command::Shutdown) => *stopping = true,
        // Requests after shutdown are ignored: the handle already reports
        // `EngineClosed`, and honoring one here would resurrect work we promised
        // to discard.
        // The paths are already sorted and deduplicated inside the request; the
        // set makes the union across requests cheap and order-independent.
        Some(Command::Submit(request)) if !*stopping => dirty.extend(request.paths),
        Some(Command::Submit(_)) => {}
    }
}

/// Waits for the next request while Idle.
///
/// Returns `None` when the engine should stop: an explicit shutdown, or every
/// handle dropped. Idle shutdown is prompt — there is nothing to finish.
async fn next_request(
    commands: &mut UnboundedReceiver<Command>,
    stopping: &mut bool,
) -> Option<RegenerationRequest> {
    loop {
        match commands.recv().await {
            None | Some(Command::Shutdown) => {
                *stopping = true;
                return None;
            }
            Some(Command::Submit(request)) => {
                if *stopping {
                    continue;
                }
                return Some(request);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::AtomicUsize;
    use std::time::Duration;
    use tokio::sync::{mpsc, oneshot};

    /// Awaits `future` with a generous watchdog.
    ///
    /// It never fires in a correct run — every wait here is unblocked by a channel
    /// the test itself drives, in microseconds. It exists because a *broken*
    /// engine (a lost dirty request, a missing event) otherwise makes these tests
    /// hang forever, which on CI means an unhelpful job timeout instead of a
    /// named failure. A watchdog is not a sleep: nothing waits on it when the
    /// code is right.
    async fn within<T>(what: &str, future: impl Future<Output = T>) -> T {
        match tokio::time::timeout(Duration::from_secs(5), future).await {
            Ok(value) => value,
            Err(_) => panic!("timed out waiting for {what} — the engine did not make progress"),
        }
    }

    fn request(paths: &[&str]) -> RegenerationRequest {
        RegenerationRequest::new(paths.iter().map(PathBuf::from)).expect("non-empty")
    }

    /// One entry into the fake operation: what it was asked to do, and the handle
    /// a test uses to finish it.
    struct Call {
        request: RegenerationRequest,
        complete: oneshot::Sender<RegenerationResult>,
    }

    /// A regeneration operation a test drives by hand.
    ///
    /// It announces each entry on `calls` and then **waits** for the test to
    /// complete it, so a run is in progress for exactly as long as the test wants.
    /// No sleeps, no clock, no timing tolerance.
    struct FakeOperation {
        calls: UnboundedSender<Call>,
        live: Arc<AtomicUsize>,
        max_live: Arc<AtomicUsize>,
    }

    impl Regenerate for FakeOperation {
        fn regenerate(
            &self,
            request: RegenerationRequest,
        ) -> Pin<Box<dyn Future<Output = RegenerationResult> + Send + '_>> {
            let calls = self.calls.clone();
            let live = self.live.clone();
            let max_live = self.max_live.clone();
            Box::pin(async move {
                let now = live.fetch_add(1, Ordering::SeqCst) + 1;
                max_live.fetch_max(now, Ordering::SeqCst);

                let (complete, finished) = oneshot::channel();
                let _ = calls.send(Call { request, complete });
                let result = finished
                    .await
                    .unwrap_or(Ok(RegenerationSuccess { partial: false }));

                live.fetch_sub(1, Ordering::SeqCst);
                result
            })
        }
    }

    /// Everything a test needs to drive one engine.
    struct Harness {
        engine: Engine,
        handle: EngineHandle,
        calls: UnboundedReceiver<Call>,
        events: UnboundedReceiver<EngineEvent>,
        max_live: Arc<AtomicUsize>,
    }

    fn harness() -> Harness {
        let (calls_tx, calls_rx) = mpsc::unbounded_channel();
        let (events_tx, events_rx) = mpsc::unbounded_channel();
        let max_live = Arc::new(AtomicUsize::new(0));
        let operation = FakeOperation {
            calls: calls_tx,
            live: Arc::new(AtomicUsize::new(0)),
            max_live: max_live.clone(),
        };
        let engine = spawn(operation, events_tx);
        let handle = engine.handle();
        Harness {
            engine,
            handle,
            calls: calls_rx,
            events: events_rx,
            max_live,
        }
    }

    fn succeed(call: Call, partial: bool) {
        call.complete
            .send(Ok(RegenerationSuccess { partial }))
            .expect("the engine is awaiting this");
    }

    fn fail(call: Call, code: &str, message: &str) {
        call.complete
            .send(Err(RegenerationFailure::new(code, message)))
            .expect("the engine is awaiting this");
    }

    /// Drains the event stream after the engine has stopped.
    async fn drain(events: &mut UnboundedReceiver<EngineEvent>) -> Vec<EngineEvent> {
        let mut collected = Vec::new();
        while let Some(event) = within("the event stream to close", events.recv()).await {
            collected.push(event);
        }
        collected
    }

    // --- request shape ----------------------------------------------------

    #[test]
    fn a_request_sorts_and_deduplicates_its_paths() {
        let request = request(&["/w/z.rs", "/w/a.rs", "/w/z.rs", "/w/m.rs"]);
        assert_eq!(
            request.paths(),
            [
                PathBuf::from("/w/a.rs"),
                PathBuf::from("/w/m.rs"),
                PathBuf::from("/w/z.rs")
            ]
        );
        assert_eq!(request.len(), 3);
        assert!(!request.is_empty());
    }

    #[test]
    fn an_empty_request_cannot_be_built() {
        // The operation can therefore never be entered with nothing to do.
        assert_eq!(RegenerationRequest::new(Vec::new()), None);
    }

    #[test]
    fn merging_keeps_the_union_sorted_and_deduplicated() {
        let mut first = request(&["/w/b.rs", "/w/a.rs"]);
        first.merge(request(&["/w/c.rs", "/w/a.rs"]));
        assert_eq!(
            first.paths(),
            [
                PathBuf::from("/w/a.rs"),
                PathBuf::from("/w/b.rs"),
                PathBuf::from("/w/c.rs")
            ]
        );
    }

    // --- one request, one run ---------------------------------------------

    #[tokio::test]
    async fn one_request_starts_exactly_one_run_and_emits_started_then_succeeded() {
        let mut h = harness();
        h.handle.submit(request(&["/w/a.rs"])).unwrap();

        let call = within("a call into the operation", h.calls.recv())
            .await
            .expect("the operation was entered");
        assert_eq!(call.request.paths(), [PathBuf::from("/w/a.rs")]);
        // Started is emitted before the operation is entered.
        assert_eq!(
            within("an engine event", h.events.recv()).await,
            Some(EngineEvent::GenerationStarted)
        );

        succeed(call, false);
        assert_eq!(
            within("an engine event", h.events.recv()).await,
            Some(EngineEvent::GenerationSucceeded { partial: false })
        );

        h.handle.shutdown().unwrap();
        within("the engine task to join", h.engine.join())
            .await
            .unwrap();
        assert!(
            within("a call into the operation", h.calls.recv())
                .await
                .is_none(),
            "exactly one run"
        );
        assert_eq!(drain(&mut h.events).await, [], "no further events");
    }

    #[tokio::test]
    async fn a_failure_emits_started_then_failed_with_the_callers_values_unchanged() {
        let mut h = harness();
        h.handle.submit(request(&["/w/a.rs"])).unwrap();
        let call = within("a call into the operation", h.calls.recv())
            .await
            .unwrap();
        assert_eq!(
            within("an engine event", h.events.recv()).await,
            Some(EngineEvent::GenerationStarted)
        );

        fail(call, "rustdoc_failed", "the crate did not compile");
        assert_eq!(
            within("an engine event", h.events.recv()).await,
            Some(EngineEvent::GenerationFailed {
                code: "rustdoc_failed".into(),
                message: "the crate did not compile".into(),
            }),
            "transported verbatim: the engine sanitizes nothing"
        );

        h.handle.shutdown().unwrap();
        within("the engine task to join", h.engine.join())
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn the_partial_flag_reaches_the_success_event() {
        let mut h = harness();
        h.handle.submit(request(&["/w/a.rs"])).unwrap();
        let call = within("a call into the operation", h.calls.recv())
            .await
            .unwrap();
        within("an engine event", h.events.recv()).await;
        succeed(call, true);
        assert_eq!(
            within("an engine event", h.events.recv()).await,
            Some(EngineEvent::GenerationSucceeded { partial: true })
        );
        h.handle.shutdown().unwrap();
        within("the engine task to join", h.engine.join())
            .await
            .unwrap();
    }

    // --- single flight ----------------------------------------------------

    #[tokio::test]
    async fn ten_requests_during_one_run_schedule_exactly_one_follow_up() {
        let mut h = harness();
        h.handle.submit(request(&["/w/first.rs"])).unwrap();
        let first = within("a call into the operation", h.calls.recv())
            .await
            .unwrap();

        // Ten changes land while the first run is still in flight.
        for index in 0..10 {
            h.handle
                .submit(request(&[&format!("/w/during{index}.rs")]))
                .unwrap();
        }
        // Not a single extra entry into the operation.
        assert!(
            h.calls.try_recv().is_err(),
            "a second run must not start while one is running"
        );

        succeed(first, false);

        // Exactly one follow-up, carrying all ten paths merged.
        let second = within("a call into the operation", h.calls.recv())
            .await
            .expect("one follow-up");
        assert_eq!(second.request.len(), 10);
        succeed(second, false);

        h.handle.shutdown().unwrap();
        within("the engine task to join", h.engine.join())
            .await
            .unwrap();
        assert!(
            within("a call into the operation", h.calls.recv())
                .await
                .is_none(),
            "exactly two runs total"
        );
    }

    #[tokio::test]
    async fn the_follow_ups_paths_are_merged_sorted_and_deduplicated() {
        let mut h = harness();
        h.handle.submit(request(&["/w/start.rs"])).unwrap();
        let first = within("a call into the operation", h.calls.recv())
            .await
            .unwrap();

        h.handle.submit(request(&["/w/z.rs", "/w/a.rs"])).unwrap();
        h.handle.submit(request(&["/w/a.rs", "/w/m.rs"])).unwrap();
        h.handle.submit(request(&["/w/z.rs"])).unwrap();

        succeed(first, false);
        let second = within("a call into the operation", h.calls.recv())
            .await
            .unwrap();
        assert_eq!(
            second.request.paths(),
            [
                PathBuf::from("/w/a.rs"),
                PathBuf::from("/w/m.rs"),
                PathBuf::from("/w/z.rs")
            ],
            "three submissions, six paths, one merged set"
        );
        succeed(second, false);
        h.handle.shutdown().unwrap();
        within("the engine task to join", h.engine.join())
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn requests_during_the_follow_up_schedule_exactly_one_third_run() {
        let mut h = harness();
        h.handle.submit(request(&["/w/one.rs"])).unwrap();
        let first = within("a call into the operation", h.calls.recv())
            .await
            .unwrap();

        h.handle.submit(request(&["/w/two.rs"])).unwrap();
        succeed(first, false);

        let second = within("a call into the operation", h.calls.recv())
            .await
            .unwrap();
        assert_eq!(second.request.paths(), [PathBuf::from("/w/two.rs")]);
        // More changes during the follow-up.
        h.handle.submit(request(&["/w/three.rs"])).unwrap();
        h.handle.submit(request(&["/w/three.rs"])).unwrap();
        succeed(second, false);

        let third = within("a call into the operation", h.calls.recv())
            .await
            .expect("one third run");
        assert_eq!(third.request.paths(), [PathBuf::from("/w/three.rs")]);
        succeed(third, false);

        h.handle.shutdown().unwrap();
        within("the engine task to join", h.engine.join())
            .await
            .unwrap();
        assert!(
            within("a call into the operation", h.calls.recv())
                .await
                .is_none(),
            "exactly three runs"
        );
    }

    #[tokio::test]
    async fn a_failed_run_still_starts_its_dirty_follow_up() {
        // The follow-up may be the very fix for the failure; discarding it would
        // strand the user until they saved again.
        let mut h = harness();
        h.handle.submit(request(&["/w/broken.rs"])).unwrap();
        let first = within("a call into the operation", h.calls.recv())
            .await
            .unwrap();

        h.handle.submit(request(&["/w/fixed.rs"])).unwrap();
        fail(first, "rustdoc_failed", "boom");

        let second = within("a call into the operation", h.calls.recv())
            .await
            .expect("failure must not eat the dirty set");
        assert_eq!(second.request.paths(), [PathBuf::from("/w/fixed.rs")]);
        succeed(second, false);

        h.handle.shutdown().unwrap();
        within("the engine task to join", h.engine.join())
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn the_maximum_observed_concurrency_is_one() {
        let mut h = harness();
        for index in 0..5 {
            h.handle
                .submit(request(&[&format!("/w/{index}.rs")]))
                .unwrap();
        }
        // Drive several runs to completion, keeping requests flowing.
        let first = within("a call into the operation", h.calls.recv())
            .await
            .unwrap();
        h.handle.submit(request(&["/w/more.rs"])).unwrap();
        succeed(first, false);
        let second = within("a call into the operation", h.calls.recv())
            .await
            .unwrap();
        h.handle.submit(request(&["/w/even-more.rs"])).unwrap();
        succeed(second, false);
        let third = within("a call into the operation", h.calls.recv())
            .await
            .unwrap();
        succeed(third, false);

        h.handle.shutdown().unwrap();
        within("the engine task to join", h.engine.join())
            .await
            .unwrap();
        assert_eq!(
            h.max_live.load(Ordering::SeqCst),
            1,
            "two pipelines must never run at once"
        );
    }

    // --- event ordering ---------------------------------------------------

    #[tokio::test]
    async fn the_dirty_follow_up_sequence_is_started_terminal_started_terminal() {
        let mut h = harness();
        h.handle.submit(request(&["/w/a.rs"])).unwrap();
        let first = within("a call into the operation", h.calls.recv())
            .await
            .unwrap();
        h.handle.submit(request(&["/w/b.rs"])).unwrap();
        succeed(first, false);
        let second = within("a call into the operation", h.calls.recv())
            .await
            .unwrap();
        fail(second, "boom", "second run failed");

        h.handle.shutdown().unwrap();
        within("the engine task to join", h.engine.join())
            .await
            .unwrap();

        assert_eq!(
            drain(&mut h.events).await,
            [
                EngineEvent::GenerationStarted,
                EngineEvent::GenerationSucceeded { partial: false },
                EngineEvent::GenerationStarted,
                EngineEvent::GenerationFailed {
                    code: "boom".into(),
                    message: "second run failed".into(),
                },
            ]
        );
    }

    #[tokio::test]
    async fn a_terminal_event_always_precedes_the_next_started() {
        let mut h = harness();
        h.handle.submit(request(&["/w/a.rs"])).unwrap();
        let first = within("a call into the operation", h.calls.recv())
            .await
            .unwrap();
        h.handle.submit(request(&["/w/b.rs"])).unwrap();
        succeed(first, false);

        // The follow-up has been entered; the sink must already show the first
        // run's terminal event before its Started.
        let second = within("a call into the operation", h.calls.recv())
            .await
            .unwrap();
        succeed(second, false);

        h.handle.shutdown().unwrap();
        within("the engine task to join", h.engine.join())
            .await
            .unwrap();

        let events = drain(&mut h.events).await;
        // Structural check: Started/terminal strictly alternate, starting with
        // Started and ending with a terminal.
        let mut expect_start = true;
        for event in &events {
            if expect_start {
                assert_eq!(event, &EngineEvent::GenerationStarted, "{events:?}");
            } else {
                assert!(event.is_terminal(), "{events:?}");
            }
            expect_start = !expect_start;
        }
        assert!(expect_start, "ends on a terminal: {events:?}");
        assert_eq!(events.len(), 4);
    }

    #[tokio::test]
    async fn no_changed_path_appears_in_any_public_event() {
        let mut h = harness();
        h.handle
            .submit(request(&["/home/someone/secret-project/src/lib.rs"]))
            .unwrap();
        let call = within("a call into the operation", h.calls.recv())
            .await
            .unwrap();
        fail(call, "code", "message");
        h.handle.shutdown().unwrap();
        within("the engine task to join", h.engine.join())
            .await
            .unwrap();

        for event in drain(&mut h.events).await {
            let rendered = format!("{event:?}");
            assert!(
                !rendered.contains("secret-project") && !rendered.contains("/home/"),
                "an event leaked a path: {rendered}"
            );
        }
    }

    // --- shutdown ---------------------------------------------------------

    #[tokio::test]
    async fn shutdown_while_idle_exits_promptly_and_closes_the_sink() {
        let mut h = harness();
        h.handle.shutdown().unwrap();
        within("the engine task to join", h.engine.join())
            .await
            .unwrap();
        assert_eq!(drain(&mut h.events).await, [], "no run, so no events");
        assert!(
            within("a call into the operation", h.calls.recv())
                .await
                .is_none(),
            "the operation never ran"
        );
    }

    #[tokio::test]
    async fn shutdown_while_running_lets_the_in_flight_run_finish_and_report() {
        // A blocking cargo child cannot be force-cancelled, so the honest contract
        // is: stop accepting work, let the current run finish, then exit.
        let mut h = harness();
        h.handle.submit(request(&["/w/a.rs"])).unwrap();
        let call = within("a call into the operation", h.calls.recv())
            .await
            .unwrap();

        h.handle.shutdown().unwrap();
        succeed(call, false);
        within("the engine task to join", h.engine.join())
            .await
            .unwrap();

        assert_eq!(
            drain(&mut h.events).await,
            [
                EngineEvent::GenerationStarted,
                EngineEvent::GenerationSucceeded { partial: false },
            ],
            "the in-flight run still had its say"
        );
    }

    #[tokio::test]
    async fn dirty_work_is_discarded_once_shutdown_is_requested() {
        let mut h = harness();
        h.handle.submit(request(&["/w/a.rs"])).unwrap();
        let call = within("a call into the operation", h.calls.recv())
            .await
            .unwrap();

        // A change lands, then shutdown, then the run finishes.
        h.handle.submit(request(&["/w/b.rs"])).unwrap();
        h.handle.shutdown().unwrap();
        succeed(call, false);

        within("the engine task to join", h.engine.join())
            .await
            .unwrap();
        assert!(
            within("a call into the operation", h.calls.recv())
                .await
                .is_none(),
            "the follow-up must not start after shutdown"
        );
        assert_eq!(
            drain(&mut h.events).await,
            [
                EngineEvent::GenerationStarted,
                EngineEvent::GenerationSucceeded { partial: false },
            ]
        );
    }

    #[tokio::test]
    async fn a_request_after_shutdown_fails_with_a_typed_error() {
        let h = harness();
        h.handle.shutdown().unwrap();
        assert!(h.handle.is_closed());
        assert_eq!(h.handle.submit(request(&["/w/a.rs"])), Err(EngineClosed));
        // Idempotent, and never a panic.
        assert_eq!(h.handle.shutdown(), Ok(()));
        within("the engine task to join", h.engine.join())
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn a_cloned_handle_shares_the_closed_state() {
        let h = harness();
        let clone = h.handle.clone();
        clone.shutdown().unwrap();
        assert!(
            h.handle.is_closed(),
            "shutdown is engine-wide, not per-handle"
        );
        assert_eq!(h.handle.submit(request(&["/w/a.rs"])), Err(EngineClosed));
        within("the engine task to join", h.engine.join())
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn dropping_every_handle_shuts_down_gracefully_while_idle() {
        let (calls_tx, mut calls_rx) = mpsc::unbounded_channel::<Call>();
        let (events_tx, mut events_rx) = mpsc::unbounded_channel();
        let engine = spawn(
            FakeOperation {
                calls: calls_tx,
                live: Arc::new(AtomicUsize::new(0)),
                max_live: Arc::new(AtomicUsize::new(0)),
            },
            events_tx,
        );
        // `join` drops the engine's own handle; no other handle exists.
        within("the engine task to join", engine.join())
            .await
            .unwrap();
        assert!(
            within("a call into the operation", calls_rx.recv())
                .await
                .is_none()
        );
        assert_eq!(drain(&mut events_rx).await, []);
    }

    #[tokio::test]
    async fn dropping_every_handle_while_running_still_finishes_the_run() {
        let mut h = harness();
        h.handle.submit(request(&["/w/a.rs"])).unwrap();
        let call = within("a call into the operation", h.calls.recv())
            .await
            .unwrap();

        // Every handle goes away mid-run: the same graceful path as shutdown.
        drop(h.handle);
        let engine = h.engine;
        let joined =
            tokio::spawn(async move { within("the engine task to join", engine.join()).await });
        succeed(call, false);
        within("the join task", joined).await.unwrap().unwrap();

        assert_eq!(
            drain(&mut h.events).await,
            [
                EngineEvent::GenerationStarted,
                EngineEvent::GenerationSucceeded { partial: false },
            ]
        );
    }

    // --- determinism ------------------------------------------------------

    #[tokio::test]
    async fn the_same_scenario_produces_identical_call_and_event_sequences() {
        async fn scenario() -> (Vec<Vec<PathBuf>>, Vec<EngineEvent>) {
            let mut h = harness();
            h.handle.submit(request(&["/w/a.rs"])).unwrap();
            let first = within("a call into the operation", h.calls.recv())
                .await
                .unwrap();
            let mut calls = vec![first.request.paths().to_vec()];

            h.handle.submit(request(&["/w/c.rs", "/w/b.rs"])).unwrap();
            h.handle.submit(request(&["/w/b.rs"])).unwrap();
            succeed(first, false);

            let second = within("a call into the operation", h.calls.recv())
                .await
                .unwrap();
            calls.push(second.request.paths().to_vec());
            fail(second, "x", "y");

            h.handle.shutdown().unwrap();
            within("the engine task to join", h.engine.join())
                .await
                .unwrap();
            (calls, drain(&mut h.events).await)
        }

        let first = scenario().await;
        assert_eq!(
            first.0,
            vec![
                vec![PathBuf::from("/w/a.rs")],
                vec![PathBuf::from("/w/b.rs"), PathBuf::from("/w/c.rs")],
            ]
        );
        for _ in 0..10 {
            assert_eq!(scenario().await, first, "no clock, no sleeps, no variance");
        }
    }

    // --- completion fairness (step 2.1) -----------------------------------

    #[tokio::test]
    async fn a_completed_run_is_observed_even_with_the_command_channel_backlogged() {
        // The starvation case. Under a command-first bias the engine would keep
        // servicing this backlog and never notice that the run had finished, so
        // the terminal event would never be emitted while edits kept arriving.
        // Completion is polled first, so the depth of the backlog is irrelevant.
        let mut h = harness();
        h.handle.submit(request(&["/w/start.rs"])).unwrap();
        let call = within("the first call", h.calls.recv()).await.unwrap();
        assert_eq!(
            within("Started", h.events.recv()).await,
            Some(EngineEvent::GenerationStarted)
        );

        // A deep backlog, queued while the run is in flight and still unread.
        for index in 0..5_000 {
            h.handle
                .submit(request(&[&format!("/w/backlog{index}.rs")]))
                .unwrap();
        }
        succeed(call, false);

        // The terminal event arrives despite 5,000 unread commands.
        assert_eq!(
            within("the terminal event under backlog", h.events.recv()).await,
            Some(EngineEvent::GenerationSucceeded { partial: false })
        );

        // And the backlog was not dropped: it is one merged follow-up.
        let follow_up = within("the follow-up", h.calls.recv()).await.unwrap();
        assert_eq!(
            follow_up.request.len(),
            5_000,
            "every queued path merged, once"
        );
        succeed(follow_up, false);

        h.handle.shutdown().unwrap();
        within("join", h.engine.join()).await.unwrap();
    }

    #[tokio::test]
    async fn every_request_queued_before_completion_lands_in_exactly_one_follow_up() {
        // The property the old command-first bias existed to protect. It still
        // holds — not from polling order, but because the drain empties the
        // channel before any follow-up is decided.
        let mut h = harness();
        h.handle.submit(request(&["/w/start.rs"])).unwrap();
        let call = within("the first call", h.calls.recv()).await.unwrap();

        h.handle.submit(request(&["/w/z.rs", "/w/a.rs"])).unwrap();
        h.handle.submit(request(&["/w/a.rs"])).unwrap();
        h.handle.submit(request(&["/w/m.rs"])).unwrap();
        succeed(call, false);

        let follow_up = within("the follow-up", h.calls.recv()).await.unwrap();
        assert_eq!(
            follow_up.request.paths(),
            [
                PathBuf::from("/w/a.rs"),
                PathBuf::from("/w/m.rs"),
                PathBuf::from("/w/z.rs")
            ],
            "three submissions, four paths, one merged follow-up"
        );
        succeed(follow_up, false);
        assert!(
            h.calls.try_recv().is_err(),
            "exactly one follow-up, not three"
        );

        h.handle.shutdown().unwrap();
        within("join", h.engine.join()).await.unwrap();
    }

    #[tokio::test]
    async fn a_request_submitted_after_the_completion_drain_is_not_lost() {
        // The drain empties the queue, the run ends with nothing dirty, and the
        // engine returns to Idle. A request submitted strictly *after* that point
        // must still start a run rather than fall between the two states.
        let mut h = harness();
        h.handle.submit(request(&["/w/first.rs"])).unwrap();
        let first = within("the first call", h.calls.recv()).await.unwrap();
        succeed(first, false);

        // Observing the terminal event proves the drain has already run and found
        // nothing: the engine is now Idle.
        assert_eq!(
            within("Started", h.events.recv()).await,
            Some(EngineEvent::GenerationStarted)
        );
        assert_eq!(
            within("terminal", h.events.recv()).await,
            Some(EngineEvent::GenerationSucceeded { partial: false })
        );

        h.handle.submit(request(&["/w/second.rs"])).unwrap();
        let second = within("the second call", h.calls.recv()).await.unwrap();
        assert_eq!(second.request.paths(), [PathBuf::from("/w/second.rs")]);
        succeed(second, false);

        h.handle.shutdown().unwrap();
        within("join", h.engine.join()).await.unwrap();
    }

    #[tokio::test]
    async fn shutdown_queued_among_requests_suppresses_the_follow_up_but_keeps_the_terminal() {
        // The queue at completion is [Submit, Submit, Shutdown]. The drain must
        // honor the Shutdown it finds *behind* those submits before deciding
        // whether to start a follow-up — otherwise the merged dirty set would
        // start a run we promised not to.
        let mut h = harness();
        h.handle.submit(request(&["/w/start.rs"])).unwrap();
        let call = within("the first call", h.calls.recv()).await.unwrap();

        h.handle.submit(request(&["/w/a.rs"])).unwrap();
        h.handle.submit(request(&["/w/b.rs"])).unwrap();
        h.handle.shutdown().unwrap();
        succeed(call, false);

        within("join", h.engine.join()).await.unwrap();

        assert_eq!(
            drain(&mut h.events).await,
            [
                EngineEvent::GenerationStarted,
                EngineEvent::GenerationSucceeded { partial: false },
            ],
            "the in-flight run still reported"
        );
        assert!(
            within("the call stream to close", h.calls.recv())
                .await
                .is_none(),
            "the dirty follow-up was discarded"
        );
    }

    #[tokio::test]
    async fn events_still_alternate_strictly_under_a_backlog() {
        let mut h = harness();
        h.handle.submit(request(&["/w/start.rs"])).unwrap();
        let call = within("the first call", h.calls.recv()).await.unwrap();
        for index in 0..500 {
            h.handle
                .submit(request(&[&format!("/w/b{index}.rs")]))
                .unwrap();
        }
        succeed(call, false);
        let follow_up = within("the follow-up", h.calls.recv()).await.unwrap();
        fail(follow_up, "code", "message");

        h.handle.shutdown().unwrap();
        within("join", h.engine.join()).await.unwrap();

        let events = drain(&mut h.events).await;
        assert_eq!(events.len(), 4, "two runs: {events:?}");
        let mut expect_start = true;
        for event in &events {
            if expect_start {
                assert_eq!(event, &EngineEvent::GenerationStarted, "{events:?}");
            } else {
                assert!(event.is_terminal(), "{events:?}");
            }
            expect_start = !expect_start;
        }
        assert!(expect_start, "ends on a terminal: {events:?}");
    }

    #[tokio::test]
    async fn concurrency_stays_at_one_under_a_backlog() {
        let mut h = harness();
        h.handle.submit(request(&["/w/start.rs"])).unwrap();
        let call = within("the first call", h.calls.recv()).await.unwrap();
        for index in 0..1_000 {
            h.handle
                .submit(request(&[&format!("/w/b{index}.rs")]))
                .unwrap();
        }
        succeed(call, false);
        let follow_up = within("the follow-up", h.calls.recv()).await.unwrap();
        succeed(follow_up, false);

        h.handle.shutdown().unwrap();
        within("join", h.engine.join()).await.unwrap();
        assert_eq!(
            h.max_live.load(Ordering::SeqCst),
            1,
            "the drain must not start a second operation"
        );
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn a_completed_run_reports_even_while_requests_arrive_continuously() {
        // The starvation scenario itself, and the only test here that can exhibit
        // it: a *continuously refilled* command channel, fed from another thread
        // so the queue is never empty when the engine polls it.
        //
        // Completion-first means the finished run is taken on the very next poll,
        // whatever is queued behind it. Under a command-first bias the engine
        // would keep servicing this flood and the terminal event would never be
        // emitted — the watchdog fires and this test fails.
        //
        // The flooder stops as soon as the terminal event is observed, so the test
        // terminates on the engine's own signal rather than on a timer. With
        // correct code the outcome is deterministic: the terminal always arrives.
        let mut h = harness();
        h.handle.submit(request(&["/w/start.rs"])).unwrap();
        let call = within("the first call", h.calls.recv()).await.unwrap();
        assert_eq!(
            within("Started", h.events.recv()).await,
            Some(EngineEvent::GenerationStarted)
        );

        let keep_flooding = Arc::new(AtomicBool::new(true));
        let submitted = Arc::new(AtomicUsize::new(0));
        let flooder = {
            let handle = h.handle.clone();
            let keep_flooding = keep_flooding.clone();
            let submitted = submitted.clone();
            std::thread::spawn(move || {
                let mut index = 0u64;
                while keep_flooding.load(Ordering::SeqCst) {
                    // Never empty: the next command is queued before the engine
                    // can drain the last one.
                    if handle
                        .submit(request(&[&format!("/w/flood{index}.rs")]))
                        .is_err()
                    {
                        break;
                    }
                    index += 1;
                    submitted.store(index as usize, Ordering::SeqCst);
                }
            })
        };

        // Wait until the flood is genuinely running and well ahead of the engine,
        // so the channel is backed up at the moment the run completes. Spinning on
        // the flooder's own counter, not on a clock.
        while submitted.load(Ordering::SeqCst) < 10_000 {
            std::hint::spin_loop();
        }
        succeed(call, false);

        // The claim: this arrives despite the channel never going quiet.
        let terminal = within(
            "the terminal event while requests arrive continuously",
            h.events.recv(),
        )
        .await;
        assert_eq!(
            terminal,
            Some(EngineEvent::GenerationSucceeded { partial: false })
        );

        keep_flooding.store(false, Ordering::SeqCst);
        flooder.join().expect("the flooder thread");
        assert!(
            submitted.load(Ordering::SeqCst) >= 10_000,
            "the flood must have actually run"
        );

        h.handle.shutdown().unwrap();
        // The flood scheduled a follow-up, and it is waiting on us. Finish every
        // run the engine enters until it drops the operation and the call stream
        // closes — that is the engine exiting, and it is what makes `join` return.
        while let Some(call) = within("the call stream to close", h.calls.recv()).await {
            let _ = call
                .complete
                .send(Ok(RegenerationSuccess { partial: false }));
        }
        within("join", h.engine.join()).await.unwrap();
        assert_eq!(
            h.max_live.load(Ordering::SeqCst),
            1,
            "still single-flight, however hard the flood pushed"
        );
    }

    // --- the shutdown/follow-up race (step 2.2) ---------------------------

    /// A hook that parks the engine at its first follow-up decision.
    ///
    /// Returns `(hooks, reached, release)`: `reached` fires when the engine is
    /// parked — i.e. the run completed and the bounded drain has already finished
    /// — and sending on `release` lets it proceed to the decision. Only the first
    /// decision is parked; later ones pass straight through.
    fn parked_at_follow_up_decision() -> (Hooks, mpsc::UnboundedReceiver<()>, oneshot::Sender<()>) {
        let (reached_tx, reached_rx) = mpsc::unbounded_channel();
        let (release_tx, release_rx) = oneshot::channel();
        let release = Arc::new(std::sync::Mutex::new(Some(release_rx)));

        let hooks = Hooks {
            before_follow_up_decision: Some(Arc::new(move || {
                let reached = reached_tx.clone();
                let release = release.clone();
                Box::pin(async move {
                    // Take the receiver without holding the lock across the await.
                    let waiting = release.lock().expect("hook mutex").take();
                    if let Some(waiting) = waiting {
                        let _ = reached.send(());
                        let _ = waiting.await;
                    }
                })
            })),
        };
        (hooks, reached_rx, release_tx)
    }

    #[tokio::test]
    async fn shutdown_between_the_drain_and_the_follow_up_decision_discards_the_follow_up() {
        // The exact interleaving, forced rather than hoped for:
        //
        //   1. a regeneration is running;
        //   2. dirty work is queued behind it;
        //   3. the run completes and the bounded drain finishes (it sampled the
        //      queue length and consumed the dirty request);
        //   4. the engine parks immediately before its follow-up decision;
        //   5. shutdown is requested — its `Shutdown` command lands in the channel
        //      AFTER the drain's sample, so no drain will ever see it;
        //   6. the engine is released;
        //   7..9. terminal emitted, no follow-up, join completes, submit refused.
        //
        // Only the shared `closed` flag can catch this: the command stream cannot.
        let (calls_tx, mut calls) = mpsc::unbounded_channel();
        let (events_tx, mut events) = mpsc::unbounded_channel();
        let max_live = Arc::new(AtomicUsize::new(0));
        let (hooks, mut reached, release) = parked_at_follow_up_decision();

        let engine = spawn_with_hooks(
            FakeOperation {
                calls: calls_tx,
                live: Arc::new(AtomicUsize::new(0)),
                max_live: max_live.clone(),
            },
            events_tx,
            hooks,
        );
        let handle = engine.handle();

        // 1. running
        handle.submit(request(&["/w/running.rs"])).unwrap();
        let call = within("the first call", calls.recv()).await.unwrap();
        assert_eq!(
            within("Started", events.recv()).await,
            Some(EngineEvent::GenerationStarted)
        );

        // 2. dirty work queued while it runs
        handle.submit(request(&["/w/dirty.rs"])).unwrap();

        // 3 + 4. complete it; the engine drains, emits the terminal, and parks.
        succeed(call, false);
        within(
            "the engine to park at its follow-up decision",
            reached.recv(),
        )
        .await
        .expect("the hook must fire");

        // 7 (early). The terminal event is already out: it is emitted before the
        // decision, so shutting down now cannot suppress it.
        assert_eq!(
            within("the terminal event", events.recv()).await,
            Some(EngineEvent::GenerationSucceeded { partial: false })
        );

        // 5. shutdown lands in the window the drain can no longer see.
        handle.shutdown().unwrap();

        // 6. release the engine into its decision.
        release.send(()).expect("the engine is parked here");

        // 8. no second run: no Started, no call.
        within("the engine task to join", engine.join())
            .await
            .unwrap();
        assert!(
            within("the call stream to close", calls.recv())
                .await
                .is_none(),
            "the dirty follow-up must not start after shutdown"
        );
        assert_eq!(
            drain(&mut events).await,
            [],
            "no second GenerationStarted after the terminal event"
        );
        assert_eq!(max_live.load(Ordering::SeqCst), 1, "still single-flight");

        // 9. the handle is closed for good.
        assert_eq!(handle.submit(request(&["/w/late.rs"])), Err(EngineClosed));
    }

    #[tokio::test]
    async fn without_shutdown_the_parked_decision_still_starts_exactly_one_follow_up() {
        // The control for the test above: same forced interleaving, no shutdown.
        // The follow-up must still run — otherwise the fix would be indistinguish-
        // able from simply dropping dirty work after every run.
        let (calls_tx, mut calls) = mpsc::unbounded_channel();
        let (events_tx, mut events) = mpsc::unbounded_channel();
        let (hooks, mut reached, release) = parked_at_follow_up_decision();

        let engine = spawn_with_hooks(
            FakeOperation {
                calls: calls_tx,
                live: Arc::new(AtomicUsize::new(0)),
                max_live: Arc::new(AtomicUsize::new(0)),
            },
            events_tx,
            hooks,
        );
        let handle = engine.handle();

        handle.submit(request(&["/w/running.rs"])).unwrap();
        let call = within("the first call", calls.recv()).await.unwrap();
        handle.submit(request(&["/w/dirty.rs"])).unwrap();
        succeed(call, false);

        within("the engine to park", reached.recv())
            .await
            .expect("the hook must fire");
        release.send(()).expect("the engine is parked here");

        // The dirty request the drain collected becomes exactly one follow-up.
        let follow_up = within("the follow-up", calls.recv()).await.unwrap();
        assert_eq!(follow_up.request.paths(), [PathBuf::from("/w/dirty.rs")]);
        succeed(follow_up, false);

        handle.shutdown().unwrap();
        within("join", engine.join()).await.unwrap();
        assert_eq!(
            drain(&mut events).await,
            [
                EngineEvent::GenerationStarted,
                EngineEvent::GenerationSucceeded { partial: false },
                EngineEvent::GenerationStarted,
                EngineEvent::GenerationSucceeded { partial: false },
            ]
        );
    }
}
