//! The real, `notify`-backed adapter against a real filesystem.
//!
//! No cargo, no nightly, no network — just a `tempfile` workspace and the native
//! backend. Assertions are on **debounced [`RegenerationRequest`] counts and
//! contents**, never on raw native event counts: an editor save or a single
//! `fs::write` can produce one native event or five depending on the backend, and
//! asserting that number would be asserting the OS's behavior rather than ours.
//!
//! # No sleep-then-assert
//!
//! A negative case ("this must produce nothing") never sleeps and hopes. It
//! triggers a **positive control** afterwards, waits for *that* request to
//! arrive, and only then asserts nothing else came — which proves the watcher was
//! alive and delivering the whole time. Timeouts are watchdogs: they never fire
//! in a passing run.
//!
//! # Platform honesty
//!
//! These run on every platform. Nothing here is gated, because every operation
//! used (create/modify/remove/rename of files, recursive and non-recursive
//! directory watches) is supported by all three native backends. The one
//! deliberately generous number is the debounce window used by most tests, which
//! is shortened so the suite does not spend the production 300 ms per burst.

use std::fs;
use std::path::{Path, PathBuf};
use std::time::Duration;

use cratevista_watch::{
    DebounceOptions, RegenerationRequest, WatchEvent, WatchInput, WatchPlan, WatchRegistration,
    WatchSet, Watcher, spawn_watcher, spawn_watcher_with,
};
use tempfile::TempDir;
use tokio::sync::mpsc::{self, UnboundedReceiver};

/// A short window: the production 300 ms/2 s is the `Debouncer`'s own unit-tested
/// contract, and repeating it here would only make the suite slow. The adapter is
/// what is under test — that it wires classification and the debouncer to a real
/// backend at all.
fn quick() -> DebounceOptions {
    DebounceOptions {
        quiet: Duration::from_millis(60),
        max_delay: Duration::from_millis(600),
    }
}

/// A watchdog. Never fires in a passing run: every wait is unblocked by a real
/// filesystem event travelling through the real backend.
async fn within<T>(what: &str, future: impl std::future::Future<Output = T>) -> T {
    match tokio::time::timeout(Duration::from_secs(20), future).await {
        Ok(value) => value,
        Err(_) => panic!("timed out waiting for {what} — no event arrived"),
    }
}

/// A workspace with the standard shape, plus the plan that watches it.
struct Fixture {
    dir: TempDir,
    events: UnboundedReceiver<WatchEvent>,
    watcher: Watcher,
}

fn root_of(dir: &TempDir) -> PathBuf {
    // `canonicalize` here mirrors what core must do for real, and matters on
    // macOS where `/var` is a symlink to `/private/var`: the backend reports
    // canonical paths, so the root must be canonical too or nothing matches.
    dir.path().canonicalize().expect("canonical temp root")
}

fn write(path: &Path, contents: &str) {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).expect("mkdir");
    }
    fs::write(path, contents).expect("write");
}

/// Builds the standard workspace: a recursive `src`, non-recursive config
/// directories, one exact manifest, one referenced doc that does not exist yet.
fn workspace() -> (TempDir, PathBuf, WatchSet, Vec<WatchRegistration>) {
    let dir = TempDir::new().expect("tempdir");
    let root = root_of(&dir);

    write(&root.join("Cargo.toml"), "[package]\nname = \"demo\"\n");
    write(&root.join("src/lib.rs"), "pub fn one() {}\n");
    fs::create_dir_all(root.join(".cratevista/flows")).expect("flows");
    fs::create_dir_all(root.join(".cratevista/overrides")).expect("overrides");
    fs::create_dir_all(root.join(".cratevista/docs")).expect("docs");
    fs::create_dir_all(root.join("target/cratevista")).expect("target");

    let set = WatchSet::new(
        &root,
        [
            WatchInput::file(root.join("Cargo.toml")),
            WatchInput::rust_root(root.join("src")),
            WatchInput::flows_dir(root.join(".cratevista/flows")),
            WatchInput::overrides_dir(root.join(".cratevista/overrides")),
            // Referenced but absent: watched through its existing parent, which is
            // what core must arrange.
            WatchInput::file(root.join(".cratevista/docs/checkout.md")),
        ],
    );
    let registrations = vec![
        WatchRegistration::non_recursive(root.join("Cargo.toml")),
        WatchRegistration::recursive(root.join("src")),
        WatchRegistration::non_recursive(root.join(".cratevista/flows")),
        WatchRegistration::non_recursive(root.join(".cratevista/overrides")),
        // The nearest existing parent of the missing referenced file.
        WatchRegistration::non_recursive(root.join(".cratevista/docs")),
    ];
    (dir, root, set, registrations)
}

fn start(options: DebounceOptions) -> Fixture {
    let (dir, _root, set, registrations) = workspace();
    let plan = WatchPlan::new(set, registrations).expect("a valid plan");
    let (sink, events) = mpsc::unbounded_channel();
    let watcher = spawn_watcher_with(plan, sink, options).expect("the native watcher must start");
    Fixture {
        dir,
        events,
        watcher,
    }
}

impl Fixture {
    fn root(&self) -> PathBuf {
        root_of(&self.dir)
    }

    /// The next regeneration request.
    ///
    /// A watcher failure is a test failure here, not something to skip past: these
    /// fixtures watch real directories that exist, so the backend has nothing to
    /// legitimately complain about.
    async fn next_request(&mut self) -> RegenerationRequest {
        match within("a regeneration request", self.events.recv()).await {
            Some(WatchEvent::Regeneration(request)) => request,
            Some(WatchEvent::WatcherFailed { code, message }) => {
                panic!("unexpected watcher failure: {code}: {message}")
            }
            None => panic!("the event stream closed unexpectedly"),
        }
    }

    /// Relative, `/`-separated paths of the next request.
    async fn next_paths(&mut self) -> Vec<String> {
        let request = self.next_request().await;
        let root = self.root().to_string_lossy().replace('\\', "/");
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

    /// Nothing is pending right now.
    fn is_quiet(&mut self) -> bool {
        self.events.try_recv().is_err()
    }

    /// Drains initial-synchronization requests until the watcher is clearly
    /// quiescent, establishing a known-idle baseline before a negative assertion.
    ///
    /// A native backend (macOS FSEvents) reports directory-granular events for the
    /// prepared tree at registration, so `Cargo.toml`/`src/lib.rs` can be staged and
    /// surface in the first request. This flushes them: it discards every request
    /// that arrives and returns only once a full debounce window passes with none —
    /// which also catches late-arriving initial events.
    async fn settle(&mut self) {
        // Comfortably larger than the `quick()` debounce `max_delay` (600 ms).
        let idle = Duration::from_millis(1200);
        loop {
            match tokio::time::timeout(idle, self.events.recv()).await {
                Ok(Some(WatchEvent::Regeneration(_))) => {} // discard sync noise
                Ok(Some(WatchEvent::WatcherFailed { code, message })) => {
                    panic!("unexpected watcher failure during settle: {code}: {message}")
                }
                Ok(None) => panic!("the event stream closed unexpectedly during settle"),
                Err(_) => return, // no request for a full window: quiescent
            }
        }
    }

    /// Collects the relative paths of every request up to and including the first
    /// that contains `sentinel`. The sentinel is a **positive control** — a real
    /// edit known to regenerate — so reaching it proves the watcher was alive and
    /// delivering the whole time (no sleep-then-hope). The returned set is every
    /// path that surfaced, which a negative assertion can then inspect.
    async fn drain_through(&mut self, sentinel: &str) -> Vec<String> {
        let mut seen = Vec::new();
        loop {
            let paths = self.next_paths().await;
            let reached = paths.iter().any(|path| path == sentinel);
            seen.extend(paths);
            if reached {
                return seen;
            }
        }
    }

    async fn stop(self) {
        self.watcher.shutdown().expect("shutdown");
        within("the watcher task to join", self.watcher_join()).await;
    }

    async fn watcher_join(self) {
        self.watcher.join().await.expect("clean join");
    }
}

// --- relevant changes -----------------------------------------------------

#[tokio::test]
async fn modifying_a_watched_rust_file_emits_one_request() {
    let mut fixture = start(quick());
    let root = fixture.root();

    write(&root.join("src/lib.rs"), "pub fn two() {}\n");

    assert_eq!(fixture.next_paths().await, ["src/lib.rs"]);
    fixture.stop().await;
}

#[tokio::test]
async fn creating_a_nested_rust_file_under_a_recursive_root_emits_one_request() {
    let mut fixture = start(quick());
    let root = fixture.root();

    // A new module in a new subdirectory: only a recursive watch sees this.
    write(
        &root.join("src/deep/nested/module.rs"),
        "pub fn three() {}\n",
    );

    let paths = fixture.next_paths().await;
    assert!(
        paths.iter().any(|path| path == "src/deep/nested/module.rs"),
        "the new nested module must be reported: {paths:?}"
    );
    fixture.stop().await;
}

#[tokio::test]
async fn a_create_modify_remove_burst_coalesces_into_one_request() {
    let mut fixture = start(quick());
    let root = fixture.root();
    let path = root.join("src/burst.rs");

    // Whatever native events these produce — and it varies by backend — they are
    // one burst over one path.
    write(&path, "pub fn a() {}\n");
    write(&path, "pub fn b() {}\n");
    write(&path, "pub fn c() {}\n");
    fs::remove_file(&path).expect("remove");

    let paths = fixture.next_paths().await;
    assert_eq!(paths, ["src/burst.rs"], "one path, however many events");
    fixture.stop().await;
}

#[tokio::test]
async fn a_rename_reports_the_relevant_destination() {
    let mut fixture = start(quick());
    let root = fixture.root();
    let from = root.join("src/before.rs");
    write(&from, "pub fn x() {}\n");
    // Let the creation settle into its own burst.
    let _ = fixture.next_paths().await;

    fs::rename(&from, root.join("src/after.rs")).expect("rename");

    let paths = fixture.next_paths().await;
    assert!(
        paths.iter().any(|path| path == "src/after.rs"),
        "the destination is what exists now: {paths:?}"
    );
    fixture.stop().await;
}

#[tokio::test]
async fn two_relevant_paths_in_one_burst_are_sorted_and_deduplicated() {
    let mut fixture = start(quick());
    let root = fixture.root();

    write(&root.join("src/zeta.rs"), "pub fn z() {}\n");
    write(&root.join("src/alpha.rs"), "pub fn a() {}\n");
    write(&root.join("src/zeta.rs"), "pub fn z2() {}\n");

    let paths = fixture.next_paths().await;
    assert_eq!(
        paths,
        ["src/alpha.rs", "src/zeta.rs"],
        "sorted, and zeta once despite two writes"
    );
    fixture.stop().await;
}

// --- config directories ---------------------------------------------------

#[tokio::test]
async fn a_direct_flow_toml_is_relevant_but_a_nested_one_is_not() {
    let mut fixture = start(quick());
    let root = fixture.root();

    // The negative first, then a positive control: if the nested file had been
    // relevant, its request would arrive before the control's.
    write(&root.join(".cratevista/flows/nested/deep.toml"), "x = 1\n");
    write(&root.join(".cratevista/flows/architecture.toml"), "y = 2\n");

    let paths = fixture.next_paths().await;
    assert_eq!(
        paths,
        [".cratevista/flows/architecture.toml"],
        "discovery is non-recursive, so the nested TOML is not an input"
    );
    fixture.stop().await;
}

#[tokio::test]
async fn a_referenced_missing_file_becomes_relevant_when_its_parent_reports_creation() {
    let mut fixture = start(quick());
    let root = fixture.root();

    // The file did not exist when the watcher started; its parent directory is
    // what was registered, and creating the file is exactly the event that must
    // regenerate.
    write(&root.join(".cratevista/docs/checkout.md"), "# Checkout\n");

    assert_eq!(fixture.next_paths().await, [".cratevista/docs/checkout.md"]);
    fixture.stop().await;
}

#[tokio::test]
async fn an_unreferenced_doc_beside_a_referenced_one_is_ignored() {
    let mut fixture = start(quick());
    let root = fixture.root();

    // Negative: never named by any configuration.
    write(&root.join(".cratevista/docs/scratch.md"), "notes\n");
    // Positive control, in the same directory and therefore the same watch.
    write(&root.join(".cratevista/docs/checkout.md"), "# Checkout\n");

    let paths = fixture.next_paths().await;
    assert_eq!(
        paths,
        [".cratevista/docs/checkout.md"],
        "only what configuration names is an input"
    );
    fixture.stop().await;
}

// --- ignored paths (each proven against a live positive control) ----------

#[tokio::test]
async fn editor_backup_and_temp_files_beside_a_watched_file_emit_nothing() {
    let mut fixture = start(quick());
    let root = fixture.root();

    // The noise an editor makes, inside a watched directory.
    write(&root.join("src/lib.rs~"), "backup\n");
    write(&root.join("src/.lib.rs.swp"), "swap\n");
    write(&root.join("src/4913"), "probe\n");
    write(&root.join("src/lib.rs.tmp"), "temp\n");

    // The positive control: proves the watcher is alive and delivering.
    write(&root.join("src/real.rs"), "pub fn real() {}\n");

    let paths = fixture.next_paths().await;
    assert_eq!(
        paths,
        ["src/real.rs"],
        "only the real save, none of the noise"
    );
    fixture.stop().await;
}

#[tokio::test]
async fn writing_our_own_generated_output_emits_nothing() {
    // The no-loop test. `target/` is not registered *and* the classifier rejects
    // it, and this proves the behavior rather than either predicate.
    let mut fixture = start(quick());
    let root = fixture.root();

    write(
        &root.join("target/cratevista/document.json"),
        "{\"schema_version\":\"1.1\"}\n",
    );
    write(&root.join("target/cratevista/generation.json"), "{}\n");
    write(&root.join("target/cratevista/diagnostics.json"), "{}\n");

    // Positive control: a real source change, after our own writes.
    write(&root.join("src/lib.rs"), "pub fn changed() {}\n");

    let paths = fixture.next_paths().await;
    assert_eq!(
        paths,
        ["src/lib.rs"],
        "our own output must never appear in a request"
    );
    // And nothing is queued behind it.
    assert!(fixture.is_quiet(), "no second request from our own writes");
    fixture.stop().await;
}

// --- debounce through the real backend ------------------------------------

#[tokio::test]
async fn continuous_changes_still_fire_at_the_maximum_deadline() {
    // Writes every ~10 ms never let the 60 ms quiet window elapse, so only the
    // 600 ms maximum can end this burst. The request arriving at all is the proof;
    // the flood stops as soon as it does, so the test ends on the watcher's own
    // signal rather than a timer.
    let mut fixture = start(quick());
    let root = fixture.root();
    let path = root.join("src/flood.rs");

    let stop = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let flooder = {
        let stop = stop.clone();
        let path = path.clone();
        std::thread::spawn(move || {
            let mut index = 0u64;
            while !stop.load(std::sync::atomic::Ordering::SeqCst) {
                let _ = fs::write(&path, format!("pub fn f{index}() {{}}\n"));
                index += 1;
                std::thread::sleep(Duration::from_millis(10));
            }
        })
    };

    let paths = fixture.next_paths().await;
    stop.store(true, std::sync::atomic::Ordering::SeqCst);
    flooder.join().expect("flooder");

    assert!(
        paths.iter().any(|p| p == "src/flood.rs"),
        "the maximum deadline must fire under a continuous stream: {paths:?}"
    );
    fixture.stop().await;
}

// --- lifecycle ------------------------------------------------------------

#[tokio::test]
async fn initial_registration_failure_returns_a_typed_error() {
    let dir = TempDir::new().expect("tempdir");
    let root = root_of(&dir);
    fs::create_dir_all(root.join("src")).expect("src");

    let set = WatchSet::new(&root, [WatchInput::rust_root(root.join("src"))]);
    // A directory that does not exist: the native backend refuses to watch it.
    let plan = WatchPlan::new(
        set,
        [WatchRegistration::recursive(root.join("does-not-exist"))],
    )
    .expect("lexically valid — existence is not a lexical property");

    let (sink, mut events) = mpsc::unbounded_channel();
    let error = spawn_watcher(plan, sink).expect_err("registration must fail");

    assert_eq!(error.code, "watch_registration_failed");
    let root_text = root.to_string_lossy().replace('\\', "/");
    assert!(
        !error.message.contains(&root_text) && !error.message.contains("does-not-exist"),
        "a watcher error must not carry a path: {}",
        error.message
    );
    // No task was spawned, so nothing will ever be sent.
    assert!(events.try_recv().is_err());
}

#[tokio::test]
async fn a_failed_replacement_retains_the_old_watcher_and_the_old_set() {
    let mut fixture = start(quick());
    let root = fixture.root();

    // A plan that is lexically fine but cannot be registered.
    let doomed_set = WatchSet::new(&root, [WatchInput::rust_root(root.join("src"))]);
    let doomed = WatchPlan::new(
        doomed_set,
        [WatchRegistration::recursive(root.join("no-such-directory"))],
    )
    .expect("lexically valid");

    let error = fixture
        .watcher
        .replace_plan(doomed)
        .await
        .expect_err("the replacement must fail");
    assert_eq!(error.code, "watch_registration_failed");

    // The proof: an input of the OLD plan still produces a request. If the old
    // watcher had been dropped, or the set half-replaced, this would hang.
    write(&root.join("src/still_watched.rs"), "pub fn ok() {}\n");
    let paths = fixture.next_paths().await;
    assert!(
        paths.iter().any(|path| path == "src/still_watched.rs"),
        "the old watcher must survive a failed replacement: {paths:?}"
    );

    // And an old config input too — the whole set, not part of it.
    write(&root.join(".cratevista/flows/a.toml"), "x = 1\n");
    let paths = fixture.next_paths().await;
    assert!(
        paths.iter().any(|path| path == ".cratevista/flows/a.toml"),
        "the complete old set survives: {paths:?}"
    );
    fixture.stop().await;
}

#[tokio::test]
async fn a_successful_replacement_activates_the_new_plan_and_retires_the_old_input() {
    let mut fixture = start(quick());
    let root = fixture.root();
    fs::create_dir_all(root.join("extra")).expect("extra");

    // The new plan watches `extra/` instead of `src/`.
    let new_set = WatchSet::new(&root, [WatchInput::rust_root(root.join("extra"))]);
    let new_plan = WatchPlan::new(new_set, [WatchRegistration::recursive(root.join("extra"))])
        .expect("a valid plan");

    fixture
        .watcher
        .replace_plan(new_plan)
        .await
        .expect("the replacement must succeed");

    // Replacement itself regenerates nothing.
    assert!(
        fixture.is_quiet(),
        "swapping a plan is not a change to the project"
    );

    // The old-only input is retired: write it first, then the new input. The new
    // input's request arriving proves the watcher is alive; the old path's absence
    // from it proves the retirement.
    write(&root.join("src/old_only.rs"), "pub fn old() {}\n");
    write(&root.join("extra/new_input.rs"), "pub fn new() {}\n");

    let paths = fixture.next_paths().await;
    assert!(
        paths.iter().any(|path| path == "extra/new_input.rs"),
        "the new plan is active: {paths:?}"
    );
    assert!(
        !paths.iter().any(|path| path == "src/old_only.rs"),
        "the old-only input must no longer be relevant: {paths:?}"
    );
    fixture.stop().await;
}

#[tokio::test]
async fn shutdown_joins_cleanly() {
    let fixture = start(quick());
    fixture.watcher.shutdown().expect("shutdown");
    assert!(fixture.watcher.is_closed());
    // Idempotent, and never a panic.
    fixture.watcher.shutdown().expect("second shutdown");
    within("the watcher task to join", fixture.watcher_join()).await;
}

#[tokio::test]
async fn replacing_a_plan_after_shutdown_is_a_typed_error() {
    let fixture = start(quick());
    let root = fixture.root();
    fixture.watcher.shutdown().expect("shutdown");

    let set = WatchSet::new(&root, [WatchInput::rust_root(root.join("src"))]);
    let plan = WatchPlan::new(set, [WatchRegistration::recursive(root.join("src"))]).unwrap();
    let error = fixture
        .watcher
        .replace_plan(plan)
        .await
        .expect_err("must be refused after shutdown");
    assert_eq!(error.code, "watcher_closed");
    within("join", fixture.watcher_join()).await;
}

#[tokio::test]
async fn dropping_every_handle_closes_cleanly() {
    let (dir, _root, set, registrations) = workspace();
    let plan = WatchPlan::new(set, registrations).expect("a valid plan");
    let (sink, mut events) = mpsc::unbounded_channel();
    let watcher = spawn_watcher_with(plan, sink, quick()).expect("start");

    // `join` drops the handle's command sender; the task sees the channel close
    // and takes the same graceful path as shutdown.
    within("join after dropping the handle", async {
        watcher.join().await.expect("clean join")
    })
    .await;

    // The task is gone, so its sink is dropped and the stream ends.
    assert!(within("the stream to close", events.recv()).await.is_none());
    drop(dir);
}

#[tokio::test]
async fn the_production_entry_point_uses_the_production_debounce() {
    // `spawn_watcher` is what core will call; the only difference from the tests'
    // entry point is the window, so prove it starts and stops for real.
    let (dir, _root, set, registrations) = workspace();
    let plan = WatchPlan::new(set, registrations).expect("a valid plan");
    let (sink, _events) = mpsc::unbounded_channel();
    let watcher = spawn_watcher(plan, sink).expect("the native watcher must start");
    watcher.shutdown().expect("shutdown");
    within("join", async { watcher.join().await.expect("clean join") }).await;
    drop(dir);
}

// --- subtree reconciliation (Linux recursive-watch hardening) --------------
//
// A recursive watch is not recursive at the OS level on Linux: inotify watches one
// directory each, installed as directories are observed to appear. Two windows
// follow, and both lose a real source edit permanently:
//
//   1. a tree that arrives COMPLETE (`mv`, `git checkout`) is reported as one
//      event for the top directory; nothing ever mentions the files inside it;
//   2. a file created immediately after a `mkdir` can beat the watch being
//      installed for that directory.
//
// The tests below pin both. The first is deterministic — it does not depend on
// winning or losing a race, because the tree is complete before it is ever moved
// into place, so the "files inside are never reported" case is guaranteed rather
// than probable.

/// Builds a complete tree OUTSIDE the watched root, then renames it in.
///
/// This is the deterministic form of the race: after the rename, `src/deep` and
/// everything under it exists, and the backend can only have reported the top
/// directory. Without reconciliation the nested file is never seen at all.
#[tokio::test]
async fn a_prepared_tree_renamed_into_a_rust_root_reports_its_existing_files() {
    let mut fixture = start(quick());
    let root = fixture.root();

    // Prepared outside `src`, so no watch of ours ever sees it being populated.
    let prepared = root.join("prepared");
    write(&prepared.join("nested/module.rs"), "pub fn nested() {}\n");
    write(
        &prepared.join("nested/deeper/leaf.rs"),
        "pub fn leaf() {}\n",
    );
    // Not Rust: it must survive the walk and still be rejected by the classifier.
    write(&prepared.join("nested/README.md"), "# no\n");

    // One atomic rename. The backend may report exactly one event, for `src/deep`.
    fs::rename(&prepared, root.join("src/deep")).expect("rename");

    let paths = fixture.next_paths().await;
    assert!(
        paths.iter().any(|path| path == "src/deep/nested/module.rs"),
        "reconciliation must find a file that existed before the rename: {paths:?}"
    );
    assert!(
        paths
            .iter()
            .any(|path| path == "src/deep/nested/deeper/leaf.rs"),
        "at any depth: {paths:?}"
    );
    assert!(
        !paths.iter().any(|path| path.ends_with("README.md")),
        "a reconciled path is still classified normally — the Rust root decided \
         only whether to look, not what counts: {paths:?}"
    );
    fixture.stop().await;
}

/// Several Rust files from one moved tree coalesce into a single request.
#[tokio::test]
async fn a_moved_tree_with_several_rust_files_coalesces_into_one_request() {
    let mut fixture = start(quick());
    let root = fixture.root();

    let prepared = root.join("prepared");
    for name in ["a.rs", "b.rs", "c.rs"] {
        write(&prepared.join("pack").join(name), "pub fn x() {}\n");
    }
    fs::rename(&prepared, root.join("src/pack")).expect("rename");

    let paths = fixture.next_paths().await;
    for name in ["a.rs", "b.rs", "c.rs"] {
        assert!(
            paths.iter().any(|path| path.ends_with(name)),
            "{name} must be in the one merged request: {paths:?}"
        );
    }

    // One burst, one request. The positive control proves the watcher is still
    // delivering rather than merely quiet.
    write(&root.join("src/control.rs"), "pub fn control() {}\n");
    let next = fixture.next_paths().await;
    assert_eq!(
        next,
        ["src/control.rs"],
        "the moved tree produced exactly one request, not one per file: {next:?}"
    );
    fixture.stop().await;
}

/// A tree moved under `target/` is our own output: reconciling it would be an
/// infinite loop.
#[tokio::test]
async fn a_prepared_tree_renamed_under_target_produces_no_regeneration() {
    let mut fixture = start(quick());
    let root = fixture.root();

    let prepared = root.join("prepared");
    write(&prepared.join("nested/module.rs"), "pub fn nested() {}\n");
    fs::rename(&prepared, root.join("target/cratevista/deep")).expect("rename");

    // Positive control: a real edit must still arrive, and must be the FIRST thing
    // that does.
    write(&root.join("src/control.rs"), "pub fn control() {}\n");
    let paths = fixture.next_paths().await;
    assert_eq!(
        paths,
        ["src/control.rs"],
        "nothing under target/ may regenerate: {paths:?}"
    );
    fixture.stop().await;
}

/// A tree moved under a hidden directory inside a Rust root is not descended into.
#[tokio::test]
async fn a_prepared_tree_renamed_under_a_hidden_directory_produces_no_regeneration() {
    let mut fixture = start(quick());
    let root = fixture.root();

    // Drain initial-synchronization events and reach a quiescent baseline first, so
    // the assertion below reflects only what the hidden-directory rename produces
    // (a native backend stages the prepared tree's initial paths on registration).
    fixture.settle().await;

    // Prepare a tree OUTSIDE any watched root, then move it into a hidden directory
    // under the recursive `src` root. A hidden directory must not be descended into.
    let prepared = root.join("prepared");
    write(&prepared.join("module.rs"), "pub fn hidden() {}\n");
    fs::rename(&prepared, root.join("src/.hidden")).expect("rename");

    // Positive control: a real edit that DOES regenerate. Collect every path up to
    // and including it — reaching it proves the watcher stayed alive and delivering.
    write(&root.join("src/control.rs"), "pub fn control() {}\n");
    let paths = fixture.drain_through("src/control.rs").await;

    // The guarantee under test: the hidden directory is never descended into, so no
    // path from inside `src/.hidden/` is ever reported. This is asserted precisely —
    // it is NOT a blanket tolerance of extra paths. (A native backend may re-list a
    // real sibling like `src/lib.rs` from a directory-granular event; that is a real
    // watched file, not evidence of a hidden-directory descent.)
    assert!(
        !paths.iter().any(|path| path.starts_with("src/.hidden")),
        "a hidden directory must not be descended into: {paths:?}"
    );
    assert!(
        paths.iter().any(|path| path == "src/control.rs"),
        "the positive control must regenerate (watcher alive): {paths:?}"
    );
    fixture.stop().await;
}

/// Editor temporaries inside a reconciled tree stay ignored.
#[tokio::test]
async fn editor_temporaries_in_a_reconciled_tree_are_ignored() {
    let mut fixture = start(quick());
    let root = fixture.root();

    let prepared = root.join("prepared");
    write(&prepared.join("real.rs"), "pub fn real() {}\n");
    write(&prepared.join("swap.rs~"), "junk\n");
    write(&prepared.join("backup.rs.bak"), "junk\n");
    write(&prepared.join("4913"), "junk\n");
    fs::rename(&prepared, root.join("src/edited")).expect("rename");

    let paths = fixture.next_paths().await;
    assert!(paths.iter().any(|path| path == "src/edited/real.rs"));
    for noise in ["swap.rs~", "backup.rs.bak", "4913"] {
        assert!(
            !paths.iter().any(|path| path.ends_with(noise)),
            "{noise} must stay ignored after reconciliation: {paths:?}"
        );
    }
    fixture.stop().await;
}

/// A directory symlink pointing outside the workspace is neither traversed nor
/// registered.
#[cfg(unix)]
#[tokio::test]
async fn a_directory_symlink_to_outside_the_workspace_is_not_traversed() {
    let outside = TempDir::new().expect("outside");
    let outside_root = root_of(&outside);
    write(&outside_root.join("secret.rs"), "pub fn secret() {}\n");

    let mut fixture = start(quick());
    let root = fixture.root();

    std::os::unix::fs::symlink(&outside_root, root.join("src/link")).expect("symlink");

    // Touch the file the symlink points at: if the link had been followed and
    // registered, this would regenerate.
    write(&outside_root.join("secret.rs"), "pub fn secret2() {}\n");

    write(&root.join("src/control.rs"), "pub fn control() {}\n");
    let paths = fixture.next_paths().await;
    assert_eq!(
        paths,
        ["src/control.rs"],
        "a symlinked directory must not be walked or watched: {paths:?}"
    );
    fixture.stop().await;
}
