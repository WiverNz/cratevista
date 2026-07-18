//! The `serve` use case and the shared server lifecycle used by `serve`/`open`.
//!
//! `cratevista-core` owns the sequence around the `cratevista-server` primitives
//! (`bind_listener` → read `local_addr` → `shutdown_channel` → spawn `run` →
//! Ctrl-C), plus resolving where the artifacts live and mapping server/snapshot
//! errors to the exit-code policy. `serve` serves an **existing** snapshot: it
//! never regenerates and never needs nightly.

use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use cratevista_server::{
    ArtifactPaths, ArtifactSnapshot, BindOptions, ServerError, SnapshotError, SnapshotLoadOptions,
    SourceAccessPolicy, bind_listener, load_snapshot, run, shutdown_channel,
};

use crate::diagnostic::Diagnostic;
use crate::exit::ExitCode;
use crate::generate::resolve_workspace_root;
use crate::usecase::{CommandFailure, CommandOutcome};

/// The default maximum size (1 MiB) of a file served by `/api/source`.
const DEFAULT_SOURCE_MAX_BYTES: u64 = 1024 * 1024;

/// Options for `cargo cratevista serve`.
#[derive(Debug, Clone)]
pub struct ServeOptions {
    /// Path to the `Cargo.toml` (locates the workspace `target/cratevista`).
    pub manifest_path: Option<PathBuf>,
    /// The bind host (default `127.0.0.1`).
    pub host: IpAddr,
    /// The requested port, or `None` for the default with increment-on-conflict.
    pub port: Option<u16>,
    /// Whether the port was set explicitly (explicit conflicts fail).
    pub port_was_explicit: bool,
    /// Enable the guarded `/api/source` endpoint.
    pub source_access: bool,
}

impl Default for ServeOptions {
    fn default() -> Self {
        ServeOptions {
            manifest_path: None,
            host: IpAddr::V4(Ipv4Addr::LOCALHOST),
            port: None,
            port_was_explicit: false,
            source_access: false,
        }
    }
}

/// `cargo cratevista serve` — load an existing snapshot and serve it until
/// Ctrl-C. Never regenerates; missing artifacts fail with exit 3.
pub fn run_serve(options: &ServeOptions) -> CommandOutcome {
    let workspace_root = resolve_workspace_root(options.manifest_path.as_deref())?;
    let state = load_state(&workspace_root, options.source_access)?;
    warn_if_non_loopback(options.host);

    let bind = BindOptions {
        host: options.host,
        port: options.port,
        port_was_explicit: options.port_was_explicit,
    };
    runtime()?.block_on(async move {
        let server = start_serve(&bind, state).await?;
        server.wait_for_shutdown().await
    })
}

/// Resolves the output dir, loads the snapshot, and builds shared state.
pub(crate) fn load_state(
    workspace_root: &Path,
    source_access: bool,
) -> Result<Arc<cratevista_server::AppState>, CommandFailure> {
    let output_dir = workspace_root.join("target").join("cratevista");
    let snapshot = load_snapshot(
        &ArtifactPaths::in_dir(&output_dir),
        &SnapshotLoadOptions::default(),
    )
    .map_err(snapshot_failure)?;
    Ok(build_state(snapshot, source_access, workspace_root))
}

/// Builds a non-watching `AppState` from a snapshot and the source-access policy.
pub(crate) fn build_state(
    snapshot: ArtifactSnapshot,
    source_access: bool,
    workspace_root: &Path,
) -> Arc<cratevista_server::AppState> {
    build_state_with(snapshot, source_access, workspace_root, false)
}

/// [`build_state`], choosing whether the state advertises watch mode.
///
/// `watching` is what registers `/api/events` and makes `/api/health.watch_enabled`
/// true, so it is passed **only** when a watcher is actually running. Claiming it
/// otherwise would leave a browser waiting on an event that can never arrive.
pub(crate) fn build_state_with(
    snapshot: ArtifactSnapshot,
    source_access: bool,
    workspace_root: &Path,
    watching: bool,
) -> Arc<cratevista_server::AppState> {
    let policy = if source_access {
        SourceAccessPolicy::Enabled {
            root: workspace_root.to_path_buf(),
            max_bytes: DEFAULT_SOURCE_MAX_BYTES,
        }
    } else {
        SourceAccessPolicy::Disabled
    };
    if watching {
        cratevista_server::AppState::new_watching(snapshot, policy)
    } else {
        cratevista_server::AppState::new(snapshot, policy)
    }
}

/// A running server owned by core: the shutdown trigger plus the serving task.
///
/// This is the concrete, justified `JoinHandle`-carrying handle the PRD allows
/// in core (the server crate itself exposes no `RunningServer`).
#[derive(Debug)]
pub(crate) struct CoreServer {
    handle: cratevista_server::ShutdownHandle,
    task: tokio::task::JoinHandle<Result<(), ServerError>>,
}

impl CoreServer {
    /// Builds a running-server handle from the shutdown trigger and serving task.
    pub(crate) fn from_parts(
        handle: cratevista_server::ShutdownHandle,
        task: tokio::task::JoinHandle<Result<(), ServerError>>,
    ) -> Self {
        CoreServer { handle, task }
    }

    /// Installs a Ctrl-C handler that triggers shutdown, then awaits the serving
    /// task to completion (joining it — no orphan task).
    pub(crate) async fn wait_for_shutdown(self) -> CommandOutcome {
        let handle = self.handle.clone();
        tokio::spawn(async move {
            if tokio::signal::ctrl_c().await.is_ok() {
                handle.trigger();
            }
        });
        join_result(self.task).await
    }

    /// [`CoreServer::wait_for_shutdown`], running `before_stop` **after** the stop
    /// signal but **before** the server is told to stop.
    ///
    /// That order is the point: watch mode shuts its session down here, so an
    /// in-flight regeneration's terminal event still reaches live SSE subscribers
    /// instead of racing a closing socket.
    pub(crate) async fn wait_for_shutdown_with<F, Fut>(self, before_stop: F) -> CommandOutcome
    where
        F: FnOnce() -> Fut,
        Fut: std::future::Future<Output = ()>,
    {
        let CoreServer { handle, mut task } = self;
        let (signal_tx, signal_rx) = tokio::sync::oneshot::channel();
        let signal = tokio::spawn(async move {
            if tokio::signal::ctrl_c().await.is_ok() {
                let _ = signal_tx.send(());
            }
        });

        let outcome = tokio::select! {
            // Ctrl-C: tear the session down first, then stop serving.
            _ = signal_rx => {
                before_stop().await;
                handle.trigger();
                join_result(task).await
            }
            // The server ended on its own (an error): the session is torn down
            // just the same, so nothing is left running.
            joined = &mut task => {
                before_stop().await;
                match joined {
                    Ok(Ok(())) => Ok(ExitCode::SUCCESS),
                    Ok(Err(error)) => Err(server_failure(error)),
                    Err(join_error) => Err(CommandFailure::runtime(Diagnostic::error(
                        "internal_invariant",
                        format!("the server task did not complete cleanly: {join_error}"),
                    ))),
                }
            }
        };
        signal.abort();
        outcome
    }

    /// Triggers shutdown and joins the task (used by tests to stop cleanly).
    #[cfg(test)]
    pub(crate) async fn stop(self) -> CommandOutcome {
        self.handle.trigger();
        join_result(self.task).await
    }
}

/// Binds, spawns the serving task, and returns the running server (no browser).
pub(crate) async fn start_serve(
    bind: &BindOptions,
    state: Arc<cratevista_server::AppState>,
) -> Result<CoreServer, CommandFailure> {
    let listener = bind_listener(bind).await.map_err(server_failure)?;
    let addr = listener.local_addr().map_err(|error| {
        CommandFailure::runtime(Diagnostic::error("bind_failed", error.to_string()))
    })?;
    let (handle, signal) = shutdown_channel();
    println!("CrateVista is serving at http://{addr}/  (press Ctrl-C to stop)");
    let task = tokio::spawn(run(listener, state, signal));
    Ok(CoreServer { handle, task })
}

/// Maps a joined serving task to a command outcome.
pub(crate) async fn join_result(
    task: tokio::task::JoinHandle<Result<(), ServerError>>,
) -> CommandOutcome {
    match task.await {
        Ok(Ok(())) => Ok(ExitCode::SUCCESS),
        Ok(Err(error)) => Err(server_failure(error)),
        Err(join_error) => Err(CommandFailure::runtime(Diagnostic::error(
            "internal_invariant",
            format!("the server task did not complete cleanly: {join_error}"),
        ))),
    }
}

/// A multi-threaded Tokio runtime for the server lifecycle.
pub(crate) fn runtime() -> Result<tokio::runtime::Runtime, CommandFailure> {
    tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .map_err(|error| {
            CommandFailure::runtime(Diagnostic::error(
                "internal_invariant",
                format!("could not start the async runtime: {error}"),
            ))
        })
}

/// Prints a prominent warning when binding a non-loopback address.
pub(crate) fn warn_if_non_loopback(host: IpAddr) {
    if !host.is_loopback() {
        eprintln!(
            "warning: binding a non-loopback address ({host}) exposes CrateVista on your network. \
             Source access and CORS remain disabled unless you enable them explicitly."
        );
    }
}

/// Maps a snapshot-load failure to a command failure (environment → 3, else 1).
pub(crate) fn snapshot_failure(error: SnapshotError) -> CommandFailure {
    let exit = if error.is_environment() {
        ExitCode::ENVIRONMENT_ERROR
    } else {
        ExitCode::RUNTIME_ERROR
    };
    let mut diagnostic = Diagnostic::error(error.code(), error.to_string());
    if let Some(remediation) = error.remediation() {
        diagnostic = diagnostic.with_remediation(remediation);
    }
    CommandFailure::new(diagnostic, exit)
}

/// Maps a server/bind failure to a runtime command failure (exit 1).
pub(crate) fn server_failure(error: ServerError) -> CommandFailure {
    CommandFailure::runtime(Diagnostic::error(error.code(), error.to_string()))
}

/// An ephemeral loopback bind for tests — used by `serve`/`open` tests.
#[cfg(test)]
pub(crate) fn test_bind() -> BindOptions {
    BindOptions {
        host: IpAddr::V4(Ipv4Addr::LOCALHOST),
        port: Some(0),
        port_was_explicit: true,
    }
}

/// Formats the served URL consistently for `serve`/`open`.
pub(crate) fn url_for(addr: SocketAddr) -> String {
    format!("http://{addr}/")
}
