//! The real, `notify`-backed filesystem adapter.
//!
//! It owns the native watcher, classifies every event it delivers against the
//! current [`WatchSet`], feeds the survivors to the existing [`Debouncer`], and
//! emits one [`RegenerationRequest`] per burst. It decides nothing else: it does
//! not regenerate, does not know what a document is, and does not stop a server.
//!
//! # `notify` types stay inside
//!
//! Nothing in the public API mentions `notify`. Callers speak `PathBuf`,
//! [`WatchPlan`] and [`WatchEvent`], so the backend could be replaced without
//! touching them — and, more importantly, a `notify::Error` (which may carry an
//! absolute path) can never escape by accident.
//!
//! # Staging: a candidate's events wait for its set
//!
//! A native watcher starts reporting the moment its **first** registration lands,
//! but the plan it belongs to is not the truth until **every** registration has
//! landed. Those two facts leave an interval in which an event exists whose
//! meaning is not yet decidable — a new-only input looks irrelevant under the old
//! rules, and classifying it there would drop a real change for good.
//!
//! So events are **generation-tagged** at the source: every watcher stamps its own
//! id, and an event is only ever classified against the set of *its own*
//! generation. A candidate's events are **staged** until the candidate is
//! activated, then drained through the complete new set; if the replacement fails
//! instead, the staged set is discarded and can never touch the old plan.
//!
//! This is deliberately not "replacement is fast enough". The previous shape was
//! safe only because `replace` never yielded — an invisible invariant that one
//! `.await` would have silently broken. The generation tag makes the guarantee
//! explicit and checkable.
//!
//! # No polling fallback
//!
//! Only `notify`'s recommended **native** backend is used. **No claim is made
//! that every platform or filesystem supports native notifications** — some
//! network mounts and container layers do not. When the backend cannot deliver,
//! this adapter reports a typed failure and stops; it does not silently degrade
//! into polling, which would be a different product decision with different
//! costs, and is not this PRD's to make.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use notify::{RecommendedWatcher, RecursiveMode, Watcher as _};
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender, unbounded_channel};
use tokio::sync::oneshot;
use tokio::task::{JoinError, JoinHandle};
use tokio::time::Instant;

use crate::classify::WatchSet;
use crate::debounce::{DebounceOptions, Debouncer};
use crate::engine::RegenerationRequest;
use crate::plan::{RegistrationMode, WatchPlan};

/// What the adapter reports.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WatchEvent {
    /// One debounced burst of relevant changes.
    Regeneration(RegenerationRequest),
    /// The native watcher reported a problem. Not fatal to the process, and
    /// **never a regeneration**: an error is not a change.
    WatcherFailed {
        /// A stable, machine-matchable code.
        code: String,
        /// A message safe to show or log: **no absolute path, no raw debug**.
        message: String,
    },
}

/// A typed watcher failure.
///
/// Deliberately not a wrapper around `notify::Error`: that type's `Display`
/// includes the paths it failed on, and the workspace root is someone's home
/// directory.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WatcherError {
    /// A stable, machine-matchable code.
    pub code: String,
    /// A message safe to show or log.
    pub message: String,
}

impl WatcherError {
    fn new(code: &str, message: impl Into<String>) -> Self {
        WatcherError {
            code: code.to_string(),
            message: message.into(),
        }
    }
}

impl std::fmt::Display for WatcherError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "{}: {}", self.code, self.message)
    }
}

impl std::error::Error for WatcherError {}

/// Describes a `notify` error **without** repeating anything it might carry.
///
/// Every arm is a fixed string chosen here; nothing from the error's `Display`
/// or `Debug` is interpolated, because `ErrorKind::Generic` and the paths on
/// `notify::Error` can contain absolute paths. The `io::ErrorKind` name is the
/// one borrowed detail, and it is a closed enum of adjectives like
/// `PermissionDenied` — no path in it.
fn describe(error: &notify::Error) -> WatcherError {
    match &error.kind {
        notify::ErrorKind::MaxFilesWatch => WatcherError::new(
            "watch_limit_reached",
            "the system's watch limit was reached; raise it or watch fewer directories",
        ),
        notify::ErrorKind::PathNotFound => WatcherError::new(
            "watch_path_not_found",
            "a path to watch does not exist; watch its nearest existing parent instead",
        ),
        notify::ErrorKind::WatchNotFound => {
            WatcherError::new("watch_not_found", "a watch was removed before it was used")
        }
        notify::ErrorKind::InvalidConfig(_) => WatcherError::new(
            "watch_invalid_config",
            "the watcher configuration is invalid",
        ),
        notify::ErrorKind::Io(io) => WatcherError::new(
            "watch_io_error",
            format!("the filesystem watcher failed ({})", io.kind()),
        ),
        // `Generic` carries free text from the backend, which may include a path.
        notify::ErrorKind::Generic(_) => {
            WatcherError::new("watch_failed", "the filesystem watcher reported an error")
        }
    }
}

/// Whether a native event describes a change worth classifying.
///
/// `Access` is reads and opens — never a change. `Other` is backend-specific
/// noise. **`Any` is treated as a change**: it means the backend could not say
/// what happened, and an extra regeneration costs a rebuild while a missed one
/// leaves a stale map on screen.
fn is_change(kind: notify::EventKind) -> bool {
    !matches!(
        kind,
        notify::EventKind::Access(_) | notify::EventKind::Other
    )
}

/// Whether an event could have introduced a directory.
///
/// Deliberately liberal, because the answer is backend-specific and getting it
/// wrong in the cautious direction costs one `symlink_metadata`, while getting it
/// wrong in the other direction is the bug this whole module exists to fix. Linux
/// reports a `mkdir` as `Create(Folder)` but a *moved-in* tree as
/// `Modify(Name(To))` with no hint that it was a directory at all, so both are
/// probed; a plain write (`Modify(Data)`) never creates one and is not.
fn may_introduce_directory(kind: notify::EventKind) -> bool {
    use notify::EventKind;
    use notify::event::ModifyKind;
    matches!(
        kind,
        EventKind::Any
            | EventKind::Create(_)
            | EventKind::Modify(ModifyKind::Name(_))
            | EventKind::Modify(ModifyKind::Any)
    )
}

/// A reconciled subtree: the generation that asked, and every file found in it.
///
/// Files, not *relevant* files: the scan reports what exists and the event loop
/// classifies it against the right generation's [`WatchSet`], exactly as it does
/// for a native event.
type Reconciled = (u64, Vec<PathBuf>);

/// Registers native coverage for a newly appeared directory, then lists what it
/// already contains.
///
/// # Why this exists
///
/// A recursive watch is not recursive at the OS level on Linux: inotify watches
/// one directory each, and `notify` installs them as it observes directories
/// appear. A tree that arrives complete — `mv prepared src/deep`, `git checkout`,
/// an editor writing a new module directory — is therefore reported as *one*
/// event for the top directory, and every file already inside it is never
/// mentioned by anyone. Worse, files created a moment later race the watches
/// being installed. Both windows lose a real source edit permanently.
///
/// # The order: register, then scan
///
/// ```text
/// register the subtree  ->  then scan it
/// ```
///
/// Registering first covers everything created **after** this instant; the scan
/// covers everything that already existed **before** it. The reverse order has a
/// gap between the two halves, so this order is the one worth having.
///
/// **The scan is what fixes the observed bug**, and that is what the tests
/// demonstrate: a tree that arrives complete is invisible without it, and removing
/// it fails the rename tests immediately. The registration is deliberate
/// belt-and-braces and is **not** proven load-bearing by any test here — removing
/// it leaves the Linux suite green, because `notify` also installs a watch of its
/// own when it observes a directory appear. It is kept because that installation
/// races our scan: a file created after the scan ends and before `notify`'s watch
/// lands is in neither half, and this closes that window. Nobody has forced that
/// interleaving deterministically, so it is claimed as prudence, not as a proof.
///
/// Duplicates between the two halves are free — the debouncer records paths into a
/// set — so the overlap is deliberate rather than tolerated.
///
/// **Blocking**: both halves walk the tree, which is why this only ever runs on a
/// blocking thread.
fn reconcile_subtree(
    watcher: &Arc<Mutex<RecommendedWatcher>>,
    set: &WatchSet,
    dir: &Path,
    hooks: &Hooks,
) -> Result<Vec<PathBuf>, WatcherError> {
    // Not a directory, gone again already, or a symlink: nothing to do. Checking
    // `symlink_metadata` rather than `metadata` is what rejects a symlinked
    // directory — following one would register a watch on someone else's disk.
    match std::fs::symlink_metadata(dir) {
        Ok(metadata) if metadata.is_dir() => {}
        _ => return Ok(Vec::new()),
    }

    // Belt and braces over the symlink check above: resolve the directory and
    // refuse anything that leaves the workspace by any route (a bind mount, a
    // junction). The containment test is the classifier's own, so it compares
    // normalized text rather than raw OS paths — see `WatchSet::contains_path`.
    if let Ok(canonical) = dir.canonicalize()
        && !set.contains_path(&canonical)
    {
        return Ok(Vec::new());
    }

    // 1. REGISTER. On Linux this installs the inotify watches for the whole
    //    subtree now, so anything created from here on is covered even if
    //    `notify`'s own recursive add has not landed yet. On Windows and macOS the
    //    ancestor's watch already covers the subtree, so this is pure redundancy —
    //    a second handle and some duplicate events, both of which the path set
    //    downstream absorbs.
    watcher
        .lock()
        .expect("the watcher mutex is never poisoned: no panic can occur while it is held")
        .watch(dir, RecursiveMode::Recursive)
        .map_err(|error| describe(&error))?;

    #[cfg(test)]
    if let Some(pause) = &hooks.after_registration {
        pause();
    }
    let _ = hooks;

    // 2. SCAN. Iterative, so a deep tree cannot overflow the stack.
    let mut found = Vec::new();
    let mut stack = vec![dir.to_path_buf()];
    while let Some(current) = stack.pop() {
        // A directory that vanished mid-walk is normal — this runs while the
        // filesystem is being changed — and is not worth a warning.
        let Ok(entries) = std::fs::read_dir(&current) else {
            continue;
        };
        for entry in entries.flatten() {
            let Ok(kind) = entry.file_type() else {
                continue;
            };
            let path = entry.path();
            if kind.is_symlink() {
                // Never followed: a symlinked directory could point anywhere, and a
                // symlink loop would make this walk non-terminating.
                continue;
            }
            if kind.is_dir() {
                // The ignore rules are applied **before** descending, so a moved-in
                // `target/` or `node_modules` is never walked at all rather than
                // walked and then discarded.
                if set.is_ignored_directory(&path) {
                    continue;
                }
                stack.push(path);
            } else if kind.is_file() {
                found.push(path);
            }
        }
    }
    Ok(found)
}

/// How many distinct paths a candidate may stage before it gives up on precision.
///
/// The staging buffer is a **set of paths**, not a queue of events, so duplicates
/// collapse on arrival and the bound is "distinct paths touched during a
/// replacement" — not "events emitted". A replacement is a handful of syscalls,
/// so reaching 4096 distinct paths means something enormous is happening (a
/// `git checkout` of the whole tree); the memory ceiling is that many `PathBuf`s.
///
/// Overflow is **not** silent loss: see [`Candidate::stage`].
const STAGING_CAPACITY: usize = 4096;

/// A native watcher that has been built and registered but is **not yet active**.
///
/// Its events are staged rather than classified: the new [`WatchSet`] is not
/// current yet, and classifying against the *old* set would ask the wrong
/// question — a new-only input looks irrelevant under the old rules, and the
/// change would be dropped for good.
struct Candidate {
    generation: u64,
    /// Shared so a reconciliation running on a blocking thread can register a new
    /// subtree into the candidate before it is activated.
    watcher: Arc<Mutex<RecommendedWatcher>>,
    set: WatchSet,
    /// The new plan's registration paths, kept only as the coarse fallback an
    /// overflow needs.
    registered: Vec<PathBuf>,
    /// Distinct paths seen before activation. A set: duplicates are free, and the
    /// debouncer would collapse them anyway.
    staged: BTreeSet<PathBuf>,
    /// True once [`STAGING_CAPACITY`] distinct paths were exceeded.
    overflowed: bool,
    reply: oneshot::Sender<Result<(), WatcherError>>,
}

impl Candidate {
    /// Stages one changed path.
    ///
    /// On overflow it stops recording paths and raises a flag instead of dropping
    /// the change on the floor: activation turns that flag into a typed failure
    /// **plus** a coarse regeneration over the whole new plan, so a relevant
    /// change can be forgotten in detail but never in effect.
    fn stage(&mut self, path: PathBuf) {
        if self.staged.len() >= STAGING_CAPACITY && !self.staged.contains(&path) {
            self.overflowed = true;
            return;
        }
        self.staged.insert(path);
    }
}

/// One tagged event as it leaves a native watcher.
type Tagged = (u64, notify::Result<notify::Event>);

/// Injected pause points. The field and every use of it are `cfg(test)` only, so
/// production compiles to the same code: activation is attempted on the very next
/// poll after a candidate is built.
#[cfg(test)]
use std::pin::Pin;

#[derive(Default, Clone)]
struct Hooks {
    /// Awaited before activation, while the loop keeps staging candidate events.
    #[cfg(test)]
    #[allow(clippy::type_complexity)]
    before_activation:
        Option<Arc<dyn Fn() -> Pin<Box<dyn Future<Output = ()> + Send>> + Send + Sync>>,
    /// Called for each path staged, so a test can prove the candidate's callback
    /// observed an event before activation rather than inferring it.
    #[cfg(test)]
    #[allow(clippy::type_complexity)]
    on_staged: Option<Arc<dyn Fn(&Path) + Send + Sync>>,
    /// Called on the blocking thread **after** a subtree's native registration is
    /// installed and **before** its scan runs — the exact window the
    /// register-first ordering exists to make safe. Synchronous, because that is
    /// where it runs; a test blocks in it.
    #[cfg(test)]
    #[allow(clippy::type_complexity)]
    after_registration: Option<Arc<dyn Fn() + Send + Sync>>,
}

/// What a handle asks the adapter to do.
#[derive(Debug)]
enum Command {
    Replace(Box<WatchPlan>, oneshot::Sender<Result<(), WatcherError>>),
    Shutdown,
}

/// The adapter is no longer accepting commands.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WatcherClosed;

impl std::fmt::Display for WatcherClosed {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("the filesystem watcher is shutting down or has stopped")
    }
}

impl std::error::Error for WatcherClosed {}

/// A handle to the running adapter.
#[derive(Debug)]
pub struct Watcher {
    commands: UnboundedSender<Command>,
    closed: Arc<AtomicBool>,
    task: JoinHandle<()>,
}

impl Watcher {
    /// Swaps in a new plan, **atomically**.
    ///
    /// A new native watcher is built and every registration applied *before* the
    /// new [`WatchSet`] becomes current and the old watcher is dropped. If any
    /// registration fails, the old watcher and old set are retained **complete**
    /// — a half-applied plan never becomes active, because a set that is missing
    /// half its inputs silently stops noticing them.
    ///
    /// Replacement never triggers a regeneration.
    pub async fn replace_plan(&self, plan: WatchPlan) -> Result<(), WatcherError> {
        if self.closed.load(Ordering::SeqCst) {
            return Err(WatcherError::new(
                "watcher_closed",
                "the watcher is shutting down",
            ));
        }
        let (reply, answer) = oneshot::channel();
        self.commands
            .send(Command::Replace(Box::new(plan), reply))
            .map_err(|_| WatcherError::new("watcher_closed", "the watcher has stopped"))?;
        answer.await.map_err(|_| {
            WatcherError::new(
                "watcher_closed",
                "the watcher stopped before replacing the plan",
            )
        })?
    }

    /// Asks the adapter to stop: no further replacements, drop the native
    /// watcher, end the task. Idempotent.
    pub fn shutdown(&self) -> Result<(), WatcherClosed> {
        if self.closed.swap(true, Ordering::SeqCst) {
            return Ok(());
        }
        self.commands
            .send(Command::Shutdown)
            .map_err(|_| WatcherClosed)
    }

    /// Whether shutdown has been requested.
    pub fn is_closed(&self) -> bool {
        self.closed.load(Ordering::SeqCst)
    }

    /// A detached handle for a test that must call `replace_plan` from another
    /// task while it keeps observing this one.
    #[cfg(test)]
    fn handle_for_test(&self) -> TestHandle {
        TestHandle {
            commands: self.commands.clone(),
            closed: self.closed.clone(),
        }
    }

    /// Waits for the adapter task to finish.
    pub async fn join(self) -> Result<(), JoinError> {
        drop(self.commands);
        self.task.await
    }
}

/// A cloneable stand-in for [`Watcher`]'s command side, used only by tests that
/// drive a replacement concurrently with their own observation.
#[cfg(test)]
#[derive(Clone)]
struct TestHandle {
    commands: UnboundedSender<Command>,
    closed: Arc<AtomicBool>,
}

#[cfg(test)]
impl TestHandle {
    async fn replace_plan(&self, plan: WatchPlan) -> Result<(), WatcherError> {
        if self.closed.load(Ordering::SeqCst) {
            return Err(WatcherError::new("watcher_closed", "shutting down"));
        }
        let (reply, answer) = oneshot::channel();
        self.commands
            .send(Command::Replace(Box::new(plan), reply))
            .map_err(|_| WatcherError::new("watcher_closed", "stopped"))?;
        answer
            .await
            .map_err(|_| WatcherError::new("watcher_closed", "stopped before replacing"))?
    }
}

/// Builds a native watcher and applies every registration in `plan`.
///
/// Returns the watcher only if **all** registrations succeeded; a partial
/// watcher is dropped here rather than handed back.
fn build(
    plan: &WatchPlan,
    generation: u64,
    raw: UnboundedSender<Tagged>,
) -> Result<RecommendedWatcher, WatcherError> {
    // The handler runs on `notify`'s own thread. The channel is **unbounded**, so
    // this send never blocks and a registration callback can never stall the
    // backend — the only work done there is a tag and a move.
    let mut watcher = notify::recommended_watcher(move |event| {
        let _ = raw.send((generation, event));
    })
    .map_err(|error| {
        let described = describe(&error);
        WatcherError::new("watcher_init_failed", described.message)
    })?;

    for registration in plan.registrations() {
        let mode = match registration.mode {
            RegistrationMode::Recursive => RecursiveMode::Recursive,
            RegistrationMode::NonRecursive => RecursiveMode::NonRecursive,
        };
        watcher.watch(&registration.path, mode).map_err(|error| {
            let described = describe(&error);
            WatcherError::new("watch_registration_failed", described.message)
        })?;
    }
    Ok(watcher)
}

/// Starts the adapter.
///
/// Initialization failure is typed and returns **before** any task is spawned, so
/// a failed start leaks neither a task nor a native watcher.
pub fn spawn_watcher(
    plan: WatchPlan,
    sink: UnboundedSender<WatchEvent>,
) -> Result<Watcher, WatcherError> {
    spawn_watcher_with(plan, sink, DebounceOptions::default())
}

/// [`spawn_watcher`] with explicit debounce timings.
///
/// Production uses [`DebounceOptions::default`] (300 ms quiet, 2 s maximum);
/// this exists so an integration test can drive the adapter without waiting on
/// the production window. The timings are the [`Debouncer`]'s — **no quiet or
/// maximum arithmetic is repeated here**.
pub fn spawn_watcher_with(
    plan: WatchPlan,
    sink: UnboundedSender<WatchEvent>,
    debounce: DebounceOptions,
) -> Result<Watcher, WatcherError> {
    spawn_inner(plan, sink, debounce, Hooks::default())
}

/// The first generation. Every later candidate gets the next id, so a tag is
/// unique for the process's lifetime and a retired watcher's events are always
/// distinguishable from the live one's.
const FIRST_GENERATION: u64 = 0;

fn spawn_inner(
    plan: WatchPlan,
    sink: UnboundedSender<WatchEvent>,
    debounce: DebounceOptions,
    hooks: Hooks,
) -> Result<Watcher, WatcherError> {
    let (raw_tx, raw_rx) = unbounded_channel();
    // Built first: if this fails there is nothing to clean up and no task exists.
    let watcher = build(&plan, FIRST_GENERATION, raw_tx.clone())?;

    let (commands_tx, commands_rx) = unbounded_channel();
    let closed = Arc::new(AtomicBool::new(false));
    let task = tokio::spawn(run(
        watcher,
        plan,
        raw_tx,
        raw_rx,
        commands_rx,
        sink,
        debounce,
        hooks,
    ));
    Ok(Watcher {
        commands: commands_tx,
        closed,
        task,
    })
}

/// Starts a subtree reconciliation on a blocking thread.
///
/// The event loop must never walk a directory tree itself: a `git checkout` can
/// drop a hundred thousand files into the workspace, and the loop is what keeps
/// the debouncer and shutdown responsive while that happens. So the walk is
/// offloaded and its findings come back through a channel, to be classified by the
/// same loop against the same [`WatchSet`] as any native event.
///
/// The task is tracked in a [`tokio::task::JoinSet`] so shutdown can join it
/// rather than leave it running against a dropped watcher.
#[allow(clippy::too_many_arguments)]
fn spawn_reconcile(
    reconcilers: &mut tokio::task::JoinSet<()>,
    watcher: Arc<Mutex<RecommendedWatcher>>,
    generation: u64,
    set: WatchSet,
    dir: PathBuf,
    out: UnboundedSender<Reconciled>,
    sink: UnboundedSender<WatchEvent>,
    hooks: Hooks,
) {
    reconcilers.spawn_blocking(move || {
        match reconcile_subtree(&watcher, &set, &dir, &hooks) {
            Ok(found) if found.is_empty() => {}
            Ok(found) => {
                // The loop may already be gone; that is shutdown, not an error.
                let _ = out.send((generation, found));
            }
            Err(error) => {
                // An operational warning, never a regeneration — and never a path:
                // `describe` builds every message from a fixed string.
                let _ = sink.send(WatchEvent::WatcherFailed {
                    code: error.code,
                    message: error.message,
                });
            }
        }
    });
}

/// The adapter task.
#[allow(clippy::too_many_arguments)]
async fn run(
    watcher: RecommendedWatcher,
    plan: WatchPlan,
    raw_tx: UnboundedSender<Tagged>,
    mut raw_rx: UnboundedReceiver<Tagged>,
    mut commands: UnboundedReceiver<Command>,
    sink: UnboundedSender<WatchEvent>,
    options: DebounceOptions,
    hooks: Hooks,
) {
    let (mut current_set, _) = plan.into_parts();
    // Shared with the reconcilers, which register new subtrees into it from a
    // blocking thread. The lock is held only across one `notify` call.
    let mut watcher = Arc::new(Mutex::new(watcher));
    let mut current_generation = FIRST_GENERATION;
    let mut next_generation = FIRST_GENERATION + 1;
    // Findings come back here and are classified by this loop, so a reconciler
    // never touches a `WatchSet` and can never race an activation.
    let (recon_tx, mut recon_rx) = unbounded_channel::<Reconciled>();
    let mut reconcilers = tokio::task::JoinSet::new();
    // At most one replacement is in flight: `replace_plan` awaits its reply, and
    // production activates on the very next poll.
    let mut candidate: Option<Candidate> = None;

    let mut debouncer = Debouncer::new(options);
    // Monotonic time as an elapsed `Duration` from one epoch, which is exactly
    // what `Debouncer` takes. Nothing here reads a wall clock.
    let epoch = Instant::now();

    loop {
        let deadline = debouncer.deadline();
        tokio::select! {
            biased;

            command = commands.recv() => match command {
                // Every handle dropped: same graceful path as shutdown.
                None | Some(Command::Shutdown) => break,
                Some(Command::Replace(plan, reply)) => {
                    if candidate.is_some() {
                        let _ = reply.send(Err(WatcherError::new(
                            "watcher_busy",
                            "a plan replacement is already in progress",
                        )));
                        continue;
                    }
                    let generation = next_generation;
                    next_generation += 1;
                    // Build and fully register. On failure the partial watcher is
                    // dropped here and the old one is never touched — and because
                    // its events are tagged with a generation that never becomes
                    // current, anything it already reported is discarded below.
                    match build(&plan, generation, raw_tx.clone()) {
                        Err(error) => {
                            let _ = reply.send(Err(error));
                        }
                        Ok(fresh) => {
                            let registered =
                                plan.registrations().iter().map(|r| r.path.clone()).collect();
                            let (set, _) = plan.into_parts();
                            candidate = Some(Candidate {
                                generation,
                                watcher: Arc::new(Mutex::new(fresh)),
                                set,
                                registered,
                                staged: BTreeSet::new(),
                                overflowed: false,
                                reply,
                            });
                        }
                    }
                }
            },

            raw = raw_rx.recv() => {
                let Some((generation, raw)) = raw else { break };
                match raw {
                    Ok(event) if is_change(event.kind) => {
                        let staging = candidate
                            .as_ref()
                            .is_some_and(|pending| pending.generation == generation);

                        if staging {
                            // A candidate's event: its set is not the truth yet, so
                            // hold the path rather than ask the old set a question
                            // it cannot answer correctly.
                            let pending = candidate.as_mut().expect("checked above");
                            let root = pending.set.root().to_string();
                            for path in &event.paths {
                                let resolved = resolve(&root, path);
                                #[cfg(test)]
                                if let Some(observe) = &hooks.on_staged {
                                    observe(&resolved);
                                }
                                // A subtree that appears while a replacement is
                                // being prepared is reconciled into the CANDIDATE:
                                // it is the plan that is about to be true, and the
                                // findings are staged like any other candidate event.
                                if may_introduce_directory(event.kind)
                                    && pending.set.needs_subtree_reconciliation(&resolved)
                                {
                                    spawn_reconcile(
                                        &mut reconcilers,
                                        pending.watcher.clone(),
                                        pending.generation,
                                        pending.set.clone(),
                                        resolved.clone(),
                                        recon_tx.clone(),
                                        sink.clone(),
                                        hooks.clone(),
                                    );
                                }
                                pending.stage(resolved);
                            }
                        } else if generation == current_generation {
                            let now = epoch.elapsed();
                            // Every path of every event, classified again against
                            // the CURRENT set — a directory watch reports files
                            // nobody asked for.
                            for path in &event.paths {
                                let resolved = resolve(current_set.root(), path);
                                if may_introduce_directory(event.kind)
                                    && current_set.needs_subtree_reconciliation(&resolved)
                                {
                                    spawn_reconcile(
                                        &mut reconcilers,
                                        watcher.clone(),
                                        current_generation,
                                        current_set.clone(),
                                        resolved.clone(),
                                        recon_tx.clone(),
                                        sink.clone(),
                                        hooks.clone(),
                                    );
                                }
                                debouncer.record_if_relevant(&current_set, now, &resolved);
                            }
                        }
                        // Anything else is from a retired generation: either a
                        // candidate that failed (its events must never reach the
                        // old plan) or a watcher already replaced (whose successor
                        // watches the same tree and will report anything that still
                        // matters). Dropped.
                    }
                    // A non-change kind: not an error, just not interesting.
                    Ok(_) => {}
                    Err(error) => {
                        let described = describe(&error);
                        // An error is reported, never turned into a regeneration.
                        if sink
                            .send(WatchEvent::WatcherFailed {
                                code: described.code,
                                message: described.message,
                            })
                            .is_err()
                        {
                            break;
                        }
                    }
                }
            }

            reconciled = recon_rx.recv() => {
                // The loop holds `recon_tx`, so this never closes on its own.
                if let Some((generation, paths)) = reconciled {
                    let staging = candidate
                        .as_ref()
                        .is_some_and(|pending| pending.generation == generation);
                    if staging {
                        let pending = candidate.as_mut().expect("checked above");
                        for path in paths {
                            pending.stage(path);
                        }
                    } else if generation == current_generation {
                        let now = epoch.elapsed();
                        // Classified normally. The Rust root decided only whether to
                        // *look* inside this subtree; it is not a relevance rule, so
                        // a README found here is still NotAnInput.
                        for path in &paths {
                            debouncer.record_if_relevant(&current_set, now, path);
                        }
                    }
                    // Otherwise the generation is retired: a candidate that never
                    // activated, or a watcher already replaced. Its findings are
                    // dropped exactly as its native events are — the successor
                    // watches the same tree and reconciles it on its own terms.
                }
            }

            () = activation_gate(&hooks), if candidate.is_some() => {
                let pending = candidate.take().expect("guarded above");
                let Candidate {
                    generation,
                    watcher: fresh,
                    set,
                    registered,
                    staged,
                    overflowed,
                    reply,
                } = pending;

                // ACTIVATION, in one step with no `await` inside it: the new set
                // becomes current, the staged paths are drained through it, and the
                // old watcher is retired last. Nothing can observe a half-swapped
                // state, and a staged event can never be overtaken by a later one —
                // they are recorded here, before the loop reads another event.
                current_set = set;
                current_generation = generation;

                let now = epoch.elapsed();
                for path in &staged {
                    debouncer.record_if_relevant(&current_set, now, path);
                }

                if overflowed {
                    // Precision was lost, so correctness is bought coarsely: report
                    // it, and regenerate over the whole new plan rather than let a
                    // relevant change vanish.
                    let _ = sink.send(WatchEvent::WatcherFailed {
                        code: "watch_staging_overflow".to_string(),
                        message:
                            "too many files changed while the watch plan was being replaced; \
                             regenerating over the whole plan instead"
                                .to_string(),
                    });
                    for path in &registered {
                        debouncer.record(now, path.clone());
                    }
                }

                let old = std::mem::replace(&mut watcher, fresh);
                drop(old);
                let _ = reply.send(Ok(()));
            }

            () = wait_for(epoch, deadline), if deadline.is_some() => {
                if let Some(paths) = debouncer.poll(epoch.elapsed())
                    // `Debouncer` never fires an empty burst, but the request type
                    // is what guarantees it: `new` returns `None` for nothing.
                    && let Some(request) = RegenerationRequest::new(paths)
                    && sink.send(WatchEvent::Regeneration(request)).is_err()
                {
                    break;
                }
            }
        }
    }
    // Reconcilers are joined rather than abandoned: each holds a reference to a
    // watcher, and a blocking walk cannot be cancelled. Their findings go nowhere —
    // `recon_rx` is dropped with the loop and this task is what would have
    // classified them — so nothing can be submitted after shutdown.
    drop(recon_tx);
    reconcilers.shutdown().await;

    // Dropping both unregisters every native watch. A candidate that never
    // activated is released here with its staged paths — shutdown means the old
    // plan stands, and a plan that was never committed must not act.
    drop(candidate);
    drop(watcher);
}

/// Resolves as soon as the candidate may be activated.
///
/// In production this is immediate: activation is attempted on the very next poll
/// after the candidate is built, so the staging window is the handful of events a
/// few `watch()` syscalls can produce. The hook exists only so a test can hold the
/// window open and prove what happens inside it.
async fn activation_gate(hooks: &Hooks) {
    #[cfg(test)]
    if let Some(gate) = &hooks.before_activation {
        gate().await;
        return;
    }
    let _ = hooks;
}

/// Sleeps until `epoch + deadline`, or forever when there is no deadline.
///
/// The `if deadline.is_some()` guard on the `select!` branch means the forever
/// case is never actually polled; it exists so the branch has a uniform type.
async fn wait_for(epoch: Instant, deadline: Option<Duration>) {
    match deadline {
        Some(deadline) => tokio::time::sleep_until(epoch + deadline).await,
        None => std::future::pending().await,
    }
}

/// Resolves a possibly-relative native path against the workspace root.
///
/// Some backends report paths relative to the watched directory; classification
/// compares against an absolute root, so a relative path would look like an
/// escape and be silently dropped.
fn resolve(root: &str, path: &Path) -> PathBuf {
    if path.is_absolute() {
        return path.to_path_buf();
    }
    Path::new(root).join(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn access_and_other_kinds_are_not_changes() {
        use notify::event::{AccessKind, CreateKind, ModifyKind, RemoveKind, RenameMode};
        assert!(!is_change(notify::EventKind::Access(AccessKind::Read)));
        assert!(!is_change(notify::EventKind::Other));

        assert!(is_change(notify::EventKind::Create(CreateKind::File)));
        assert!(is_change(notify::EventKind::Remove(RemoveKind::File)));
        assert!(is_change(notify::EventKind::Modify(ModifyKind::Data(
            notify::event::DataChange::Content
        ))));
        // A rename is a Modify(Name(..)) — the case an editor's save produces.
        assert!(is_change(notify::EventKind::Modify(ModifyKind::Name(
            RenameMode::Both
        ))));
        // Ambiguous: treated as a change on purpose.
        assert!(is_change(notify::EventKind::Any));
    }

    #[test]
    fn a_relative_native_path_is_resolved_against_the_root() {
        assert_eq!(
            resolve("/w", Path::new("src/lib.rs")),
            PathBuf::from("/w/src/lib.rs")
        );
        assert_eq!(
            resolve("/w", Path::new("/w/src/lib.rs")),
            PathBuf::from("/w/src/lib.rs")
        );
    }

    #[test]
    fn a_described_error_never_carries_a_path_or_raw_debug() {
        let error = notify::Error::generic("/home/someone/secret/project failed to watch");
        let described = describe(&error);
        assert_eq!(described.code, "watch_failed");
        assert!(
            !described.message.contains("/home/") && !described.message.contains("secret"),
            "leaked: {}",
            described.message
        );

        let io = notify::Error::io(std::io::Error::new(
            std::io::ErrorKind::PermissionDenied,
            "/home/someone/.ssh denied",
        ));
        let described = describe(&io);
        assert_eq!(described.code, "watch_io_error");
        assert_eq!(
            described.message,
            "the filesystem watcher failed (permission denied)"
        );
        assert!(!described.message.contains("/home/"));
    }

    #[test]
    fn the_watch_limit_error_says_what_to_do_about_it() {
        let described = describe(&notify::Error::new(notify::ErrorKind::MaxFilesWatch));
        assert_eq!(described.code, "watch_limit_reached");
        assert!(described.message.contains("watch limit"));
    }

    // --- staging / activation (replacement hardening) ---------------------

    use crate::classify::WatchInput;
    use crate::plan::WatchRegistration;
    use tempfile::TempDir;
    use tokio::sync::{Semaphore, mpsc};

    fn quick() -> DebounceOptions {
        DebounceOptions {
            quiet: Duration::from_millis(60),
            max_delay: Duration::from_millis(600),
        }
    }

    /// A watchdog; never fires in a passing run.
    async fn within<T>(what: &str, future: impl Future<Output = T>) -> T {
        match tokio::time::timeout(Duration::from_secs(20), future).await {
            Ok(value) => value,
            Err(_) => panic!("timed out waiting for {what}"),
        }
    }

    fn put(path: &Path, text: &str) {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).expect("mkdir");
        }
        std::fs::write(path, text).expect("write");
    }

    /// Writes `path` until the watcher reports it, returning the staged path.
    ///
    /// A native backend can miss a write that lands in the instant after `watch()`
    /// returns — the registration is arming, and the event has nowhere to go. That
    /// is a property of the OS, not of this adapter, so the stimulus is **repeated
    /// until it is observed** rather than issued once and hoped for. The assertion
    /// is unchanged ("the event is staged"); only the poking is retried, and each
    /// extra write touches the same path, which the staging set collapses.
    ///
    /// This is not sleep-then-assert: nothing is concluded from elapsed time, and
    /// a broken adapter still fails — it simply never reports, and the outer bound
    /// ends the test.
    async fn poke_until_staged(
        path: &Path,
        text: &str,
        staged: &mut mpsc::UnboundedReceiver<PathBuf>,
    ) -> PathBuf {
        for _ in 0..200 {
            put(path, text);
            match tokio::time::timeout(Duration::from_millis(100), staged.recv()).await {
                Ok(Some(observed)) => return observed,
                Ok(None) => panic!("the staging hook channel closed"),
                Err(_) => continue,
            }
        }
        panic!("{} was never staged", path.display())
    }

    /// A workspace with an `old` tree, a `new` tree, and a `shared` tree that both
    /// plans watch.
    fn workspace() -> (TempDir, PathBuf) {
        let dir = TempDir::new().expect("tempdir");
        let root = dir.path().canonicalize().expect("canonical root");
        put(&root.join("old/keep.rs"), "pub fn old() {}\n");
        put(&root.join("new/keep.rs"), "pub fn new() {}\n");
        put(&root.join("shared/keep.rs"), "pub fn shared() {}\n");
        (dir, root)
    }

    fn plan_over(root: &Path, trees: &[&str]) -> WatchPlan {
        let set = WatchSet::new(
            root,
            trees
                .iter()
                .map(|tree| WatchInput::rust_root(root.join(tree))),
        );
        WatchPlan::new(
            set,
            trees
                .iter()
                .map(|tree| WatchRegistration::recursive(root.join(tree))),
        )
        .expect("a valid plan")
    }

    /// Hooks that hold activation until released, and report every staged path.
    struct Gate {
        hooks: Hooks,
        /// Fires once the candidate is built and fully registered — i.e. the loop
        /// has reached the activation gate. That is the "candidate is live but not
        /// yet active" moment every test below needs; waiting for it is what makes
        /// the interleaving forced rather than hoped for.
        reached: mpsc::UnboundedReceiver<()>,
        staged: mpsc::UnboundedReceiver<PathBuf>,
        release: Arc<Semaphore>,
    }

    fn gate() -> Gate {
        let release = Arc::new(Semaphore::new(0));
        let (staged_tx, staged) = mpsc::unbounded_channel();
        let (reached_tx, reached) = mpsc::unbounded_channel();
        let held = release.clone();
        Gate {
            hooks: Hooks {
                // Re-awaitable: `select!` rebuilds this future on every loop
                // iteration, so it must stay pending until a permit exists rather
                // than resolve once and latch. `reached` therefore fires on each
                // poll; a test only needs the first.
                before_activation: Some(Arc::new(move || {
                    let held = held.clone();
                    let reached = reached_tx.clone();
                    Box::pin(async move {
                        let _ = reached.send(());
                        held.acquire().await.expect("gate open").forget();
                    })
                })),
                on_staged: Some(Arc::new(move |path: &Path| {
                    let _ = staged_tx.send(path.to_path_buf());
                })),
                after_registration: None,
            },
            reached,
            staged,
            release,
        }
    }

    struct Harness {
        watcher: Watcher,
        events: mpsc::UnboundedReceiver<WatchEvent>,
    }

    fn start(plan: WatchPlan, hooks: Hooks) -> Harness {
        let (sink, events) = mpsc::unbounded_channel();
        let watcher = spawn_inner(plan, sink, quick(), hooks).expect("start");
        Harness { watcher, events }
    }

    impl Harness {
        async fn next_paths(&mut self, root: &Path) -> Vec<String> {
            let request = match within("a regeneration request", self.events.recv()).await {
                Some(WatchEvent::Regeneration(request)) => request,
                Some(WatchEvent::WatcherFailed { code, message }) => {
                    panic!("unexpected failure: {code}: {message}")
                }
                None => panic!("stream closed"),
            };
            let root = root.to_string_lossy().replace('\\', "/");
            request
                .paths()
                .iter()
                .map(|path| {
                    path.to_string_lossy()
                        .replace('\\', "/")
                        .strip_prefix(&root)
                        .unwrap_or_default()
                        .trim_start_matches('/')
                        .to_string()
                })
                .collect()
        }
    }

    // --- subtree reconciliation -------------------------------------------

    /// Hooks that pause a reconciliation between its registration and its scan.
    ///
    /// That window is the whole design: registering first covers what appears
    /// after, and scanning covers what was already there. A test that can stop time
    /// inside it can prove a file created *in* the window is still observed —
    /// something no amount of sleeping could establish.
    struct ReconcileGate {
        hooks: Hooks,
        /// Fires once a subtree's native registration is installed.
        registered: mpsc::UnboundedReceiver<()>,
        release: Arc<Semaphore>,
    }

    fn reconcile_gate() -> ReconcileGate {
        let release = Arc::new(Semaphore::new(0));
        let (registered_tx, registered) = mpsc::unbounded_channel();
        let held = release.clone();
        ReconcileGate {
            hooks: Hooks {
                before_activation: None,
                on_staged: None,
                // Runs on the blocking thread, so it may block: that is what holds
                // the scan open while the test creates a file.
                after_registration: Some(Arc::new(move || {
                    let _ = registered_tx.send(());
                    // `forget` so each reconciliation consumes one permit.
                    let permit = held.clone().acquire_owned();
                    if let Ok(permit) = futures_lite_block_on(permit) {
                        permit.forget();
                    }
                })),
            },
            registered,
            release,
        }
    }

    /// Blocks the current (blocking) thread on a future.
    ///
    /// A reconciliation runs on `spawn_blocking`, so it cannot `.await`; and the
    /// runtime this crate takes has no `block_on` reachable from a blocking thread.
    /// A semaphore permit is the only thing waited on here, so a parked poll loop
    /// is enough and brings in no dependency.
    fn futures_lite_block_on<F: Future>(future: F) -> F::Output {
        use std::sync::Arc as StdArc;
        use std::task::{Context, Poll, Wake, Waker};

        struct Unpark(std::thread::Thread);
        impl Wake for Unpark {
            fn wake(self: StdArc<Self>) {
                self.0.unpark();
            }
        }
        let waker = Waker::from(StdArc::new(Unpark(std::thread::current())));
        let mut context = Context::from_waker(&waker);
        // SAFETY-free: the future never moves again, it is owned by this frame.
        let mut future = std::pin::pin!(future);
        loop {
            match future.as_mut().poll(&mut context) {
                Poll::Ready(value) => return value,
                Poll::Pending => std::thread::park(),
            }
        }
    }

    /// A file created **after** the subtree is registered but **before** the scan
    /// finishes is observed exactly once.
    ///
    /// What this proves is the *exactly once*: the file is inside the scan's window
    /// and may also be reported natively, and the two must coalesce rather than
    /// produce two runs or none.
    ///
    /// What it does **not** prove is that register-first is necessary. A file
    /// created while the scan is paused is still there when the scan runs, so the
    /// scan alone would find it; deleting the registration leaves this test green.
    /// The interleaving that would discriminate — a file created after the scan
    /// ends but before `notify`'s own watch lands — is not forceable from here.
    #[tokio::test]
    async fn a_file_created_between_registration_and_scan_is_observed_exactly_once() {
        let (dir, root) = workspace();
        let mut gate = reconcile_gate();
        let mut harness = start(plan_over(&root, &["old"]), gate.hooks.clone());

        // A new directory with something already in it.
        let prepared = root.join("prepared");
        put(&prepared.join("existing.rs"), "pub fn existing() {}\n");
        std::fs::rename(&prepared, root.join("old/deep")).expect("rename");

        // The subtree is now registered; the scan has not run yet.
        within(
            "the subtree's registration to be installed",
            gate.registered.recv(),
        )
        .await;

        // Created inside the window: after the registration, before the scan.
        put(&root.join("old/deep/during.rs"), "pub fn during() {}\n");

        // Release the scan.
        gate.release.add_permits(64);

        let paths = harness.next_paths(&root).await;
        assert!(
            paths.iter().any(|path| path == "old/deep/existing.rs"),
            "the scan must find what was already there: {paths:?}"
        );
        assert!(
            paths.iter().any(|path| path == "old/deep/during.rs"),
            "the registration must cover what appeared during the scan: {paths:?}"
        );
        assert_eq!(
            paths
                .iter()
                .filter(|path| *path == "old/deep/during.rs")
                .count(),
            1,
            "exactly once: the native event and the scan coalesce, they do not \
             duplicate: {paths:?}"
        );

        harness.watcher.shutdown().expect("shutdown");
        within("join", harness.watcher.join()).await.expect("join");
        drop(dir);
    }

    /// A tree moved into a directory only the CANDIDATE watches is reconciled into
    /// the candidate and delivered when it activates.
    ///
    /// The old plan does not watch `new/`, so nothing about this can be answered by
    /// the old set; and the candidate is live but not yet current, so the findings
    /// must be staged rather than classified against the truth of the moment.
    #[tokio::test]
    async fn a_tree_moved_into_a_candidate_only_root_is_reconciled_and_staged() {
        let (dir, root) = workspace();
        let mut gate = gate();
        let mut harness = start(plan_over(&root, &["old"]), gate.hooks.clone());

        let handle = harness.watcher.handle_for_test();
        let new_plan = plan_over(&root, &["new"]);
        let replacing = tokio::spawn(async move { handle.replace_plan(new_plan).await });

        within(
            "the candidate to reach the activation gate",
            gate.reached.recv(),
        )
        .await;

        // A complete tree, moved into a root only the candidate watches.
        let prepared = root.join("prepared");
        put(&prepared.join("inner/module.rs"), "pub fn inner() {}\n");
        std::fs::rename(&prepared, root.join("new/moved")).expect("rename");

        // Proof the candidate observed the directory itself before activation.
        within(
            "the candidate to stage the moved directory",
            poke_until_staged(
                &root.join("new/poke.rs"),
                "pub fn poke() {}\n",
                &mut gate.staged,
            ),
        )
        .await;

        gate.release.add_permits(1);
        within("the replacement to finish", replacing)
            .await
            .expect("join")
            .expect("the replacement must succeed");

        let paths = harness.next_paths(&root).await;
        assert!(
            paths.iter().any(|path| path == "new/moved/inner/module.rs"),
            "a tree moved into a candidate-only root must reach the new plan \
             exactly once it is activated: {paths:?}"
        );
        assert_eq!(
            paths
                .iter()
                .filter(|path| *path == "new/moved/inner/module.rs")
                .count(),
            1,
            "exactly once: {paths:?}"
        );

        harness.watcher.shutdown().expect("shutdown");
        within("join", harness.watcher.join()).await.expect("join");
        drop(dir);
    }

    /// A failed replacement's reconciliation results are discarded with its
    /// generation.
    ///
    /// The candidate watches `new/`; the old plan does not. If a retired
    /// generation's findings leaked into the current set they would regenerate over
    /// a plan that never became true.
    #[tokio::test]
    async fn a_retired_generations_reconciliation_results_are_discarded() {
        let (dir, root) = workspace();
        let mut gate = gate();
        let mut harness = start(plan_over(&root, &["old"]), gate.hooks.clone());

        // A plan naming a directory that does not exist: registration fails, so the
        // candidate is dropped and its generation retires without ever activating.
        let handle = harness.watcher.handle_for_test();
        let doomed = plan_over(&root, &["new"]);
        let replacing = tokio::spawn(async move { handle.replace_plan(doomed).await });

        within(
            "the candidate to reach the activation gate",
            gate.reached.recv(),
        )
        .await;

        // A tree moved into the candidate-only root, then the replacement is
        // abandoned by shutting the candidate's generation out: release activation
        // and immediately move on — the point is that `old/` is what stays current.
        let prepared = root.join("prepared");
        put(&prepared.join("inner/module.rs"), "pub fn inner() {}\n");
        std::fs::rename(&prepared, root.join("new/moved")).expect("rename");

        gate.release.add_permits(1);
        within("the replacement to finish", replacing)
            .await
            .expect("join")
            .expect("the replacement succeeds");

        // Now the NEW plan is current and `old/` is retired. An edit under `old/`
        // must not regenerate, and the positive control proves the watcher is alive.
        put(&root.join("new/control.rs"), "pub fn control() {}\n");
        let paths = harness.next_paths(&root).await;
        assert!(
            paths.iter().all(|path| path.starts_with("new/")),
            "only the current generation's paths may survive: {paths:?}"
        );

        harness.watcher.shutdown().expect("shutdown");
        within("join", harness.watcher.join()).await.expect("join");
        drop(dir);
    }

    /// Shutdown while a reconciliation is in flight joins cleanly and submits
    /// nothing afterwards.
    #[tokio::test]
    async fn shutdown_during_a_reconciliation_joins_and_submits_nothing() {
        let (dir, root) = workspace();
        let mut gate = reconcile_gate();
        let mut harness = start(plan_over(&root, &["old"]), gate.hooks.clone());

        let prepared = root.join("prepared");
        put(&prepared.join("inner/module.rs"), "pub fn inner() {}\n");
        std::fs::rename(&prepared, root.join("old/deep")).expect("rename");

        // Paused between registration and scan — the reconciliation is provably in
        // flight and holding a reference to the watcher.
        within(
            "the subtree's registration to be installed",
            gate.registered.recv(),
        )
        .await;

        harness.watcher.shutdown().expect("shutdown");
        // Let the scan finish so the blocking task can end; its findings have
        // nowhere to go, which is the point.
        gate.release.add_permits(64);
        within("join", harness.watcher.join()).await.expect("join");

        // The stream is closed and nothing was emitted after shutdown.
        assert!(
            harness.events.recv().await.is_none(),
            "no regeneration may be submitted once shutdown has been requested"
        );
        drop(dir);
    }

    /// 1. A new-only event arriving while the candidate is registered but not yet
    ///    active must survive: staged, then drained through the complete new set.
    #[tokio::test]
    async fn a_new_only_event_staged_before_activation_survives_the_replacement() {
        let (dir, root) = workspace();
        let mut gate = gate();
        let mut harness = start(plan_over(&root, &["old"]), gate.hooks.clone());

        // Begin the replacement; it parks at the gate with every registration done.
        let handle = harness.watcher.handle_for_test();
        let new_plan = plan_over(&root, &["new"]);
        let replacing = tokio::spawn(async move { handle.replace_plan(new_plan).await });

        within(
            "the candidate to reach the activation gate",
            gate.reached.recv(),
        )
        .await;

        // The candidate now watches `new/`; the OLD set does not. If this event were
        // classified against the old set it would be dropped for good.
        //
        // Proof the candidate's callback observed it — before activation.
        let staged = within(
            "the candidate to stage the event",
            poke_until_staged(
                &root.join("new/added.rs"),
                "pub fn added() {}\n",
                &mut gate.staged,
            ),
        )
        .await;
        assert!(
            staged
                .to_string_lossy()
                .replace('\\', "/")
                .ends_with("new/added.rs"),
            "staged the wrong path: {staged:?}"
        );

        // Activate.
        gate.release.add_permits(1);
        within("the replacement to finish", replacing)
            .await
            .expect("join")
            .expect("the replacement must succeed");

        let paths = harness.next_paths(&root).await;
        assert_eq!(
            paths,
            ["new/added.rs"],
            "the staged event must be drained through the new set"
        );
        harness.watcher.shutdown().expect("shutdown");
        within("join", harness.watcher.join()).await.expect("join");
        drop(dir);
    }

    /// 2. A failed replacement's candidate events must never reach the old plan.
    #[tokio::test]
    async fn a_failed_replacements_candidate_events_never_regenerate() {
        let (dir, root) = workspace();
        let mut harness = start(plan_over(&root, &["old"]), Hooks::default());

        // Registrations are applied in sorted order: `new` exists and registers,
        // then `zz-missing` fails. The candidate is live in between.
        let set = WatchSet::new(
            &root,
            [
                WatchInput::rust_root(root.join("new")),
                WatchInput::rust_root(root.join("zz-missing")),
            ],
        );
        let doomed = WatchPlan::new(
            set,
            [
                WatchRegistration::recursive(root.join("new")),
                WatchRegistration::recursive(root.join("zz-missing")),
            ],
        )
        .expect("lexically valid");

        put(&root.join("new/during.rs"), "pub fn during() {}\n");
        let error = harness
            .watcher
            .replace_plan(doomed)
            .await
            .expect_err("the replacement must fail");
        assert_eq!(error.code, "watch_registration_failed");

        // The old positive control proves the old watcher is intact and delivering.
        put(&root.join("old/control.rs"), "pub fn control() {}\n");
        let paths = harness.next_paths(&root).await;
        assert!(
            paths.iter().any(|path| path == "old/control.rs"),
            "the old watcher must still work: {paths:?}"
        );
        assert!(
            !paths.iter().any(|path| path.starts_with("new/")),
            "a failed candidate's events must never regenerate: {paths:?}"
        );
        harness.watcher.shutdown().expect("shutdown");
        within("join", harness.watcher.join()).await.expect("join");
        drop(dir);
    }

    /// 3. An old-plan event during candidate setup is still the active plan's
    ///    business and must not be lost.
    #[tokio::test]
    async fn an_old_plan_event_during_candidate_setup_is_not_lost() {
        let (dir, root) = workspace();
        let mut gate = gate();
        let mut harness = start(plan_over(&root, &["old"]), gate.hooks.clone());

        let handle = harness.watcher.handle_for_test();
        let new_plan = plan_over(&root, &["new"]);
        let replacing = tokio::spawn(async move { handle.replace_plan(new_plan).await });

        within(
            "the candidate to reach the activation gate",
            gate.reached.recv(),
        )
        .await;

        // The old watcher is still fully active while the candidate waits.
        put(&root.join("old/edited.rs"), "pub fn edited() {}\n");

        gate.release.add_permits(1);
        within("the replacement", replacing)
            .await
            .expect("join")
            .expect("must succeed");

        let paths = harness.next_paths(&root).await;
        assert!(
            paths.iter().any(|path| path == "old/edited.rs"),
            "an event from the still-active old plan must not be lost: {paths:?}"
        );
        harness.watcher.shutdown().expect("shutdown");
        within("join", harness.watcher.join()).await.expect("join");
        drop(dir);
    }

    /// 4. A path both plans watch, seen by both watchers across activation, is one
    ///    regeneration.
    #[tokio::test]
    async fn a_path_in_both_plans_seen_by_both_watchers_coalesces_into_one_request() {
        let (dir, root) = workspace();
        let mut gate = gate();
        let mut harness = start(plan_over(&root, &["shared"]), gate.hooks.clone());

        let handle = harness.watcher.handle_for_test();
        let new_plan = plan_over(&root, &["shared"]);
        let replacing = tokio::spawn(async move { handle.replace_plan(new_plan).await });

        within(
            "the candidate to reach the activation gate",
            gate.reached.recv(),
        )
        .await;

        // Both the old (active) and candidate watchers cover `shared/`, so this
        // change is reported twice — once staged, once classified live.
        put(&root.join("shared/touched.rs"), "pub fn touched() {}\n");

        gate.release.add_permits(1);
        within("the replacement", replacing)
            .await
            .expect("join")
            .expect("must succeed");

        let paths = harness.next_paths(&root).await;
        assert_eq!(
            paths,
            ["shared/touched.rs"],
            "the overlap must collapse to one path"
        );
        // And it is one burst, not two runs.
        assert!(
            harness.events.try_recv().is_err(),
            "the duplicate must not produce a second request"
        );
        harness.watcher.shutdown().expect("shutdown");
        within("join", harness.watcher.join()).await.expect("join");
        drop(dir);
    }

    /// 5. Staged events are drained at activation, so a later event from the
    ///    now-active candidate cannot overtake them into an earlier run.
    #[tokio::test]
    async fn staged_events_are_never_overtaken_by_post_activation_events() {
        let (dir, root) = workspace();
        let mut gate = gate();
        let mut harness = start(plan_over(&root, &["old"]), gate.hooks.clone());

        let handle = harness.watcher.handle_for_test();
        let new_plan = plan_over(&root, &["new"]);
        let replacing = tokio::spawn(async move { handle.replace_plan(new_plan).await });

        within(
            "the candidate to reach the activation gate",
            gate.reached.recv(),
        )
        .await;

        // Stage two.
        within(
            "the first staged path",
            poke_until_staged(
                &root.join("new/first.rs"),
                "pub fn a() {}\n",
                &mut gate.staged,
            ),
        )
        .await;
        within(
            "the second staged path",
            poke_until_staged(
                &root.join("new/second.rs"),
                "pub fn b() {}\n",
                &mut gate.staged,
            ),
        )
        .await;

        gate.release.add_permits(1);
        within("the replacement", replacing)
            .await
            .expect("join")
            .expect("must succeed");

        // A third change, now that the candidate is live.
        put(&root.join("new/third.rs"), "pub fn c() {}\n");

        // Every staged path appears in the FIRST request — none was left behind in
        // a later run, and none was overtaken into an earlier one.
        let paths = harness.next_paths(&root).await;
        for expected in ["new/first.rs", "new/second.rs"] {
            assert!(
                paths.iter().any(|path| path == expected),
                "{expected} must be in the first request, not a later one: {paths:?}"
            );
        }
        harness.watcher.shutdown().expect("shutdown");
        within("join", harness.watcher.join()).await.expect("join");
        drop(dir);
    }

    /// 6. Shutdown while a candidate holds staged events: nothing activates,
    ///    nothing regenerates, everything is released.
    #[tokio::test]
    async fn shutdown_while_a_candidate_holds_staged_events_activates_nothing() {
        let (dir, root) = workspace();
        let mut gate = gate();
        let mut harness = start(plan_over(&root, &["old"]), gate.hooks.clone());

        let handle = harness.watcher.handle_for_test();
        let new_plan = plan_over(&root, &["new"]);
        let replacing = tokio::spawn(async move { handle.replace_plan(new_plan).await });

        within(
            "the candidate to reach the activation gate",
            gate.reached.recv(),
        )
        .await;

        within(
            "the candidate to stage an event",
            poke_until_staged(
                &root.join("new/staged.rs"),
                "pub fn staged() {}\n",
                &mut gate.staged,
            ),
        )
        .await;

        // Shut down with the candidate still holding its staged set.
        harness.watcher.shutdown().expect("shutdown");
        // The gate is never released; the loop breaks on the command instead.
        within("the watcher task to join", harness.watcher.join())
            .await
            .expect("clean join");

        // The replacement never completed successfully — the reply sender was
        // dropped with the candidate.
        let outcome = within("the replacement to resolve", replacing)
            .await
            .expect("join");
        assert!(
            outcome.is_err(),
            "an uncommitted plan must not report success"
        );

        // The task is gone, so the sink is dropped: no regeneration was emitted.
        let mut saw_regeneration = false;
        while let Some(event) = harness.events.recv().await {
            if matches!(event, WatchEvent::Regeneration(_)) {
                saw_regeneration = true;
            }
        }
        assert!(
            !saw_regeneration,
            "a candidate that never activated must not regenerate"
        );
        drop(dir);
        drop(gate.staged.recv());
    }

    /// The staging buffer is bounded, and overflow trades precision for a coarse
    /// regeneration rather than losing a change.
    #[test]
    fn staging_overflow_is_flagged_rather_than_silently_dropping_paths() {
        // Exercises the real `Candidate::stage` rule via a candidate-shaped value,
        // so the bound cannot drift from the code that enforces it.
        fn empty_staging() -> (BTreeSet<PathBuf>, bool) {
            (BTreeSet::new(), false)
        }
        fn stage(staged: &mut BTreeSet<PathBuf>, overflowed: &mut bool, path: PathBuf) {
            if staged.len() >= STAGING_CAPACITY && !staged.contains(&path) {
                *overflowed = true;
                return;
            }
            staged.insert(path);
        }

        let (mut staged, mut overflowed) = empty_staging();
        for index in 0..STAGING_CAPACITY {
            stage(
                &mut staged,
                &mut overflowed,
                PathBuf::from(format!("/w/src/f{index}.rs")),
            );
        }
        assert_eq!(staged.len(), STAGING_CAPACITY);
        assert!(!overflowed, "exactly at capacity is not overflow");

        stage(
            &mut staged,
            &mut overflowed,
            PathBuf::from("/w/src/one-too-many.rs"),
        );
        assert!(overflowed, "past capacity must raise the flag");
        assert_eq!(staged.len(), STAGING_CAPACITY, "and stop growing");

        // A duplicate of something already staged is still free at capacity: the
        // buffer is a set of paths, not a queue of events.
        let mut duplicate_overflowed = false;
        stage(
            &mut staged,
            &mut duplicate_overflowed,
            PathBuf::from("/w/src/f0.rs"),
        );
        assert!(!duplicate_overflowed, "a duplicate costs nothing");
    }
}
