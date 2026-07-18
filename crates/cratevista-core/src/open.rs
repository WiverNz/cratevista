//! The `open` use case: generate, serve, wait for readiness, then open a browser.
//!
//! The browser opener is invoked **only after** a bounded loopback `/api/health`
//! readiness probe succeeds — never before `run` is actively serving. A
//! readiness timeout shuts the server down, joins the task, and returns
//! `server_readiness_failed` (exit 1). Browser-open failure is non-fatal: the
//! URL is printed, a warning is emitted, and the server keeps running.

use std::future::Future;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::sync::Arc;
use std::time::Duration;

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;

use cratevista_server::{AppState, BindOptions, bind_listener, run, shutdown_channel};

use crate::clock::Clock;
use crate::diagnostic::Diagnostic;
use crate::generate::{GenerateOptions, resolve_workspace_root, run_generate};
use crate::serve::{
    CoreServer, build_state, build_state_with, join_result, runtime, server_failure,
    snapshot_failure, url_for, warn_if_non_loopback,
};
use crate::usecase::{CommandFailure, CommandOutcome};
use crate::watch_runtime::{self, CargoWork, SessionWork};

/// Options for `cargo cratevista open`.
#[derive(Debug, Clone)]
pub struct OpenOptions {
    /// The generation options (consistent with `generate`).
    pub generate: GenerateOptions,
    /// The bind host (default `127.0.0.1`).
    pub host: IpAddr,
    /// The requested port, or `None` for the default with increment-on-conflict.
    pub port: Option<u16>,
    /// Whether the port was set explicitly.
    pub port_was_explicit: bool,
    /// Enable the guarded `/api/source` endpoint.
    pub source_access: bool,
    /// Watch the workspace and regenerate on change.
    ///
    /// `serve` has no equivalent: it serves an existing snapshot and never
    /// regenerates, so there would be nothing for a watcher to trigger.
    pub watch: bool,
}

impl Default for OpenOptions {
    fn default() -> Self {
        OpenOptions {
            generate: GenerateOptions::default(),
            host: IpAddr::V4(Ipv4Addr::LOCALHOST),
            port: None,
            port_was_explicit: false,
            source_access: false,
            watch: false,
        }
    }
}

/// Opens a URL in the user's default browser. Abstracted so `open` is testable
/// without launching a real browser.
pub trait BrowserOpener: Send + Sync {
    /// Attempts to open `url`; returns a human-readable error on failure.
    fn open(&self, url: &str) -> Result<(), String>;
}

/// The real system browser opener (via the `opener` crate).
pub struct SystemOpener;

impl BrowserOpener for SystemOpener {
    fn open(&self, url: &str) -> Result<(), String> {
        opener::open_browser(url).map_err(|error| error.to_string())
    }
}

/// A bounded, loopback-only readiness check. Abstracted so `open` is
/// deterministic in tests.
pub trait ReadinessProbe: Send + Sync {
    /// Resolves to `true` once the server answers `/api/health` with `200`
    /// within the probe's bounded budget, else `false`.
    fn probe(&self, addr: SocketAddr) -> impl Future<Output = bool> + Send;
}

/// The production probe: up to `attempts` loopback `/api/health` requests spaced
/// by `delay`, each with a short connect/read timeout. No external network.
pub struct HttpProbe {
    /// Maximum attempts.
    pub attempts: u32,
    /// Delay between attempts.
    pub delay: Duration,
    /// Per-attempt timeout.
    pub timeout: Duration,
}

impl Default for HttpProbe {
    fn default() -> Self {
        HttpProbe {
            attempts: 50,
            delay: Duration::from_millis(20),
            timeout: Duration::from_millis(500),
        }
    }
}

impl ReadinessProbe for HttpProbe {
    async fn probe(&self, addr: SocketAddr) -> bool {
        for attempt in 0..self.attempts {
            if attempt > 0 {
                tokio::time::sleep(self.delay).await;
            }
            if health_ok(addr, self.timeout).await {
                return true;
            }
        }
        false
    }
}

/// One loopback `GET /api/health`; returns `true` on an HTTP `200`.
async fn health_ok(addr: SocketAddr, timeout: Duration) -> bool {
    let attempt = async {
        let mut stream = TcpStream::connect(addr).await.ok()?;
        let request =
            format!("GET /api/health HTTP/1.1\r\nHost: {addr}\r\nConnection: close\r\n\r\n");
        stream.write_all(request.as_bytes()).await.ok()?;
        let mut buffer = Vec::new();
        stream.read_to_end(&mut buffer).await.ok()?;
        Some(buffer)
    };
    match tokio::time::timeout(timeout, attempt).await {
        Ok(Some(bytes)) => {
            let head = String::from_utf8_lossy(&bytes);
            head.starts_with("HTTP/1.1 200") || head.starts_with("HTTP/1.0 200")
        }
        _ => false,
    }
}

/// `cargo cratevista open` — generate, serve, become ready, open the browser.
///
/// With `--watch`, the same contract holds and the same failures mean the same
/// things; the difference is that coverage is established *before* the initial
/// generation and the session keeps regenerating afterwards. See [`run_watching`].
pub fn run_open(options: &OpenOptions, clock: Arc<dyn Clock>) -> CommandOutcome {
    if options.watch {
        return run_watching(options, clock);
    }

    // 1. Generate (nightly only if a documentable target exists). A failure here
    //    keeps its own exit code (e.g. missing nightly → 3).
    run_generate(&options.generate, clock.as_ref())?;

    // 2. Resolve where generate wrote, then load + validate the snapshot.
    let workspace_root = resolve_workspace_root(options.generate.manifest_path.as_deref())?;
    let snapshot = load_initial_snapshot(&workspace_root)?;
    let state = build_state(snapshot, options.source_access, &workspace_root);
    warn_if_non_loopback(options.host);

    runtime()?.block_on(async move {
        let server = start_open(
            &bind_options(options),
            state,
            &SystemOpener,
            &HttpProbe::default(),
        )
        .await?;
        server.wait_for_shutdown().await
    })
}

/// Loads and verifies the snapshot `generate` just wrote.
fn load_initial_snapshot(
    workspace_root: &std::path::Path,
) -> Result<cratevista_server::ArtifactSnapshot, CommandFailure> {
    let output_dir = workspace_root.join("target").join("cratevista");
    cratevista_server::load_snapshot(
        &cratevista_server::ArtifactPaths::in_dir(&output_dir),
        &cratevista_server::SnapshotLoadOptions::default(),
    )
    .map_err(snapshot_failure)
}

fn bind_options(options: &OpenOptions) -> BindOptions {
    BindOptions {
        host: options.host,
        port: options.port,
        port_was_explicit: options.port_was_explicit,
    }
}

/// `cargo cratevista open --watch`.
///
/// # The order, and why it is this order
///
/// ```text
/// 1. resolve GenerateOptions + canonical workspace root
/// 2. build the initial COMPLETE CorePlan          (cargo metadata + config)
/// 3. start the real watcher on it
/// 4. start the ingress owner in Bootstrap mode
/// 5. run the initial generation                   <- events here are buffered
/// 6. load + verify the initial snapshot
/// 7. AppState::new_watching(snapshot, source policy)
/// 8. build the production Transaction over the ALREADY ACTIVE plan
/// 9. spawn the single-flight engine
/// 10. attach the ingress to the EngineHandle
/// 11. the attach flushes the bootstrap window as ONE merged request
/// 12. bind, probe, open the browser
/// ```
///
/// Steps 2–4 come before step 5 for the reason the whole recovery phase exists:
/// the initial `cargo doc` is the slowest thing that will ever happen here, and
/// everything it reads would otherwise be unwatched while it reads it. An edit
/// landing in that window would be lost silently and permanently.
///
/// Steps 5 and 6 keep ordinary `open`'s contract exactly: a failure binds nothing,
/// opens no browser, and returns the same code it always did — after joining the
/// watch tasks, so nothing is left detached.
fn run_watching(options: &OpenOptions, clock: Arc<dyn Clock>) -> CommandOutcome {
    // 1.
    let workspace_root = resolve_workspace_root(options.generate.manifest_path.as_deref())?;
    let work: Arc<dyn SessionWork> = Arc::new(CargoWork::new(
        &workspace_root,
        options.generate.clone(),
        clock.clone(),
    ));
    warn_if_non_loopback(options.host);

    let options = options.clone();
    runtime()?.block_on(async move {
        // 2–11. Coverage, the initial generation, and the session.
        let started = watch_runtime::start(
            work,
            || run_generate(&options.generate, clock.as_ref()).map(|_| ()),
            || load_initial_snapshot(&workspace_root),
            |snapshot, watching| {
                build_state_with(snapshot, options.source_access, &workspace_root, watching)
            },
        )
        .await?;
        let (state, session) = (started.state, started.session);
        if session.is_some() {
            println!("Watching for changes. Press Ctrl-C to stop.");
        }

        // 12.
        let server = match start_open(
            &bind_options(&options),
            state,
            &SystemOpener,
            &HttpProbe::default(),
        )
        .await
        {
            Ok(server) => server,
            Err(failure) => {
                if let Some(session) = session {
                    session.shutdown().await;
                }
                return Err(failure);
            }
        };

        // The session is torn down on Ctrl-C *before* the server stops, so a
        // regeneration still in flight can publish its terminal event.
        server
            .wait_for_shutdown_with(|| async {
                if let Some(session) = session {
                    session.shutdown().await;
                }
            })
            .await
    })
}

/// Binds, spawns the serving task, probes readiness, and (only then) opens the
/// browser. On readiness failure it shuts the task down, joins it, and returns
/// `server_readiness_failed`. Returns a running [`CoreServer`] on success.
pub(crate) async fn start_open<O: BrowserOpener, P: ReadinessProbe>(
    bind: &BindOptions,
    state: Arc<AppState>,
    opener: &O,
    probe: &P,
) -> Result<CoreServer, CommandFailure> {
    let listener = bind_listener(bind).await.map_err(server_failure)?;
    let addr = listener.local_addr().map_err(|error| {
        CommandFailure::runtime(Diagnostic::error("bind_failed", error.to_string()))
    })?;
    let url = url_for(addr);
    let (handle, signal) = shutdown_channel();
    println!("CrateVista is serving at {url}");
    let task = tokio::spawn(run(listener, state, signal));

    if probe.probe(addr).await {
        match opener.open(&url) {
            Ok(()) => println!("Opened {url} in your browser."),
            Err(error) => eprintln!(
                "warning: could not open a browser automatically ({error}). Open {url} manually."
            ),
        }
        Ok(CoreServer::from_parts(handle, task))
    } else {
        // Readiness failed: shut down, join (no orphan), and report.
        handle.trigger();
        let _ = join_result(task).await;
        Err(CommandFailure::runtime(
            Diagnostic::error(
                "server_readiness_failed",
                "the server did not become ready in time",
            )
            .with_remediation("Retry; if this persists, check for a conflicting local process."),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    use cratevista_server::{
        ArtifactPaths, SnapshotLoadOptions, SourceAccessPolicy, load_snapshot,
    };

    use crate::serve::test_bind;

    fn hex(bytes: &[u8]) -> String {
        blake3::hash(bytes).to_hex().to_string()
    }

    fn write_valid_snapshot(dir: &std::path::Path) {
        use cratevista_schema::canonical::to_canonical_string;
        use cratevista_schema::{
            ArtifactHashes, Counts, DiagnosticsReport, ExplorerDocument, GenerationReport,
            Generator, Project, Timestamp,
        };
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
            generated_at: Timestamp::new("2026-07-14T00:00:00Z"),
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
        std::fs::write(dir.join("document.json"), &document).unwrap();
        std::fs::write(dir.join("diagnostics.json"), &diagnostics).unwrap();
        std::fs::write(dir.join("generation.json"), &generation).unwrap();
    }

    fn test_state() -> Arc<AppState> {
        let dir = tempfile::tempdir().unwrap();
        write_valid_snapshot(dir.path());
        let snapshot = load_snapshot(
            &ArtifactPaths::in_dir(dir.path()),
            &SnapshotLoadOptions::default(),
        )
        .unwrap();
        AppState::new(snapshot, SourceAccessPolicy::Disabled)
    }

    /// Records the URL opened (and how many times).
    #[derive(Default)]
    struct RecordingOpener {
        opened: Mutex<Vec<String>>,
        fail: bool,
    }

    impl BrowserOpener for RecordingOpener {
        fn open(&self, url: &str) -> Result<(), String> {
            self.opened.lock().unwrap().push(url.to_string());
            if self.fail {
                Err("simulated failure".into())
            } else {
                Ok(())
            }
        }
    }

    struct AlwaysReady;
    impl ReadinessProbe for AlwaysReady {
        async fn probe(&self, _addr: SocketAddr) -> bool {
            true
        }
    }

    struct NeverReady;
    impl ReadinessProbe for NeverReady {
        async fn probe(&self, _addr: SocketAddr) -> bool {
            false
        }
    }

    /// An opener that fails if called before readiness is signalled.
    struct OrderCheckingOpener {
        ready: Arc<std::sync::atomic::AtomicBool>,
        called_before_ready: Arc<std::sync::atomic::AtomicBool>,
        opened: Mutex<Vec<String>>,
    }

    impl BrowserOpener for OrderCheckingOpener {
        fn open(&self, url: &str) -> Result<(), String> {
            use std::sync::atomic::Ordering;
            if !self.ready.load(Ordering::SeqCst) {
                self.called_before_ready.store(true, Ordering::SeqCst);
            }
            self.opened.lock().unwrap().push(url.to_string());
            Ok(())
        }
    }

    /// A probe that flips `ready` to true right before returning success, so the
    /// opener can assert it was not called earlier.
    struct FlipReady {
        ready: Arc<std::sync::atomic::AtomicBool>,
    }
    impl ReadinessProbe for FlipReady {
        async fn probe(&self, _addr: SocketAddr) -> bool {
            self.ready.store(true, std::sync::atomic::Ordering::SeqCst);
            true
        }
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn successful_readiness_opens_once_with_actual_url() {
        let opener = RecordingOpener::default();
        let server = start_open(&test_bind(), test_state(), &opener, &AlwaysReady)
            .await
            .expect("starts");
        let opened = opener.opened.lock().unwrap().clone();
        assert_eq!(opened.len(), 1, "opener called exactly once");
        assert!(opened[0].starts_with("http://127.0.0.1:"));
        assert!(opened[0].ends_with('/'));
        // Clean shutdown, no orphan task.
        server.stop().await.unwrap();
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn opener_not_called_before_readiness() {
        let ready = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let called_before = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let opener = OrderCheckingOpener {
            ready: ready.clone(),
            called_before_ready: called_before.clone(),
            opened: Mutex::new(Vec::new()),
        };
        let probe = FlipReady {
            ready: ready.clone(),
        };
        let server = start_open(&test_bind(), test_state(), &opener, &probe)
            .await
            .expect("starts");
        assert!(
            !called_before.load(std::sync::atomic::Ordering::SeqCst),
            "opener must not run before readiness"
        );
        server.stop().await.unwrap();
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn readiness_timeout_shuts_down_and_returns_error() {
        let opener = RecordingOpener::default();
        let error = start_open(&test_bind(), test_state(), &opener, &NeverReady)
            .await
            .unwrap_err();
        assert_eq!(error.diagnostic.code, "server_readiness_failed");
        assert_eq!(error.exit, crate::exit::ExitCode::RUNTIME_ERROR);
        // The opener was never called (never became ready), and the task was
        // already joined inside start_open (no orphan).
        assert!(opener.opened.lock().unwrap().is_empty());
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn real_http_probe_succeeds_against_a_running_server() {
        // Exercises the production probe end-to-end: real bind + run + TCP health.
        let listener = bind_listener(&test_bind()).await.unwrap();
        let addr = listener.local_addr().unwrap();
        let (handle, signal) = shutdown_channel();
        let task = tokio::spawn(run(listener, test_state(), signal));
        assert!(
            HttpProbe::default().probe(addr).await,
            "server is reachable"
        );
        handle.trigger();
        task.await.unwrap().unwrap();
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn browser_open_failure_leaves_server_running() {
        let opener = RecordingOpener {
            opened: Mutex::new(Vec::new()),
            fail: true,
        };
        let server = start_open(&test_bind(), test_state(), &opener, &AlwaysReady)
            .await
            .expect("start succeeds despite opener failure");
        // Server still healthy: probe it directly.
        // (The task is alive; stopping it cleanly proves no orphan/panic.)
        server.stop().await.unwrap();
        assert_eq!(opener.opened.lock().unwrap().len(), 1);
    }
}
