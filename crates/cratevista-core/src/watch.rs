//! The core half of watch mode: what to watch, and what one regeneration *is*.
//!
//! Three things live here and nothing else:
//!
//! 1. **[`build_watch_plan`]** — turns the real workspace into a
//!    [`WatchPlan`](cratevista_watch::WatchPlan). `cratevista-watch` is
//!    deliberately ignorant of cargo and of configuration, so somebody has to
//!    know that a Rust root is a target's `src_path` parent and that a flow file
//!    is `.cratevista/flows/*.toml`. That somebody is core.
//! 2. **The regeneration transaction** — the exact order in which a regeneration
//!    becomes visible, and what is *not* done when a stage fails.
//! 3. **[`to_server_event`]** — `EngineEvent` → `ServerEvent`.
//!
//! # Why core owns the conversion
//!
//! `cratevista-watch` has an `EngineEvent`; `cratevista-server` has a
//! `ServerEvent`. They are deliberately different types, and neither crate
//! depends on the other: the watcher must be testable without a server, and the
//! server must not learn what a watcher is. Core already depends on both, so the
//! conversion costs no new edge and the two crates stay independent.
//!
//! # What this phase does not do
//!
//! Nothing here is wired to a CLI, an `open`, a real `Watcher` or a real
//! `Engine`. It is the foundation the wiring phase will call.

use std::future::Future;
use std::path::{Component, Path, PathBuf};
use std::pin::Pin;
use std::sync::Arc;

use cratevista_config::ConfigOutcome;
use cratevista_metadata::MetadataIngest;
use cratevista_schema::EntityKind;
use cratevista_server::ServerEvent;
use cratevista_watch::{
    EngineEvent, Regenerate, RegenerationFailure, RegenerationRequest, RegenerationResult,
    RegenerationSuccess, RegistrationMode, WatchInput, WatchPlan, WatchRegistration, WatchSet,
};

use crate::generate::GenerateOptions;

/// Stable, machine-matchable codes for watch setup and regeneration failures.
///
/// Every one of these can reach a browser, so every message paired with them is
/// written to be safe there — see [`WatchSetupError`].
pub mod code {
    /// The workspace root could not be resolved or is not a directory.
    pub const WORKSPACE_INVALID: &str = "watch_workspace_invalid";
    /// A logical input's path text falls outside the workspace.
    pub const INPUT_OUTSIDE_WORKSPACE: &str = "watch_input_outside_workspace";
    /// A registration target resolves outside the workspace through a symlink.
    pub const SYMLINK_ESCAPE: &str = "watch_symlink_escape";
    /// The watch plan could not be assembled.
    pub const PLAN_FAILED: &str = "watch_plan_failed";
    /// The root `Cargo.toml` could not be read.
    pub const ROOT_MANIFEST_UNREADABLE: &str = "watch_root_manifest_unreadable";
    /// The root `Cargo.toml` is not valid TOML.
    pub const ROOT_MANIFEST_INVALID: &str = "watch_root_manifest_invalid";
    /// `run_generate` failed.
    pub const GENERATION_FAILED: &str = "watch_generation_failed";
    /// The freshly written artifacts could not be loaded or verified.
    pub const ARTIFACTS_UNREADABLE: &str = "watch_artifacts_unreadable";
    /// The native watcher refused the new plan.
    pub const PLAN_REPLACE_FAILED: &str = "watch_plan_replace_failed";
}

/// A watch-setup failure.
///
/// # Browser-safe by construction
///
/// `message` is written here, never borrowed from cargo, rustdoc or `io::Error`
/// — those carry absolute paths, `CARGO_HOME`, usernames and whole command
/// lines. The full detail belongs in this process's own tracing, where the person
/// who owns the machine can already see it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WatchSetupError {
    /// A stable code from [`code`].
    pub code: &'static str,
    /// A short message safe to publish.
    pub message: String,
}

impl WatchSetupError {
    pub(crate) fn new(code: &'static str, message: impl Into<String>) -> Self {
        WatchSetupError {
            code,
            message: message.into(),
        }
    }
}

impl std::fmt::Display for WatchSetupError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "{}: {}", self.code, self.message)
    }
}

impl std::error::Error for WatchSetupError {}

// ---------------------------------------------------------------------------
// Part 1 — the plan builder
// ---------------------------------------------------------------------------

/// A plan together with the logical inputs it was built from.
///
/// Core retains the inputs so it can build **recovery coverage as a superset** of
/// what is currently active, without ever asking `cratevista-watch` what it is
/// watching. That keeps the native watcher and its registrations private, and
/// keeps the watch crate free of any dependency on core or metadata.
#[derive(Debug, Clone)]
pub struct CorePlan {
    /// The plan to hand the watcher.
    pub plan: WatchPlan,
    /// The logical inputs it came from — core's own record, not the watcher's.
    ///
    /// Sorted and deduplicated, so the same workspace always produces the same
    /// list and feeding it back in as the next run's `active` cannot accumulate.
    pub inputs: Vec<WatchInput>,
}

/// Builds **recovery coverage**: a superset of `active` that adds the manifests
/// the root `Cargo.toml` declares, **without running cargo**.
///
/// This exists because [`build_watch_plan`] needs a successful `cargo metadata`,
/// and metadata fails in precisely the case that matters: a root manifest that
/// declares a member whose `Cargo.toml` is missing or malformed. Without this the
/// new member's manifest is never watched, so the fix is never observed.
///
/// It never narrows: every input of `active` is preserved.
pub fn build_recovery_plan(
    workspace_root: &Path,
    active: &[WatchInput],
    no_config: bool,
) -> Result<CorePlan, WatchSetupError> {
    let canonical = canonical_root(workspace_root)?;
    let inputs = crate::watch_recovery::recovery_inputs(&canonical, active, no_config)?;
    plan_from_inputs(&canonical, inputs)
}

/// Builds the plan for a real workspace.
///
/// Runs the same metadata and configuration ingestion the pipeline does, so the
/// watched set is derived from what generation actually read — not from a second,
/// drifting idea of what a project looks like.
pub fn build_watch_plan(
    workspace_root: &Path,
    options: &GenerateOptions,
) -> Result<CorePlan, WatchSetupError> {
    let canonical = canonical_root(workspace_root)?;

    let metadata = cratevista_metadata::ingest(&crate::generate::metadata_options(options))
        .map_err(|_| {
            // The real error names paths and the cargo argv; neither is publishable.
            WatchSetupError::new(
                code::PLAN_FAILED,
                "the workspace could not be inspected; see the terminal for details",
            )
        })?;

    let config = if options.no_config {
        None
    } else {
        Some(cratevista_config::load_config(&canonical))
    };

    plan_from(&canonical, &metadata, config.as_ref())
}

/// [`plan_from`] with an already-ingested workspace.
///
/// Exposed so the builder's rules — every one of which is about paths, not about
/// cargo — are testable without a toolchain or a network. `config` is loaded for
/// real from `root` when `no_config` is false, because the referenced-file rules
/// are exactly what must be exercised.
pub fn plan_for_test(
    workspace_root: &Path,
    metadata: &MetadataIngest,
    no_config: bool,
) -> Result<CorePlan, WatchSetupError> {
    let canonical = canonical_root(workspace_root)?;
    let config = if no_config {
        None
    } else {
        Some(cratevista_config::load_config(&canonical))
    };
    plan_from(&canonical, metadata, config.as_ref())
}

/// The pure half: everything decided from an already-ingested workspace.
///
/// Separate from [`build_watch_plan`] so every rule below is testable without
/// cargo, a toolchain or a network.
fn plan_from(
    canonical_root: &Path,
    metadata: &MetadataIngest,
    config: Option<&ConfigOutcome>,
) -> Result<CorePlan, WatchSetupError> {
    let inputs = logical_inputs(canonical_root, metadata, config)?;
    plan_from_inputs(canonical_root, inputs)
}

/// Assembles a plan from already-decided logical inputs.
///
/// Shared by the complete and recovery builders so both get the same containment
/// checks, the same registration rules and the same sorting.
fn plan_from_inputs(
    canonical_root: &Path,
    mut inputs: Vec<WatchInput>,
) -> Result<CorePlan, WatchSetupError> {
    // Canonicalize: sorted and deduplicated, so the same workspace always yields
    // the same `CorePlan`. Deduplication is load-bearing rather than tidy — the
    // retained inputs are fed back in as `active` on the next run, so a duplicate
    // here would be re-added on every regeneration and the list would grow without
    // bound for as long as watch mode runs.
    inputs.sort();
    inputs.dedup();

    for input in &inputs {
        if !is_lexically_inside(canonical_root, &input.path) {
            return Err(WatchSetupError::new(
                code::INPUT_OUTSIDE_WORKSPACE,
                "an input to watch is outside the workspace",
            ));
        }
    }
    let set = WatchSet::new(canonical_root, inputs.clone());
    let registrations = registrations_for(canonical_root, &inputs)?;

    let plan = WatchPlan::new(set, registrations).map_err(|error| {
        // `PlanError` is already workspace-relative, but this message is written
        // here rather than forwarded, so a future change to that type cannot
        // silently start publishing a path.
        let _ = error;
        WatchSetupError::new(
            code::PLAN_FAILED,
            "the set of files to watch could not be assembled",
        )
    })?;
    Ok(CorePlan { plan, inputs })
}

/// Every logical input, before anything is decided about registration.
fn logical_inputs(
    root: &Path,
    metadata: &MetadataIngest,
    config: Option<&ConfigOutcome>,
) -> Result<Vec<WatchInput>, WatchSetupError> {
    let mut inputs = vec![
        WatchInput::file(root.join("Cargo.toml")),
        // Watched even when absent: a lockfile appearing is a real change to what
        // the document says, and the whole point of watching a *missing* path is
        // that creating it repairs the document without a restart.
        WatchInput::file(root.join("Cargo.lock")),
    ];

    for entity in &metadata.entities {
        let Some(source) = &entity.source else {
            continue;
        };
        let path = root.join(source.path.as_str());

        if entity.kind == EntityKind::new(EntityKind::PACKAGE) {
            // Members only. An external package's id carries `@version`, and its
            // sources live in the cargo registry — outside the workspace, not ours
            // to watch, and unchanged by anything the user does here.
            if is_member_package(entity.id.as_str()) {
                inputs.push(WatchInput::file(path));
            }
        } else if entity.kind == EntityKind::new(EntityKind::TARGET) {
            // A target's source is its `src_path` (`src/lib.rs`); the Rust root is
            // the directory holding it, watched recursively because a new module
            // can appear in a new subdirectory.
            if is_member_target(entity)
                && let Some(parent) = path.parent()
            {
                inputs.push(WatchInput::rust_root(parent.to_path_buf()));
            }
        }
    }

    if let Some(config) = config {
        // Watched even when absent, for the same reason as `Cargo.lock`.
        inputs.push(WatchInput::file(
            root.join(cratevista_config::discover::ROOT_CONFIG),
        ));
        inputs.push(WatchInput::flows_dir(
            root.join(".cratevista").join("flows"),
        ));
        inputs.push(WatchInput::overrides_dir(
            root.join(".cratevista").join("overrides"),
        ));
        // Flow docs, flow examples and override docs — including the ones whose
        // file does not exist yet, which is exactly the set PRD 08's
        // `referenced_files` was added to expose.
        for reference in &config.referenced_files {
            inputs.push(WatchInput::file(root.join(reference.path.as_str())));
        }
    }

    // The declared member patterns, kept in the COMPLETE plan too. Metadata only
    // knows the members that exist right now, so a plan built from it alone would
    // stop covering `crates/*` the moment it succeeded — and creating
    // `crates/new/Cargo.toml` afterwards would trigger nothing at all.
    inputs.extend(crate::watch_recovery::member_pattern_inputs(root)?);

    Ok(inputs)
}

/// Whether a package entity id names a workspace member.
///
/// Members are `package:{name}`; externals are `package:{name}@{version}`.
fn is_member_package(id: &str) -> bool {
    id.starts_with("package:") && !id.contains('@')
}

/// Whether a target entity belongs to a workspace member.
fn is_member_target(entity: &cratevista_schema::Entity) -> bool {
    entity
        .parent
        .as_ref()
        .is_some_and(|parent| is_member_package(parent.as_str()))
}

/// Turns logical inputs into the paths the OS watcher is actually given.
///
/// The two are not the same thing, and conflating them is how watch modes break:
/// - a **file** watch follows an inode, so an editor's write-temp-then-rename
///   leaves it watching a file nobody will ever touch again. Watch the containing
///   *directory* instead;
/// - a **missing** path cannot be registered at all. Watch the nearest existing
///   ancestor, and let classification decide whether what appears is the intended
///   file.
///
/// Classification is unchanged by any of this: the [`WatchSet`] still admits only
/// the exact file, `*.rs` under a Rust root, or a direct `*.toml` in a config
/// directory.
fn registrations_for(
    root: &Path,
    inputs: &[WatchInput],
) -> Result<Vec<WatchRegistration>, WatchSetupError> {
    let mut registrations = Vec::new();
    for input in inputs {
        let (target, mode) = match input.kind {
            // The containing directory, so a replaced file is still observed.
            cratevista_watch::InputKind::ExactFile => match input.path.parent() {
                Some(parent) => (parent.to_path_buf(), RegistrationMode::NonRecursive),
                None => continue,
            },
            cratevista_watch::InputKind::RustSourceRoot => {
                (input.path.clone(), RegistrationMode::Recursive)
            }
            cratevista_watch::InputKind::FlowsDir | cratevista_watch::InputKind::OverridesDir => {
                (input.path.clone(), RegistrationMode::NonRecursive)
            }
            // A member pattern is registered by its **static prefix**, recursively:
            // `crates/*` -> watch `crates`; `tools/*/plugins/*` -> watch `tools`.
            // The registration is deliberately broader than the rule — the OS
            // cannot match globs — while classification stays narrow, so a
            // vendored `crates/a/nested/Cargo.toml` still arrives and is still
            // rejected as NotAnInput.
            cratevista_watch::InputKind::WorkspaceMemberManifestPattern => {
                let text = input.path.to_string_lossy().replace('\\', "/");
                let root_text = root.to_string_lossy().replace('\\', "/");
                let relative = text
                    .strip_prefix(&root_text)
                    .unwrap_or_default()
                    .trim_start_matches('/');
                let prefix = cratevista_watch::pattern::static_prefix(relative);
                let target = if prefix.is_empty() {
                    root.to_path_buf()
                } else {
                    root.join(prefix)
                };
                (target, RegistrationMode::Recursive)
            }
        };

        match nearest_existing(root, &target) {
            Some(existing) if existing == target => {
                registrations.push(WatchRegistration {
                    path: contained(root, &existing)?,
                    mode,
                });
            }
            // The intended target does not exist. Watch the nearest ancestor that
            // does, **recursively**, because several path components may be
            // missing at once (`.cratevista/docs/checkout.md` with no
            // `.cratevista/` yet) and only a recursive watch sees the whole
            // chain being created.
            Some(existing) => {
                registrations.push(WatchRegistration {
                    path: contained(root, &existing)?,
                    mode: RegistrationMode::Recursive,
                });
            }
            // Not even the workspace root exists — impossible here, since the root
            // was canonicalized, but refusing beats registering nothing silently.
            None => {
                return Err(WatchSetupError::new(
                    code::PLAN_FAILED,
                    "no existing directory could be found to watch an input through",
                ));
            }
        }
    }

    registrations.sort();
    registrations.dedup();
    Ok(registrations)
}

/// Canonicalizes an **existing** registration target and proves it is still
/// inside the workspace.
///
/// This is the check the lexical one cannot make: `<root>/link/src` is innocent
/// text no matter where `link` points, and only resolving it reveals an escape.
/// Registering an escaped path would watch someone's home directory.
///
/// The *intended* path is never canonicalized — only the existing ancestor being
/// registered. A missing file has nothing to resolve, and resolving its parent is
/// what proves the watch is safe.
fn contained(canonical_root: &Path, existing: &Path) -> Result<PathBuf, WatchSetupError> {
    let canonical = existing.canonicalize().map_err(|_| {
        WatchSetupError::new(
            code::PLAN_FAILED,
            "a directory to watch could not be resolved",
        )
    })?;
    if !canonical.starts_with(canonical_root) {
        return Err(WatchSetupError::new(
            code::SYMLINK_ESCAPE,
            "a path to watch resolves outside the workspace",
        ));
    }
    Ok(canonical)
}

/// The nearest ancestor of `path` (including itself) that exists, stopping at
/// `root`.
fn nearest_existing(root: &Path, path: &Path) -> Option<PathBuf> {
    let mut candidate = path;
    loop {
        if candidate.exists() {
            return Some(candidate.to_path_buf());
        }
        if candidate == root {
            return None;
        }
        candidate = candidate.parent()?;
        if !candidate.starts_with(root) {
            return None;
        }
    }
}

/// Resolves and validates the workspace root once.
fn canonical_root(workspace_root: &Path) -> Result<PathBuf, WatchSetupError> {
    let canonical = workspace_root.canonicalize().map_err(|_| {
        WatchSetupError::new(
            code::WORKSPACE_INVALID,
            "the workspace root could not be resolved",
        )
    })?;
    if !canonical.is_dir() {
        return Err(WatchSetupError::new(
            code::WORKSPACE_INVALID,
            "the workspace root is not a directory",
        ));
    }
    Ok(canonical)
}

/// Lexical containment: no `..`, and under the root.
fn is_lexically_inside(root: &Path, path: &Path) -> bool {
    if path.components().any(|part| part == Component::ParentDir) {
        return false;
    }
    path.starts_with(root)
}

// ---------------------------------------------------------------------------
// Part 2 — the regeneration transaction
// ---------------------------------------------------------------------------

/// The stages of one regeneration, injected so the order can be proven without
/// cargo, a toolchain, a filesystem or a native watcher.
pub trait Stages: Send + Sync + 'static {
    /// The verified artifact snapshot this workspace produces.
    type Snapshot: Send + 'static;

    /// Runs `run_generate`. **Blocking**: it shells out to cargo and rustdoc.
    fn generate(&self) -> Result<(), RegenerationFailure>;

    /// Loads and verifies what `generate` just wrote. **Blocking.**
    fn load(&self) -> Result<Self::Snapshot, RegenerationFailure>;

    /// Whether a snapshot is partial-but-valid.
    fn partial(snapshot: &Self::Snapshot) -> bool;

    /// Builds **recovery coverage**: a superset of the active plan, derived from
    /// the root manifest alone. **Blocking** (it reads one file).
    ///
    /// Called first, and separately from [`Stages::build_plan`], because the
    /// complete plan needs `cargo metadata` — which fails in exactly the case
    /// recovery exists for.
    fn build_recovery_plan(&self) -> Result<WatchPlan, RegenerationFailure>;

    /// Builds the complete plan from the workspace as it is **now**.
    /// **Blocking**: it runs `cargo metadata` and reads configuration.
    ///
    /// Coverage is established before anything reads the workspace, so an edit
    /// during generation is observed rather than lost.
    fn build_plan(&self) -> Result<WatchPlan, RegenerationFailure>;

    /// Swaps the native watcher onto the candidate plan.
    ///
    /// The last step before generation, and the last that can fail *without*
    /// leaving a coverage gap. Once it succeeds the plan stays active even if a
    /// later stage fails.
    fn replace_plan(
        &self,
        plan: WatchPlan,
    ) -> Pin<Box<dyn Future<Output = Result<(), RegenerationFailure>> + Send + '_>>;

    /// The final, **infallible** commit: the new snapshot becomes what is served.
    ///
    /// The only all-or-nothing publication here. The plan is coverage and may
    /// already have moved ahead; this is the step that changes what anyone sees.
    fn commit(&self, snapshot: Self::Snapshot);
}

/// One regeneration, coverage first.
///
/// ```text
/// build recovery -> activate recovery -> build complete -> activate complete
///   -> generate -> load + verify -> commit
/// ```
///
/// Plan evolution is `previous -> recovery -> complete`; snapshot publication does
/// not move until the final commit.
///
/// # A WatchPlan is liveness coverage, not published state
///
/// This is the invariant everything else follows from:
///
/// > **The plan may lead the served snapshot. It must never lag the inputs a
/// > regeneration used.**
///
/// A plan is not a document and shows nobody anything — it only decides which
/// files are *observed*. Activating it early costs at most a redundant
/// regeneration; activating it late loses edits, and a lost edit is invisible and
/// permanent. **Extra observation is acceptable; missing observation is not.**
///
/// # Why the previous order was wrong
///
/// It ran `generate -> load -> build plan -> replace plan -> swap`, so a run that
/// introduced a new member, source root or referenced doc was not watching those
/// files *while it ran*: an edit landing between the start of generation and the
/// activation of the plan was simply dropped.
///
/// Worse, it made a **failed** run unrecoverable. The plan was replaced only after
/// a successful load, so a generation that failed left the old plan active — and
/// the fix, an edit to the very files the failed run introduced, was not watched.
/// The user would correct the error and nothing would happen. Building coverage
/// *first* is what makes "the next edit repairs it" true.
///
/// # Why plan-first is safe
///
/// [`build_watch_plan`] canonicalizes the workspace root and rejects symlink
/// escapes and outside-workspace registrations before a plan exists, so an
/// *activated* plan is already a safe one. It exposes no document data — it
/// watches files. A malformed configuration stays repairable, because the config
/// root and the config directories are themselves logical inputs.
///
/// If the plan cannot be built safely, **nothing** proceeds: publishing a newer
/// snapshot while retaining an older plan is exactly the lag this order forbids.
///
/// # What a failure does not do
///
/// After a failure past activation the newer plan **stays active** (coverage may
/// lead) and the previous **in-memory snapshot keeps being served** (publication is
/// all-or-nothing). Newer artifacts may sit on disk; that is not a lie, because
/// what is *served* is the old snapshot, and the next successful run commits
/// whatever is on disk then.
pub struct Transaction<S: Stages> {
    stages: Arc<S>,
}

impl<S: Stages> Transaction<S> {
    /// Builds a transaction over the given stages.
    pub fn new(stages: S) -> Self {
        Transaction {
            stages: Arc::new(stages),
        }
    }
}

impl<S: Stages> Regenerate for Transaction<S> {
    fn regenerate(
        &self,
        request: RegenerationRequest,
    ) -> Pin<Box<dyn Future<Output = RegenerationResult> + Send + '_>> {
        // The changed paths are what triggered this run; they are absolute paths on
        // someone's machine and they are **never** looked at again. Nothing derived
        // from them can reach a failure, an event or a browser.
        drop(request);

        let stages = self.stages.clone();
        Box::pin(async move {
            // 1. Recovery coverage, from the root manifest alone. This is first
            //    because step 3 needs `cargo metadata`, and metadata fails in the
            //    very case that needs watching: a declared member whose manifest is
            //    missing or malformed. A superset of the active plan, so nothing is
            //    ever narrowed.
            let blocking = stages.clone();
            let recovery = blocking_stage(move || blocking.build_recovery_plan()).await?;

            // 2. Activate it. If the root manifest itself is unreadable we never
            //    get here: the current plan stands, and it already watches the root
            //    `Cargo.toml`, which is what observes its repair.
            stages.replace_plan(recovery).await?;

            // 3. The complete, metadata-derived plan. On failure recovery coverage
            //    **stays active** — it is what will see the fix.
            let blocking = stages.clone();
            let plan = blocking_stage(move || blocking.build_plan()).await?;

            // 4. Activate it. On failure recovery coverage stays active: never the
            //    older, narrower plan.
            stages.replace_plan(plan).await?;

            // 5. Generate, now that everything it will read is already observed.
            //    `run_generate` is synchronous and shells out to cargo and rustdoc,
            //    so it must never run on a runtime thread. If it fails the newer
            //    plan **stays active** — the fix will be an edit to the very files
            //    this run introduced, and those are watched now.
            let blocking = stages.clone();
            blocking_stage(move || blocking.generate()).await?;

            // 6. Verify what was written. A failure here also keeps the newer plan
            //    and the older snapshot: coverage may lead, publication may not.
            let blocking = stages.clone();
            let snapshot = blocking_stage(move || blocking.load()).await?;

            // 7. The commit. Infallible by contract, last, and the only
            //    all-or-nothing publication in the transaction.
            let partial = S::partial(&snapshot);
            stages.commit(snapshot);

            Ok(RegenerationSuccess { partial })
        })
    }
}

/// Runs one blocking stage off the runtime.
async fn blocking_stage<T, F>(stage: F) -> Result<T, RegenerationFailure>
where
    F: FnOnce() -> Result<T, RegenerationFailure> + Send + 'static,
    T: Send + 'static,
{
    match tokio::task::spawn_blocking(stage).await {
        Ok(result) => result,
        // The blocking thread panicked or was cancelled. Report it as a failure
        // rather than propagating a panic into the engine, which would take the
        // whole watch loop down with it.
        Err(_) => Err(RegenerationFailure::new(
            code::GENERATION_FAILED,
            "the regeneration task stopped unexpectedly",
        )),
    }
}

// ---------------------------------------------------------------------------
// Part 3 — failure mapping
// ---------------------------------------------------------------------------

/// Maps a `run_generate` failure to a browser-safe failure.
///
/// **The underlying message is not forwarded.** cargo and rustdoc failures arrive
/// full of absolute paths, `CARGO_HOME`, usernames and whole command lines; the
/// stable `code` is the useful part and the only part that travels. The detail
/// stays in this process's diagnostics, where it is already visible to whoever ran
/// the command.
pub fn generation_failure(failure: &crate::usecase::CommandFailure) -> RegenerationFailure {
    RegenerationFailure::new(
        failure.diagnostic.code.clone(),
        "generation failed; see the terminal for details",
    )
}

/// Maps a snapshot-load failure.
pub fn artifacts_failure() -> RegenerationFailure {
    RegenerationFailure::new(
        code::ARTIFACTS_UNREADABLE,
        "the generated files could not be verified; the previous document is still shown",
    )
}

/// Maps a watch-setup failure.
pub fn setup_failure(error: &WatchSetupError) -> RegenerationFailure {
    // `WatchSetupError`'s message is written to be publishable, so it is the one
    // message here that is forwarded rather than replaced.
    RegenerationFailure::new(error.code, error.message.clone())
}

/// Maps a watcher plan-replacement failure.
pub fn replace_failure() -> RegenerationFailure {
    RegenerationFailure::new(
        code::PLAN_REPLACE_FAILED,
        "the file watcher could not be updated; the previous document is still shown",
    )
}

// ---------------------------------------------------------------------------
// Part 4 — event conversion
// ---------------------------------------------------------------------------

/// Converts an engine event into the event the server publishes.
///
/// Exact, and deliberately dull: `code` and `message` are moved through
/// unchanged, nothing is added, and no variant has anywhere to put a path.
///
/// **`WatchEvent::WatcherFailed` has no mapping here on purpose.** A watcher
/// problem is an operational warning about *this machine* — a watch limit, a
/// permission — not a failed generation run. Publishing it as
/// `GenerationFailed` would tell the browser a document failed to build when
/// nothing of the sort happened. The runtime owner logs it to the terminal
/// instead.
pub fn to_server_event(event: EngineEvent) -> ServerEvent {
    match event {
        EngineEvent::GenerationStarted => ServerEvent::GenerationStarted,
        EngineEvent::GenerationSucceeded { partial } => {
            ServerEvent::GenerationSucceeded { partial }
        }
        EngineEvent::GenerationFailed { code, message } => {
            ServerEvent::GenerationFailed { code, message }
        }
    }
}
