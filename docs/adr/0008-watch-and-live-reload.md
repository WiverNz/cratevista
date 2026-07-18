# ADR-0008: Watch mode, live reload, and deferring the persistent cache

- **Status**: Accepted — implemented and verified as PRD 09 (2026-07-17). The
  backend (`cratevista-watch`, `open --watch`, `/api/events`) and the frontend
  live-reload client (`web/src/api/liveReload.ts`, coherent loader) have all landed.
- **Date**: 2026-07-16
- **Issue**: 09 — watch mode, caching, live reload
- **Supersedes**: none
- **Related**: [ADR-0006 (server and security)](0006-server-and-security.md),
  [ADR-0007 (config format)](0007-config-format-toml.md),
  `ISSUES/issue_12_persistent_cache.md`

## Context

`cargo cratevista open` generates once and serves. Editing code then means
killing the process and re-running it. Issue 09 asks for three things — watch,
live reload, and a persistent cache — that look like one feature but have very
different correctness budgets.

The pieces this builds on already exist and are Implemented / Verified:
`AppState` holds one `ArcSwap<ArtifactSnapshot>` with a `replace_snapshot` seam
documented as PRD 09's; `load_snapshot` already does torn-read detection, BLAKE3
verification and schema gating; the CSP already allows same-origin SSE
(`connect-src 'self'`); and the frontend already has a concurrent three-artifact
loader with a stale-response token.

## Decision

**Ship watch mode and live reload. Defer the persistent cache to issue 12.**

Watching is a correctness-preserving convenience: worst case, it regenerates too
often. A cache is a correctness *hazard*: worst case, it shows a wrong
architecture map and nobody notices. Bundling them would have let the second
ride in on the first's approval.

### Ownership

Three crates, one direction, no cycle:

- **`cratevista-watch`** — filesystem watching, path classification, debounce, and
  a single-flight engine driven by an **injected regeneration closure**. It never
  calls `run_generate`, never links core, and (per the approved decision) **does
  not depend on `cratevista-schema`** — its data model is `PathBuf` and plain
  strings, and a dependency with no type behind it is just coupling.
- **`cratevista-server`** — gains only `GET /api/events` and a broadcast channel.
  **No `notify`, no debounce, no watch dependency**: the server does not watch; it
  publishes events it is handed and swaps a snapshot it is handed. ADR-0006's
  "reload-free artifact server" stands.
- **`cratevista-core`** — the orchestrator, as everywhere else: builds the watched
  input set, owns the engine, and wires `run_generate` → `load_snapshot` →
  `replace_snapshot` → event sink.

### `open --watch` only

`--watch` goes on `open`, which already generates and serves. **`serve` stays
artifact-only** and accepts neither `--watch` nor generation flags.

The issue names `serve --watch`, so this deviates deliberately. `serve` is
published as "no regeneration"; giving it a flag that regenerates would either
contradict that contract or require generation flags that are inert without
`--watch` — a trap where `serve --features x --watch`-less silently does nothing.
`open --watch` needs no new argument group and no new contract. One command, one
meaning.

### Never swap something invalid

The publish order is **generate → load + verify → swap → emit event**, and every
failure path simply never reaches the swap. There is no rollback because nothing
was swapped. Three existing mechanisms stack here and **none is reimplemented**:
prepare-then-commit writes, `load_snapshot`'s marker + BLAKE3 + schema checks, and
swapping only on `Ok`.

A failed regeneration keeps serving the last valid snapshot and never terminates
watch mode. Failure detail travels **only** on the event stream: `/api/diagnostics`
serves the *current snapshot's* diagnostics, and mixing a failed run's output into
a successful run's artifact set would break the coherence the snapshot exists to
guarantee.

### Events are state notifications, not a log

`GET /api/events` emits exactly three types — `generation-started`,
`generation-succeeded`, `generation-failed` — and **never an SSE `id:`**. With no
`id:`, a browser has nothing to put in `Last-Event-ID` and **replay is impossible
by construction** rather than by policy. That is the point: an event means "a
newer snapshot exists", and the truth is always whatever `/api/document` returns
*now*. Replaying "succeeded" from three snapshots ago would be worse than
replaying nothing.

Every (re)connect triggers one unconditional refetch, which makes missed events a
non-issue without any replay machinery.

### Two small additive amendments

Both are recorded in their owning PRDs, which stay Implemented / Verified:

- **PRD 06 / A1** — `X-CrateVista-Snapshot` on the three artifact routes. Three
  artifacts are three requests; today nothing swaps mid-session, so a mixed
  triple is unobservable, but **a live swap makes it reachable**. The client
  discards a mixed triple and retries, bounded at three attempts.
- **PRD 06 / A2** — `/api/health.watch_enabled`. The frontend opens an
  `EventSource` **only** when it is true, because `EventSource` reconnects
  forever against a 404 — without the probe, every `serve` session and every
  static export would carry a permanent background error loop.
- **PRD 08** — `ConfigOutcome.referenced_files: Vec<RepoRelativePath>`. Watch
  must react to manual docs the config references; those paths are resolved
  inside `embed_files` and discarded. Re-deriving them in the watcher would mean
  a second TOML parser and a second answer to "which files are inputs".

### Why the cache is deferred

Beyond the correctness asymmetry: the key's main input **does not exist**.
`cache_key` takes a caller-supplied `input_digest`, nothing computes one, there is
no per-target source enumeration, and `GenerationReport.input_hashes` is an empty
map. Building a digest means a new file-walk with its own ignore policy — a second
place to answer "which files are inputs", able to disagree with the watcher.

The existing key is **kept and untouched**; issue 12 reuses it and supplies the
digest. No second key format is introduced. **`--no-cache` is not added**, because
a flag that disables a nonexistent cache is a lie in `--help` and would have to be
supported forever.

## Consequences

- Watch mode costs one full generation per burst. Acceptable: debounce (300 ms
  quiet, 2 s max) and single-flight bound the work, and cargo caches builds.
- `cratevista-watch` is testable without cargo, nightly, or a real filesystem —
  the engine takes a closure and the debouncer takes a clock. The real watcher is
  covered separately with count-based assertions, never sleep-then-assert.
- The server gains one route and one channel; ADR-0006's security posture is
  unchanged (same bind, same CSP, no new directive, no paths in events).
- Two issue-09 acceptance criteria move to issue 12, and `cache_key`'s doc comment
  is re-pointed there.
- Users who want caching get nothing yet. Accepted: they currently have nothing,
  and a correct slow map beats a fast wrong one.

## Alternatives considered

- **WebSocket** — rejected: updates are one-way; SSE adds no dependency and is
  already CSP-allowed.
- **Full page reload on update** — rejected: discards zoom/pan and re-runs ELK
  layout on every save, which is what watch mode exists to avoid. The existing
  loader already solves refetch.
- **Push the document over SSE** — rejected: duplicates `/api/document` and
  bypasses the verified snapshot path.
- **Watching inside `cratevista-server`** — rejected: ADR-0006 keeps the server a
  reload-free artifact server, and core is the orchestrator.
- **A `watch` subcommand** — rejected: `open --watch` reuses an existing command
  whose meaning already is "generate, serve, show me".
- **Shipping a cache anyway, keyed on mtime** — rejected: it would be the one
  component able to silently show the wrong architecture.
