# PRD — Add watch mode and live reload

> Renamed from "Add watch mode, caching, and live reload" on 2026-07-17: persistent
> caching left this PRD's scope under locked decision B4 and now lives in
> `ISSUES/issue_12_persistent_cache.md`. The source issue keeps its original
> title (it is the historical record); the file name already omitted "caching",
> and nothing references the old heading.

## Status

**Implemented / Verified (2026-07-17).** Every phase has landed; the final
frontend live-reload phase completes the PRD. **Landed:** the prerequisite phase
(2026-07-17 — the two PRD-06 amendments, the PRD-08 amendment and D5; see their
ledgers in PRDs 06 and 08), **step 1** (`cratevista-watch` created: lexical
classification + the debounce state machine), **step 2** (+2.1 fairness, +2.2
shutdown race — the single-flight engine with an injected regeneration
operation), the **server-events phase** (the event channel + `GET /api/events`),
the **real-watcher adapter** (`notify`-backed, feeding classification and the
debouncer), and the **core foundation** (WatchPlan builder, regeneration
transaction, `EngineEvent → ServerEvent`), and the **recovery-coverage phase**
(two-step `previous → recovery → complete` coverage: Parts 1–3 and 7 landed first,
Part 4's member/static-prefix symlink tests followed, and Parts 4–6 are now
complete — the barrier test, the core pattern matrix and active-`CorePlan`
ownership at every failure prefix; see its ledger), and the **backend wiring
phase** (`open --watch`, the core-owned `WatchSession`, coverage-safe bootstrap,
safe degradation, the production `Stages` adapter and deterministic shutdown; see
its ledger), and the **Linux recursive-watch hardening phase** (subtree
reconciliation, which fixed a real defect the wiring phase had misfiled as a
flake; see its ledger), and the **frontend live-reload phase** (the coherent
three-attempt loader, `web/src/api/liveReload.ts`, the last-rendered-snapshot
preservation and the non-blocking watch banners; see its ledger). `cargo cratevista
open --watch` now **reloads the explorer automatically**: the browser subscribes to
`/api/events`, refetches a coherent artifact triple on every success and on every
(re)connection, and keeps the current graph — layout, zoom, pan and selection —
through a regeneration that is running, has failed, or produced an incoherent
triple. **No manual refresh is required.** The remaining deferred item is
persistent caching, which is **not** part of PRD 09 (it is `issue_12`). Finalized
on 2026-07-16 against the
Implemented / Verified PRDs 01–08 and the real code. The four previously blocking
decisions are **locked** by the maintainer (see "## Locked decisions"), D1–D5 are
**confirmed exactly as proposed**, and the two additive amendments this PRD
depends on are recorded in their owning PRDs (which stay Implemented / Verified):
**PRD 06** gains `X-CrateVista-Snapshot` + `/api/health.watch_enabled`; **PRD 08**
gains `ConfigOutcome.referenced_files`. Persistent caching is split out to
`ISSUES/issue_12_persistent_cache.md`; the plan is recorded in
`docs/adr/0008-watch-and-live-reload.md`.

## Source issue

`ISSUES/issue_09_watch_and_live_reload.md`

## Summary

Add `cratevista-watch` plus a `/api/events` SSE route so `cargo cratevista open
--watch` regenerates the document when relevant inputs change and the open
explorer refreshes itself — debounced, never overlapping, never swapping an
invalid snapshot, and never retriggering on CrateVista's own output.

## Problem statement

Manual regenerate/restart cycles make local exploration painful. CrateVista
should watch the inputs that actually affect the document, regenerate once per
burst, and push the result to the open UI.

## Goals

- Watch the real input set; ignore generated and irrelevant paths.
- Debounced, single-flight regeneration.
- Preserve the last valid snapshot on failure and report why.
- Push updates to the frontend over same-origin SSE.
- Clean cancellation and shutdown.

## Non-goals

- New analysis logic (reuses PRDs 03–05 unchanged).
- Re-implementing config validation, graph sanitation, artifact integrity or
  snapshot loading — all four exist and are reused verbatim.
- **Persistent caching** — deferred to issue 12 (locked decision B4).
- Frontend rework (a small live-reload hook only).

## Current repository state

Verified against the real code on 2026-07-17.

- **`cratevista-watch` exists** (created by this PRD) and is a registered
  `[workspace]` member. It ships `classify` (lexical `WatchSet`), `debounce` (the
  timestamp-driven state machine), `engine` (single-flight), `event`
  (`EngineEvent`), `plan` (`WatchPlan`/`WatchRegistration`) and `watcher` (the
  real `notify`-backed adapter, including **subtree reconciliation**:
  `reconcile_subtree`, `spawn_reconcile`, `may_introduce_directory`, and
  `WatchSet::{needs_subtree_reconciliation, is_ignored_directory, contains_path}`).
  **Its only dependencies are `notify` and `tokio`** (`rt`, `sync`, `macros`,
  `time`); it depends on **no other cratevista crate**. **`cratevista-core` is its only reverse dependency** (added by the
  core-foundation phase); the server still depends on neither.
- **`/api/events` exists for watch-enabled states only, and the frontend consumes
  it.** The router (`crates/cratevista-server/src/router.rs`) serves `/api/health`,
  `/api/document`, `/api/generation`, `/api/diagnostics`, `/api/source` and an SPA
  `fallback` unconditionally, **plus `/api/events` when `AppState::watch_enabled()`**
  — for an ordinary `serve` the route is not registered and the fallback answers
  with the existing unknown-API JSON `404`. Server modules are **flat** (`api.rs`,
  `events.rs`, `router.rs`, `state.rs`, …); there is **no `routes/` directory**.
- **The frontend live-reload client is `web/src/api/liveReload.ts`.** It exports
  `LiveReload` (one `EventSource` per mounted app, probe-gated, disposable),
  `probeWatchEnabled` (fail-closed `/api/health.watch_enabled` read) and
  `parseFailure`. `web/src/api/load.ts` now runs up to `COHERENCE_ATTEMPTS` (3)
  attempts, checking `X-CrateVista-Snapshot` (`SNAPSHOT_HEADER`) across all three
  artifacts before accepting, and adds the typed `incoherent-snapshot` outcome.
  `web/src/App.tsx` mounts the client, reuses the one `ArtifactLoader` for reloads,
  preserves the store across reloads, and renders the `RegeneratingIndicator`,
  `GenerationFailedBanner` and `ReloadErrorBanner` from `web/src/components/Panels.tsx`.
- **`AppState` holds ONE `ArcSwap<ArtifactSnapshot>`, not two ArcSwaps.**
  `crates/cratevista-server/src/state.rs`:
  `AppState { snapshot: ArcSwap<ArtifactSnapshot>, source: SourceAccessPolicy,
  watch_enabled: bool, events: broadcast::Sender<ServerEvent> }` — the
  `watch_enabled` field arrived with **A2 (LANDED 2026-07-17)**, alongside
  `AppState::new_watching` and `watch_enabled()`; the bounded `events` channel
  (capacity **16**) arrived with the server-events phase, alongside
  `subscribe_events()`/`publish_event()`. **`AppState::new` is unchanged and still
  means "not watching"**, and neither constructor takes an events argument. With
  `snapshot() -> Arc<ArtifactSnapshot>` (handlers call it
  **once per request**, so one request never mixes generations),
  **`replace_snapshot(ArtifactSnapshot)`** — already documented in that file as
  "the PRD-09 live-reload seam" — and `source_policy()`.
- **`ArtifactSnapshot` is one coherent unit**: `{ document, generation,
  diagnostics, document_bytes, generation_bytes, diagnostics_bytes, marker:
  SnapshotMarker, partial }`. The API handlers serve the **exact stored canonical
  bytes** with `Cache-Control: no-store`.
- **`SnapshotMarker` holds the raw `generation.json` bytes** (`as_bytes`,
  `matches`) — **not header-safe**, which is why amendment A1 added a derived
  token rather than exposing the marker directly. **A1 has since LANDED
  (2026-07-17):** `SnapshotMarker { bytes, token }` with
  **`token() -> &str`** (64-char lowercase-hex BLAKE3, hashed **once** at
  construction), served as **`X-CrateVista-Snapshot`** on `/api/document`,
  `/api/generation` and `/api/diagnostics`.
- **`load_snapshot(&ArtifactPaths, &SnapshotLoadOptions) -> Result<ArtifactSnapshot, SnapshotError>`**
  already performs torn-read protection (reads `generation.json` as marker A,
  then document, diagnostics, then `generation.json` again as marker B), BLAKE3
  `artifact_hashes` verification, schema-major gating, and bounded retry
  (`max_retries` 4, `retry_delay` ~25 ms). **"No candidate snapshot is ever
  published on a failed check"** is its existing documented contract. PRD 09
  reuses it and adds no integrity logic.
- **CLI (`crates/cargo-cratevista/src/cli.rs`)**: `Command::{Init, Doctor,
  Generate, Serve, Open, Build}`. **There is no `Watch` command**, and there is no
  shared watch argument group. **`--watch` exists on the `Open` variant only**
  (added by the backend wiring phase) — declared there rather than on `ServerArgs`
  precisely because `Serve` shares that group and must keep rejecting it.
  `GenerateArgs` (shared by `Generate` and `Open`) carries
  `--keep-going`, `--features`, `--all-features`, `--no-default-features`,
  `--document-private-items`, `--toolchain`, `--external-deps`,
  `--document-bins`, `--no-config`. `ServerArgs` (shared by `Serve` and `Open`)
  carries `--host`, `--port`, `--source`. **`Serve` has no `GenerateArgs`** — it
  is published as "Serve an already-generated document and the embedded UI (**no
  regeneration**)", and locked decision B1 keeps it that way.
- **`cratevista-core::watch_runtime` exists** (backend wiring phase) — the
  core-owned session behind `open --watch`. It is **crate-private**: nothing in it
  is part of core's public API, because `cargo-cratevista` reaches it only through
  `run_open`.
  - `SessionWork` — the injected seam for the four blocking operations
    (`build_recovery`, `build_complete`, `generate`, `load`); `CargoWork` is the
    production implementation over `build_recovery_plan` / `build_watch_plan` /
    `run_generate` / `load_snapshot`.
  - `PlanSink` — the seam a plan is activated through; implemented for
    `cratevista_watch::Watcher`. It exists so a test can make a replacement *fail*.
  - `ProductionStages` — the real `Stages` behind `Transaction`, holding the
    retained `CorePlan`, the pending-plan slot and `AppState`.
  - `Ingress { Bootstrap { pending } | Active { handle } }`, `spawn_ingress`,
    `IngressHandle::{activate, stop, join}` — the single owner of the watcher's
    event receiver.
  - `bootstrap_watch` → `Option<Bootstrapped>`, `Bootstrapped::abandon`,
    `start` → `Started { state, session }`, `activate`, `WatchSession::shutdown`.
- **`OpenOptions.watch`** and `Command::Open { .., watch: bool }`; `run_open` now
  takes `Arc<dyn Clock>` (a regeneration outlives one borrowed call), and `Clock`
  gained a `Send + Sync` supertrait. `serve` is untouched.
- **`cratevista-core::serve::build_state_with(.., watching)`** chooses
  `AppState::new` or `AppState::new_watching`; `CoreServer::wait_for_shutdown_with`
  runs a hook after the stop signal but before the server stops.
- **`cratevista-core::watch_recovery` exists**: reads the **root `Cargo.toml`
  directly** (core now has a direct **`toml`** dependency) to build recovery
  coverage without cargo — `recovery_inputs` and `member_pattern_inputs`. The
  latter is used by **both** plan builders, so a declared `crates/*` covers a
  member created later under **recovery *and* complete** coverage.
- **`cratevista-watch` has a first-class member-pattern input**:
  `InputKind::WorkspaceMemberManifestPattern`,
  `WatchInput::workspace_member_pattern(pattern, excludes)`, `WatchInput.excludes`,
  and a dependency-free matcher (`pattern::matches`, `pattern::static_prefix`)
  supporting literals, `*`, `?`, `[ab]`/`[!ab]` and `**`, which **fails closed** on
  malformed syntax. `cratevista-watch` still ships **only `notify` and `tokio`**.
- **`cratevista-core::watch` exists** (core-foundation phase): `CorePlan { plan,
  inputs }` (core's own logical record, so recovery can be a superset without any
  `Watcher::current_plan`), `build_recovery_plan`, `build_watch_plan`
  + `plan_for_test`, `WatchSetupError` + `code::{watch_workspace_invalid,
  watch_input_outside_workspace, watch_symlink_escape, watch_plan_failed,
  watch_generation_failed, watch_artifacts_unreadable, watch_plan_replace_failed}`,
  the `Stages` trait + `Transaction<S>` (which implements
  `cratevista_watch::Regenerate`), the failure mappers, and `to_server_event`.
  **`cratevista-core → cratevista-watch` is the one new edge**; the server still
  depends only on `cratevista-schema`, and nothing depends on core.
- **Core generation API after PRD 08**:
  `run_generate(&GenerateOptions, &dyn Clock) -> CommandOutcome` where
  `CommandOutcome = Result<ExitCode, CommandFailure>`. It is **synchronous and
  blocking**, and **returns no document** — it writes the three artifacts and
  returns an exit code. `GenerateOptions` includes **`no_config: bool`** (PRD 08).
  `cratevista-core` depends on config, graph, metadata, rustdoc, schema, server,
  tokio, opener, blake3.
- **Config discovery (PRD 08)**: `cratevista_config::discover(&Path) ->
  Discovered { root: Option<PathBuf>, flows: Vec<PathBuf>, overrides: Vec<PathBuf> }`,
  with `ROOT_CONFIG = "cratevista.toml"`, `CONFIG_DIR = ".cratevista"`,
  `FLOWS_DIR = "flows"`, `OVERRIDES_DIR = "overrides"`. Non-recursive,
  `*.toml`-only.
- **`ConfigOutcome` now exposes `referenced_files`** — B3 **LANDED 2026-07-17**:
  `Vec<ReferencedConfigFile { path: RepoRelativePath, kind: ReferencedFileKind }>`
  (`FlowDoc | FlowExample | OverrideDoc`), sorted and deduplicated by
  `(path, kind)`, built by the new `cratevista_config::referenced` module with
  **no filesystem access**. Core can build the WatchSet from it directly.
- **`cratevista_schema::RepoRelativePath` already exists** and is the type
  config's `docs.rs`/`overlay.rs` already validate paths through. B3 reuses it;
  no new path type is invented.
- **`RawRootConfig` is `#[serde(deny_unknown_fields)]`** and reserves exactly
  `version`, `[metadata]`, `[rustdoc]`, `[server]`. **There is no `[watch]` or
  `[cache]` section**, and adding one to `cratevista.toml` today produces a
  `config_invalid_structure` diagnostic.
- **The rustdoc cache key exists; a cache does not.**
  `cratevista_rustdoc::cache_key(&RustdocTarget, &RustdocOptions,
  &CompatibilityTuple, input_digest: &str) -> String` (BLAKE3, domain-framed
  `cratevista-rustdoc-cache:v1:`, truncated to 32 hex chars) is implemented and
  tested; its doc comment now points at **`ISSUES/issue_12_persistent_cache.md`**
  for `input_digest` (D5, landed 2026-07-17 — comment only).
  **Nothing computes an `input_digest`**, there is **no cache store**, and
  **`GenerationReport.input_hashes` is written as an empty `BTreeMap`** by
  `crates/cratevista-core/src/generate.rs`.
- **The CSP already permits same-origin SSE.** `CONTENT_SECURITY_POLICY`
  includes **`connect-src 'self'`** (and `default-src 'self'`). **No CSP change
  is needed for `/api/events`**, and none is proposed.
- **The frontend already has the refetch seam**: `web/src/api/load.ts` is a
  "concurrent artifact loader" fetching `/api/document`, `/api/generation` and
  `/api/diagnostics` together under **one `AbortController` + a monotonic token**
  that discards stale responses.

## Terminology

**Debounce**: coalesce a burst of filesystem events into one regeneration.
**Single-flight**: at most one generation running at any time. **Live reload**:
server→browser notification that a new snapshot is available. Otherwise per
`ISSUES/CONTEXT.md`.

## Decision 1 — Watch command and ownership

### CLI surface — LOCKED (B1)

**`cargo cratevista open --watch` only.**

```bash
cargo cratevista open --watch
```

`--watch` goes on **`open`**, which already flattens both `GenerateArgs` and
`ServerArgs` — so watch inherits every generation option (`--features`,
`--toolchain`, `--no-config`, …) and every server option (`--host`, `--port`,
`--source`) with **no new argument group and no new contract**.

**`serve` remains artifact-only**: it accepts **neither `--watch` nor
`GenerateArgs`**, and its published "no regeneration" contract (stated in its
clap doc comment and in PRDs 05 and 08) is **unchanged**. Those PRDs therefore
need no edit, and `/api/health.watch_enabled` is always `false` under `serve`.

This deliberately deviates from the issue, which names `cargo cratevista serve
--watch`. The deviation is the point: giving `serve` a regenerating flag would
either contradict its contract or introduce generation flags that are inert
without `--watch` — a trap where `serve --features x` silently does nothing.
`open --watch` means exactly what `open` already means, plus "keep going".

Because `--watch` is on `open` alone, it is declared on the **`Open` variant**,
**not** on the shared `ServerArgs` — putting it there would surface it on `serve`
too, which is precisely what this decision rejects. A CLI test asserts
`serve --help` lists neither `--watch` nor any generation flag.

### Ownership

- **`cratevista-watch`** owns filesystem watching, path classification and
  debounce. It depends on **`notify` and `tokio` only** — never on
  core/graph/config/server — and it **never calls `run_generate` itself**. It
  exposes a pure engine driven by an injected regeneration closure.
  **It must NOT depend on `cratevista-schema`**: its data model is `PathBuf` plus
  plain strings, and **no type from that crate is required**. A dependency with
  no type behind it is coupling with no payer — if a future change genuinely
  needs a schema type, the dependency is added *then*, with the type as its
  justification. Core converts the typed `RepoRelativePath`s from config (B3)
  into absolute `PathBuf`s **before** handing them to watch, so the schema type
  stops at the core boundary.
- **`cratevista-server`** gains **only** the `/api/events` route, a broadcast
  channel in `AppState`, and the two amendments (A1/A2). **No `notify`, no
  debounce, no `cratevista-watch` dependency**: the server does not watch
  anything; it publishes events it is told about and swaps a snapshot it is
  handed.
- **`cratevista-core`** stays the top-level orchestrator: it builds the watched
  input set, owns the engine, calls `run_generate`, `load_snapshot`,
  `AppState::replace_snapshot`, and the event sink. This preserves the existing
  arrow `core → {watch, server, config, graph}` with no cycle.

## Decision 2 — Event handling

- **Debounce: 300 ms quiet window, coalescing** (D1). Every event resets the
  timer; generation fires when the window goes quiet, with a **2 s maximum delay**
  so a continuous stream (`cargo fmt` over a large tree, a rebase) cannot starve
  regeneration indefinitely. Both are constants, injectable for tests.
- **Create/modify/remove/rename bursts coalesce into one request.** The debouncer
  holds a **set of changed paths plus a fire decision**, not a queue of events, so
  a rename (remove+create) and an editor's write-temp-then-rename collapse into
  one regeneration. `notify::EventKind::{Create, Modify, Remove}` and rename's
  `Modify(Name(_))` all map to one internal `Changed(path)`.
- **The watched input set** (built by core, injected into watch):
  - `Cargo.toml`, `Cargo.lock`, and **every workspace member manifest**
    (`cargo metadata` `manifest_path`s, already available via
    `cratevista-metadata`);
  - **Rust sources**: the directory of each target's `src_path` (metadata already
    exposes it), filtered to `*.rs`;
  - `cratevista.toml`, `.cratevista/flows/*.toml`, `.cratevista/overrides/*.toml`
    (from `cratevista_config::discover`);
  - **explicitly referenced docs/examples** — `ConfigOutcome.referenced_files`
    (PRD-08 amendment, B3): sorted, deduplicated `RepoRelativePath`s covering
    `[[flow]].docs`, `[[flow.example]].path` and `[[override]].docs`, **including
    references whose file is currently missing, oversized or non-UTF-8** and
    **excluding** invalid or traversing spellings.
- **Ignored, always**: `target/**` (which subsumes **`target/cratevista/**` — our
  own output, the loop risk**), `.git/**`, `web/node_modules/**`, `web/dist/**`,
  and dotfile directories. The ignore set is **fixed, not configurable** (D3).
- Watching **only explicitly referenced** docs/examples — never a directory glob
  over `.cratevista/docs/` — matches PRD 08's opt-in rule: an unreferenced file
  there is not an input, so touching it must not regenerate.

### A WatchPlan is liveness coverage, not published state

> **The plan may lead the served snapshot. It must never lag the inputs a
> regeneration used.**

A plan shows nobody anything; it only decides which files are *observed*.
Activating it early costs at most a redundant regeneration. Activating it late
loses edits — and a lost edit is invisible and permanent. **Extra observation is
acceptable; missing observation is not.**

Two consequences, both of which the transaction below depends on:

- coverage is established **before** generation reads the workspace, so an edit
  landing mid-run is observed rather than dropped;
- a **failed** run keeps the coverage it activated, so the fix is watched.

Only `ArtifactSnapshot` remains an all-or-nothing publication commit. A failed
generation may therefore sit on a newer plan while the server serves the previous
snapshot, and that pairing is correct rather than a compromise.

### Coverage evolves in two steps, and never lags

> **Superseded.** An earlier version of this section said the WatchSet is rebuilt
> *only after a successful generation*, and that a failure retains the *previous*
> WatchSet. Both statements were wrong and are deleted: they described an order
> that lost edits, and made a failed run unable to observe its own fix.

The input set is **itself derived from the inputs**: adding a workspace member, a
flow file or a `docs = [...]` entry all change what must be watched. So coverage
is rebuilt every run, **before** anything reads the workspace, in two steps:

```text
previous coverage → recovery coverage → complete coverage
```

- **Recovery coverage** is built from the **root manifest alone**, with no cargo.
  It is a **superset of the previous coverage**: every existing input is kept, and
  the root `Cargo.toml`, `Cargo.lock`, the config inputs, the declared member
  manifests and the declared **member patterns** are added. It exists because the
  complete plan needs `cargo metadata`, and metadata fails in exactly the case
  that needs watching — a declared member whose manifest is missing or malformed.
  Without it the user fixes that manifest and nothing happens.
- **Complete coverage** is the metadata/config-derived plan: concrete manifests,
  Rust roots from each target's `src_path` parent, config inputs and referenced
  docs. It **retains every declared workspace-member pattern**. It may drop
  obsolete concrete inputs, but never the patterns — metadata only knows the
  members that exist *now*, so a plan built from it alone would stop covering
  `crates/*` the moment it succeeded, and creating `crates/new/Cargo.toml`
  afterwards would trigger nothing.

**Atomic swap, never incremental patching.** Each step registers the new watches
and only then drops the old registration, so the watcher is never half-updated.

**A recursive root is reconciled, register-first, whenever a directory appears
beneath it.** A "recursive watch" is a fiction on Linux: inotify watches one
directory each, so a subtree that arrives complete — `mv`, `git checkout` — is
reported as a single event for its top directory and everything inside it is never
mentioned by anyone. The rule is therefore:

```text
register the new subtree  ->  then scan it
```

Registration covers what appears after that instant; the scan covers what already
existed before it. Everything found is then **classified normally** — the Rust
root decides only whether to *look*, never what counts, so a `README.md` in a
reconciled tree is still `NotAnInput`. Findings from the native watch and from the
scan overlap deliberately: the debouncer records into a set, so one burst is still
one sorted, deduplicated request.

**The rule both steps serve:** *coverage may lead the served snapshot; it may
never lag the inputs a regeneration used.* Extra observation costs a redundant
rebuild; missing observation loses an edit invisibly and permanently. Neither
plan activation publishes any document state — only the final commit does.

### Directories are watched where necessary; missing files via the nearest parent

Watching a file path only reports changes to **that inode**. It cannot report a
file that does not exist yet, and on many platforms it goes silent after the file
is replaced (editors write-temp-then-rename, discarding the watched inode).

- **Watch the containing directory** wherever the interesting events are
  **create / delete / rename**: `.cratevista/flows/`, `.cratevista/overrides/`,
  and every Rust source root. A new `*.rs` file, a new flow TOML, or a deleted
  override must trigger regeneration, and only a directory watch sees them.
- **A referenced file that does not exist is watched through its nearest existing
  parent directory.** `docs = [".cratevista/docs/checkout.md"]` pointing at a
  missing file is watched via `.cratevista/docs/` — or, if that does not exist
  either, via the nearest ancestor that does, walking up to the workspace root.
  This is precisely the `config_missing_file` case, and it is the one that most
  needs to work: the user's next action is **creating that file**, and creation
  must regenerate. Watching a nonexistent path would simply fail to register.
- Directory watches are **non-recursive** except for Rust source roots, which are
  recursive (a new module can appear in a new subdirectory).
- Directory watches necessarily deliver events for files we do not care about —
  which is why classification is applied per event, below.

### Classification is applied again to every event

Path classification runs **twice, at two different times, for two different
reasons**:

1. When **building** the WatchSet — to decide what to register.
2. When **each event arrives** — to decide whether it counts.

The second is not redundant. Watching directories means the OS hands us events
for **every** file in them, including ones the WatchSet never named: an editor's
`.swp`/`~` backup beside a `.rs` file, a `4913` probe file, a `document.json.tmp`
from our own prepare-then-commit writer if a root ever overlapped `target/`.
**Every event is re-classified against the current output/config/source rules
before it can reach the debouncer**, and anything under `target/**`, `.git/**`,
`web/node_modules/**` or `web/dist/**` is dropped there.

**The loop therefore cannot close for two independent reasons**: we never register
a watch inside `target/`, *and* the classifier rejects our own output even if a
future WatchSet change accidentally registered an overlapping root. The no-loop
test asserts the **behavior**, so it fails if either guard is removed.

## Decision 3 — Generation concurrency

- **Single-flight, guaranteed structurally.** Exactly **one** generation task
  exists. The engine is a `tokio` task owning the state machine
  `Idle → Running → Running+Dirty → Running → Idle`, and the generation call sits
  in **one** place. There is no lock to forget to take and no second call site.
- **Changes during a run set a dirty flag, never a queue.** `dirty` is a `bool`,
  so N changes arriving mid-run schedule **exactly one** follow-up. On completion:
  if `dirty`, clear it and run once more; else go idle.
- **Two cargo/rustdoc pipelines can never run concurrently** because
  `run_generate` is **synchronous and blocking** and is invoked through
  `tokio::task::spawn_blocking` from that single task, awaited to completion
  before the state machine advances. (This is also why it must never be called on
  an async runtime thread — it shells out to cargo and rustdoc.)
- The engine takes the regeneration closure as a parameter, so
  `cratevista-watch` never links core and every concurrency test runs with a fake
  closure — deterministic, no cargo, no nightly.

## Decision 4 — Failure semantics

- **The last valid snapshot is retained and keeps being served.** Failure paths
  simply **never call `replace_snapshot`**. There is no rollback, because nothing
  was swapped.
- **Plan retention at every failure stage** — coverage is never rolled back:
  - **recovery build fails** → the **previous** coverage stays active (and it
    already watches the root `Cargo.toml`, which is what observes its repair);
  - **recovery replacement fails** → the **previous** coverage stays active,
    complete and unchanged; nothing is generated;
  - **complete build fails** → **recovery** coverage stays active — it is what
    observes the fix to whatever made metadata fail;
  - **complete replacement fails** → **recovery** coverage stays active, never the
    older, narrower plan;
  - **generation or load fails** → **complete** coverage stays active.
  In every case the **previous `ArtifactSnapshot` keeps being served** until the
  final commit.
- **The WatchPlan is coverage, not published state — and it is not rolled back.**
  A failed run keeps the **newer** plan it activated. That is deliberate: the fix
  for a failed run is usually an edit to the very files that run introduced, and a
  rolled-back plan would not be watching them, so the user would correct the error
  and nothing would happen. See "## Decision 2 — Event handling" for the
  invariant.
- **A partially written or invalid artifact set can never be swapped in**, for
  three stacked reasons that already exist: `run_generate` writes via
  **prepare-then-commit**; `load_snapshot` re-reads with **marker A/B torn-read
  detection + BLAKE3 verification + schema gating** and never publishes a
  candidate on a failed check; and the swap happens **only** on
  `load_snapshot(...) == Ok`. PRD 09 adds **no** integrity logic of its own.
- **Publish order: generate → load+verify → swap → emit `generation-succeeded`.**
  The event fires only after the swap, so a client refetching on the event cannot
  observe the old snapshot.
- **A failed regeneration never terminates watch mode.** The engine catches
  `CommandFailure`, emits `generation-failed` with the failure's `code` and
  message, logs it, and returns to `Idle`. Watch mode exits only on Ctrl-C or an
  unrecoverable watcher error.
- **Diagnostics from a failed run are NOT published to `/api/diagnostics`** —
  that endpoint serves the *current snapshot's* diagnostics, and the current
  snapshot is the last **valid** one. Publishing failure diagnostics there would
  mix a failed run's output into a successful run's artifact set and break the
  snapshot's coherence guarantee. Failure detail travels **only** on the
  `generation-failed` event.
- **Startup with no valid initial snapshot** (`open --watch` is the only watch
  entry point, per B1):
  - `open` already generates before serving. If generation succeeds, the server
    binds and watch proceeds.
  - If the initial generation **fails**, `open --watch` keeps `open`'s **existing**
    exit code and does not bind — unchanged from today's `open`, and watch adds no
    new startup path. A server with no document to show would be worse than a
    clear error.
  - Once the server is bound, every later failure follows the steady-state rule
    above: keep serving the last valid snapshot, report, keep watching.
  - `serve` cannot enter watch mode at all (B1), so "watch with a stale on-disk
    snapshot" is not a reachable state and needs no special rule.

### Startup must not repeat the coverage gap (a requirement on the wiring phase)

`open --watch` **must not** do:

```text
initial generation → start watcher
```

That is the same lost-event gap as the old transaction order, at startup: every
edit made while the first generation runs — which is the slowest one, with a cold
cargo cache — would be dropped, and the explorer would open showing a document
that is already stale with no event coming to fix it.

The wiring phase must therefore **either**:

- **establish watcher coverage before the initial generation** (build the plan,
  start the `Watcher`, and only then generate — the same coverage-first order the
  transaction uses); **or**
- **queue every relevant event across the initial-generation window** and deliver
  them to the engine once it starts, so a change during startup becomes the first
  dirty follow-up.

The first is simpler and matches the transaction, so it is the expected choice:
`build_watch_plan` needs only the workspace and `GenerateOptions`, both of which
exist before generation runs.

## Decision 5 — SSE and frontend behavior

### The `/api/events` contract

- **`GET /api/events`**, same-origin, `Content-Type: text/event-stream`,
  `Cache-Control: no-store`. Same loopback bind and headers as every other route
  — **the CSP is unchanged**, because `connect-src 'self'` already allows it.
- **Bounded event types — exactly three**, each a named SSE `event:` with a small
  JSON `data:` payload:

  | Event | Payload | Meaning |
  | --- | --- | --- |
  | `generation-started` | `{}` | A run began (lets the UI show a spinner) |
  | `generation-succeeded` | `{ "partial": bool }` | A new snapshot **is already live**; refetch now |
  | `generation-failed` | `{ "code": string, "message": string }` | The previous snapshot is still live |

  No other event type is emitted. A `: keepalive` comment every **15 s** (D4)
  keeps idle proxies and browsers from silently dropping the stream.
- **`generation-failed` never exposes an absolute path or a raw command line.**
  This is a hard requirement, not a preference: a generation failure is exactly
  where a naive implementation leaks the most — cargo and rustdoc failures arrive
  as stderr containing `/home/<user>/…`, `C:\Users\<user>\…`, `CARGO_HOME`, and
  full `rustc --edition=2024 …` invocations. The payload is therefore built from
  **`CommandFailure`'s stable `code` plus its already-sanitized message** (the
  same content `/api/diagnostics` is allowed to carry under PRD 06's "never
  expose" rule) — it is **never** raw child-process stderr, and **never** the
  changed file paths that triggered the run. The browser learns *that* generation
  failed and *which stable code* it failed with; the full detail stays in the
  terminal, where it is already visible to the person who owns the machine.
- **Replay is explicitly unsupported, by construction.** The server **never emits
  an SSE `id:` field**, so a browser has nothing to put in `Last-Event-ID` and
  cannot request replay. This is deliberate: these are **state notifications, not
  a log** — an event means "a newer snapshot exists", and the truth is always
  whatever `/api/document` returns *now*. Replaying a "succeeded" from three
  snapshots ago would be worse than no replay. **If a client sends
  `Last-Event-ID`, the server ignores it** (no error).
- **Reconnect**: the server sends `retry: 1000` (D4) and `EventSource`
  auto-reconnects. **On every (re)connect the client refetches once**,
  unconditionally. That single rule makes missed events a non-issue — a client
  disconnected across a swap converges on reconnect with no replay machinery.
- **Backpressure**: a bounded `tokio::sync::broadcast` (capacity 16, D4) per
  process; a lagging client is **dropped**, not buffered, and its `EventSource`
  reconnects and refetches. Unbounded buffering for a dead client is how a local
  dev server leaks memory.
- **Why the snapshot header is required — LOCKED (B2).** The three artifacts are
  three requests, so a swap landing between them can serve a document from
  generation N with diagnostics from N+1. Today this is unobservable (nothing
  swaps while a server runs); **watch mode makes it reachable**, and a graph
  rendered against the wrong diagnostics is a silent wrong answer rather than a
  visible error. `ArtifactSnapshot` already carries a `marker`, but it holds the
  **raw `generation.json` bytes** and no endpoint exposed it. The approved
  **PRD-06 amendment A1** adds `SnapshotMarker::token()` (cached lowercase-hex
  BLAKE3 — raw bytes cannot go in a header) and returns it as
  `X-CrateVista-Snapshot` on all three routes.

### Frontend

- **`EventSource` is created only when `/api/health.watch_enabled === true`**
  (PRD-06 amendment A2). The client probes health first and, if watching is off,
  **never constructs an `EventSource` at all**. This is not defensive tidiness:
  `EventSource` **reconnects forever** on failure, so pointing one at a server
  without `/api/events` produces an endless 404 loop — in every `serve` session
  and, since the hook ships inside the committed bundle, in **every static
  export** too. The probe is one request; the alternative is a permanent
  background error in the majority of sessions. A missing or unparseable
  `/api/health` is treated as `false`.
- **Mixed-triple handling** (A1): every artifact response carries
  `X-CrateVista-Snapshot`. The loader compares the three values; **if they
  disagree, it discards the whole triple and refetches** — the triple is
  incoherent. **Bounded at three attempts total.** Three is enough: a mismatch
  requires a swap inside one fetch window and swaps are debounce-limited, so a
  second collision is already unlikely and a third is pathological. Unbounded
  retry against a fast-regenerating workspace would spin forever, which is worse
  than the mixed read it prevents.
- **A failed retry keeps the currently rendered snapshot.** After three failed
  attempts the loader **does not clear the graph, does not render an empty state,
  and does not reload**: the explorer keeps showing the snapshot already on
  screen, and the hook reports a **non-blocking reload error** (the same banner
  channel as `generation-failed`), leaving the UI usable. The next
  `generation-succeeded`, or the next reconnect refetch, clears it. This mirrors
  the server-side rule exactly — **a failure never destroys the last good state**,
  it only declines to replace it. Wiping a perfectly good graph because a refetch
  raced would be a self-inflicted outage.
- **Refetch the three artifacts; do not reload the page.** The existing
  `web/src/api/load.ts` already fetches all three concurrently under one
  `AbortController` and a monotonic token that discards stale responses — the
  live-reload hook calls **that**, adding no new fetching logic (it gains the
  header comparison and bounded retry). A full page reload would discard graph
  state (zoom/pan, ELK layout) and re-run layout on every save, which is precisely
  what watch mode exists to avoid.
- **New file `web/src/api/liveReload.ts`**: probes health, subscribes to
  `/api/events` when enabled, calls the loader on `generation-succeeded` and on
  every (re)connect, and surfaces `generation-failed` / reload errors as a
  non-blocking banner while the current graph stays on screen.
- **`web/dist` must be rebuilt and committed** with this change, and
  `npm run check:dist` / `check:embed-rebuild` must pass — the existing workflow
  is preserved exactly.

## Caching: deferred — LOCKED (B4)

**Persistent caching is deferred to `ISSUES/issue_12_persistent_cache.md`** and
watch mode ships without it. This deliberately contradicts the issue's stated
scope and moves two of its acceptance criteria; the maintainer approved the split
on 2026-07-16. The rationale is recorded in
`docs/adr/0008-watch-and-live-reload.md`.

Why deferral is the honest call:

1. **The prerequisite does not exist.** `cache_key` needs an `input_digest`, and
   **nothing computes one**. There is no per-target source-file enumeration
   (`RustdocPlan` carries `package_root`, not file lists) and
   `GenerationReport.input_hashes` is an **empty map**. A correct digest means a
   new file-walk + hashing subsystem with its own ignore policy — a second place
   to answer "which files are inputs", able to disagree with the watcher.
2. **A stale cache is the worst failure this product can have.** The output is an
   architecture map people trust. A wrong-but-fast map is worse than a slow
   correct one, and cache bugs are exactly the class that survives review.
3. **The cheap win is already there.** Cargo caches builds; debounce +
   single-flight remove redundant runs. Watch-mode correctness does not depend on
   a cache at all.
4. **A safe spec is large**: ownership, directory, per-stage key granularity,
   invalidation, **corruption recovery**, size/cleanup policy, concurrent access
   from two watching processes, and `--no-cache`. That is its own issue.

**Consequences, now binding:**

- `cratevista_rustdoc::cache_key` **stays exactly as-is** — the existing
  cache-key format is kept and **no second format is invented**. Its doc comment
  naming issue 09 as the `input_digest` supplier is **re-pointed at issue 12**
  (D5).
- **`--no-cache` is NOT added.** A flag that disables a cache that does not exist
  is a lie in `--help`, and it would have to keep working forever.
- Issue-09 criteria "Cache keys include all inputs that affect output" and
  "`--no-cache` behavior is defined and tested" **have moved** to
  `ISSUES/issue_12_persistent_cache.md`, **created by this PRD** (11 is taken by
  `issue_11_source_path_duplication.md`).
- **ADR-0008 is written** as *"watch mode, live reload, and deferring the
  persistent cache"* — `docs/adr/0008-watch-and-live-reload.md`, and is now
  **Accepted** (the implementation has landed). INDEX line 115 reserves 0008 as
  "caching/watch", which still fits.

## Technical design

### Module boundaries

```text
crates/cratevista-watch/src/
  lib.rs        # public API; #![forbid(unsafe_code)]
  classify.rs   # WatchSet + include/ignore decision (pure, lexical)
  debounce.rs   # burst coalescing (pure; caller-supplied timestamps)
  engine.rs     # single-flight state machine (injected regen operation)
  event.rs      # EngineEvent (the engine's own outcome events)
  plan.rs       # WatchPlan / WatchRegistration / RegistrationMode
  watcher.rs    # the real notify-backed adapter (WatchEvent, Watcher)
```

Depends on `notify` and `tokio` **only** — **not `cratevista-schema`** (no type
from it is required). A dependency-boundary test mirrors `cratevista-config`'s and
asserts `cargo tree -p cratevista-watch -i {core, server, schema}` reports no
path.

Two event types, deliberately distinct and not shared: **`EngineEvent`** is what a
regeneration run reports, and **`cratevista_server::ServerEvent`** is what the SSE
route renders. Core converts one to the other, so neither crate depends on the
other.

Server additions (**flat modules — there is no `routes/` directory**):

```text
crates/cratevista-server/src/events.rs   # GET /api/events, broadcast subscribe
crates/cratevista-server/src/state.rs    # + events: broadcast::Sender<ServerEvent>
crates/cratevista-server/src/router.rs   # + .route("/api/events", get(events::events))
crates/cratevista-server/src/snapshot.rs # + SnapshotMarker::token()               (A1)
crates/cratevista-server/src/api.rs      # + X-CrateVista-Snapshot, watch_enabled  (A1/A2)
```

Core addition: `crates/cratevista-core/src/watch.rs` — builds and atomically
rebuilds the `WatchSet` from metadata + `cratevista_config::discover` +
`referenced_files` (B3), owns the engine, and wires `run_generate` →
`load_snapshot` → `replace_snapshot` → event sink.

### Data model

> **These were design sketches and the code has moved past them.** The shipped
> types are listed here as they actually are; the sketches they replaced (a
> `WatchSet` of `roots`/`files`/`ignores`, and a `WatchEvent::Changed(PathBuf)`)
> never existed in this shape and are deleted rather than maintained as fiction.

```rust
// cratevista-watch — classification is by *kind*, not by three parallel lists.
pub enum InputKind { ExactFile, RustSourceRoot, FlowsDir, OverridesDir, WorkspaceMemberManifestPattern }
pub struct WatchInput { pub path: PathBuf, pub kind: InputKind, pub excludes: Vec<String> }
pub struct WatchSet { /* private: normalized, sorted, deduplicated inputs + root */ }
pub struct WatchPlan { /* private: a WatchSet + its sorted WatchRegistrations */ }
pub struct DebounceOptions { pub quiet: Duration, pub max_delay: Duration }  // 300 ms / 2 s

// One debounced burst, or an operational warning — never a bare path.
pub enum WatchEvent {
    Regeneration(RegenerationRequest),
    WatcherFailed { code: String, message: String },
}

// cratevista-core — a plan plus the logical inputs it came from, so recovery can
// be a superset of what is live without asking the watcher what it holds.
pub struct CorePlan { pub plan: WatchPlan, pub inputs: Vec<WatchInput> }

// cratevista-server — unchanged.
pub enum ServerEvent {
    GenerationStarted,
    GenerationSucceeded { partial: bool },
    GenerationFailed { code: String, message: String },
}
```

### Control flow

`notify` → `classify` (drop `target/**`, `.git/**`, unreferenced paths) →
`debounce` (300 ms quiet / 2 s max) → `engine` (single-flight + dirty flag) →
the regeneration transaction, **coverage first**:

```text
parse safe root recovery inputs
  → build/activate recovery coverage
  → build/activate complete metadata/config coverage
  → generate
  → load + verify
  → commit snapshot
```

Neither plan activation publishes document state; only the commit does.

**This supersedes two earlier versions of this section.** The original put the
WatchSet rebuild *after* the swap; the core-foundation phase moved it before the
swap but still *after* generation, on the reasoning that the plan and snapshot
"must become current together". That reasoning was wrong, and the order it
produced lost edits:

- a run introducing a new member, source root or referenced doc was **not
  watching those files while it ran**, so an edit between the start of generation
  and the activation of the plan was dropped;
- and a **failed** run never reached the activation at all, leaving the old plan
  active — so the fix, an edit to the files the failed run introduced, was
  unwatched. The user would correct the error and nothing would happen.

The plan and the snapshot are **not** the same kind of thing: the plan is
liveness coverage, the snapshot is published state. Coverage may lead; it may
never lag. Newer artifacts may sit on disk after a late failure; that is not a
lie, because what is *served* is the previous in-memory snapshot, and the next
successful run commits whatever is on disk then.

### Error handling

Generation failure → event + log, engine returns to `Idle`, server keeps serving.
A `load_snapshot` failure after a *successful* generate is treated identically (it
means the artifacts are unreadable or invalid — never swap). Watcher errors →
warning + `WatcherError` event; if `notify` cannot initialize at all, watch mode
reports it and **the server still serves** the existing snapshot rather than
exiting. **No poll fallback** (D2).

### Compatibility

Additive. `open` without `--watch` and `serve` behave exactly as today; no new
route is reachable and the frontend hook never starts (`watch_enabled: false`).
**No schema change and no `SchemaVersion` bump** — a transport header and a health
field are not schema artifacts, and watch publishes existing artifacts unchanged.
PRD 10's static build has no server and no events.

### Security and privacy

Same loopback bind, same CSP, same `/api/source` policy. `/api/events` exposes
**no paths** (see the `generation-failed` rule above), and **changed file paths
are never sent to the browser**. `X-CrateVista-Snapshot` is a hash of
already-public bytes and reveals nothing new. `watch_enabled` is a bare boolean.
The watcher reads only paths inside the workspace and never `.git/**`; B3's
exclusion of traversing spellings prevents registering a watch outside the
workspace.

## CLI/API/configuration changes

- **`--watch` on `open` only** (B1). `serve` is untouched: no `--watch`, no
  `GenerateArgs`, contract unchanged.
- `GET /api/events` (SSE).
- **`X-CrateVista-Snapshot`** on `/api/document`, `/api/generation`,
  `/api/diagnostics` (PRD-06 amendment A1).
- **`/api/health` gains `watch_enabled: bool`** (PRD-06 amendment A2); `false`
  under `serve`, always present.
- **No `--no-cache`** (B4). **No `[watch]`/`[cache]` config sections** —
  `RawRootConfig` is `deny_unknown_fields`, so adding them needs a PRD-08
  amendment; ignore patterns are therefore **not configurable** (D3).

## Files and modules to create or modify

- **Create**: `crates/cratevista-watch/**` (+ `[workspace] members` + `notify` in
  `[workspace.dependencies]`), `crates/cratevista-server/src/events.rs`,
  `crates/cratevista-core/src/watch.rs`, `web/src/api/liveReload.ts`.
- **Modify**: `server/src/{state,router,api,snapshot}.rs`,
  `cargo-cratevista/src/cli.rs`, `core/src/open.rs`, `web/src/api/load.ts` (header
  comparison + bounded retry), `web/src/App.tsx` (mount the hook), `web/dist`
  (rebuild + commit), `docs/configuration.md` (watch section + the fixed ignore
  set), `README.md`.
- **Amendments in their owning crates** (recorded in PRDs 06/08, which keep their
  Implemented / Verified status): `SnapshotMarker::token()` + the header;
  `watch_enabled`; `ConfigOutcome.referenced_files` in `config/src/{lib,docs}.rs`.
- **Comment-only**: re-point `crates/cratevista-rustdoc/src/cache.rs`'s
  `input_digest` reference from issue 09 to issue 12 (D5).
- **Created by this PRD (documentation, already landed)**:
  `ISSUES/issue_12_persistent_cache.md`, `docs/adr/0008-watch-and-live-reload.md`.

## Testing strategy

### Unit tests (deterministic; no real filesystem, no cargo)

- `classify`: `.rs` / manifest / `cratevista.toml` / flow TOML / **referenced**
  doc → watched; `target/**`, **`target/cratevista/**`**, `.git/**`,
  `web/node_modules/**`, `web/dist/**`, and an **unreferenced**
  `.cratevista/docs/*.md` → ignored.
- `debounce` (**injected clock — no sleeps, no wall-clock assertions**): a burst
  coalesces to one fire; two bursts separated by a quiet window fire twice; a
  continuous stream fires at `max_delay`; create+remove+rename of one path
  coalesces once.
- `engine` (fake regen closure): a change during a run schedules **exactly one**
  follow-up; ten mid-run changes still schedule **one**; the closure is **never**
  entered re-entrantly (a counter asserts max concurrency 1); failure leaves the
  previous snapshot and emits `GenerationFailed`; success emits
  `GenerationSucceeded` **after** the swap (the fake sink reads state to prove
  ordering).
- `generation-failed` payload: a failure whose message contains an absolute path
  and a command line is sanitized — asserted to contain no leading `/`, no
  drive-letter, no UNC prefix.

### Integration tests (real watcher, bounded timeouts)

- Real `notify` on a `tempfile` workspace with a **fake regen closure** (no
  cargo/nightly): touching a watched `.rs` fires exactly one regeneration;
  writing `target/cratevista/document.json` fires **none** (the no-loop test);
  deleting and renaming a watched file each fire exactly one.
- **Coverage rebuild**: coverage is built and activated **before** each
  generation, so adding a new source file, a new flow TOML, or a newly referenced
  doc is watched **without restarting** — and a failed run keeps the coverage it
  activated, so the fix stays observable.
- **Missing-file watch**: a `docs` entry naming a nonexistent file is watched via
  its nearest existing parent; creating the file fires exactly one regeneration.
- **Per-event classification**: an editor backup/temp file created beside a
  watched `.rs` inside a watched directory fires **none**.
- **No timing-only assertions.** Every wait is "await a signal (channel/counter)
  with a generous bound (5 s) and assert the *count*", never "sleep 400 ms then
  assert". Negative cases wait for a **positive control** event to arrive and only
  then assert the counter is still zero — so they cannot pass by being slow.
- Server: two subscribed SSE clients both receive events; a dropped client does
  not stall the engine; shutdown closes streams and joins tasks; `/api/events`
  emits no `id:`; the three artifact routes return an identical
  `X-CrateVista-Snapshot` that changes after a swap; `watch_enabled` is `false`
  under `serve` and `true` under `open --watch`.
- Config (B3): a fixture referencing a missing file, an oversized example and a
  `..`-traversing path yields the first two in `referenced_files` and not the
  third; a shared `.md` appears once; output is sorted and stable.
- CLI: `serve --help` lists neither `--watch` nor any generation flag;
  `open --help` lists `--watch`.

### End-to-end / browser

- **Real-server Playwright test** (existing harness + CSP): with the page open,
  replace the served artifacts and emit `generation-succeeded` → the explorer
  refreshes **without a page reload** (assert no navigation) and shows the new
  content, with **zero CSP violations and zero page errors**.
- **Failed regeneration retains the prior document**: emit `generation-failed` →
  the previously rendered graph is still on screen and a failure banner appears.
- **Failed loader retry retains the prior document**: force three mismatched
  triples → the graph stays, a non-blocking reload error appears, no empty state.
- **No `EventSource` without the capability**: a `watch_enabled: false` server
  produces zero `/api/events` requests.
- Gated (nightly + real cargo): `open --watch` on a fixture workspace; edit a
  `.rs`; assert `/api/document` changes exactly once.

### Fixtures

Reuse `web/e2e/fixtures/*`; add a synthetic event-sequence fixture for debounce
and a config fixture for `referenced_files`. No new committed artifact is needed
(no schema change).

## Performance considerations

Debounce + single-flight bound the work; the cost is one full `run_generate` per
burst (no cache — B4). On the PRD-07 benchmark workspace this is the existing
generate cost, unchanged. The SSE channel is bounded (16) and drops slow clients.
The WatchSet rebuild is a `cargo metadata` read plus a directory listing, outside
the critical section.

## Observability and diagnostics

`tracing` spans for classification, debounce fire, generation start/end +
duration, swap, WatchSet rebuild, and event fan-out. The CLI prints a one-line
summary per regeneration.

## Documentation changes

`docs/adr/0008-watch-and-live-reload.md` (created; now **Accepted**);
`docs/configuration.md` watch section (**stating that ignore patterns are not
configurable**); README watch mode.

## Rollout and migration

Additive; `--watch` is opt-in on `open`. `serve`'s help text is unchanged.

## Risks and mitigations

- **Reload loop from our own output** → `target/**` never watched **and** per-event
  classification + a no-loop test gated on a positive control.
- **Overlapping generations** → structural single-flight + a max-concurrency
  counter test.
- **Stale coverage** → coverage is rebuilt **before** anything reads the
  workspace, in two steps (`previous → recovery → complete`), and each step is
  activated atomically. It is never rebuilt "after a successful generation": that
  order lost edits and left a failed run unable to observe its own fix. Tested by
  the barrier test, the recovery-retention tests and the ownership matrix.
- **The initial generation is the widest such window** (a cold `cargo doc` is the
  slowest thing that happens) → `open --watch` builds and activates the complete
  plan *before* generating, and buffers what it sees into one merged request.
- **Flaky CI timing** → injected clock in unit tests; count-based assertions with
  generous bounds in integration tests; no sleep-then-assert anywhere.
- **`notify` platform quirks (WSL/Windows)** → integration tests run on CI's
  matrix; no poll fallback (D2), and watch failure never stops serving.
- **Mixed-generation refetch** → `X-CrateVista-Snapshot` + bounded retry (A1/B2).
- **Path leak in failure events** → payload built from stable code + sanitized
  message, with a test.

## Alternatives considered

- **WebSocket** → rejected: updates are one-way; SSE adds no dependency and is
  already CSP-allowed.
- **Full page reload on update** → rejected: discards layout/zoom and re-runs ELK
  on every save; the existing loader already solves refetch.
- **Push the document over SSE** → rejected: duplicates `/api/document`, bypasses
  the verified snapshot path, and puts a large payload on the event channel.
- **Watching inside `cratevista-server`** → rejected: the server stays a
  reload-free artifact server (PRD 06) and core is the orchestrator.
- **`serve --watch` / a `watch` subcommand** → rejected by B1; `open --watch`
  reuses a command that already means "generate, serve, show me".
- **Publishing failure diagnostics to `/api/diagnostics`** → rejected: breaks the
  snapshot's coherence guarantee.
- **`EventSource` without a capability probe** → rejected: infinite 404 reconnect
  loop under `serve` and in static exports.

## Implementation sequence

1. ~~`classify` + `debounce` (pure, injected clock).~~ **LANDED 2026-07-17.**

> **Step 1 LANDED 2026-07-17 — PRD 09 stays Approved.** Only classification and
> debounce exist. **Not built:** the engine, any `notify` adapter, core
> orchestration, `/api/events`, `--watch`, frontend live reload, `web/dist`
> changes, and real-filesystem integration tests.
>
> **Shipped:** `crates/cratevista-watch` (`#![forbid(unsafe_code)]`), registered
> in `[workspace] members`, with `classify` and `debounce`.
> `classify`: `InputKind::{ExactFile, RustSourceRoot, FlowsDir, OverridesDir}`,
> `WatchInput::{file, rust_root, flows_dir, overrides_dir}`, `WatchSet::{new,
> classify, is_relevant, relevant, root, len, is_empty}`,
> `Classification::{Relevant(InputKind), Outside, Ignored(IgnoreReason),
> NotAnInput}`, `IgnoreReason::{GeneratedOutput, VersionControl,
> FrontendArtifacts, HiddenDirectory, EditorTemporary}`, `is_lexically_absolute`.
> `debounce`: `DebounceOptions{quiet, max_delay}` (`DEFAULT_QUIET` **300 ms**,
> `DEFAULT_MAX_DELAY` **2 s**), `Debouncer::{new, record, record_if_relevant,
> deadline, poll, reset, is_idle, pending, options}`.
>
> **Zero dependencies — the manifest's `[dependencies]` is empty.** Classification
> is lexical and the debouncer takes caller-supplied `Duration` timestamps, so
> nothing here needs `notify` or `tokio`; they arrive with the code that uses
> them. The public data model is `PathBuf`/`Duration`/plain values, so **no
> `cratevista-schema` dependency is required** and none was added.
>
> **`Duration`-as-timestamp, not `Instant`** *(deviation worth naming)*: the PRD
> said "injected clock". `Instant` cannot be constructed at an arbitrary value, so
> a test could only ever say `Instant::now() + offset` — reintroducing the real
> clock. Timestamps are therefore a caller-supplied monotonic `Duration` since the
> caller's own epoch. Nothing sleeps and nothing reads a clock, so exact-boundary
> assertions are possible at all.
>
> **Boundaries are defined, not left to a scheduler**: firing is **inclusive** at
> `now == deadline`, asserted one nanosecond either side of both the quiet and the
> maximum deadline.
>
> **The lexical/symlink split is documented** in `lib.rs` and `classify.rs`:
> `classify` rejects text that escapes the root and **is not a symlink check**;
> core's later WatchSet builder must canonicalize existing registration targets
> and refuse any resolved outside the canonical root.
>
> **Tests: +59 (workspace 405 → 464).** 30 classify: every input class
> (manifests/lockfile/`cratevista.toml`; `*.rs` at depth incl. new files; flow and
> override TOMLs); non-recursive config dirs (a nested TOML is **not** an input);
> recursive Rust roots; `src` not matching `srcgen`; referenced vs **unreferenced**
> docs; a **missing referenced file still classifiable**; `target/**`, `.git/**`,
> `web/node_modules/**`, `web/dist/**`; hidden dirs ignored **with the
> `.cratevista` exception explicitly asserted**; a hidden *file* is not a hidden
> directory; editor temp/backup/probe names (`~`, `.swp`, `.#`, `#…#`, `4913`,
> `.tmp`, `.bak`, JetBrains) ignored **while the rename destination still fires**,
> and a temp whose destination is irrelevant stays irrelevant; lexical
> outside-workspace rejection; `..` normalizing back inside; Windows and Unix
> spellings (both root and event, either separator) without platform gates;
> sorted/deduplicated output; determinism over repeats. 24 debounce: defaults;
> first event starts both deadlines; later events reset **only** quiet; max stays
> anchored to the first; earlier deadline wins; **exact boundaries** on both;
> create/modify/remove/rename coalescing; sorted/dedup output; reset; two
> independent bursts; a second burst re-anchoring the max; non-monotonic clamp;
> custom timings; **irrelevant events neither starting nor extending a burst**;
> determinism. 5 boundary: no cratevista crate depended on, **no dependency at
> all**, nothing depends on watch yet, workspace registration, `forbid(unsafe_code)`.
>
> **Negative controls** (each reverted): dropping the `.cratevista` exception fails
> 4 classify tests; anchoring the max deadline to the *last* event (the starvation
> bug) fails 4 debounce tests; matching source roots by string prefix fails the
> `srcgen` test.
>
> **My own test was wrong once**: `a_continuous_stream_cannot_starve_regeneration`
> stopped emitting at 990 ms, so the quiet window legitimately fired first and the
> maximum was never exercised. Fixed the test to stream past 2 s, not the code.
2. ~~`engine` single-flight with a fake closure.~~ **LANDED 2026-07-17.**

> **Step 2 LANDED 2026-07-17 — PRD 09 stays Approved.** **Not built:** `notify` or
> any real watcher, core orchestration, snapshot loading/swapping, `/api/events`,
> `--watch`, frontend live reload, `web/dist` changes, real-filesystem tests.
>
> **Shipped:** `crates/cratevista-watch/src/{engine,event}.rs`.
> `event`: `EngineEvent::{GenerationStarted, GenerationSucceeded{partial},
> GenerationFailed{code, message}}` + `is_terminal()`.
> `engine`: `RegenerationRequest::{new (sorts/dedups, `None` when empty), paths,
> len, is_empty, merge}`, `RegenerationSuccess{partial}`,
> `RegenerationFailure::{new, code, message}`, `RegenerationResult`, the
> `Regenerate` trait (boxed future, object-safe), `spawn`, `Engine::{handle,
> join}`, `EngineHandle::{submit, shutdown, is_closed}` (cloneable), `EngineClosed`
> (a typed `std::error::Error`, never a panic).
>
> **`tokio` earned, minimally: `rt` + `sync` + `macros`.** `rt` for the one task,
> `sync` for the channels, `macros` for `select!`. **`time` is not taken** —
> nothing in the shipped crate sleeps or reads a clock. `notify` is still absent.
> `cargo tree -p cratevista-watch` shows `tokio` as its only dependency, and
> `cargo tree --invert cratevista-watch` shows **no reverse dependencies**.
>
> **Empty requests are impossible by type**: `RegenerationRequest::new` returns
> `Option`, so the operation cannot be entered with nothing to do — no runtime
> check to forget.
>
> **The engine sanitizes nothing** and says so in its docs: `code`/`message` are
> transported verbatim, because the engine has no idea what a workspace root looks
> like — which is exactly why it must not try. Core supplies browser-safe values.
> `EngineEvent` **carries no path in any variant**, so a consumer cannot leak one
> by forwarding an event; a test greps every rendered event for a leak.
>
> **One call site, one task** → concurrency is bounded at one structurally; a test
> asserts the observed maximum is 1 anyway.
>
> **`select!` is `biased;`** *(a decision worth naming)*: the command branch is
> polled before the completion branch, so a request that arrived **before** the
> operation finished is always folded into the dirty set rather than lost to a
> random branch choice. Without it, the loss is rare, timing-dependent and nearly
> untestable — proven below by a negative control, where removing `biased` makes
> four tests **hang**.
>
> **⚠ Superseded by step 2.1 below: command-first bias fixed the lost request but
> introduced completion starvation.**

> **Step 2.1 LANDED 2026-07-17 — fairness correction. PRD 09 stays Approved.** No
> public API change; no new shipped dependency.
>
> **The starvation step 2 shipped.** `biased;` polled **commands first**, so while
> the command channel was continuously ready the completion branch was never
> polled at all. A finished regeneration could sit unobserved — **and its terminal
> event unemitted** — for as long as edits kept arriving. Not hypothetical: with a
> second thread refilling the channel, the terminal never arrives.
>
> **The new scheduling rule.**
> 1. `biased;` with **completion first**. While the operation is pending its branch
>    yields, so commands are folded into the dirty set exactly as before; the
>    instant it is ready it is taken, whatever is queued behind it.
> 2. On completion, **drain the already-queued commands with non-blocking
>    `try_recv`**, bounded by `commands.len()` sampled at that instant.
> 3. `Shutdown` found in the drain is honored **before** deciding on a follow-up.
> 4. Emit the run's terminal event.
> 5. Start at most one merged follow-up, and only if shutdown was not requested.
>
> Nothing is lost by preferring completion: the commands are still queued, and the
> drain takes them before any follow-up is decided. A request arriving *after* the
> drain stays queued for the follow-up's own `select!` or the idle wait.
>
> **Two bugs my own tests found while writing this** — both worth recording,
> because each moved the starvation rather than removing it:
> - **The drain was unbounded** (`while try_recv()` until empty). A channel
>   refilled faster than it is read is *never* empty, so the drain span forever:
>   the same starvation, relocated from the `select!` into the drain. Fixed by
>   sampling `commands.len()` first, so the drain always terminates however hard
>   the producer pushes.
> - **The dirty accumulator was `Option<RegenerationRequest>`** and the drain
>   called `merge` once per command; `merge` rebuilds its whole `BTreeSet`, so
>   folding N queued commands was **quadratic** and stalled the terminal event
>   behind a deep backlog. The run loop now accumulates into a `BTreeSet<PathBuf>`
>   directly (O(log n) per path) and builds one `RegenerationRequest` at the end.
>   `merge` remains public and unchanged.
>
> **No sleeps, yields or wall-clock fairness assumptions** are used in the fix.
>
> **Tests: +7 (watch 84; workspace 489 → 496).** `a_completed_run_reports_even_while_requests_arrive_continuously`
> is the one that exhibits starvation at all: a second **OS thread** floods the
> channel continuously and stops only when the terminal event is observed, so the
> test terminates on the engine's own signal rather than a timer, and the outcome
> is deterministic with correct code. It needs `rt-multi-thread` — **dev-only** —
> because on a current-thread runtime the engine never yields while commands are
> ready, so the channel always drains and the scenario cannot occur. Also added:
> completion observed under a 5,000-deep pre-queued backlog with all 5,000 merged
> into one follow-up; every request queued before completion landing in exactly one
> follow-up; a request submitted after the drain still starting a run; `Shutdown`
> queued behind two submits suppressing the follow-up while the in-flight terminal
> is still emitted; strict alternation under backlog; concurrency still 1 under
> backlog.
>
> **An honest note on the finite-backlog tests**: they pass under *both* orderings,
> because a finite backlog drains and completion is then observed. They are
> regression tests for the drain, **not** proof of the fairness fix. Only the
> continuous-flood test distinguishes the two, and it is the one cited below.
>
> **Negative controls** (each reverted): command-first bias → the flood test fails
> via the watchdog ("timed out waiting for the terminal event while requests arrive
> continuously"); an unbounded drain → the flood test hangs past 160 s.

> **Step 2.2 LANDED 2026-07-17 — shutdown/follow-up race closed. PRD 09 stays
> Approved.** No public API change; no new dependency.
>
> **The invariant did NOT already exist — production needed a fix.** `spawn`
> created the shared `closed: Arc<AtomicBool>` and gave it **only to the handle**;
> the engine task never received it, so `stopping` was set *solely* by observing a
> `Shutdown` **command**. Because step 2.1's drain is bounded by a queue length
> sampled at completion, a `shutdown()` landing after that sample leaves its
> command unread and `stopping` false — and the engine would then start a dirty
> follow-up it had already promised not to, while the caller had already been told
> the engine was closing.
>
> **The fix:** `spawn_inner` now passes `closed.clone()` into the task, and the
> follow-up decision reads `if stopping || closed.load(SeqCst) { return; }`. This
> is race-free by ordering, not by luck: `EngineHandle::shutdown` flips the flag
> **before** it sends the command, so any shutdown requested before this instant is
> visible here, and one requested after it is — by definition — after the decision
> (that follow-up then observes the command in its own `select!` and stops after
> its own terminal event). Completion-first selection, the bounded drain and every
> step-2.1 fairness behavior are untouched.
>
> **The forced interleaving.** Between emitting a terminal event and deciding on a
> follow-up the engine has **no `await` point**, so no other task can interleave
> there by scheduling alone — the window is unobservable without an injected pause.
> A `Hooks` struct provides one, and **both the field and the `await` that consults
> it are `#[cfg(test)]`**, so production compiles to exactly the previous code: no
> branch, no allocation, no behavior change (`cargo build -p cratevista-watch`
> compiles the field out entirely; the `run` parameter is `_hooks` because in a
> production build it is genuinely unused). The test drives:
> 1. submit → the operation is entered (a run is in flight);
> 2. submit again → dirty work is queued behind it;
> 3. complete the run → the engine drains (sampling the queue length and consuming
>    the dirty request) and emits the terminal event;
> 4. the engine **parks** at the hook, immediately before the follow-up decision,
>    and signals the test on a channel;
> 5. the test calls `shutdown()` — the `Shutdown` command lands in the channel
>    *after* the drain's sample, so **no drain will ever see it**;
> 6. the test releases the engine through a `oneshot`;
> 7. the terminal event is asserted (already emitted before the decision, so
>    shutting down cannot suppress it);
> 8. **no second `GenerationStarted` and no second operation call**;
> 9. `join` completes and a later `submit` returns `EngineClosed`.
>
> Every step is a channel/barrier signal; the dev-only timeout remains a watchdog
> only. A paired control (`without_shutdown_the_parked_decision_still_starts_exactly_one_follow_up`)
> runs the same forced interleaving **without** the shutdown and asserts the
> follow-up still runs — otherwise the fix would be indistinguishable from simply
> dropping dirty work after every run.
>
> **Tests: +2 (watch 86; workspace 496 → 498).** Retained and still passing:
> continuous flooding cannot starve completion; pre-completion commands merge into
> one follow-up; max concurrency 1; `Started → terminal` ordering; no path in any
> `EngineEvent`.
>
> **Negative control** (reverted): dropping the `closed` check at the decision
> point → `shutdown_between_the_drain_and_the_follow_up_decision_discards_the_follow_up`
> fails — the engine starts the forbidden follow-up and never joins.

> **Server-events phase LANDED 2026-07-17 (implementation-sequence item 4) — PRD 09
> stays Approved; PRD 06 stays Implemented / Verified.** **Not built:** `notify`,
> any watcher, core orchestration, a `cratevista-watch` dependency, `--watch`,
> frontend `EventSource`/live reload, `web/dist` changes, or any artifact
> generation / snapshot replacement. `cratevista-core` and `cargo-cratevista` are
> **untouched** (0 changed files).
>
> **Shipped:** `crates/cratevista-server/src/events.rs` — `ServerEvent::{GenerationStarted,
> GenerationSucceeded{partial}, GenerationFailed{code, message}}` with `name()` and
> `data()`, `EVENT_CHANNEL_CAPACITY = 16`, `KEEPALIVE_INTERVAL = 15s`,
> `RETRY_INTERVAL = 1000ms`, the `events` handler, and a hand-written `EventStream`.
> `AppState` gained a bounded `broadcast::Sender<ServerEvent>` plus
> `subscribe_events()`/`publish_event()`; **the raw sender is not exposed**, and both
> constructors keep their existing signatures and build their own channel.
> `router.rs` registers `/api/events` **only** when `state.watch_enabled()`.
>
> **The route is conditional, not the channel.** A non-watching state still owns a
> channel (an unused sender costs a pointer) but has no route, so `serve` keeps the
> existing unknown-API JSON `404` — not an endpoint that accepts a connection and
> then says nothing forever. `/api/health.watch_enabled` remains the capability
> source; the route is not registered "because the frontend promises not to connect".
>
> **The exact wire** (asserted byte-for-byte against the real handler body):
>
> ```text
> retry: 1000
>
> event: generation-started
> data: {}
>
> event: generation-succeeded
> data: {"partial":false}
>
> event: generation-failed
> data: {"code":"rustdoc_failed","message":"the crate did not compile"}
> ```
>
> **No `id:` is ever emitted**, so a browser has nothing to put in `Last-Event-ID`
> and **replay is impossible by construction**; a supplied `Last-Event-ID` is
> ignored (200, no replay), not rejected. Headers: `text/event-stream`,
> `Cache-Control: no-store`, plus the existing global security headers — the CSP is
> unchanged, because `connect-src 'self'` already allowed this. No
> `X-CrateVista-Snapshot`: this is not an artifact route.
>
> **Lagging ends the stream.** A subscriber that overflows the 16-slot channel gets
> `RecvError::Lagged` and its stream **terminates** rather than skipping ahead — a
> truncated history would let a client believe it had seen everything, whereas
> ending makes its `EventSource` reconnect and refetch, which is the only correct
> recovery for state notifications. A closed sender ends the stream cleanly. There
> is **no per-client queue**: each subscriber is a broadcast cursor.
>
> **One new dependency: `futures-core`** *(deviation worth naming)*. Not for
> convenience: `broadcast::Receiver` has **no poll-based API**, so `/api/events`
> hand-writes a `Stream` adapter (a boxed `recv` future carrying the receiver
> between polls — no `unsafe`, no self-reference), and the `Stream` trait must be
> **nameable** to be implemented. `futures-core` was already in the tree via axum,
> so it adds no build cost. `tokio-stream` was the alternative: a larger surface for
> the same one trait. axum's own `Sse`/`Event`/`KeepAlive` do the framing, so no SSE
> library was added.
>
> **The server infers nothing**: it publishes what it is told and renders it. It
> never calls `replace_snapshot`, and converting `cratevista-watch`'s `EngineEvent`
> into a `ServerEvent` is core's job in the next phase — **the server does not
> depend on `cratevista-watch`**, and a new `tests/dependency_boundary.rs` fails if
> it ever does (also barring core/config/graph/metadata/rustdoc and `notify`).
>
> **Tests: +19 (server lib 63 → 79, +3 boundary; workspace 498 → 517).** Route:
> non-watching → the existing JSON 404 (`not_found` / "unknown API route");
> watching → SSE with the right headers. Wire: `retry: 1000` first; each variant's
> exact name and payload; no frame carries `id:`; `Last-Event-ID` ignored; the
> keepalive is a **comment**, not a fourth event type (via an injected 10 ms
> interval — the production constant stays 15 s, and the wait is on the stream, not
> a sleep). Fan-out: two subscribers get the same events in publication order; a
> dropped subscriber neither blocks nor fails the other; publishing with **no**
> subscribers is harmless and is not replayed to a later joiner; **overflowing
> capacity 16 terminates the lagging stream**; a closed sender ends it cleanly. The
> existing artifact / snapshot-header / health / CSP / source tests are unchanged and
> still green. The dev-only `timeout` remains a watchdog; every assertion is on a
> frame or a channel.
>
> **A stale test of mine was replaced**: `api_events_is_not_exposed_yet` (written
> when the route existed nowhere) is now `a_non_watching_state_serves_no_event_stream`
> — the old name had become false.
>
> **Negative controls** (each reverted): emitting an `id:` → 4 tests fail including
> the no-id check; registering `/api/events` unconditionally → the 404 test fails;
> making a lagged receiver skip ahead instead of terminating → the overflow test
> fails.

> **Real-watcher adapter phase LANDED 2026-07-17 — PRD 09 stays Approved.**
> **Not built:** core orchestration, `run_generate`/`load_snapshot`/`AppState`
> replacement, `EngineEvent → ServerEvent` conversion, `--watch`, frontend live
> reload, `web/dist`, or any cargo/rustdoc test. No crate depends on
> `cratevista-watch` yet.
>
> **Shipped:** `crates/cratevista-watch/src/plan.rs` — `RegistrationMode::{NonRecursive,
> Recursive}`, `WatchRegistration::{non_recursive, recursive}`, `WatchPlan::{new,
> watch_set, registrations, into_parts}`, `PlanError::{OutsideWorkspace,
> IgnoredLocation}`; and `crates/cratevista-watch/src/watcher.rs` —
> `WatchEvent::{Regeneration, WatcherFailed{code, message}}`, `WatcherError`,
> `WatcherClosed`, `spawn_watcher`, `spawn_watcher_with`, `Watcher::{replace_plan,
> shutdown, is_closed, join}`. **No `notify` type appears in the public API.**
>
> **Dependencies: `notify` 8 (default native backend) and `tokio` + `time`.**
> `cargo tree -p cratevista-watch --depth 1` shows exactly `notify` and `tokio`.
> **`time` is now earned**: the production adapter needs a real timer
> (`sleep_until`), while `Debouncer` still takes caller-supplied timestamps and
> reads no clock — the adapter converts one `Instant` epoch into the elapsed
> `Duration`s the debouncer wants. **`PollWatcher` is neither enabled nor used**,
> and a boundary test asserts `default-features = false` is *not* set so the
> recommended native backend stays. `tempfile` is a new **dev**-dependency for the
> real-filesystem tests.
>
> **`Debouncer` is the only debounce state machine.** The adapter reschedules from
> `Debouncer::deadline()` and fires on `Debouncer::poll()`; no quiet/max arithmetic
> is repeated. `RegenerationRequest::new` returning `Option` is what guarantees an
> empty request can never be emitted.
>
> **Event mapping:** `Create`/`Modify` (including rename `Modify(Name(_))`)/`Remove`
> → changed; `Access` and `Other` → ignored. **`Any` is treated as a change** — the
> backend is saying it does not know, and an extra regeneration costs a rebuild
> while a missed one leaves a stale map on screen. Every path of a multi-path event
> is classified; relative paths are resolved against the root first (some backends
> report relative paths, and an unresolved one would look like an escape and be
> dropped). **Every path is classified again against the *current* set** before it
> reaches the debouncer. Errors become `WatcherFailed`, **never** a regeneration.
>
> **No path or raw debug in a watcher message.** `describe()` maps each
> `notify::ErrorKind` to a fixed string chosen here; nothing from the error's
> `Display`/`Debug` is interpolated, because `ErrorKind::Generic` and
> `notify::Error::paths` can carry absolute paths. The one borrowed detail is
> `io::ErrorKind`, a closed enum of adjectives (`PermissionDenied`). `PlanError`
> carries a workspace-relative label or `<outside the workspace>`.
>
> **Atomic replacement, proven:** build the new native watcher → register every
> path → **only then** make the new `WatchSet` current → drop the old watcher. A
> failed replacement retains the **complete** old watcher and old set; no partial
> plan becomes active; the brief two-watcher overlap is harmless because
> classification and the debouncer's path set collapse duplicates. Replacement
> itself emits nothing.
>
> **Tests: +43 (watch 84 → 125: 100 lib, 7 boundary, 18 real-filesystem; workspace
> 517 → 549).** Real backend, real `tempfile` workspace, **no cargo, no nightly**:
> modify a watched `.rs`; create a nested `.rs` under a recursive root;
> create/modify/remove bursts coalesce; a rename reports its destination; two paths
> in one burst sorted and deduplicated; a direct flow TOML is relevant but a nested
> one is not; **a referenced missing file becomes relevant when its registered
> parent reports creation**; an unreferenced doc beside it stays ignored; editor
> backup/swap/probe/temp files emit nothing; **writing `target/cratevista/*.json`
> emits nothing**; continuous writes still fire at the maximum deadline; initial
> registration failure is typed with no leaked task; a failed replacement retains
> the old watcher (proven by a subsequent *old* positive control, and a second one
> for the config half of the set); a successful replacement activates the new plan
> and retires the old-only input; replacement after shutdown is typed; shutdown and
> handle-drop join cleanly.
>
> **Every negative case uses a positive control**, never sleep-then-assert: the
> noise is written first, then a real source change, and the assertion is that the
> arriving request contains *only* the real change — which proves the watcher was
> alive and delivering throughout. The 20 s `timeout` is a watchdog that never fires
> in a passing run (the suite runs in **0.63 s**).
>
> **Platform gates: none.** Every operation used (create/modify/remove/rename,
> recursive and non-recursive directory watches) is supported by all three native
> backends, so nothing is Linux-only. **No raw native event count is asserted** —
> one `fs::write` may produce one native event or five depending on the backend;
> the assertions are on debounced request counts and contents. Tests canonicalize
> the temp root, mirroring what core must do, because macOS reports `/private/var`
> for `/var`.
>
> **Deviations worth naming.** (1) `spawn_watcher_with(plan, sink, DebounceOptions)`
> is public alongside `spawn_watcher`: the integration tests drive a 60 ms/600 ms
> window so the suite does not spend the production 300 ms per burst, and
> `spawn_watcher` — what core will call — keeps the production defaults, with a test
> asserting it starts and stops for real. (2) A **bare** trailing hidden directory
> (`/w/.idea`) is *accepted* by `WatchPlan`: a lexical check cannot distinguish it
> from a hidden *file*, so the hidden-directory rule (which needs a component after
> the dot) cannot fire on a final component. It is wasteful, not wrong — every event
> underneath still classifies as ignored — and a test documents exactly that. My
> first draft asserted it was refused; the test was wrong, not the code.
>
> **Negative controls** (each reverted): dropping the old watcher *before* building
> the new one → the failed-replacement test fails via the watchdog; adopting the new
> `WatchSet` before registrations succeed → the same test fails.
>
> **Hardening (same day): the pre-activation event race.** A candidate native
> watcher starts reporting the moment its **first** registration lands, but its
> plan is not the truth until **every** registration has. An event arriving in that
> window has no decidable meaning yet — under the old set a new-only input looks
> irrelevant, and classifying it there would drop a real change permanently.
>
> **Was the shipped code vulnerable? No — but only by accident, which is the
> problem.** `replace` was a *synchronous* fn called inline from the `select!` arm,
> so the task could not poll `raw_rx` while it ran: candidate events queued unread
> and, on success, were classified after `current_set` had already become the new
> one. Correct — and resting entirely on an invisible invariant ("`replace` must
> never yield") that one `.await` would have silently broken. It also violated the
> letter of the contract on **failure**, where queued candidate events *were*
> classified against the old set (benign in practice: a new-only path was dropped,
> and a shared path would have been reported by the old watcher anyway).
>
> **The mechanism: generation-tagged events + staged activation.** Every watcher
> stamps its own id at the source (`Tagged = (u64, notify::Result<Event>)`), and an
> event is only ever classified against the set of *its own* generation:
> - an event whose tag matches a **live candidate** is **staged**, never classified;
> - an event whose tag matches the **current** generation is classified as before;
> - an event from any **retired** generation (a failed candidate, or a replaced
>   watcher) is dropped.
>
> `replace` is gone as a function; replacement is now two loop states. `Command::Replace`
> builds and fully registers the candidate, then holds it in `Option<Candidate>`;
> the loop **keeps running**, staging that candidate's events. Activation is one
> step with **no `await` inside it**: new set becomes current → staged paths drain
> through it → old watcher dropped → reply sent. Nothing observes a half-swapped
> state, and a staged event cannot be overtaken, because it is recorded before the
> loop reads another event. Failure or shutdown drops the candidate **with** its
> staged set, so an uncommitted plan can never act.
>
> **Boundedness.** The staging buffer is a `BTreeSet<PathBuf>` — a set of paths, not
> a queue of events — so duplicates cost nothing and the bound is *distinct paths
> touched during one replacement*, capped at **`STAGING_CAPACITY = 4096`**; the
> memory ceiling is that many `PathBuf`s. The notify callback sends into an
> **unbounded** channel, so a registration callback never blocks. **Overflow is not
> silent loss**: it raises a flag, and activation emits a typed
> `WatchEvent::WatcherFailed { code: "watch_staging_overflow" }` **and** records the
> whole new plan's registration paths, forcing one coarse regeneration rather than
> forgetting a relevant change.
>
> **Forced interleavings** (a `cfg(test)` `Hooks` seam — field *and* every use are
> `cfg(test)`, so production compiles unchanged and activation is attempted on the
> very next poll). The gate reports when the candidate is **registered and parked**,
> which is the window's opening edge, and `on_staged` reports each staged path:
> 1. **new-only staged event survives** — park at the gate, write `new/added.rs`
>    (which the *old* set does not watch), prove `on_staged` saw it, release, assert
>    exactly one request containing `new/added.rs`;
> 2. **failed replacement** — a plan whose first sorted registration exists and whose
>    second does not; assert `watch_registration_failed`, no `new/` path in any
>    request, and an old-plan positive control still delivering;
> 3. **old-plan event during setup** — written while parked; not lost;
> 4. **overlap duplicate** — a path both plans watch, seen by both watchers across
>    activation → **one** request, and no second one;
> 5. **activation ordering** — two staged events, then a third from the now-active
>    candidate; both staged paths are in the **first** request, never overtaken;
> 6. **shutdown with staged events** — nothing activates, nothing regenerates, the
>    reply resolves as an error, and both watchers are released on join.
>
> **Tests: +7 (watch 125 → 132; workspace 549 → 556).** Six forced-interleaving
> tests plus a staging-overflow bound test. Every existing test kept and green:
> failed replacement preserving the old watcher, successful replacement retiring
> old-only inputs, missing-referenced-file creation, `target/` no-loop protection,
> the native-backend suite, and the dependency boundary (**still exactly `notify` +
> `tokio`; no cratevista dependency**). **No public API change.**
>
> **A flake I found and fixed rather than tolerated.** One run in six timed out: a
> write landing in the instant after `watch()` returns can be missed while the
> registration is still arming — a property of the OS, not the adapter. The staging
> tests now **re-poke the stimulus until the backend reports it** (`poke_until_staged`)
> instead of writing once and hoping. Nothing is concluded from elapsed time and a
> broken adapter still fails; only the poking retries, and the extra writes touch
> one path, which the staging set collapses. **6/6 consecutive full-package runs
> clean afterwards.**
>
> **Negative control** (reverted): classifying candidate events against the old
> active set — the pre-hardening behavior — fails three staging tests, including
> the new-only-event test, which times out because the change is dropped for good.

> **Core-foundation phase LANDED 2026-07-17 — PRD 09 stays Approved.** **Not
> built:** `--watch`, `open`/`serve` startup changes, a real `Watcher`/`Engine`
> started from `open`, frontend live reload, `web/dist`, or any browser/nightly
> E2E. `cargo-cratevista`, `cratevista-server` and `web/dist` are **untouched (0
> changed files)**.
>
> **Shipped:** `crates/cratevista-core/src/watch.rs` — `build_watch_plan`
> (+ `plan_for_test`), `WatchSetupError` with the four setup codes plus three
> regeneration codes, the `Stages` trait, `Transaction<S>` implementing
> `cratevista_watch::Regenerate`, the failure mappers, and `to_server_event`.
> `generate.rs` gained a shared `metadata_options(&GenerateOptions)` so the watch
> plan ingests the **same** workspace generation reads — a second, drifting idea of
> which packages exist would watch the wrong files.
>
> **Dependency: `cratevista-core → cratevista-watch`, and nothing else changed.**
> `cargo tree --invert cratevista-watch` → `watch ← core ← cargo-cratevista`; the
> **server still depends only on `cratevista-schema`**. Core owns the
> `EngineEvent → ServerEvent` conversion precisely so a server→watch edge never has
> to exist. **No new external dependency.**
>
> **Logical input vs registration — they are not the same path**, and conflating
> them is how watch modes break:
> - an exact file (`crates/demo/Cargo.toml`) is registered as its **containing
>   directory, non-recursively** — a file watch follows an inode, and an editor's
>   write-temp-then-rename leaves it watching a file nobody will touch again;
> - a Rust root (`crates/demo/src`, the parent of a target's `src_path`) is
>   **recursive**;
> - `.cratevista/flows`, `.cratevista/overrides` are **non-recursive**;
> - a **missing** intended path (`Cargo.lock`, `cratevista.toml`, an unreferenced-
>   yet doc, `docs/deep/nested/guide.md` with three components missing) is watched
>   through its **nearest existing ancestor, recursively** — a missing path cannot
>   be registered, and only a recursive watch sees a whole chain being created.
>   Classification is unchanged by any of it: the set still admits only the exact
>   file, `*.rs` under a Rust root, or a direct `*.toml` in a config directory.
>
> **Symlink containment is the real check**, and it is here rather than in
> `cratevista-watch` (whose validation is lexical by design): the root is
> canonicalized **once**, every **existing** registration target is canonicalized
> and must still start with it, and an escape is `watch_symlink_escape`. The
> **intended missing path is never canonicalized** — there is nothing to resolve,
> and resolving the *ancestor being registered* is what proves the watch is safe. A
> `#[cfg(unix)]` test symlinks `crates/demo/src` outside the workspace and asserts
> the rejection.
>
> **`--no-config` removes every configuration input** — root, flows, overrides,
> docs and examples — while the code half is untouched. With config on, all three
> `referenced_files` kinds become exact inputs, including files that do not exist
> yet.
>
> **External dependency sources are never watched**: member packages are
> `package:{name}`, externals carry `@version`, and only members' manifests and
> targets become inputs.
>
> **Browser-safe by construction.** Cargo/rustdoc/`io::Error` messages are **not
> forwarded** — they carry absolute paths, `CARGO_HOME`, usernames and whole
> command lines. The stable `code` travels; the message is written here. Full detail
> stays in this process's tracing, where whoever ran the command can already see it.
> **`WatcherFailed` is deliberately *not* mapped to `GenerationFailed`**: a watch
> limit or a permission problem is an operational warning about the machine, not a
> failed build, and telling the browser otherwise would be a lie.
>
> **Changed paths never escape**: `Transaction::regenerate` drops the
> `RegenerationRequest` immediately — the paths that triggered a run are absolute
> paths on someone's machine and are never looked at again.
>
> **Tests: +33 (workspace 556 → 589).** 16 plan-builder (real temp workspaces,
> synthetic `MetadataIngest`, **no cargo**): root manifest + missing lockfile;
> multiple members; custom/nested `src_path` parents; two targets sharing a root →
> one sorted registration; no external roots; config on vs `--no-config`; absent
> `cratevista.toml` still an input; direct vs nested flow TOML; all three referenced
> kinds; missing reference via nearest parent; three missing components via one
> recursive ancestor; an existing file registered via its directory, not itself;
> canonical registrations; the symlink escape; an intended missing path never
> canonicalized; determinism over five builds; no absolute path in an error. 12
> transaction (injected stages, recorded call order, **no cargo/rustdoc/watcher**):
> the full order; partial still rebuilds the plan; success only after the swap; each
> of the four failure stages stopping exactly where it should with **nothing
> committed**; stable browser-safe codes; no changed path in a failure or event;
> identical sequences across repeats; and the conversion, exact in all three
> variants. 5 core dependency-boundary.
>
> **A stale test of mine was superseded**: `no_crate_depends_on_watch_yet` is now
> `only_core_depends_on_watch` — this phase adds the edge it forbade, while still
> barring the server and everything else.
>
> **Negative controls** (each reverted): swapping **before** replacing the plan —
> the older PRD wording — fails four transaction tests including
> `a_plan_replacement_failure_leaves_both_untouched`; forwarding a raw cargo message
> (`could not compile /home/... rustc --edition=2024`) fails both safety tests.
>
> **Coverage-first correction (same day).** The order this ledger shipped —
> `generate → load → build plan → replace plan → commit` — was **wrong**, and the
> tests above locked the wrong thing in. Two defects:
> 1. a run that introduced a new member, source root or referenced doc was **not
>    watching those files while it ran**, so an edit between the start of
>    generation and the activation of the plan was dropped;
> 2. a **failed** run never reached activation, so the old plan stayed active and
>    the fix — an edit to the files the failed run introduced — was unwatched. The
>    claim in this ledger that "the previous WatchSet observes the fix" was
>    therefore **false** in exactly the case that matters.
>
> **The corrected order is `build plan → replace plan → generate → load →
> commit`,** under the invariant that **a WatchPlan is liveness coverage, not
> published state: it may lead the served snapshot, but may never lag the inputs a
> regeneration used.** Extra observation costs a redundant rebuild; missing
> observation loses an edit invisibly and permanently. Plan-first is safe because
> `build_watch_plan` already canonicalizes the root and rejects symlink escapes and
> outside-workspace registrations, so an *activated* plan is already a safe one and
> exposes no document data; a malformed config stays repairable because the config
> root and directories are themselves inputs. If the plan cannot be built safely,
> nothing proceeds — publishing a newer snapshot behind an older plan is the lag
> this forbids. **`ArtifactSnapshot` remains the only all-or-nothing commit.**
>
> **Failure semantics now:** plan-build failure → no replacement, no generation, no
> commit (`watch_plan_failed` or a more specific setup code); replacement failure →
> complete old plan stays active, no generation, no commit
> (`watch_plan_replace_failed`); generation failure → **newer plan retained**, old
> snapshot retained (`watch_generation_failed`); load failure → **newer plan
> retained**, old snapshot retained (`watch_artifacts_unreadable`).
>
> **No public API change**: `Stages` and `Transaction` keep their shape; only the
> order inside `regenerate` and the docs changed.
>
> **Tests: +6 (workspace 589 → 595).** The transaction suite was rewritten for the
> new order (15) and gained a `active_plan` probe so "the fix is observable" is an
> assertion rather than a claim: `a_failed_generation_still_leaves_its_new_input_observable`
> and `a_failed_load_still_leaves_its_new_input_observable` assert the old plan does
> **not** cover `new/src/lib.rs`, that the candidate does, and that after the
> failure the active plan still does. A new `tests/watch_engine.rs` (3) drives the
> transaction through the **real engine**: a failed first run expands coverage and a
> new-only edit starts **exactly one** more run with `Started → Failed → Started →
> Succeeded` and max concurrency **1**; and the **gap-during-generation** case —
> plan activated, generation paused on a channel, an event for a candidate-only path
> submitted mid-run, generation released — yields **exactly one** dirty follow-up.
> No sleeps or wall-clock assertions anywhere.
>
> **Negative controls** (each reverted): moving `build_plan` after `generate` fails
> **7** transaction tests; rolling the candidate plan back on generation failure
> fails `a_generation_failure_retains_the_new_plan_and_does_not_load_or_commit`;
> committing before plan activation fails 3 tests. **"Commit before load" could not
> be written at all** — `commit(Self::Snapshot)` can only be fed by `load`, so the
> trait makes that ordering a *type error* rather than a bug a test must catch.

> **Recovery-coverage phase — LANDED 2026-07-17. PRD 09 stays Approved: the
> `--watch` flag, the startup wiring and frontend live reload are still unbuilt.**
>
> **Why the coverage-first order was still not enough.** The order above
> (`build plan → replace plan → generate → …`) assumed the plan could always be
> built. It cannot: `build_watch_plan` runs `cargo metadata`, and metadata
> **fails** in precisely the case that most needs watching — a root manifest that
> declares a member whose `Cargo.toml` is missing or invalid. The complete build
> failed before the new member manifest could ever enter coverage, the old plan
> stayed active, and the user's fix was unwatched. So the earlier ledger's promise
> that "the newer coverage stays and the fix is reachable" only held when the plan
> could be built at all.
>
> **The answer: two-step coverage.** `previous → recovery → complete`. Recovery is
> built from the **root manifest alone**, without cargo, as a **superset** of the
> active plan. Only `ArtifactSnapshot` remains an all-or-nothing publication.
>
> **Part 1 (landed):** `cratevista-watch` gained a first-class
> `InputKind::WorkspaceMemberManifestPattern`,
> `WatchInput::workspace_member_pattern(pattern, excludes)`, `WatchInput.excludes`,
> and `pattern.rs` — a dependency-free matcher (literals, `*`, `?`, `[ab]`,
> `[!ab]`, `**`) that **fails closed**: a malformed pattern matches *nothing*,
> because the opposite failure is a typo that silently makes every vendored
> `Cargo.toml` in the tree a workspace member. Classification is narrow: the path
> must be a `Cargo.toml`, the pattern is matched against its **parent** (so
> `crates/*` covers `crates/new/Cargo.toml` but not `crates/a/nested/Cargo.toml`),
> and excludes are applied last. **`cratevista-watch` still ships only `notify` and
> `tokio`.**
>
> **Part 2 (landed):** core derives each pattern's **static prefix** and registers
> it **recursively** — `crates/*` → `crates`; `tools/*/plugins/*` → `tools`;
> `**/demo` → the root. The registration is deliberately broader than the rule (the
> OS cannot match globs) while classification stays narrow, so a vendored manifest
> still arrives and is still rejected.
>
> **Part 3 (landed):** `crates/cratevista-core/src/watch_recovery.rs` —
> `recovery_inputs` and `member_pattern_inputs`, reading the root `Cargo.toml`
> directly (core gained a direct **`toml`** dependency for this). **Both** builders
> emit pattern inputs: recovery from `recovery_inputs`, and the complete plan from
> `logical_inputs`. That second call is the load-bearing one — metadata knows only
> the members that exist *now*, so without it the complete plan would stop covering
> `crates/*` the moment it succeeded. Unsafe member entries (absolute, UNC,
> drive-qualified, traversing) are **skipped** so the remaining safe members keep
> coverage; no external location is ever registered.
>
> **Part 4 (landed): the fix arrives while the complete plan is still being
> built.** This is the interleaving the whole phase exists for, and it is now
> pinned by an engine-level barrier test rather than argued for in prose:
> `crates/new/Cargo.toml` is uncovered by the previous plan; recovery activates the
> declared `crates/*`; `build_plan` blocks on a channel; the repair *and* an edit to
> an existing file are submitted while that barrier is provably still held; the
> barrier releases; run 1 fails. The result is `Started → Failed → Started →
> Succeeded`, **exactly one** dirty follow-up, and the merged follow-up request
> carries both paths. Concurrency is measured across the **whole** regeneration
> rather than one stage — run 1 stays live for as long as the barrier holds, so a
> second run starting during the pause would show as `max_live == 2`; it is 1.
> **No sleep decides anything**; the only timeout is a watchdog.
>
> Part 4 also pins member containment through symlinks, on both routes an escape
> can take: a member *directory* (`crates/escape`) and a glob's *static prefix*
> (`crates` itself). Both yield `watch_symlink_escape` and neither message carries
> an absolute path. These are `#[cfg(unix)]`, and they now **execute** — the suite
> was run under WSL Ubuntu (25 recovery tests on Linux against 23 on Windows;
> Linux workspace 653 passed, 0 failed), so they are no longer type-checked-only.
>
> **Part 5 (landed): the core pattern matrix, against *both* builders.** Every rule
> below is asserted for recovery *and* for the complete plan, deliberately: the two
> reach the declared patterns by different routes, and a rule holding in only one of
> them would be a coverage gap on exactly the runs that succeeded. `crates/**`
> covers a nested manifest and `crates/*` does not (a single `*` must not silently
> become recursive once metadata succeeds). A Windows-spelled `'crates\*'` /
> `'crates\skipped'` normalizes to the same canonical input as the Unix spelling.
> Duplicate patterns, duplicate excludes, duplicate logical inputs and duplicate
> registrations all collapse; declaration order does not change the plan; repeated
> recovery and repeated complete builds produce identical inputs *and*
> registrations. Excludes apply to an already-existing member and to one that does
> not exist yet alike. An unrelated vendored `Cargo.toml` under the broad `crates`
> registration classifies as **`NotAnInput`** — the precise answer, not merely
> "not relevant".
>
> **A real defect Part 5 exposed.** `CorePlan.inputs` were neither sorted nor
> deduplicated, and the runtime feeds them back in as the next run's `active`. A
> duplicate would therefore be re-added on **every** regeneration and the retained
> list would grow without bound for as long as watch mode ran. `plan_from_inputs`
> now canonicalizes (`sort` + `dedup`), which required deriving `PartialOrd`/`Ord`/
> `Hash` on `WatchInput` — additive, and no new dependency.
> `feeding_recovery_its_own_inputs_back_is_a_fixed_point` pins it.
>
> **Part 6 (landed): active-`CorePlan` ownership at every failure prefix.** The
> transaction tests previously proved call order and probed coverage one path at a
> time; they now compare the active plan's **whole logical input set** against three
> deliberately distinct modelled plans — `previous` (one source root), `recovery`
> (previous + root manifest + the member tree + `crates/*`) and `complete` (metadata
> roots + config input + `crates/*`, minus recovery's now-obsolete concrete guess).
> The fake identifies which plan is active **from the plan itself**, not from a
> counter, and panics if the transaction ever activates a plan no builder produced.
> `the_modelled_plans_have_distinct_logical_inputs` stops all of it passing against
> one plan wearing three names.
>
> | after | active logical inputs | also asserted |
> | --- | --- | --- |
> | recovery build fails | `previous` | 0 replacements; no build/generate/load/commit |
> | recovery replace fails | `previous` | retained ≠ `recovery` |
> | complete build fails | `recovery` | — |
> | complete replace fails | `recovery` | retained ≠ `complete`; both replaces attempted |
> | generation fails | `complete` | snapshot uncommitted |
> | load fails | `complete` | snapshot uncommitted |
> | success | `complete` | committed once, last; pattern retained |
> | partial success | `complete` | committed once; `partial: true` only after commit |
>
> The retained copy moves **only** when a `replace_plan` actually succeeds: a
> core-side record running ahead of the watcher would describe coverage that does
> not exist, which is the same lie as lagging, told the other way. **No
> `Watcher::current_plan` was added** and no `notify` type is exposed — core knows
> what is active because it retained what it successfully activated.
>
> **Evidence:** `crates/*` with no `crates/new` on disk → `crates/new/Cargo.toml`
> classifies relevant under **recovery**, and still under the **complete** plan
> after a successful metadata build that knew only `crates/existing`.
> `crates/a*` accepts `api` and rejects `billing` and `api/nested`.
>
> **Negative controls** (each applied, observed, reverted): dropping pattern inputs
> from the complete plan → the retention test fails; dropping them from recovery →
> 3 fail; bypassing the pattern predicate for a static prefix → `crates/a*` wrongly
> accepts `crates/billing`, 4 classify tests fail; ignoring excludes → 2 fail; a
> malformed class matching anything → 3 fail; removing `crates/*` from recovery in
> the barrier test → the fix is no longer classifiable and the test fails.
>
> The control this phase was missing, now run: **rolling active coverage back from
> recovery to the previous plan.** After a complete-**build** failure it fails 4
> tests, including `a_failed_complete_build_leaves_the_new_member_observable` —
> precisely because the new member manifest is no longer covered, which is the
> regression the whole phase exists to prevent. After a complete-**replacement**
> failure it fails 2. Updating the retained `CorePlan` *before* the replacement
> succeeds fails 3. Dropping input deduplication fails 4 recovery tests and 2
> complete-plan tests.
>
> **Tests: watch 132 lib. Core: recovery 23 (25 on Linux), plan 25, transaction 28,
> engine 4. Workspace 647 → 672 on Windows; 653 on Linux at the time (see the
> Linux hardening ledger below: that figure was recorded from a full-suite run in
> which a since-fixed Linux-only flake happened to pass). `cratevista-watch` still
> ships only `notify` and `tokio`.**

> **Backend wiring phase — LANDED 2026-07-17. `open --watch` works end to end.
> PRD 09 stays Approved: the frontend does not consume `/api/events` yet.**
>
> **The startup order, and the bug it exists to prevent.** The obvious sequence —
> *generate, then start watching* — reproduces the exact defect the recovery phase
> was created to fix, on the very first run: a cold `cargo doc` is the slowest
> thing that will ever happen here, and everything it reads would be unwatched
> while it reads it. An edit landing in that window would be lost silently and
> permanently. So `open --watch` runs:
>
> ```text
> 1. resolve GenerateOptions + canonical workspace root
> 2. build the initial COMPLETE CorePlan          (cargo metadata + config)
> 3. start the real watcher on it
> 4. start the ingress owner in Bootstrap mode
> 5. run the initial generation                   <- events here are buffered
> 6. load + verify the initial snapshot
> 7. AppState::new_watching(snapshot, source policy)
> 8. build the production Transaction over the ALREADY ACTIVE plan
> 9. spawn the single-flight engine
> 10. attach the ingress to the EngineHandle
> 11. the attach flushes the bootstrap window as ONE merged request
> 12. bind, probe, open the browser
> ```
>
> Steps 2–11 live in `watch_runtime::start`, behind three closures (`generate`,
> `load`, `state_for`). That is what makes the order testable without cargo,
> rustdoc, nightly or a browser — none of which the order depends on.
>
> **The bootstrap handoff.** `Ingress::Bootstrap { pending } → Active { handle }`,
> owned by **one task** that holds the watcher's receiver for its whole lifetime.
> The transition is therefore a local move rather than a handoff between tasks,
> which is precisely why an event racing activation can be neither dropped nor
> submitted twice: both arrive at the same `select!`, and only one is handled at a
> time. `IngressHandle::activate` awaits a receipt, so the caller cannot start
> serving while the window is still open.
>
> The `select!` is **`biased`**, and the order is the contract rather than a
> detail: `stop` first (no new filesystem work after shutdown is requested, decided
> here rather than by luck downstream), then events, then activation. Left random —
> the default — an event already queued would *sometimes* join the merged bootstrap
> request and *sometimes* be submitted separately, so the same edits would produce a
> different number of runs from one process to the next. Activation cannot starve:
> the debouncer emits at most one burst per quiet window.
>
> **Safe degradation (D2, honoured).** If the initial plan cannot be built safely,
> or `notify` cannot initialize, or registrations cannot be installed, watch mode
> is **off** and ordinary `open` continues: one browser-safe warning plus terminal
> detail, `AppState::new` (never `new_watching`), `watch_enabled: false`,
> `/api/events` unregistered, no capability advertised. `spawn_watcher` returns its
> typed error *before* spawning anything, so a failed start leaves no partial task.
> The alternative — failing `open` because a per-user inotify limit was hit — would
> trade a working feature for a broken one.
>
> **Watcher failure at runtime: the exact distinction.**
> - `WatchEvent::WatcherFailed` is **recoverable**. The adapter is still running and
>   will still report changes; it is a terminal warning and nothing more. It is
>   **never** converted to `ServerEvent::GenerationFailed` — telling a browser a
>   document failed to build when nothing was generated is simply false.
> - The event stream **ending** is **unrecoverable**: the adapter task itself is
>   gone, so no change can ever be reported again. The ingress returns and the
>   session is joined at shutdown. The server keeps serving the last good snapshot
>   rather than tearing the document away; what it stops doing is pretending to
>   watch. *Known limitation:* an already-connected SSE stream stays open and idle,
>   and `watch_enabled` — decided at startup — still reads true. Correcting a live
>   `AppState` is frontend-visible behaviour and belongs with the frontend phase.
>
> **Production `Stages` and the blocking boundary.** `Transaction` already wraps
> `build_recovery_plan`, `build_plan`, `generate` and `load` in `spawn_blocking`, so
> `ProductionStages` implements them as plain blocking calls and adds no pool of its
> own; `replace_plan` is genuinely async and awaits `Watcher::replace_plan`. The
> initial generation uses `block_in_place` (it is inside `block_on`, and its
> `CommandFailure`/exit code must survive). Nothing duplicates validation, artifact
> integrity or sanitization: `run_generate` and `load_snapshot` are called unchanged.
>
> **Why a pending-plan slot.** `Stages::replace_plan` receives a `WatchPlan`, which
> carries no logical inputs — and core must retain the `CorePlan` so the next run's
> recovery is a superset of what is live. Asking the watcher would mean a
> `Watcher::current_plan`, publishing watcher internals for bookkeeping core can do
> itself. So each builder parks its `CorePlan` in a slot and a **successful**
> replacement promotes it. Unambiguous because the transaction is sequential and the
> engine is single-flight.
>
> **Shutdown ownership.** `WatchSession` owns the `Watcher`, the ingress task, the
> `Engine` and the forwarder, and joins all four:
>
> ```text
> 1. ingress stops accepting filesystem work
> 2. watcher shutdown requested
> 3. engine shutdown requested
> 4. any in-flight generation still emits its terminal event
> 5. join engine        (which is what step 4 waits for)
> 6. drain + join the forwarder
> 7. join ingress, then the native watcher
> 8. the caller stops and joins the server
> ```
>
> The server is stopped **after** the session, via
> `CoreServer::wait_for_shutdown_with`: on Ctrl-C the session is torn down first, so
> an in-flight regeneration's terminal event still reaches live SSE subscribers
> instead of racing a closing socket. The watcher is held as `Arc<Watcher>` because
> `ProductionStages` needs it; joining the engine (step 5) drops the only other
> reference, which is what makes `Arc::into_inner` succeed at step 7. No
> `process::exit`, and no detached task.
>
> **Evidence.** 28 `watch_runtime` tests + 5 CLI tests. Bootstrap: 0 events → no
> submission; 1 → one follow-up; many → one sorted, deduplicated merged request; an
> event racing activation → observed exactly once, over **50 iterations**. Ownership
> through the real `ProductionStages` with fake work: initial complete plan retained;
> a refused recovery replacement leaves `previous`; a refused complete replacement
> leaves `recovery` (never the older plan); generation/load failure leaves `complete`
> and the snapshot untouched; success commits exactly what `load` returned. Ordering:
> the snapshot swap is observed **before** `GenerationSucceeded`, so a browser that
> reloads on the event never fetches the old document. Shutdown: idle; while a
> generation is paused (it finishes and announces); an edit submitted after shutdown
> starts no second run. Real watcher over a tempdir: an artifact write during
> bootstrap produces **no** run while a real source edit does — proven with a
> positive control rather than a sleep.
>
> **Two vacuous tests caught while writing them.** The snapshot fixture was
> canonical JSON with a fixed `generated_at`, so the "regenerated" snapshot was
> **byte-identical** to the initial one and every swap assertion passed whether or
> not `commit` did anything. `assert_ne!` on the marker token exposed it; the fixture
> now stamps a different time. Separately, `IngressHandle::activate` became `async`
> and one test dropped the future instead of awaiting it — the activation never
> happened and the test still passed until the race assertion failed.
>
> **Known Linux flake — SUPERSEDED by the Linux recursive-watch hardening phase
> below.** This phase recorded
> `creating_a_nested_rust_file_under_a_recursive_root_emits_one_request` as failing
> ~4/5 in isolation on Linux, verified it was not caused by the wiring (3/3 at the
> committed baseline with the phase's changes stashed), and left it as out of
> scope. Calling it a "flake" was the mistake: it was a **real defect** in the
> adapter, and a test failing 4/5 was reporting it accurately. It is fixed below.
>
> **Tests: Windows workspace 672 → 705, 0 failed. Linux 686, 0 failed on a
> full-suite run — a figure that only looked clean because the defect above is
> timing-dependent. `cratevista-server` still depends on neither core, watch nor
> notify; `cratevista-watch` still ships only `notify` and `tokio`. No schema
> change, no `SchemaVersion` bump, no `web/dist` change.**

> **Linux recursive-watch hardening — LANDED 2026-07-17. PRD 09 stays Approved:
> the remaining work is frontend-only.**
>
> **The defect, and why the previous phase was wrong about it.** The wiring phase
> filed
> `creating_a_nested_rust_file_under_a_recursive_root_emits_one_request` as a
> "pre-existing Linux flake, out of scope". It was not a flake. A recursive watch
> is not recursive at the OS level on Linux — inotify watches one directory each,
> installed as directories are observed to appear — and the adapter only ever
> classified what the backend chose to report. Two windows followed, both losing a
> real source edit permanently: a tree that arrives **complete** is reported as one
> event for its top directory and its contents are never mentioned; and a file
> created immediately after a `mkdir` can beat the watch being installed. A test
> failing 4/5 was reporting a real bug accurately.
>
> **The fix: subtree reconciliation.** When an event names an existing directory
> beneath a `RustSourceRoot` (`WatchSet::needs_subtree_reconciliation`), the
> adapter registers that subtree recursively and then scans it, and feeds
> everything found back through the **existing** classifier, debouncer and staging.
> Nothing about relevance changed: the Rust root decides only whether to look.
>
> **Honest accounting of register-first.** The **scan** is what fixes the observed
> defect, and the evidence says so: removing the scan fails the rename tests
> immediately, while removing the *registration* leaves the Linux suite green
> 10/10, because `notify` also installs a watch of its own when it sees a directory
> appear. The registration is kept as belt-and-braces for the window where that
> installation races our scan — a file created after the scan ends and before
> `notify`'s watch lands is in neither half — but **no test here proves it
> load-bearing**, because that interleaving is not forceable from outside `notify`.
> It is prudence, not proof, and the code says so too.
>
> **Blocking boundary.** The walk never runs on the event loop: `spawn_reconcile`
> offloads it to `spawn_blocking` and the findings return through a channel to be
> classified by the same loop against the same `WatchSet`. The traversal is
> iterative (deep trees cannot overflow the stack), never follows a symlink, applies
> the ignore rules **before** descending, and tolerates a path vanishing mid-walk.
> The reconcilers live in a `JoinSet` that shutdown drains, so no blocking task
> outlives the adapter and nothing can be submitted after shutdown.
>
> **Generations.** A reconciliation is tagged with the generation that requested it.
> The current generation's findings go to the debouncer; a **candidate's** go to its
> staging buffer (its set is not the truth yet, so classifying against the old set
> would ask the wrong question); a **retired** generation's are dropped exactly as
> its native events are.
>
> **A bug this phase's own tests caught.** The first implementation passed
> `WatchSet::root()` — a normalized, forward-slash **lexical** string — into
> `PathBuf` and used `strip_prefix` against real paths from `read_dir`. On Windows
> those are verbatim (`\\?\C:\...`) while the root reads `//?/C:/...`, so the
> prefix never matched, every subdirectory looked outside the workspace, and the
> walk silently found nothing. Relativization now goes through the classifier's own
> `normalize`/`relative_to` (`WatchSet::{is_ignored_directory, contains_path}`),
> which is the only place that knows how those two representations relate.
>
> **Deterministic evidence.** The headline test does not race: a tree is built
> **outside** the watched root and renamed in atomically, so "the files inside are
> never reported" is guaranteed rather than probable. It asserts the nested
> `module.rs` and a `deeper/leaf.rs` are found, and that a `README.md` beside them
> is not.
>
> **Negative controls** (each applied, observed, reverted): disabling
> reconciliation fails the two rename tests and the editor-temporary test; treating
> reconciled paths as relevant instead of classifying them (the "use the Rust-root
> prefix as the relevance rule" mistake) fails the `README.md` and
> editor-temporary assertions; removing the registration leaves Linux green, which
> is what the honest accounting above is based on. The `target/` and hidden-tree
> tests are **guards, not discriminating controls**: they pass with reconciliation
> disabled, and what actually protects them is classification plus the ignore check
> in `needs_subtree_reconciliation`.
>
> **Repeated isolated Linux runs** (WSL Ubuntu, the platform the defect is on):
> `creating_a_nested_rust_file_under_a_recursive_root_emits_one_request` **20/15+**
> — 20/20 and then 15/15, against ~4/5 *failing* before; the deterministic rename
> test 15/15; the coalescing test 15/15; the candidate-replacement test 15/15; the
> register/scan interleaving test 15/15; retired-generation 15/15; shutdown-during-
> reconciliation 15/15; the whole `real_watcher` suite 10/10.
>
> **Tests: Windows workspace 705 → 714, 0 failed. Linux 686 → 690, 0 failed.
> `cratevista-watch` still ships only `notify` and `tokio` — the traversal is
> `std::fs` and the matcher is our own. No new production dependency, no public
> `notify` type, no schema change, no `web/` change.**

> **Frontend live-reload phase — LANDED 2026-07-17. This completes PRD 09, which
> moves to Implemented / Verified.**
>
> **What the browser now does.** On mount, `LiveReload.start()` reads
> `/api/health.watch_enabled`. Only an explicit `true` opens an `EventSource` to
> `/api/events`; a 404, a non-2xx, unparseable JSON or a non-boolean field all mean
> **watch disabled**, so `serve`, a degraded `open --watch`, a static export and any
> older server never open a stream. The probe fails **closed** because the cost of
> guessing wrong is an `EventSource` reconnecting for ever against a route that does
> not exist.
>
> **The health capability probe** is the single gate. It never throws into the app:
> every failure path returns `false`. There is exactly **one** `EventSource` per
> mounted application, and `dispose()` (run on unmount) closes it and silences every
> callback, so a message already queued when React tears the tree down cannot update
> a gone component.
>
> **EventSource connection / reconnect.** Every successful connection — the first
> and every reconnect — triggers **one** refetch. This is the whole convergence
> mechanism: SSE events are not durable (the server sends no `id:`, so replay is
> impossible by construction), so an event published while the browser was
> disconnected is simply gone, and reloading on every `open` is what recovers it.
> An ordinary `error` does **not** close the stream or raise a banner per attempt:
> the browser reconnects on its own using the server's `retry: 1000`, and the
> reconnect's `open` reloads and converges. `generation-started` shows a
> non-blocking indicator and fetches nothing; `generation-succeeded` (including
> `partial: true`) refetches; `generation-failed` stops the indicator, keeps the
> graph, surfaces the server's safe `code`/`message`, and **fetches nothing** —
> a failed generation wrote nothing, so the artifacts on disk are the ones already
> shown.
>
> **The coherent triple rule.** A watch-mode server can swap the snapshot between
> the three concurrent artifact requests of one load, which would render a document
> from one generation beside diagnostics from another — undetectable downstream. So
> every response carries `X-CrateVista-Snapshot`, and an attempt is accepted only if
> the arrived responses agree. Up to **three attempts total** (not one plus three
> retries), all under one token and one `AbortController`:
>
> | arrived responses | verdict |
> | --- | --- |
> | all carry the same header | coherent → accept |
> | all carry a header, not all equal | incoherent → retry |
> | some carry a header, some do not | incoherent → retry |
> | **none carries a header** | **coherent → accept (static-export rule)** |
>
> The **all-absent** case is a PRD-10 compatibility requirement, not a shortcut:
> immutable static files have no server to stamp a header and cannot disagree, so
> their silence is not ambiguity. A live server always stamps all three, so this can
> never mask a real collision. A **degraded** artifact (a 404 that the existing
> rules already tolerate) never arrived and has no header opinion, so it does not
> make a triple incoherent. Coherence is decided **before** anything is parsed, so a
> mixed triple is never partially exposed. Three incoherent attempts return the typed
> `incoherent-snapshot` outcome — distinct from `document-error`, because nothing is
> broken and the next load will almost certainly succeed.
>
> **Preservation of the last rendered snapshot.** Initial load and reload have
> different failure semantics. With nothing rendered, a failed load stays fatal
> (the blocking `ErrorState`). Once a document is on screen, a failed reload —
> `document-error`, `incoherent-snapshot` or `incompatible` — **keeps it**: the
> graph, its ELK layout, the viewport and the selection are all still valid, so the
> UI shows a non-blocking banner and leaves them untouched. The Zustand store is
> **reused** across reloads (recreating it would reset the user's place on every
> rebuild) and re-pointed only when the new document no longer contains the active
> view. The next successful reload clears every transient watch banner. A stale
> reload (a newer load superseded it) commits nothing **and reports nothing**, so a
> slow failed reload can never replace a newer success with an error banner.
>
> **Accessibility.** The regenerating indicator is `role="status"` / `aria-live`
> polite — progress must not steal a screen reader's place in the graph the user is
> reading; failures use `role="alert"`, matching the existing `ErrorState`. No
> banner ever renders an absolute path or a changed path; the failure text is the
> server's already-safe `code`/`message`, transported unchanged.
>
> **Reload race / cancellation.** Live reload reuses the one `ArtifactLoader`, so
> every trigger — a success event, an `open`, a reconnect — inherits its monotonic
> token and single `AbortController`. A newer invocation aborts the whole of an
> older one; a coherence retry never cancels itself; unmount aborts any in-flight
> reload; and an aborted stale load returns neither data nor a banner. There is **no
> second artifact-fetching implementation** in the live-reload path.
>
> **Unit evidence (Vitest, 244 total; +59 this phase, all green in isolation).**
> `tests/coherence.test.ts` (17): the header matrix above, three-attempts-total,
> per-attempt concurrency (exactly three requests, all in flight), coherence-before-
> parse, and cancellation (a newer load aborts every attempt; a retry does not
> cancel itself; a stale load returns no data and no error). `tests/live-reload.test.ts`
> (28): the probe's fail-closed defaults, one-EventSource, open/reconnect/success
> refetch, failure-does-not-refetch, disposal silencing later callbacks, and the
> error-does-not-reconnect-loop rule. `tests/watch-ui.test.tsx` (14): through the
> real `<App>` — graph preserved on started/failed, banner shows the safe pair, the
> exhausted-coherence and reload-error banners keep the graph, a stale reload never
> banners over a success, and one EventSource per mount with unmount cleanup.
>
> **Browser evidence (Playwright, real bundle + real CSP).** A controllable
> watch-server double (`web/e2e/support/watch-server.ts`) serves the **real built
> `web/dist`** with the **exact production CSP** copied from
> `crates/cratevista-server/src/router.rs`, and speaks the real SSE vocabulary
> (`retry: 1000`, no `id:`) and the `X-CrateVista-Snapshot` header. It is a test
> harness, not a production path: the real binary cannot swap a snapshot or publish
> an event on command, and a real `open --watch` regeneration is out of scope for a
> browser test. The six scenarios (`web/e2e/tests/live-reload.spec.ts`) pass:
> (1) a published success swaps the document in place with **no page navigation**;
> (2) started keeps the graph, failed keeps it and shows a banner; (3) a coherence
> collision resolves to the coherent triple; (4) an exhausted retry keeps the old
> graph and shows a reload banner with **no empty state**; (5) `watch_enabled=false`
> opens **zero** `/api/events` connections; (6) a reconnect across a snapshot swap
> converges with no replay (≥ 2 connections observed). The always-on instrumentation
> fails any test on a CSP violation, page error or failed same-origin request, so
> **zero CSP violations / zero page errors** holds across the whole suite. All **80**
> Playwright tests pass (73 pre-existing against the real `serve` binary + 7 new).
>
> **Static-export evidence** (`web/e2e/tests/static-export.spec.ts`): the same real
> bundle, served with **no** `/api/health`, **no** `/api/events` and **header-less**
> artifacts, renders normally — the all-absent-headers loader rule and the
> fail-closed probe are what carry it — opens no `EventSource`, and creates no
> reconnect loop. PRD 10's static builder is not implemented yet, so there is no
> committed static explorer to render; this proves the two loader/probe rules the
> future export depends on, and adds **no** static-only code path beyond them.
>
> **Distribution.** `web/dist` was rebuilt with `npm run build` and committed;
> `npm run check:dist` confirms the committed bundle matches a fresh production
> build byte-for-byte, and `npm run check:embed-rebuild` confirms the embedded
> server assets track the rebuilt source (served bytes change with the dist and
> revert on restore). `npm run lint` (0 errors), `npm run typecheck` clean.
>
> **No backend change.** This phase touched only `web/` and docs: no Rust source
> changed, `cratevista-server` still depends on neither core, watch nor notify,
> `cratevista-watch` still ships only `notify` + `tokio`, **no schema change and no
> `SchemaVersion` bump** (a transport header and a health boolean are not schema),
> and no new frontend runtime dependency (`EventSource` and `fetch` are platform
> APIs). Rust gates unchanged: Windows 714, Linux 690, both 0 failed.
>
> **Known pre-existing test-infra flake (not this phase).** The complete gate run
> reported for this phase **passed** (Vitest 244/244, Playwright 80/80, the Rust
> workspace, and every dist check). Honesty requires the caveat that the Vitest
> suite is **not** reliably deterministic under full-suite parallel load: it
> intermittently fails one of the existing xyflow-mock component tests
> (`controls.test.tsx` / `app.test.tsx`, a different one each time) at roughly
> 1-in-6, while each file passes 100% in isolation. This reproduces on the committed
> baseline with this phase's changes stashed (5/6 there too), so it is a jsdom
> timing fragility in the **existing** component tests, not a live-reload defect and
> not a PRD-09 acceptance gap. PRD-09's own three test files (59 tests) are stable —
> 59/59 every run. The harness now injects a network-free `liveReload` by default
> (`watchDisabledLiveReload`) so no component test makes a real `fetch`, but that
> did not change the flake rate, confirming the cause is elsewhere. The flaky
> component test is tracked as a separate pre-existing repository issue.
>
> **`spawn_blocking` is not used here**: this crate has no blocking work, and
> calling it merely to imitate core would put a fake seam in real code. The later
> core adapter wraps the synchronous `run_generate`/load/swap in
> `spawn_blocking` and completes the injected operation.
>
> **Shutdown is honest about cargo**: it stops accepting requests (the handle
> returns `EngineClosed` immediately, via a shared flag, so a submit racing the
> task's exit loses deterministically), lets the in-flight run finish and emit its
> terminal event, discards the dirty follow-up, and closes the sink by dropping it
> on task exit. **No claim is made that a blocking cargo child can be
> force-cancelled.** Dropping every handle takes the same graceful path.
>
> **Tests: +25 (watch 59 → 84; workspace 464 → 489).** 3 request-shape
> (sort/dedup, empty impossible, merge). 19 engine: one request → one run with
> `Started → Succeeded`; failure → `Started → Failed` with values verbatim;
> `partial` reaching the event; **ten requests during a run → exactly one
> follow-up** (and `try_recv` proves no second run started); merged paths sorted
> and deduplicated; requests during the follow-up → exactly one third run; **a
> failed run still starts its dirty follow-up** (it may be the fix); **max observed
> concurrency = 1**; the `Started → terminal → Started → terminal` sequence;
> strict alternation ending on a terminal; **no path in any event**; idle shutdown;
> running shutdown; dirty discarded on shutdown; typed error after shutdown; a
> cloned handle sharing closed state; handle-drop while idle and while running;
> determinism over 10 repeats. 3 boundary: tokio-only, no notify, shipped features
> exactly `rt`/`sync`/`macros`. **No sleeps, no clock, no filesystem, no cargo, no
> nightly** — every completion is driven by a `oneshot` the test holds.
>
> **A defect in my own tests, found and fixed.** The first controls showed a broken
> engine made tests **hang forever** rather than fail — on CI that is an unhelpful
> job timeout. The tests now wrap each await in a **dev-only** `tokio::time::timeout`
> watchdog (5 s), so a scheduling bug fails in seconds with a named message. It is
> **not** a sleep or a timing tolerance: it never fires in a correct run (every
> wait is unblocked by a channel in microseconds). `time` is therefore a
> **dev-dependency only** and never ships; the boundary test now separates runtime
> from dev sections (mirroring `cratevista-config`'s) so the shipped-features
> assertion stays honest.
>
> **Negative controls** (each reverted): removing `biased;` → 4 tests fail
> (2 by watchdog timeout — the lost-request race); making failure discard the dirty
> set → `a_failed_run_still_starts_its_dirty_follow_up` fails by watchdog; emitting
> a duplicate `Started` → 6 tests fail including the alternation check.
3. ~~Amendments: PRD-06 A1/A2 (`SnapshotMarker::token`, header, `watch_enabled`) and
   PRD-08 `referenced_files` — each green on its own.~~ **LANDED 2026-07-17**,
   ahead of steps 1–2 (they are independent of the engine and unblock it), plus
   D5. Workspace tests 376 → **405**.
4. ~~Server `events.rs` + broadcast in `AppState` + route.~~ **LANDED 2026-07-17**
   (server-events phase; see its ledger).
5. ~~Core `watch.rs`: WatchSet construction + atomic rebuild + wiring; CLI
   `open --watch`.~~ **LANDED 2026-07-17** — the core watch foundation, the
   coverage-before-generation transaction (superseding "atomic rebuild"), the
   backend wiring (`WatchSession`, `open --watch`) and the Linux recursive-watch
   hardening; see their ledgers.
6. ~~Frontend `liveReload.ts` + loader header/retry + hook; rebuild/commit
   `web/dist`.~~ **LANDED 2026-07-17** (frontend live-reload phase; `web/dist`
   rebuilt and committed; see its ledger).
7. ~~Real-watcher integration + browser tests; ADR-0008 → Accepted; docs.~~
   **LANDED 2026-07-17** — real-watcher and Playwright browser verification,
   ADR-0008 **Accepted**, and README/`docs/server.md` updated; see the ledgers.

## Acceptance criteria

- [x] Editing a relevant `.rs` file triggers one debounced regeneration. *(debounce unit + real-watcher integration + gated E2E)*
- [x] Editing ignored output does not create a regeneration loop. *(no-loop test, asserted against a positive control)*
- [x] Browser data refreshes after successful generation. *(browser test: refetch, no navigation)*
- [x] A failed generation leaves the previous valid graph visible and shows diagnostics. *(engine failure test + browser test)*
- [x] Concurrent regeneration is prevented. *(max-concurrency-1 counter + dirty-flag tests)*
- [x] Watch mode shuts down cleanly. *(shutdown test joins tasks, closes SSE)*
- [x] Tests use deterministic synthetic filesystem events where possible. *(injected-clock unit suites)*
- [x] The last valid snapshot is never replaced by an invalid or partial one. *(load_snapshot failure → no swap)*
- [x] `--watch` exists on `open` only; `serve` accepts neither `--watch` nor generation flags. *(CLI test over `--help` for both)*
- [x] `open --watch` startup with a failing initial generation is defined and tested. *(startup test: existing exit code, no bind)*
- [x] `/api/events` emits only the three bounded event types and never an `id:`. *(route test)*
- [x] Recovery and complete coverage are built and activated atomically **before** generation; complete coverage retains declared member patterns, and a failed run retains the newest successfully activated coverage so its fix stays observable. A newly added source file, flow TOML or referenced doc is watched without restarting. *(recovery/complete transaction tests + the fix-during-complete-plan barrier test + recovery-retention tests + active-`CorePlan` ownership assertions at every failure prefix + the source/flow/referenced-file coverage tests)*
- [x] A referenced file that does not exist yet is watched through its nearest existing parent, and creating it triggers exactly one regeneration. *(missing-file watch test)*
- [x] Classification is re-applied to every event, so directory watches never admit ignored paths. *(per-event test: an editor backup beside a watched `.rs` triggers nothing)*
- [x] `generation-failed` payloads never contain an absolute path or a raw command line. *(payload sanitization test)*
- [x] The frontend creates `EventSource` only when `/api/health.watch_enabled` is true. *(hook test: `false` and absent-health both construct none)*
- [x] A mixed artifact triple is discarded and retried, bounded at three attempts. *(loader test with differing `X-CrateVista-Snapshot` headers)*
- [x] A failed loader retry retains the rendered snapshot and reports a non-blocking reload error. *(loader + browser test: graph still on screen, banner shown, no empty state)*
- [x] `ConfigOutcome.referenced_files` is sorted, deduplicated, includes missing/oversized/non-UTF-8 references and excludes traversing spellings. *(config fixture test)*
- [x] `cratevista-watch` depends on neither `cratevista-schema` nor core/graph/config/server. *(dependency-boundary test)*

Every in-scope acceptance criterion above is met.

### Deferred from this PRD

By locked decision **B4**, persistent caching is **not** part of PRD 09 and its two
former criteria are moved to `ISSUES/issue_12_persistent_cache.md`. They are **not**
PRD-09 completion gaps, and the cache functionality is **not** implemented:

- Cache keys include all inputs that affect output. → **issue 12.**
- `--no-cache` behavior is defined and tested. → **issue 12.** Watch mode ships
  without a cache, so PRD 09 adds no `--no-cache`.

Verification:

```bash
cargo test -p cratevista-watch --all-features
cargo test -p cratevista-server --all-features
cargo test -p cratevista-config --all-features
cargo test -p cratevista-core --all-features
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-features
cargo +1.97.0 check --workspace --all-features
cd web && npm run test && npm run check:dist && npm run check:embed-rebuild && npm run e2e
```

## Locked decisions

All four blockers were decided by the maintainer on **2026-07-16**. Nothing here
is open.

**B1 — CLI surface: `open --watch` only.** `serve` remains artifact-only and
accepts **neither `--watch` nor `GenerateArgs`**; its "no regeneration" contract
is unchanged, so PRDs 05 and 08 need no edit. Deviates from the issue's
`serve --watch` deliberately — see "### CLI surface — LOCKED (B1)".

**B2 — `X-CrateVista-Snapshot` on all three artifact routes.** Recorded as an
additive **PRD-06 amendment (A1)**; PRD 06 stays Implemented / Verified. The
frontend discards a mixed triple and retries, **bounded at three attempts**; a
failed retry retains the rendered snapshot and reports a non-blocking reload
error.

**B3 — `ConfigOutcome.referenced_files: Vec<RepoRelativePath>`.** Recorded as an
additive, read-only **PRD-08 amendment**; PRD 08 stays Implemented / Verified.
Sorted and deduplicated; covers flow docs, flow examples and override docs;
**includes** valid references whose file is missing, oversized or non-UTF-8;
**excludes** invalid or traversing spellings. Reuses the existing
`cratevista_schema::RepoRelativePath` — no new path type.

**B4 — Persistent caching deferred** to `ISSUES/issue_12_persistent_cache.md`
(created by this PRD). **No `--no-cache`.** The existing `cache_key` format is
kept and its issue reference re-pointed. See "## Caching: deferred — LOCKED (B4)".

## Confirmed parameters (D1–D5)

Confirmed **exactly as proposed** on 2026-07-16.

- **D1 — Debounce: 300 ms quiet window, 2 s maximum delay.** Constants, injectable
  for tests.
- **D2 — No `notify` poll fallback.** If a platform cannot deliver events (some
  WSL or network mounts), watch reports it via a warning + `WatcherError` and
  **serving continues** — the server never exits because watching degraded. A
  polling mode may be proposed later; it is not in this PRD.
- **D3 — Ignore patterns are not configurable.** The ignore set is fixed
  (`target/**`, `.git/**`, `web/node_modules/**`, `web/dist/**`, dotfile dirs).
  Making it configurable needs a `[watch]` section, which `RawRootConfig`'s
  `deny_unknown_fields` currently rejects. Documented as a limitation in
  `docs/configuration.md`.
- **D4 — SSE: keepalive comment every 15 s, `broadcast` capacity 16,
  `retry: 1000`.**
- **D5 — `cache_key`'s doc comment is re-pointed** from issue 09 to issue 12.
  Comment-only; no behavior, no signature, no key-format change.

## Corrections to the previous draft

Every item below was asserted by the pre-PRD-06/07/08 draft and is **false**
against the real code:

1. "`AppState` with `ArcSwap` for **both** the document and the
   `GenerationReport`" → there is **one** `ArcSwap<ArtifactSnapshot>` covering
   document + generation + diagnostics + bytes + marker, and `replace_snapshot`
   already exists.
2. "pipeline pure functions (`metadata::build`, `rustdoc::normalize`)" → the real
   entry points are **`cratevista_metadata::ingest`** and
   **`cratevista_rustdoc::ingest`**; `metadata::build` and `rustdoc::normalize`
   do not exist.
3. The engine closure typed as returning `(Document, GenerationReport, Vec<Diagnostic>)`
   → `run_generate` returns **`Result<ExitCode, CommandFailure>`** and writes
   artifacts; the document is obtained via **`load_snapshot`**, which also
   verifies it.
4. "`crates/cratevista-server/src/routes/events.rs`" → the server has **no
   `routes/` directory**; its modules are flat.
5. "Config `[watch] ignore`, `[cache] enabled` reserved (issue 08 binds)" →
   PRD 08 reserves only `version`/`[metadata]`/`[rustdoc]`/`[server]`, and
   `RawRootConfig` is **`deny_unknown_fields`**, so `[watch]` is a
   `config_invalid_structure` **error** today (D3).
6. "Config files list from issue 08" → `discover()` yields the **TOML files
   only**; referenced docs/examples were not exposed (fixed by B3).
7. "publish failure diagnostics to `/api/diagnostics`" → that endpoint serves the
   current *valid* snapshot's diagnostics; doing so would break snapshot
   coherence. Failures travel on the event stream only.
8. "Cache stages … keys … `--no-cache`" → no cache exists;
   `GenerationReport.input_hashes` is an **empty map** and **nothing computes
   `input_digest`** (B4).
9. Non-goal "issue 07 adds a small live-reload client hook only" → PRD 07 is
   **Implemented / Verified**; the hook is added **by this PRD**, and `web/dist`
   must be rebuilt and committed here.
10. `serve --watch` as the entry point → **`open --watch` only** (B1).
11. Startup with no valid initial snapshot was undefined → now specified.
12. The verification block invented `--features live` / `RUSTDOC_LIVE=1`, which
    **exist in no `Cargo.toml`**; replaced with the real gate commands.
13. "SSE works through the embedded server and static-only?" (an unanswered
    question) → resolved: `connect-src 'self'` already allows it; the static build
    has no server, and the hook never starts because `watch_enabled` is absent.

## Traceability

Issue-09 criteria → the acceptance list above, minus the two cache criteria moved
to `ISSUES/issue_12_persistent_cache.md`. Builds on PRD 06
(`AppState::replace_snapshot`, `load_snapshot`, shutdown, CSP, + amendments
A1/A2), PRD 07 (`web/src/api/load.ts`, `web/dist` workflow), PRD 08 (`discover`,
`--no-config`, + the `referenced_files` amendment), and PRD 05's pipeline via
`run_generate`. Adds `/api/events`, consumed by the new frontend hook.
