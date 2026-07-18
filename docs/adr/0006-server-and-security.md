# ADR 0006 — Loopback server, snapshot integrity, and security

- Status: Accepted
- Date: 2026-07-14
- Related: PRD `PRD/issue_06_server_and_embedded_ui.md`; ADR-0001 (crate
  boundaries); ADR-0002 (exit codes); ADR-0003 (schema versioning).

## Context

`cargo cratevista serve` / `open` must serve the three generated artifacts
(`document.json`, `generation.json`, `diagnostics.json`) and a prebuilt web UI
from the installed binary alone, safely, on a developer's machine. The artifacts
are written by a **prepare-then-commit** writer (per-file rename, `generation.json`
last) and are **not** one crash-atomic transaction, so a naive reader can observe
a torn set. End users must not need Node.js.

## Decision

### Crate boundary

`cratevista-server` depends only on `cratevista-schema` plus HTTP/embedding
crates (`axum`, `tokio`, `tower`/`tower-http`, `blake3`, `rust-embed`,
`mime_guess`, `arc-swap`, `thiserror`). It does **not** depend on core, the CLI,
or any analyzer crate, and never invokes cargo/rustdoc, builds the graph, parses
config, or writes artifacts. `cratevista-core` owns the `serve`/`open`
orchestration and depends on the server.

### Snapshot integrity — hashes, not marker equality

Marker equality alone (comparing `generation.json` bytes before/after reading
the other two files) is **insufficient**: a torn commit can leave an old
`generation.json` observable both before and after the newer siblings are
renamed. Integrity is instead proven by **BLAKE3 `artifact_hashes`** embedded in
`generation.json` (PRD-02 additive schema amendment; written by the PRD-05
writer amendment). The loader:

1. reads `generation.json` (marker A) → `document.json` → `diagnostics.json` →
   `generation.json` (marker B);
2. requires **A == B** (only a mid-read-commit detector → retry otherwise);
3. requires `artifact_hashes` present (else `snapshot_integrity_unavailable`);
4. validates each digest is 64 lowercase-hex chars **before** comparing (else
   `invalid_artifact_hash`);
5. requires the BLAKE3 of the loaded `document`/`diagnostics` bytes to equal the
   embedded digests (else retry → `snapshot_hash_mismatch`);
6. validates **both** versioned artifacts: unsupported `document`/`diagnostics`
   major → `schema_version_unsupported`; the two disagreeing →
   `schema_version_mismatch` (`generation.json` has no `schema_version` and is
   excluded); then `ExplorerDocument::validate()`.

Marker/hash mismatches retry within a bounded budget; missing hashes, invalid
encoding, malformed JSON, and schema-version problems are non-transient. No
candidate snapshot is published on any failed check. No public error contains a
filesystem path.

### Digest encoding

Each digest is BLAKE3 over the exact canonical UTF-8 bytes committed to disk,
encoded as lowercase hexadecimal, exactly 64 ASCII characters, no `0x` prefix,
no whitespace. The writer hashes the same bytes it writes; the server validates
the encoding before comparison.

### Schema-version policy

Adding the optional `GenerationReport.artifact_hashes` field is an **additive,
backward-compatible** change under ADR-0003. `SchemaVersion` (`1.0`) versions
`document.json` and `diagnostics.json`; `generation.json` carries no
`schema_version`, and neither versioned artifact changed structurally, so
**`SchemaVersion` is not bumped**. Old `generation.json` without the field still
deserializes (`artifact_hashes = None`) but fails integrity with
`snapshot_integrity_unavailable`.

### Lifecycle API

Four unambiguous primitives, so port behavior and handlers are testable without
a long-running process:

- `bind_listener(&BindOptions) -> Result<tokio::net::TcpListener, ServerError>`
- `shutdown_channel() -> (ShutdownHandle, ShutdownSignal)`
- `build_router(Arc<AppState>) -> axum::Router`
- `run(listener, Arc<AppState>, ShutdownSignal) -> Result<(), ServerError>`
  (blocks until shutdown)

Core owns the sequence: bind → read `local_addr` → `shutdown_channel` → spawn
`run` → (for `open`) probe readiness → open browser → Ctrl-C triggers shutdown →
join. Ctrl-C is handled in core; the server is signal-agnostic. There is no
`serve()`/`RunningServer` in the server crate; core holds a private
`JoinHandle`-carrying handle.

### Readiness before browser

`open` never launches the browser before the server is serving. Core spawns
`run` as a task, then runs a **bounded, loopback-only** `/api/health` readiness
probe; only after a `200` does it invoke the browser opener. A readiness timeout
triggers shutdown, joins the task, and returns `server_readiness_failed` (exit
1). Browser-open failure is non-fatal (print URL + warning). Both the opener and
the probe are injectable so `open` is deterministic in tests.

### Binding and ports

Default bind `127.0.0.1:7420`; default port increments through `7421..=7440` on
conflict; an **explicit** port fails immediately if occupied; port `0` yields an
OS-assigned ephemeral port. Bind-first, then report `local_addr()` — no pre-bind
probe race. Non-loopback binding requires an explicit `--host` and emits a
security warning; it never auto-enables CORS or source access.

### HTTP surface and headers

`GET/HEAD /api/{document,generation,diagnostics}` serve the **exact stored
canonical bytes** (no per-request re-serialization) with
`Content-Type: application/json; charset=utf-8` and `Cache-Control: no-store`.
`/api/health` returns `{status, schema_version, partial}` (partial-but-valid is
still `200`). Unknown `/api/*` → JSON `404`; wrong method → `405` with `Allow`.
Unknown non-API paths fall back to `index.html` (SPA). Every response carries one
named CSP constant — `default-src 'self'; script-src 'self'; style-src 'self';
base-uri 'self'; object-src 'none'; frame-ancestors 'none'` (no `unsafe-inline`)
— plus `X-Content-Type-Options: nosniff`, `Referrer-Policy: same-origin`,
`X-Frame-Options: DENY`. No permissive CORS. The CSP is the single PRD-07
extension point.

### Embedded assets

The prebuilt `web/dist` is **committed** and embedded via `rust-embed` with
`debug-embed`, so a missing `web/dist` fails the build in every profile and end
users never need Node.js. **Cargo never runs npm**: neither the build script nor
any crate invokes a package manager or a network. Contributors rebuild the
bundle explicitly with `npm run build` and commit the result; `check:dist`
guards it against drift. `index.html` is `no-cache`; only **fingerprinted**
filenames (a middle dot-separated segment of ≥8 hex chars, e.g.
`app.4f3a2b1c.js`) receive `immutable` long-term caching — which is why the Vite
config pins hex content hashes (`hashCharacters: "hex"`), as base64url hashes
would silently fall back to `no-cache`.

#### Build-correctness amendment (2026-07-16)

`crates/cratevista-server/build.rs` declares the embedded directory as a package
input:

```rust
fn main() {
    println!("cargo::rerun-if-changed=../../web/dist");
}
```

That is its entire contents: it runs no npm, mutates nothing, adds no build
dependency, generates no Rust, and introduces **no runtime watch behaviour** —
live reload remains PRD 09's concern.

It is required because `web/dist` was not otherwise a Cargo input, so
`npm run build && cargo build` could silently keep serving the previously
embedded UI. The subtlety worth recording: rust-embed's derive expands to
`include_bytes!`, whose paths rustc records in its dep-info, so **modifying** an
already-embedded file was already tracked. **Adding or removing** a file was
not — and that is exactly what a frontend rebuild does whenever a content hash
changes the emitted filenames. Directory-level tracking is what closes that gap.

`web/scripts/check-embed-rebuild.mjs` proves it, and its add-file probe is the
negative control: with `build.rs` removed, the modify probe still passes while
the add-file probe fails, serving the SPA fallback instead of the new asset.

### Source endpoint

`/api/source` is **off by default** (`403`, `source_disabled`). When enabled, a
single repo-relative `path` is validated by `RepoRelativePath` (rejecting
absolute / drive / UNC / `..`), resolved by **canonicalize-and-contain** under
the project root (so a symlink cannot escape → `source_outside_root`), required
to be a regular file (`source_not_file`) within a size limit
(`source_too_large`) and valid UTF-8 (`source_not_utf8`). No response includes
the resolved absolute path. This is a strong containment check, not a
perfectly-atomic TOCTOU-proof sandbox — stated honestly.

### Exit codes (serve/open)

Success/normal shutdown → 0; bind / snapshot / readiness / runtime failure → 1;
usage → 2; missing artifacts / missing nightly (open's generate step) /
`snapshot_integrity_unavailable` → 3. `serve`/`open` no longer return exit 4.

## Consequences

- A request never mixes generations, and a torn/corrupt/stale artifact set is
  detected and refused with an actionable, path-free diagnostic.
- The server is fully testable (handlers via `oneshot`, ports via ephemeral
  binds, lifecycle via the shutdown handle, readiness/opener via injection)
  without nightly, Node.js, or external network.
- PRD 09 can hot-swap the snapshot through `AppState` and reuse the shutdown
  handle without changing handlers; PRD 07 replaces the placeholder `web/dist`
  and may deliberately widen the single CSP constant.
