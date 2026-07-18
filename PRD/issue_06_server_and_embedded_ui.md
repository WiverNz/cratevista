# PRD — Serve the document with an embedded web application

## Status

**Implemented / Verified.** All four phases have landed and every acceptance criterion below has implementation + test evidence. Phase 0 (the PRD-02 `artifact_hashes` schema amendment + the PRD-05 writer amendment) shipped first; then the hash-verified snapshot loader (`cratevista-server`: `options`/`error`/`snapshot`), the server surface (`state`/`router`/`api`/`assets`/`source`/`bind`/`shutdown`), and the `cratevista-core` `serve`/`open` orchestration + CLI wiring. `cratevista-server` depends only on `cratevista-schema` (verified via `cargo tree`); all stable gates pass (`fmt`, `clippy -D warnings`, `test`, `+1.97.0 check`). See `docs/adr/0006-server-and-security.md` and `docs/server.md`. It also records the static-hosting decisions that shape the SPA/asset behaviour; see "## Static-hosting decisions".

## Source issue

`ISSUES/issue_06_server_and_embedded_ui.md`

## Summary

`cratevista-server` provides a **loopback** HTTP server (axum/tokio) that serves an existing generated artifact snapshot — `/api/document`, `/api/generation`, `/api/diagnostics`, `/api/health`, a guarded `/api/source` — plus the prebuilt SPA embedded via `rust-embed` (no Node.js for end users). It loads the three PRD-05 artifacts **snapshot-consistently** (BLAKE3 hash-verified against the hashes embedded in `generation.json`, with bounded retry) so a request never mixes files from different generations, and it exposes a replaceable `AppState` + a shutdown handle so PRD 09 can add watcher-driven live reload without rewriting handlers. `cratevista-core` now provides `serve`/`open` use cases (path resolution, generation before `open`, a bounded readiness probe, browser opening, exit-code mapping); `cargo cratevista serve`/`open` now dispatch to them (the exit-code-4 stubs are gone).

## Problem statement

The generated document must be explorable in a browser from the installed binary alone, safely (loopback-only by default, no traversal, no arbitrary file exposure), with a stable API contract for the frontend (issue 07). Because PRD 05 commits the three artifacts by **per-file rename with `generation.json` written last** (not one crash-atomic transaction), a naïve reader can observe a new `document.json` alongside an old `generation.json`/`diagnostics.json`. The server must read a **consistent snapshot** or refuse to publish one.

## Goals

- Load a hash-verified three-file snapshot (marker A == B + BLAKE3 `artifact_hashes` verification + bounded retry) and validate the document before serving.
- Serve the embedded SPA + `/api/{document,generation,diagnostics,health}` and a path-validated, off-by-default `/api/source`.
- Bind `127.0.0.1` by default; default port `7420` with increment-on-conflict; bind the listener first and report its actual address.
- SPA fallback, correct content types, security headers, no permissive CORS, no arbitrary repo file exposure, no absolute paths in public errors.
- Graceful shutdown via a handle (testable without OS signals); `open` waits for readiness, then launches the browser (failure is non-fatal).
- A replaceable `AppState` seam so PRD 09 can hot-swap the snapshot.

## Non-goals

- Building the frontend (issue 07) — this crate embeds the committed `web/dist`, which PRD 07 has since replaced with the real production bundle. Cargo never invokes npm; contributors rebuild it with `npm run build` and commit the result.
- Watch/live-reload and the SSE `/api/events` endpoint (issue 09). PRD 06 provides only the replaceable-state seam; it does **not** implement SSE or a reserved events route.
- Persistent cache (**`ISSUES/issue_12_persistent_cache.md`** — split out of issue 09 on 2026-07-16; PRD 09 ships watch mode without one), static-site build output (issue 10), TOML overlay parsing (issue 08).
- Regenerating artifacts inside `serve` (see "Serve/open semantics").

## Current repository state (implemented)

Verified against the real code (PRD 06 is Implemented / Verified):

- **`cratevista-server`** is implemented (`#![forbid(unsafe_code)]`). Modules: `lib` (public API + re-exports), `error` (`SnapshotError`/`ServerError` + stable codes), `options` (`ArtifactPaths`, `SnapshotLoadOptions`, `BindOptions`, `SourceAccessPolicy`), `snapshot` (`ArtifactSnapshot`, `SnapshotMarker`, `load_snapshot`), `state` (`AppState` over `ArcSwap`), `router` (`build_router`, `run`, CSP/security headers, fallback), `api` (health/document/generation/diagnostics handlers), `assets` (rust-embed + MIME + SPA fallback + cache policy), `source` (`SourceError` + guarded `/api/source`), `bind` (`bind_listener`), and `shutdown` (`ShutdownHandle`/`ShutdownSignal`/`shutdown_channel`). It **depends only on `cratevista-schema`** (plus the HTTP/embedding crates) — verified by `cargo tree`.
- **PRD 05 pipeline** writes `target/cratevista/{document.json, generation.json, diagnostics.json}` via `cratevista_core::artifacts::commit_artifacts` — a **prepare-then-commit** write: `document.json`/`diagnostics.json` are serialized, their exact canonical bytes are **BLAKE3-hashed and embedded** in the `GenerationReport`, and all three are written to `*.tmp` sibling files then committed by per-file same-directory rename in order **document → diagnostics → generation (last, the completion marker)**. Each rename is atomic where the OS supports it; the three-file set is **not** one crash-atomic transaction, so the embedded `artifact_hashes` are the reader's integrity mechanism. Artifact file names are the `cratevista_core::artifacts::{DOCUMENT_FILE, DIAGNOSTICS_FILE, GENERATION_FILE}` constants; the server hard-codes the same names (it does not depend on core).
- **`cratevista-schema`** provides everything the server serves and validates: `ExplorerDocument { schema_version: SchemaVersion, project, entities, relations, views }` + `ExplorerDocument::validate() -> Result<(), Vec<SchemaError>>`; `GenerationReport { generator, generated_at: Timestamp, toolchain: Option<String>, rustdoc_format_version: Option<u32>, input_hashes, counts, durations_ms, artifact_hashes: Option<ArtifactHashes>, partial: bool }` — **no generation-id field**; `ArtifactHashes { document_blake3: String, diagnostics_blake3: String }` (lowercase-hex, 64-char BLAKE3 digests); `DiagnosticsReport { schema_version, diagnostics }`; `SchemaVersion` (`CURRENT = "1.0"`, `MAJOR.MINOR`, `as_str()`); `RepoRelativePath::new(&str) -> Result<_, SourcePathError>` (rejects absolute paths / drive letters / UNC / any `..`; normalizes `\`→`/`) — the **traversal guard**; `canonical::to_canonical_string`.
- **`cratevista-core`** provides `run_generate`, `run_serve` (`serve` module), `run_open` (`open` module), `ExitCode::{SUCCESS(0), RUNTIME_ERROR(1), USAGE_ERROR(2), ENVIRONMENT_ERROR(3), NOT_IMPLEMENTED(4)}`, `CommandFailure`/`CommandOutcome`, `Diagnostic`, `paths::{resolve_project_root, find_cargo_manifest}`, and `artifacts::{commit_artifacts, blake3_hex}`. It depends on `cratevista-server` for the server lifecycle and owns the `serve`/`open` orchestration.
- **`cargo-cratevista`** `Serve { server: ServerArgs }` / `Open { generate: GenerateArgs, server: ServerArgs }` are implemented and dispatch to `cratevista_core::{run_serve, run_open}`; `serve`/`open` no longer return exit 4 (only `build` remains a stub).

### Implemented Phase-0 amendments

The snapshot-integrity mechanism shipped as PRD 06's first phase (the PRD-02 additive schema amendment + the PRD-05 writer follow-up amendment):

- `ArtifactHashes { document_blake3: String, diagnostics_blake3: String }` and the optional `GenerationReport.artifact_hashes: Option<ArtifactHashes>` field are implemented in `cratevista-schema`. The field is **optional for backward deserialization** (a pre-amendment `generation.json` still deserializes with `artifact_hashes = None`), but the **current generator always populates it**.
- `cratevista_core::artifacts::commit_artifacts` (with `blake3` added to `cratevista-core`) computes the two digests over the **exact canonical `document.json`/`diagnostics.json` bytes it writes** and embeds them in `generation.json`. A pre-amendment artifact set (no hashes) is refused by the server with `snapshot_integrity_unavailable`.
- **`SchemaVersion` remains `1.0`** — the field lives on the unversioned `generation.json`, and neither versioned artifact (`document.json`/`diagnostics.json`) changed. The checked-in JSON Schema (`cratevista-document.schema.json`) is generated from `ExplorerDocument` only, so it was **unaffected** and its drift test passes unchanged. The `full_mvp.generation.json` fixture and the round-trip / backward-compatibility tests were updated.

This is an **additive, backward-compatible schema amendment; no breaking schema change.**

- ADRs present: 0001–0006, 0010. **ADR-0006 (server/security)** and `docs/server.md` document the loopback default, hash-verified snapshot contract, source opt-in policy, and CSP/headers.

## Responsibility boundary

**`cratevista-server` owns:** loading + validating an existing artifact snapshot; snapshot-consistent reloads; loopback HTTP serving; embedded static assets; API routing; guarded source-file access; server lifecycle + graceful shutdown; a replaceable `AppState` for PRD-09 live reload.

**`cratevista-core` owns:** `serve`/`open` use-case orchestration; resolving project/output paths; running generation before `open` (per the CLI contract); browser opening; CLI diagnostics + exit codes.

**`cratevista-server` must NOT:** invoke cargo metadata or rustdoc; build the semantic graph; write generation artifacts; parse TOML overlays; watch the filesystem (PRD 06); implement a persistent cache; depend on `cargo-cratevista`; expose arbitrary filesystem paths.

## Dependency direction

`cratevista-server` **may depend on**: `cratevista-schema`; `axum` + `tokio`; `tower`/`tower-http` (headers, graceful shutdown util); `serde`/`serde_json`; `blake3` (verify artifact hashes); `rust-embed` (asset embedding) + `mime_guess`; `arc-swap` (replaceable state); `thiserror`; optionally `tracing`.

It **must NOT depend on**: `cargo-cratevista`, `cratevista-metadata`, `cratevista-rustdoc`, `cratevista-config`, `cratevista-watch`. **Evaluated: it does NOT need `cratevista-graph`** — it serves schema artifacts and re-validates them with `ExplorerDocument::validate()`; it never rebuilds them.

`cratevista-core` depends on `cratevista-server` for `serve`/`open` orchestration (plus a browser-opener crate — see below). `cargo-cratevista` depends only on `cratevista-core`.

### Dependencies (resolved, from `Cargo.lock`)

All build on stable Rust 1.97.0. Resolved patch versions:

- Server: `axum 0.8.9`, `tokio 1.52.3` (features `rt-multi-thread`, `net`, `macros`, `sync`, `time`), `tower 0.5.3`, `tower-http 0.6.11` (feature `set-header`), `blake3 1.8.5` (workspace), `rust-embed 8.12.0` (feature `debug-embed`, so a missing `web/dist` fails the build), `mime_guess 2.0.5`, `arc-swap 1.9.2`, `thiserror` (workspace), `tracing` (workspace). Dev: `tower` (`ServiceExt::oneshot` for handler tests), `tempfile`.
- Core (added for the `serve`/`open` lifecycle): `cratevista-server` (path), `blake3 1.8.5` (compute the writer's artifact hashes), `tokio 1.52.3` (features `rt-multi-thread`, `net`, `io-util`, `time`, `signal`, `macros`) for the runtime, Ctrl-C handling, and the readiness probe, and the cross-platform browser opener `opener 0.7.2`.

## Snapshot loading contract

One concrete loader; the three artifacts are loaded and swapped as **one unit** so a request never mixes generations.

```rust
pub struct ArtifactPaths {
    pub document: PathBuf,     // <out>/document.json
    pub generation: PathBuf,   // <out>/generation.json
    pub diagnostics: PathBuf,  // <out>/diagnostics.json
}

pub struct SnapshotLoadOptions {
    pub max_retries: u32,      // default 4
    pub retry_delay: Duration, // default ~25 ms
    pub supported_major: u32,  // = 1 (SchemaVersion MAJOR)
}

/// A cheap discriminator for a committed generation, used only to detect that a
/// commit landed *during* a read (so the loader retries). It does NOT prove the
/// other two artifacts belong to this generation — the embedded `artifact_hashes`
/// do that. Stored as the `generation.json` bytes (also served at /api/generation).
pub struct SnapshotMarker(Arc<[u8]>);

pub struct ArtifactSnapshot {
    // Parsed values (validated) …
    pub document: Arc<ExplorerDocument>,
    pub generation: Arc<GenerationReport>,
    pub diagnostics: Arc<DiagnosticsReport>,
    // … and the exact on-disk bytes, reused verbatim as API response bodies
    // (already canonical + valid — no per-request re-serialization).
    pub document_bytes: Arc<[u8]>,
    pub generation_bytes: Arc<[u8]>,
    pub diagnostics_bytes: Arc<[u8]>,
    pub marker: SnapshotMarker,
    pub partial: bool,         // = generation.partial (for /api/health)
}

pub fn load_snapshot(
    paths: &ArtifactPaths,
    options: &SnapshotLoadOptions,
) -> Result<ArtifactSnapshot, SnapshotError>;
```

**Why marker equality alone is insufficient.** The three files are committed by per-file rename with `generation.json` last, and are **not** one crash-atomic transaction. A torn state is reachable where the writer has already renamed the new `document.json`/`diagnostics.json` but has **not yet** renamed the new `generation.json`; a reader then observes the **old** `generation.json` both before *and* after reading the new siblings — `A == B` yet the snapshot is **mixed**. Therefore the loader must **verify content hashes**: `generation.json` carries the BLAKE3 hashes of the exact `document.json`/`diagnostics.json` bytes committed with it (PRD-02 `GenerationReport.artifact_hashes`; written by PRD 05). Do **not** rely on mtimes.

**Algorithm** (hash-verified; bounded retry):

1. Read `generation.json` bytes → **marker A**.
2. Read `document.json` bytes.
3. Read `diagnostics.json` bytes.
4. Read `generation.json` bytes again → **marker B**.
5. Require **A == B** (detects a commit landing mid-read → retry otherwise).
6. Parse the `GenerationReport` from A (`MalformedGeneration` on failure).
7. Require `artifact_hashes` to be present (`snapshot_integrity_unavailable` when absent — a pre-amendment artifact set).
8. **Validate the digest encoding** of both `document_blake3` and `diagnostics_blake3` **before** any comparison: each must be exactly 64 lowercase-hex ASCII characters (no `0x` prefix, no whitespace). A malformed digest → `InvalidArtifactHash` (a dedicated code, distinct from `MalformedGeneration`, so the diagnostic is actionable). This is a validation failure, **not** a retry condition.
9. Compute BLAKE3 over the loaded `document.json` and `diagnostics.json` bytes (lowercase hex, same encoding).
10. Require **both** computed hashes to equal `generation.artifact_hashes.document_blake3` / `diagnostics_blake3`.
11. Parse `document.json` (`MalformedDocument`) and `diagnostics.json` (`MalformedDiagnostics`).
12. **Validate both versioned artifacts.** `GenerationReport` carries **no `schema_version`** and is **not** part of this comparison. Gate `document.schema_version` and `diagnostics.schema_version`:
    - unsupported `document.schema_version` **major** (≠ `supported_major`) → `SchemaVersionUnsupported`;
    - unsupported `diagnostics.schema_version` **major** → `SchemaVersionUnsupported`;
    - `document.schema_version` and `diagnostics.schema_version` **differ** from each other → `SchemaVersionMismatch` (the two versioned artifacts disagree — a torn/incoherent set);
    - then run `ExplorerDocument::validate()` (`InvalidDocument`).
    No candidate snapshot is published on any of these failures.
13. Publish the `ArtifactSnapshot`.

On a **marker mismatch (step 5)** or **hash mismatch (step 9)**: retry within the bounded policy (`max_retries`, `retry_delay`). After exhaustion return `ArtifactChangedDuringRead` (marker kept changing) or `SnapshotHashMismatch` (marker stable but hashes never matched — i.e. a persistently torn set) as appropriate. **Never publish the candidate snapshot** on a failed check.

`SnapshotMarker` stores the `generation.json` bytes only to detect mid-read commits and to let PRD 09 notice a *new* generation; integrity is proven by the hashes, not by the marker.

**Behavior:**

| condition | outcome |
|---|---|
| any of the three files missing | `ArtifactsMissing` — **fatal at startup**; core maps to exit 3 with remediation "run `cargo cratevista generate`". |
| read/permission error | `ArtifactReadFailed` — fatal at startup (exit 1); message carries no absolute path. |
| `generation.json` lacks `artifact_hashes` (pre-amendment artifacts) | `SnapshotIntegrityUnavailable` — fatal at startup (exit 3); remediation "run `cargo cratevista generate`" (regenerate with the current tool). |
| `artifact_hashes` present but a digest is not 64 lowercase-hex chars | `InvalidArtifactHash` — fatal at startup (exit 1); a corrupt/hand-edited `generation.json`; not a retry condition. |
| marker changes every attempt (retries exhausted) | `ArtifactChangedDuringRead` — fatal at startup (exit 1); "regeneration in progress; retry". |
| marker stable but the embedded hashes never match the loaded bytes (retries exhausted) | `SnapshotHashMismatch` — fatal at startup (exit 1); a torn/corrupt artifact set. |
| malformed JSON (per file) | `MalformedDocument`/`MalformedGeneration`/`MalformedDiagnostics` — fatal (exit 1). |
| unsupported `document`/`diagnostics` schema major | `SchemaVersionUnsupported` — fatal (exit 1). |
| `document.schema_version` ≠ `diagnostics.schema_version` | `SchemaVersionMismatch` — fatal (exit 1); the two versioned artifacts disagree. |
| `validate()` fails | `InvalidDocument` (carries the `SchemaError`s) — fatal (exit 1). |
| stale `*.tmp` files present | ignored — the loader reads only the committed file names. |
| **startup with no valid snapshot** | **fatal** — the server does not start. |
| **PRD-09 reload fails** | the last valid snapshot is **preserved** and kept serving (documented seam; enforcement is PRD 09). |

## Application state

A single replaceable snapshot so handlers always observe one coherent generation:

```rust
pub struct AppState {
    snapshot: arc_swap::ArcSwap<ArtifactSnapshot>, // ONE unit: doc+generation+diagnostics+marker
    source: SourceAccessPolicy,
    project_root: PathBuf,   // absolute; used only server-side for source containment
}
```

- Every request handler calls `snapshot.load()` once and reads document/generation/diagnostics from that **same** `Arc<ArtifactSnapshot>` — the three are **never** mixed across generations within a request.
- Snapshot replacement is **atomic at the snapshot level** (`ArcSwap::store`).
- PRD 06 loads once at startup. **PRD 09** adds watcher-driven `snapshot.store(new)` with no handler changes; a failed reload keeps the previous snapshot.
- **No SSE** is implemented here and **no events route is reserved** (a bare 404/501 stub adds no value).

**Decision (resolved): `arc-swap`.** It is the established cross-PRD seam (INDEX "AppState ArcSwap + shutdown"), is well-maintained, builds well below MSRV 1.97, and gives lock-free reads. A `std::sync::RwLock<Arc<ArtifactSnapshot>>` is an acceptable equivalent (same atomic-swap-at-snapshot semantics); the PRD standardizes on `arc-swap` to avoid re-litigating it in PRD 09.

## Bind and port behavior

**Bind the listener first, then report its actual local address** (no probe-then-bind race).

- Bind address default: **`127.0.0.1`** (IPv4 loopback). `--host ::1` allowed for IPv6 loopback.
- Default port **`7420`**. When the default is occupied **and the port was not explicit**, try `7421..=7440` in order, binding the first that succeeds; if all fail → `PortRangeExhausted`.
- When the user **explicitly** supplies a port and it is unavailable → `BindFailed` (address-in-use), no fallback.
- **Never bind `0.0.0.0` by default.** A non-loopback `--host` requires explicit user action **and** prints a prominent security warning (the UI would be reachable from other machines).
- Port `0` → OS-assigned ephemeral port; the reported address reflects the actual bound port.
- Distinguish **address-in-use** (`AddrInUse` → try-next or `BindFailed`) from **other** bind failures (permission/invalid address → `BindFailed` immediately, no port walk).

```rust
pub struct BindOptions { pub host: IpAddr, pub port: Option<u16>, pub port_was_explicit: bool }
pub async fn bind_listener(options: &BindOptions) -> Result<tokio::net::TcpListener, ServerError>; // .local_addr() is the truth
```

## HTTP routes

MVP route table (methods: `GET`, plus `HEAD` where trivially derivable):

| method | path | response |
|---|---|---|
| GET/HEAD | `/api/health` | `200` JSON `{status, schema_version, partial}` |
| GET/HEAD | `/api/document` | `200` the stored `document.json` bytes (`application/json`) |
| GET/HEAD | `/api/generation` | `200` the stored `generation.json` bytes |
| GET/HEAD | `/api/diagnostics` | `200` the stored `diagnostics.json` bytes |
| GET/HEAD | `/api/source?path=<repo-relative>` | `200` UTF-8 text when enabled + valid; else a JSON error (403/404/…) |
| GET/HEAD | `/` and static asset paths | embedded asset with correct MIME; else **SPA fallback** to `index.html` |

- **Method handling:** `GET`/`HEAD` served; any other method on a known route → **`405`** (with `Allow: GET, HEAD`). Unknown `/api/*` route → **JSON `404`** (stable code). Non-API path: serve the embedded asset if it exists, else fall back to `index.html` (SPA client routing) — the fallback applies **only** when the requested embedded asset does not exist.
- **`/api/events` (SSE) and watch-triggered replacement are PRD 09**, not implemented here; no route is registered for them in PRD 06.

## API response contract

**Resolved: Option B** — the snapshot stores the validated **canonical JSON bytes** (exactly as PRD 05 wrote them) alongside the parsed values, so `/api/document`/`/api/generation`/`/api/diagnostics` return pre-serialized `Bytes` with **no per-request serialization**. The parsed values back `validate()` and `/api/health`.

**Headers:**

- API JSON responses: `Content-Type: application/json; charset=utf-8`; `Cache-Control: no-store` (generated content changes on regeneration); `X-Content-Type-Options: nosniff`.
- Static assets: content type via `mime_guess` (correct `charset` for text); fingerprinted assets (issue 07) get `Cache-Control: public, max-age=31536000, immutable`; `index.html` gets `Cache-Control: no-cache` (always revalidate) so a rebuild is picked up.

**Never expose:** absolute artifact paths; internal errors with machine paths; raw rustdoc JSON; Cargo metadata JSON; source-file contents unless explicitly enabled.

**JSON error shape** (stable machine-readable codes; no absolute paths):

```json
{ "error": { "code": "source_disabled", "message": "source content serving is disabled" } }
```

## Health endpoint

`200` with a useful, non-sensitive body:

```json
{ "status": "ok", "schema_version": "1.0", "partial": false }
```

- `schema_version` from the loaded document; `partial` from `generation.partial`.
- **A partial-but-valid artifact stays `200`** with `"partial": true` (resolved). Health reflects "server is serving a valid snapshot", and partial generation is a valid snapshot.
- **Excludes** workspace absolute path, usernames, `CARGO_HOME`, environment variables, and toolchain command lines.

## Required additive amendments for PRD 09 (approved 2026-07-16)

**PRD 06 remains Implemented / Verified.** Both amendments below are additive;
they are recorded here because this PRD owns the server surface, and PRD 09 must
not change it silently. Neither touches `SchemaVersion` (a transport header and a
health field are not schema artifacts), neither changes the CSP, and neither
alters an existing response body.

> **A1 + A2 LANDED 2026-07-17** as PRD-09's prerequisite phase — **PRD 06 stays
> Implemented / Verified**. Nothing else from PRD 09 exists: no `/api/events`, no
> `--watch`, no watcher, no frontend client.
>
> **Shipped (A1):** `SnapshotMarker` became `{ bytes, token }` with a private
> `new()` that hashes **once**; `token() -> &str` returns the cached
> 64-char lowercase-hex BLAKE3 of the exact marker bytes. `api.rs` gained
> `SNAPSHOT_HEADER` (`x-cratevista-snapshot`) and an `artifact_json` helper; each
> of the three handlers loads the snapshot **once** and takes both bytes and token
> from that one `Arc`, so the header can never describe a different generation
> than the body. `as_bytes`/`matches` are untouched, so the loader's torn-read
> detection is unchanged.
>
> **Shipped (A2):** `AppState` gained a `watch_enabled: bool`;
> **`AppState::new` is unchanged and means "not watching"**, with a new
> `AppState::new_watching` for the watching case and a `watch_enabled()` reader.
> Making it a second constructor rather than a parameter on `new` kept every
> existing caller compiling **and** kept the safe default un-settable by accident.
> `/api/health` always includes `"watch_enabled"`; `serve`/`open` report `false`
> in this phase because nothing constructs the watching state yet.
>
> **Tests: +10 (server 53 → 63).** 4 marker unit tests: the token is 64 chars,
> lowercase hex, and exactly `blake3(marker_bytes)`; deterministic (equal bytes →
> equal token, different bytes → different token); **computed once, proven by
> `std::ptr::eq` on two `token()` calls and across a clone** — a per-request
> implementation returns a fresh `String` and fails this; and `matches`/`as_bytes`
> still work. 6 route tests: all three artifact routes expose **one token from one
> `AppState` snapshot**; **`replace_snapshot` moves all three atomically** (with
> an `assert_ne!` proving the two fixtures genuinely differ, so it cannot pass
> vacuously); the header is **absent** from `/api/health`, `/index.html` and
> `/api/source`; `Cache-Control: no-store` and the exact stored bytes survive
> alongside it; health reports `false` by default and `true` for a directly
> constructed watching state; and `/api/events` still does not exist.
>
> **Negative control:** removing the header from `/api/diagnostics` alone fails
> both cross-route tests — the guards are real.
>
> **Dependencies: none added.** `blake3` was already a `cratevista-server`
> dependency (the loader verifies `artifact_hashes` with it), so the token cost no
> new crate. `cargo tree -p cratevista-server` still shows `cratevista-schema` as
> its only workspace dependency.

### A1 — `X-CrateVista-Snapshot` on the three artifact routes (PRD-09 B2)

`/api/document`, `/api/generation` and `/api/diagnostics` gain **one identical
header** naming the snapshot they were served from:

```text
X-CrateVista-Snapshot: <64-char lowercase hex>
```

**Why it is needed:** the three artifacts are three requests. Today nothing swaps
while a server runs, so a client cannot observe a mix; **PRD 09's live swap makes
it reachable**, and a document from generation N rendered with diagnostics from
N+1 is a silent wrong answer. The header lets the client detect the mix it cannot
otherwise see.

**Value:** `SnapshotMarker` currently holds the **raw `generation.json` bytes**
(`SnapshotMarker(Arc<[u8]>)`, `as_bytes`, `matches`), which are **not
header-safe** — arbitrary bytes cannot go in an HTTP header. The amendment adds
**`SnapshotMarker::token(&self) -> &str`**: the lowercase-hex BLAKE3 of the marker
bytes, **computed once when the snapshot is built** and cached, never per request.
It is opaque and stable: equal tokens mean the same generation. It exposes no
path (it is a hash of already-public bytes).

**Client contract (PRD 09 owns the frontend half):** fetch the triple, compare the
three header values; if they disagree, discard the triple and retry — **bounded at
three attempts total**. Three suffices because a mismatch requires a swap landing
inside one fetch window, and swaps are debounce-limited; unbounded retry against a
fast-regenerating workspace would spin.

`ArtifactSnapshot.marker` is unchanged, and the existing loader keeps using
`matches()` for torn-read detection.

### A2 — `/api/health` gains `watch_enabled` (PRD-09 capability probe)

```json
{ "status": "ok", "schema_version": "1.1", "partial": false, "watch_enabled": true }
```

**Additive field, always present**, `false` unless the process is running
`open --watch`. `serve` always reports `false`, because after PRD-09 B1 `serve` has
no `--watch`.

**Why:** the frontend must not open an `EventSource` against a server with no
`/api/events` route — that yields a failed request plus `EventSource`'s automatic
**infinite reconnect loop** against a 404, in every `serve` session and every
static export. Probing health first is one request; the alternative is a permanent
background error. **The client creates `EventSource` only when
`watch_enabled === true`.**

**Excludes, unchanged:** no absolute path, username, `CARGO_HOME`, environment or
command line. `watch_enabled` is a bare boolean and reveals only that watching is
on.

> The `schema_version` in the "## Health endpoint" example above reads `"1.0"`
> because it predates PRD 08's additive `SchemaVersion` 1.0 → 1.1 bump. The field
> is still "whatever the loaded document reports"; only the illustrative literal
> aged.

## Static asset embedding

**Resolved: `rust-embed`.** A checked-in `web/dist/` (repo root) is embedded at compile time; PRD 07 replaced the placeholder's contents with the real production bundle without touching server route code.

### Build-correctness amendment (added 2026-07-16, during PRD-07 verification)

**Status of PRD 06 is unchanged (Implemented / Verified).** This records an
amendment discovered while browser-verifying PRD 07.

`web/dist` is **committed**, and **Cargo never invokes npm** — not from any crate
and not from the build script. But because the directory was not a Cargo package
input, Cargo had no reason to rebuild `cratevista-server` when the bundle
changed: `npm run build && cargo build` could silently keep serving the
previously embedded UI. `crates/cratevista-server/build.rs` now declares it:

```rust
fn main() {
    println!("cargo::rerun-if-changed=../../web/dist");
}
```

That is the whole file. It runs no npm, mutates no filesystem state, adds no
build dependency, generates no Rust, changes no runtime behaviour, and adds no
watch/SSE/static-export functionality.

Why a build script is needed even though rust-embed's derive expands to
`include_bytes!` (whose paths rustc records as dependencies): that tracking only
covers files present at the last compile, so **modifying** an embedded file
already triggered a rebuild, while **adding or removing** one did not — and a
frontend rebuild adds and removes files every time a content hash changes an
emitted filename. Only directory-level tracking closes that gap.

`web/scripts/check-embed-rebuild.mjs` is the regression guard, and its add-file
probe is the negative control: with `build.rs` removed, the modify probe still
passes while the add-file probe fails (the server falls back to `index.html`
instead of serving the new asset). It restores the checkout in a `finally` block
even when an assertion fails. See `PRD/issue_07_interactive_explorer_ui.md`
("PRD-06 build-correctness amendment") and `docs/adr/0006-server-and-security.md`.

- The placeholder was **CSP-compatible with no inline JS or CSS** (so it worked under the strict CSP below and PRD 07 need not weaken it): three files —
  *(Historical: PRD 07 has since replaced these with the real Vite bundle —
  `index.html` plus hex-fingerprinted `assets/*`, including the same-origin ELK
  worker. `app.js`/`style.css` no longer exist. The CSP was amended once, additively,
  for React Flow's inline `style` attribute: `style-src-attr 'unsafe-inline'`.)*
  - `web/dist/index.html` — the shell, linking `./style.css` and `./app.js` (no inline `<script>`/`<style>`), with a visible "the full CrateVista explorer UI arrives in PRD 07" placeholder message.
  - `web/dist/app.js` — a small external script that fetches `/api/health` and renders the status (proving the server works).
  - `web/dist/style.css` — minimal external styles.
- `#[derive(rust_embed::RustEmbed)] #[folder = "$CARGO_MANIFEST_DIR/../../web/dist"]` — a **missing `web/dist` fails the build** (clear compile-time error). No `npm`/Node runs during `cargo build`; end users need no Node.js. A `debug-embed` (dev) feature can serve `web/dist` from disk for contributor iteration.
- Correct MIME types (`mime_guess` on the asset path); SPA `index.html` fallback for unknown client routes; immutable cache for fingerprinted assets; `no-cache` for `index.html`.

## Source endpoint security

Source **locations** (path + span) are always in the document; source-file **contents** are opt-in and **off by default**.

```rust
pub struct SourceAccessPolicy {
    pub enabled: bool,       // default false
    pub project_root: PathBuf, // absolute canonical root; server-side only, never in responses
    pub max_bytes: usize,    // default e.g. 1 MiB
}
```

- **Disabled (default):** `GET /api/source` → `403` with stable code `source_disabled`.
- **Enabled:** `GET /api/source?path=src/lib.rs`:
  1. Parse `path` with `RepoRelativePath::new` — rejects absolute paths, drive letters, UNC, and any `..` (`source_path_invalid` → `400`).
  2. Join beneath the **canonicalized** `project_root`.
  3. **Canonicalize the resolved file** and assert it is still under the canonicalized `project_root` (symlink-escape guard) → else `source_outside_root` (`403`).
  4. Require a **regular file** (reject directories / non-regular) → `source_not_file` (`404`).
  5. Enforce `max_bytes` (`source_too_large` → `413`); do **not** allow line ranges to bypass the size limit (line ranges are **deferred** to a concrete PRD-07 need).
  6. Read as UTF-8; non-UTF-8 → `source_not_utf8` (`415`).
  7. Return the text; **never** include the absolute resolved path in the body or error.

**Practical guarantee (stated honestly):** the canonicalize-then-`starts_with(project_root)` check prevents `..` and symlink-escape at check time; on Windows canonicalization yields `\\?\` paths (normalized before comparison). This is **not** a perfect sandbox — a **TOCTOU** window exists between canonicalization and open. It is mitigated by opening the file and re-checking metadata, and by the off-by-default policy; the PRD does not claim kernel-enforced containment. Directory listing is never offered.

## Server public API

Four concrete, unambiguous primitives that **separate bind / shutdown / router / execution** (so port behavior and handlers are testable without a long-running process). There is **no `serve`/`RunningServer`** helper that both runs-until-shutdown and returns a value — `run` is the single execution entry point and it awaits until shutdown. Browser opening is **not** here — it is a core concern.

```rust
/// Bind configuration (the only option struct for host/port; there is no
/// separate `ServerOptions` duplicating these fields).
pub struct BindOptions {
    pub host: IpAddr,           // default 127.0.0.1
    pub port: Option<u16>,      // None → default 7420 with increment-on-conflict
    pub port_was_explicit: bool,
}

/// Whether the guarded `/api/source` endpoint serves file contents (default off).
pub enum SourceAccessPolicy {
    Disabled,                   // default
    Enabled { root: PathBuf, max_bytes: u64 },
}

/// Binds a Tokio listener (bind-first); `listener.local_addr()` is the actual address.
pub async fn bind_listener(options: &BindOptions) -> Result<tokio::net::TcpListener, ServerError>;

/// A trigger/observer pair. `ShutdownHandle::trigger()` requests graceful shutdown;
/// `ShutdownSignal` is awaited by `run`.
pub fn shutdown_channel() -> (ShutdownHandle, ShutdownSignal);

pub fn build_router(state: Arc<AppState>) -> axum::Router;

/// Runs the server on an already-bound Tokio listener until `shutdown` fires,
/// then completes graceful shutdown and returns.
pub async fn run(
    listener: tokio::net::TcpListener,
    state: Arc<AppState>,
    shutdown: ShutdownSignal,
) -> Result<(), ServerError>;
```

`tokio::net::TcpListener` is used **consistently** (no implicit `std`→`tokio` conversion). `run` **blocks until shutdown**; it is never expected to return a handle. `cratevista-core` owns the lifecycle and **spawns `run(...)` as a Tokio task** so it can drive readiness/browser/Ctrl-C concurrently (see "Serve/open semantics" for the exact `open` sequence). The browser opener is **never** invoked before `run` is actively serving — core probes `/api/health` first. Tests `tokio::spawn(run(...))`, exercise the endpoints, then call `ShutdownHandle::trigger()` and `join` the task. (No `RunningServer` type — a `start()` that returned a `JoinHandle` would add a second lifecycle API with no MVP benefit; if PRD 09 needs a spawn helper it can add one then with an explicit `JoinHandle`.)

## Serve/open semantics

**Resolved: the "no surprising work in `serve`" MVP.**

- **`generate`** — unchanged (PRD 05): generate only.
- **`serve`** — serves **existing** artifacts; if any artifact is missing, fail with an actionable message pointing at `generate` (exit 3). `serve` **never** regenerates and therefore never needs nightly or the network.
- **`open`** — runs `generate` (nightly if a documentable target exists), starts the server, waits for it to be **ready**, and only then opens the browser.

**`open` orchestration (concrete, in `cratevista-core`).** The opener is **never** called before `run` is actively serving:

1. Load and validate the snapshot (Phase-0 hash verification; failure → the snapshot exit-code mapping above).
2. `bind_listener` and read `listener.local_addr()` — the **real** address.
3. `shutdown_channel()`.
4. `tokio::spawn(run(listener, state, shutdown_signal))` — the serving task.
5. Perform a **bounded loopback readiness probe** against `http://{local_addr}/api/health`.
6. **Only after a successful `200`** invoke the browser opener with the actual URL.
7. **Browser-open failure is non-fatal:** the server stays running, the URL is printed, a warning is emitted, and the command does not fail an otherwise-healthy server (`browser_open_failed` is a warning, not an error exit).
8. **Readiness failure** triggers `ShutdownHandle::trigger()`, **joins** the server task, and returns `server_readiness_failed` (runtime **exit 1**).
9. Ctrl-C triggers `ShutdownHandle::trigger()` and the server task is **joined** (no orphan task).

**Readiness probe.** Loopback-only (targets the bound `local_addr`); a bounded timeout + retry count (small, e.g. a handful of ~25 ms attempts); requires **no external network access**; and is **injectable / deterministic in tests** (the probe function is a seam so tests can force success, force timeout, and assert ordering). `serve` (no browser) does not need the probe on its critical path, but reuses the same readiness seam where a test asserts the server is live.

**Exit codes** (`serve`/`open` no longer return `4`):

- bind failure / snapshot inconsistency / malformed-or-invalid artifacts / `server_readiness_failed` / server runtime error → **`1`**.
- CLI usage error → **`2`**.
- **missing artifacts** (serve prerequisite) / missing Cargo or nightly (open's generate step) → **`3`**.

## Graceful shutdown

- **Ctrl-C is handled in `cratevista-core`** (installs the `tokio::signal::ctrl_c` handler and calls `ShutdownHandle::trigger`), keeping `cratevista-server` signal-agnostic and testable.
- The server runs axum with `with_graceful_shutdown(shutdown.wait())`: the listener stops accepting, in-flight requests get a bounded grace period, the serving task is joined, and the Tokio runtime exits cleanly (no orphan runtime).
- **Tests trigger shutdown via the `ShutdownHandle`** (no OS signals) and assert the server task completes.
- PRD 09 reuses the **same** handle to coordinate watcher + server shutdown.

## CORS and browser security

- **No permissive CORS.** No `Access-Control-Allow-Origin: *` and no CORS layer by default — the embedded UI is **same-origin**. A non-loopback bind does **not** enable CORS.
- Security headers (via a `tower-http` set-header layer / small middleware), compatible with the placeholder **and** the future SPA:
  - `X-Content-Type-Options: nosniff`
  - `Referrer-Policy: same-origin`
  - `X-Frame-Options: DENY`
  - **Content-Security-Policy** (one documented constant; the placeholder uses **no inline JS/CSS**, so **no `unsafe-inline`**):
    ```
    default-src 'self'; script-src 'self'; style-src 'self'; base-uri 'self'; object-src 'none'; frame-ancestors 'none'
    ```
    **PRD-07 update path:** this CSP is a single named constant with a comment marking it as the PRD-07 extension point. PRD 07 may deliberately widen `connect-src`/`style-src`/`img-src` for the SPA (e.g. inline SVG styling), but the placeholder and a self-hosted external-file SPA both work under this policy unchanged — bundled assets are never blocked without a deliberate edit.
- The `/api/source` endpoint stays opt-in.

## Error handling and diagnostics

Stable `SnapshotError` / `ServerError` codes (no filesystem paths in public API errors; CLI errors may include safe **relative** paths):

`artifacts_missing`, `artifact_read_failed`, `artifact_changed_during_read`, `snapshot_integrity_unavailable` (a pre-amendment artifact set lacks `artifact_hashes` → regenerate), `invalid_artifact_hash` (an `artifact_hashes` digest is not 64 lowercase-hex ASCII chars — corrupt/hand-edited `generation.json`), `snapshot_hash_mismatch` (marker stable but the embedded hashes never matched the loaded bytes — a torn/corrupt set), `malformed_document`, `malformed_generation`, `malformed_diagnostics`, `invalid_document`, `schema_version_unsupported`, `schema_version_mismatch` (`document.schema_version` ≠ `diagnostics.schema_version`), `bind_failed`, `port_range_exhausted`, `server_readiness_failed` (the loopback `/api/health` readiness probe did not succeed within the bounded budget — `open` only), `source_disabled`, `source_path_invalid`, `source_outside_root`, `source_not_file`, `source_too_large`, `source_not_utf8`, `browser_open_failed`, `shutdown_failed`, `internal_invariant`.

Startup/snapshot/bind errors surface through `cratevista-core` as a `Diagnostic` + exit code (paths, if any, are relative). Per-request API errors return the JSON error shape with the code only.

## Files and modules (implemented)

```
crates/cratevista-server/src/
  lib.rs      # public API + re-exports
  error.rs    # SnapshotError, ServerError + stable codes (SourceError lives in source.rs)
  options.rs  # ArtifactPaths, SnapshotLoadOptions, BindOptions, SourceAccessPolicy
  snapshot.rs # ArtifactSnapshot, SnapshotMarker, load_snapshot (marker + hash verify + retry)
  state.rs    # AppState (ArcSwap<ArtifactSnapshot>), snapshot swap
  router.rs   # build_router, run; CSP/security headers; method/404/405/fallback wiring
  api.rs      # health/document/generation/diagnostics handlers
  assets.rs   # rust-embed asset serving + MIME + SPA fallback + cache policy
  source.rs   # SourceError + /api/source handler + SourceAccessPolicy enforcement
  bind.rs     # bind_listener (bind-first, port policy)
  shutdown.rs # ShutdownHandle/ShutdownSignal/shutdown_channel
crates/cratevista-core/src/
  serve.rs    # ServeOptions, run_serve, shared CoreServer lifecycle (resolve → load → bind → run + Ctrl-C)
  open.rs     # OpenOptions, run_open, BrowserOpener/ReadinessProbe (run_generate → serve → wait-ready → open, non-fatal)
crates/cargo-cratevista/src/
  cli.rs                 # GenerateArgs + ServerArgs flattened into Generate/Serve/Open
  dispatch.rs            # builds ServeOptions/OpenOptions and dispatches
  commands/serve.rs      # dispatch to cratevista_core::run_serve
  commands/open.rs       # dispatch to cratevista_core::run_open (SystemClock)
web/dist/index.html      # checked-in placeholder, embedded (until PRD 07)
web/dist/app.js          # checked-in placeholder (no inline JS)
web/dist/style.css       # checked-in placeholder (no inline CSS)
docs/adr/0006-server-and-security.md
docs/server.md           # endpoint & security reference
```

## Testing strategy

All normal tests are **stable-only** and **loopback-local** (no nightly, no external network, no Node.js). Handler behavior is tested via `tower::ServiceExt::oneshot` against `build_router` (no real port); bind behavior uses real loopback listeners on port `0`.

**Writer/reader integrity** (across PRD 05 core + PRD 06 server, using canonical fixtures): the writer's `artifact_hashes` equal BLAKE3 over the **exact canonical bytes** of the committed `document.json`/`diagnostics.json`.

**Snapshot** (`snapshot.rs`, temp dir fixtures): valid three-file snapshot (matching marker + matching hashes) loads + validates; **stable marker + matching hashes → one read succeeds**; **new `document.json`/`diagnostics.json` alongside an old `generation.json`** (the torn-commit counterexample) → hash mismatch → **rejected** (retry, then `snapshot_hash_mismatch`); **marker A == B but `document_blake3` mismatch** → rejected; **marker A == B but `diagnostics_blake3` mismatch** → rejected; **mismatch once then a retry succeeds**; **persistent mismatch exhausts retries** → `artifact_changed_during_read` (marker kept changing) / `snapshot_hash_mismatch` (marker stable, hashes never matched); **artifact set without `artifact_hashes`** → `snapshot_integrity_unavailable` (remediation: run `generate`); **a digest that is not 64 lowercase-hex chars (e.g. wrong length, uppercase, `0x` prefix)** → `invalid_artifact_hash` (validated before comparison; not retried); each artifact missing → `artifacts_missing`; malformed each artifact → the matching code; invalid document → `invalid_document`; **unsupported `document` or `diagnostics` schema major** → `schema_version_unsupported`; **`document.schema_version` ≠ `diagnostics.schema_version`** → `schema_version_mismatch`; partial `GenerationReport` loads fine; **no absolute path in any hash/snapshot error**.

**Router** (`oneshot`): each API endpoint returns the expected bytes/status; `/api/document` round-trips through `serde_json` into `ExplorerDocument`; diagnostics are a separate response (never inside the document); `/api/health` reflects the `partial` flag; unknown `/api/*` → JSON `404`; unsupported method → `405`; SPA fallback returns `index.html`; static assets have correct MIME; API `Cache-Control: no-store`; security headers present.

**Binding**: default `7420`; default conflict increments to the next free port; explicit conflict fails (`bind_failed`); exhausted range → `port_range_exhausted`; listener reports its actual `local_addr`; default bind is loopback (assert `is_loopback()`).

**Source**: disabled by default → `403`; a normal repo-relative file returns its text when enabled; absolute path rejected; `..` traversal rejected; percent-encoded traversal rejected (decoded before validation); symlink escape rejected (where the platform supports symlink creation in tests); directory rejected; oversize file rejected; non-UTF-8 rejected; **errors contain no absolute path**.

**Lifecycle**: a test `tokio::spawn(run(listener, state, signal))`, exercises endpoints, then calls `ShutdownHandle::trigger()` and **joins** the task to completion (graceful shutdown without OS signals).

**Placeholder assets under CSP**: the placeholder `index.html`/`app.js`/`style.css` load and function under the approved strict CSP (no `unsafe-inline` needed); a header test asserts the exact CSP string; a test asserts the placeholder HTML contains **no inline `<script>`/`<style>`**.

**Readiness / `open` orchestration** (injected probe + injected opener, deterministic): **the opener is not called before readiness** (a recording opener asserts it fires only after a `200` from `/api/health`); **successful readiness calls the opener exactly once with the actual bound URL**; **a forced readiness timeout triggers shutdown, joins the server task, and returns `server_readiness_failed` (exit 1)**; **browser-opener failure leaves the server running and healthy** (opener returns `Err`, server still answers `/api/health`); **no orphan task remains** after any of these paths (the spawned `run` task is always joined).

**Core/CLI integration**: `serve` on an existing generated snapshot (reuse a checked-in fixture snapshot or a `run_generate` on a bin-only workspace — no nightly) becomes live; missing artifacts → actionable failure (exit 3); `open` with an **injected no-op opener** generates, serves, becomes ready, and invokes the opener with the URL; browser-open failure leaves the server healthy and prints the URL; graceful shutdown via the handle completes; exit codes match the policy; **serving existing artifacts needs neither nightly nor Node.js**.

A checked-in fixture snapshot (a small valid `document.json`/`generation.json`/`diagnostics.json` under `tests/fixtures/`, whose `generation.json` carries correct `artifact_hashes`) keeps router/snapshot tests independent of `generate`.

## Performance considerations

Assets and artifact bytes are served from memory (`Arc<[u8]>`); no per-request disk read or re-serialization once a snapshot is loaded. Large documents are served as `Bytes` without copying.

## Observability and diagnostics

Optional `tracing` request spans (method, path, status) in the server; `cratevista-core` logs the bound URL and any port-fallback notice. `/api/diagnostics` surfaces the generation diagnostics (unchanged from the artifact).

## Documentation changes

Added `docs/adr/0006-server-and-security.md` (loopback default, hash-verified snapshot contract, source opt-in policy, CSP/headers) and `docs/server.md` (endpoint & security reference); updated README `serve`/`open` usage (also touched by issue 10) and the CHANGELOG.

## Rollout and migration

New crate implemented; `serve`/`open` stubs replaced. Asset embedding + `web/dist` coordinated with issues 07/10. **Additive, backward-compatible schema amendment; no breaking schema change.**

**Implementation history (all four phases complete):**

- **Phase 0 — prerequisite amendments (done):** `ArtifactHashes` + the optional `GenerationReport.artifact_hashes` field landed in `cratevista-schema`; the `GenerationReport` fixture (`full_mvp.generation.json`) and its round-trip/backward-compat tests were updated; `cratevista_core::artifacts` (with `blake3` added) computes the digests over the exact canonical bytes and embeds them in `generation.json`; the writer/`generate` fixtures and tests were updated. (The checked-in JSON Schema is `ExplorerDocument`-only, so it and its drift test were unaffected; `SchemaVersion` stays `1.0`.) These were the PRD-02 and PRD-05 amendments, delivered here. *(Phase-0 schema + writer/`generate` tests.)*
- **Phase 1 — snapshot loading (done):** `snapshot.rs` — marker read, hash-encoding validation, exact-byte BLAKE3 verification, dual/cross-artifact schema-version validation, bounded retry, stable error codes. *(full snapshot integrity + schema-version + retry test matrix, incl. the torn-commit counterexample.)*
- **Phase 2 — server surface (done):** `AppState`/`ArcSwap`, `router`, embedded `assets` (+ placeholder `web/dist`), API handlers + security headers, `source` security, `bind` (bind-first port policy), `shutdown`. *(router/health/HEAD/404/405, MIME/SPA-fallback, header/CSP, bind, and source tests.)*
- **Phase 3 — orchestration + CLI (done):** `cratevista-core` `serve`/`open` (readiness probe, non-fatal browser open, Ctrl-C), `cargo-cratevista` `serve`/`open` wiring, ADR-0006 + `docs/server.md`. *(lifecycle/readiness tests incl. a real `HttpProbe` vs a real server, and CLI `serve`-missing→exit-3 tests.)*

**Implementation deviations (verified; none leaves an acceptance criterion unchecked):**

- The checked-in JSON Schema (`cratevista-document.schema.json`) is generated from `ExplorerDocument` only, so the additive `GenerationReport.artifact_hashes` field **did not change it** and needed no regeneration; the drift test passes unchanged. There is no separate checked-in schema for `generation.json`.
- **`SchemaVersion` remains `1.0`** — the field is on the unversioned `generation.json`, and neither versioned artifact changed structurally, so no MINOR/MAJOR bump was warranted (bumping would have churned every `document`/`diagnostics` fixture for no compatibility benefit).
- `web/dist/` was previously git-ignored; because the prebuilt bundle is a committed, embedded deliverable, it was **un-ignored** so a clean checkout builds (rust-embed with `debug-embed` requires the folder at build time), and the three placeholder files are tracked.
- A server run-loop / graceful-shutdown I/O error maps to the stable code **`shutdown_failed`** (the nearest stable code for a serve-lifecycle failure).

## Risks and mitigations

- **Serving a torn snapshot** → **hash-verified** read (BLAKE3 hashes embedded in `generation.json`) + bounded retry + single-unit `AppState` swap; dedicated tests, including the torn-commit counterexample. Marker equality alone is explicitly **not** relied on.
- **Accidental public exposure** → loopback default, explicit non-loopback + warning, bound-addr loopback test.
- **Traversal/symlink escape** → `RepoRelativePath` guard + canonicalize-and-contain + off-by-default source; dedicated tests; honest TOCTOU statement.
- **Missing frontend build breaks the crate** → checked-in placeholder `web/dist` + `debug-embed` dev feature.
- **CSP blocking the future SPA** → single documented CSP constant marked as the PRD-07 update point.

## Alternatives considered

- Serializing schema values per request (Option A): rejected — storing canonical bytes avoids repeated serialization and guarantees byte-stable responses.
- Three separate `ArcSwap`s for document/generation/diagnostics: rejected — cannot guarantee a coherent per-request snapshot; a single `ArcSwap<ArtifactSnapshot>` can.
- Regenerating inside `serve`: rejected — surprising expensive/nightly work; `open` owns generation.
- Probing a port then binding: rejected — bind-first + report actual address avoids the race.
- `RwLock<Arc<ArtifactSnapshot>>` instead of `arc-swap`: acceptable equivalent; standardized on `arc-swap` for the cross-PRD seam.
- A reserved `/api/events` 404/501 stub: rejected — adds no value; PRD 09 registers the route when it implements SSE.

## Implementation sequence

*(Historical — the order in which the implemented phases landed.)*

**Phase 0 (prerequisite amendments):** `ArtifactHashes` + optional `GenerationReport.artifact_hashes` in `cratevista-schema` (JSON Schema is `ExplorerDocument`-only and was unaffected; updated the fixture + round-trip/backward-compat tests); amended `cratevista_core::artifacts` (add `blake3`) to compute + embed the digests (updated writer/`generate` fixtures + tests).

**Phase 1 (snapshot loading):**
1. `options` + `error` (stable codes incl. `invalid_artifact_hash`, `snapshot_hash_mismatch`, `snapshot_integrity_unavailable`, `schema_version_mismatch`, `server_readiness_failed`).
2. `snapshot` (marker read → hash-encoding validation → BLAKE3 verification → dual/cross-artifact schema-version validation → bounded retry) with unit tests.

**Phase 2 (server surface):**
3. `state` (`ArcSwap`) + `assets` (rust-embed + placeholder `web/dist`).
4. `router` + `api` handlers (health/document/generation/diagnostics) + security headers.
5. `bind` (bind-first port policy) + `shutdown`.
6. `source` route + `SourceAccessPolicy`.

**Phase 3 (orchestration + CLI):**
7. `cratevista-core` `serve`/`open` (readiness probe, non-fatal browser opener, Ctrl-C) + CLI wiring; ADR-0006.

## Acceptance criteria

- [x] **Phase 0 (prerequisite amendments) lands and passes first:** `ArtifactHashes` Rust type + `GenerationReport.artifact_hashes: Option<ArtifactHashes>` exist in `cratevista-schema`; the writer (`cratevista_core::artifacts`) computes and embeds the digests; the current `generate` always emits `artifact_hashes`; the `GenerationReport` fixture + round-trip/backward-compat tests are updated; exact-byte hash tests pass (hash == BLAKE3 of committed canonical bytes; one-byte change flips the digest; `generation.json` is not self-hashed; digests are 64 lowercase-hex chars). *(The checked-in JSON Schema is `ExplorerDocument`-only, so it is unaffected and its drift test passes unchanged; `SchemaVersion` stays `1.0` — the field is on the unversioned `generation.json`.)* *(Phase-0 schema + writer tests)*
- [x] The loader reads a **hash-consistent** snapshot: it requires marker A == B **and** that the loaded `document.json`/`diagnostics.json` bytes match the BLAKE3 `artifact_hashes` embedded in `generation.json` (digest encoding validated first — 64 lowercase-hex chars → else `invalid_artifact_hash`), with bounded retry; a snapshot never mixes generations (the torn-commit counterexample is rejected). A pre-amendment artifact set (no hashes) → `snapshot_integrity_unavailable`. *(snapshot tests: matching / torn-commit-rejected / hash-mismatch-rejected / invalid-hash-encoding / integrity-unavailable / changes-once-then-retry / keeps-changing-fails)*
- [x] The loader validates **both** versioned artifacts: an unsupported `document` or `diagnostics` schema major → `schema_version_unsupported`; `document.schema_version` ≠ `diagnostics.schema_version` → `schema_version_mismatch`; `GenerationReport` (no `schema_version`) is excluded from the comparison; no candidate is published on failure. *(schema-version tests)*
- [x] A locally installed binary serves the UI with **no Node.js**; assets are embedded. *(embed test; install E2E in issue 10)*
- [x] `/api/document` returns a valid explorer document; `/api/generation` and `/api/diagnostics` are separate. *(oneshot + `serde_json` round-trip)*
- [x] `/api/health` returns `{status, schema_version, partial}`; partial-but-valid stays `200` with `partial=true`. *(health test)*
- [x] Static assets have correct content types; unknown SPA routes return `index.html`; unknown `/api/*` → JSON `404`; bad method → `405`. *(router tests)*
- [x] Source endpoint is off by default (`403`); when enabled it rejects absolute/`..`/encoded-traversal/symlink-escape/directory/oversize/non-UTF-8; errors expose no absolute path. *(source tests)*
- [x] Default server binds loopback; default port `7420` with increment-on-conflict; explicit conflict fails; listener reports its real address. *(bind tests)*
- [x] No absolute machine path appears in any public API error. *(error tests)*
- [x] `serve` serves existing artifacts and fails actionably (exit 3) when missing; `open` generates, serves, **probes `/api/health` (loopback, bounded) and opens the browser only after a `200`**; a readiness timeout shuts down + joins the task and returns `server_readiness_failed` (exit 1); browser-open failure leaves the server healthy; no orphan task remains. *(core/CLI + readiness tests)*
- [x] Ctrl-C / handle-triggered graceful shutdown completes; testable without OS signals. *(shutdown test)*
- [x] No permissive CORS; security headers present; CSP documented with a PRD-07 update path. *(header tests)*
- [x] `serve`/`open` no longer return exit code 4. *(exit-code tests)*
- [x] `cratevista-server` does not depend on core/CLI/metadata/rustdoc/graph/config/watch. *(`cargo tree -i` for each)*

Verification:

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-features            # stable-only; loopback-local; no nightly/Node
cargo +1.97.0 check --workspace --all-features
cargo tree -p cratevista-server -i cargo-cratevista   # no path
cargo tree -p cratevista-server -i cratevista-metadata # no path
cargo tree -p cratevista-server -i cratevista-rustdoc  # no path
cargo tree -p cratevista-server -i cratevista-graph    # no path
cargo tree -p cratevista-server -i cratevista-config   # no path (crate not present yet)
cargo tree -p cratevista-server -i cratevista-watch    # no path (crate not present yet)
```

Also verify: `serve`/`open` no longer return exit 4; loopback default; source access off by default; the snapshot loader never mixes generations; API errors expose no absolute paths; serving existing artifacts needs neither nightly nor Node.js; assets are embedded; no watch/SSE behavior is implemented.

## Static-hosting decisions

CrateVista serves the explorer from an embedded axum server rather than a static
file host, but the SPA/asset behaviour in this issue and the static build in issue
10 follow the same static-hosting rules, stated here as direct decisions.

1. **Relative asset base.** The embedded SPA uses relative asset paths so the same
   build works regardless of mount path (mirrors issue 10 `--base-path`).

2. **SPA fallback.** Unknown non-API routes return `index.html`, so deep links
   resolve client-side. The asset route specifies this explicitly.

3. **Document served as a separate resource.** The generated document is exposed
   at `GET /api/document` (serve) or as a relative `./document.json` (static build,
   issue 10) — distinct from the app shell, with an explicit 404 and no directory
   listing, behind CrateVista's guarded routes.

4. **Runtime fetch with graceful fallback.** When the document is missing or
   malformed, the UI shows a clear error state and never crashes.

5. **Data/asset separation.** Application assets are embedded; the document is read
   as a consistent snapshot from `target/cratevista/`, and no arbitrary local file
   is ever exposed over HTTP.

6. **Visual scope: PRD 06 serves a functional placeholder; the polished explorer UI is PRD 07.** PRD 06 must **not** claim to implement the final explorer UI.
   - **PRD 06 visual acceptance (limited to the placeholder):** a **non-blank** embedded page; a clear CrateVista placeholder/status presentation; a successful `/api/health` display; correct **SPA fallback** (deep links return `index.html`); **no CSP-blocked assets** (external `app.js`/`style.css` load under the strict CSP); **no flash caused by missing static assets** (the shell + styles are embedded and served locally).
   - **Deferred to PRD 07:** the polished dark explorer shell; the full overview graph; production visual polish; and inspector/timeline/toolbar interactions.

7. **Server-side responsibilities (both issues)**
   - Fast first paint of the embedded shell from a single binary with **no Node.js** (PRD 06, placeholder; PRD 07, real UI).
   - `/api/document` returns a schema-valid document; **rendering** the full overview screen is **PRD 07's** responsibility, not PRD 06's.

8. **CrateVista visualizes generated Rust data**
   - The document is **generated** and lives under `target/cratevista/`; the server reads it there as a consistent snapshot.
   - `/api/generation` and `/api/diagnostics` are first-class, because generation metadata and diagnostics are part of the product.
   - Source-content serving is an explicit opt-in endpoint: file contents stay behind a guarded, off-by-default route while source *locations* are always exposed.

## Open questions

**None blocking.** All material decisions are resolved above:

- Snapshot integrity = **BLAKE3 `artifact_hashes` embedded in `generation.json`** (PRD-02 additive amendment) verified against the loaded `document.json`/`diagnostics.json` bytes; the `generation.json`-bytes marker only detects a mid-read commit (retry) and is **not** treated as proof of consistency; bounded retry; single-unit `ArcSwap` state.
- `serve` = serve-existing (no regeneration); `open` = generate + serve + browser (non-fatal open).
- Bind loopback first + report actual address; port `7420` + increment; source off by default; no CORS; documented CSP.
- `rust-embed` assets + placeholder `web/dist`; `arc-swap` state; `opener` (or `Command` fallback) for the browser.

Non-blocking follow-ups (deferred, not required for approval): optional IPv6-loopback default ordering; WSL browser-open specifics (`wslview`/`explorer.exe` else print URL); whether `/api/generation` and `/api/diagnostics` ever merge (kept separate for now); optional `HEAD` coverage breadth; source line-range support (deferred until a PRD-07 need).

## Traceability

Issue-06 checkboxes → tests above. `/api/document` consumed by issue 07; the single-unit `AppState` `ArcSwap` + `ShutdownHandle` consumed by issue 09 (SSE `/api/events` + watcher-driven snapshot replacement); asset embedding + `web/dist` reused by issue 10's static build. The **hash-verified snapshot loader** is the reader half of PRD 05's prepare-then-commit artifact write; the BLAKE3 `artifact_hashes` it verifies come from the PRD-02 additive `GenerationReport` amendment (written by PRD 05).
