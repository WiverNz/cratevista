# CrateVista server — endpoints & security

`cargo cratevista serve` / `open` run a **loopback** HTTP server that serves an
existing generated snapshot (`target/cratevista/{document,generation,diagnostics}.json`)
and the embedded web UI. Design rationale is in
[`docs/adr/0006-server-and-security.md`](adr/0006-server-and-security.md).

## Endpoints

| Method    | Path               | Response                                                        |
|-----------|--------------------|----------------------------------------------------------------|
| GET, HEAD | `/api/document`    | Exact `document.json` bytes (`application/json`, `no-store`)    |
| GET, HEAD | `/api/generation`  | Exact `generation.json` bytes                                  |
| GET, HEAD | `/api/diagnostics` | Exact `diagnostics.json` bytes                                 |
| GET, HEAD | `/api/health`      | `{ "status": "ok", "schema_version": "1.1", "partial": false }`|
| GET       | `/api/source`      | Guarded source-file contents (off by default → `403`)          |
| GET       | `/*` (non-API)     | Embedded asset, or `index.html` fallback (SPA)                 |

`/api/health` reports the **loaded snapshot's** `schema_version`, not the
server's own — so an artifact set produced by an older generator reports the
version it was written with (e.g. `1.0`). The server accepts any minor within
the supported **major**; see "Schema versions" below.

- The JSON API returns the **exact stored canonical bytes** (no per-request
  re-serialization).
- Unknown `/api/*` → JSON `404` `{ "error": { "code": "not_found" } }`.
- A wrong method on a known route → `405` with an `Allow` header.
- `/api/health` stays `200` even when the document is partial (`partial: true`).
- `/api/events` (SSE) exists **only when the server is watching** (`open --watch`);
  an ordinary `serve` has `watch_enabled: false` and the route is not registered,
  so it answers with the usual unknown-`/api/*` `404`. When present it emits only
  `generation-started`, `generation-succeeded` and `generation-failed`, never an
  `id:` (replay is impossible by construction), and carries no path. All three
  artifact endpoints carry `X-CrateVista-Snapshot` so the browser can detect and
  retry a triple that spans a mid-load snapshot swap (PRD 09).

## Snapshot integrity

The three artifacts are committed by per-file rename with `generation.json` last
and are not one crash-atomic transaction. The server loads a **hash-verified**
snapshot: it requires the `generation.json` marker to be stable across the read
**and** the loaded `document.json` / `diagnostics.json` bytes to match the BLAKE3
`artifact_hashes` embedded in `generation.json`. It never serves a torn or stale
set. Startup failures (all path-free) and their exit codes:

| Code                            | Meaning                                                       | Exit |
|---------------------------------|--------------------------------------------------------------|------|
| `artifacts_missing`             | One or more artifacts absent → run `generate`                | 3    |
| `snapshot_integrity_unavailable`| Pre-amendment `generation.json` (no hashes) → regenerate     | 3    |
| `invalid_artifact_hash`         | A digest is not 64 lowercase-hex chars                       | 1    |
| `snapshot_hash_mismatch`        | Hashes never matched the loaded bytes (torn/corrupt set)     | 1    |
| `artifact_changed_during_read`  | A generation kept landing mid-read                           | 1    |
| `malformed_document/generation/diagnostics` | Invalid JSON                                     | 1    |
| `invalid_document`              | Referential-integrity validation failed                      | 1    |
| `schema_version_unsupported`    | Unsupported `document`/`diagnostics` major                   | 1    |
| `schema_version_mismatch`       | `document` and `diagnostics` versions differ                 | 1    |

## Schema versions

`document.json` and `diagnostics.json` carry a `MAJOR.MINOR` `schema_version`
(currently **`1.1`**). The rules:

- **The major is the compatibility boundary.** The server accepts any `1.x`
  artifact and rejects other majors with `schema_version_unsupported`. Minor
  bumps are additive by policy (ADR-0003), so an older `1.0` snapshot keeps
  loading and serving unchanged.
- **`document` and `diagnostics` must carry the same exact version**, or the set
  is rejected with `schema_version_mismatch`. A generator run always writes a
  matching pair; a mismatch means the artifacts came from different runs.
- `generation.json` carries **no** `schema_version` — its integrity is covered by
  the embedded `artifact_hashes` instead.
- `1.1` added the optional `View.docs` / `View.examples` fields (PRD-08
  Amendment A). Regenerate with `cargo cratevista generate` to move an existing
  snapshot to the current version; nothing forces you to.

## Security

- **Loopback by default** (`127.0.0.1`). A non-loopback bind requires an explicit
  `--host` and prints a warning; it never auto-enables CORS or source access.
- **No permissive CORS**; the UI is same-origin.
- Every response carries a strict **Content-Security-Policy**:

  ```text
  default-src 'self'; script-src 'self'; style-src 'self';
  style-src-attr 'unsafe-inline'; connect-src 'self'; worker-src 'self';
  base-uri 'self'; object-src 'none'; frame-ancestors 'none'
  ```

  plus `X-Content-Type-Options: nosniff`, `Referrer-Policy: same-origin`,
  `X-Frame-Options: DENY`. There is exactly **one** `unsafe-inline` token and it
  belongs to `style-src-attr` — React Flow writes node geometry to the inline
  `style` attribute. `script-src`/`style-src` themselves stay `'self'`: no
  inline scripts, no `unsafe-eval`, no remote origins, and the layout worker is
  a same-origin asset (`worker-src 'self'`), never a `blob:`. This is asserted
  against the live server in the browser test suite.
- No response exposes an absolute filesystem path, username, environment, Cargo
  home, or raw rustdoc/metadata.

## Build and packaging

- The web UI is **committed** at `web/dist` and embedded into the binary at
  compile time (`rust-embed`, `debug-embed` — a missing `web/dist` fails the
  build in every profile). **End users never need Node.js.**
- **Cargo never invokes npm.** No crate and no build script runs a package
  manager, a network fetch, or a code generator. Contributors who change the
  frontend rebuild it explicitly with `npm run build` and commit `web/dist`;
  `npm run check:dist` fails if the committed bundle is stale.
- `crates/cratevista-server/build.rs` contains only:

  ```rust
  fn main() {
      println!("cargo::rerun-if-changed=../../web/dist");
  }
  ```

  This makes Cargo's incremental rebuild correct: without it, `cargo build`
  after a frontend rebuild could keep serving the previously embedded UI.
  rust-embed's `include_bytes!` expansion already caused rustc to track
  *modifications* to embedded files, but **not additions or removals** — which
  is exactly what a rebuild produces when content hashes change the emitted
  filenames. Live reload itself (`open --watch` → `/api/events` → the browser's
  coherent refetch) is delivered by PRD 09; this embedding guarantee is what makes
  a freshly built UI actually served. `npm run check:embed-rebuild` proves the chain.
- Only **fingerprinted** assets (a middle dot-separated segment of ≥8 hex
  characters, e.g. `index.936d2e17.js`) are served `immutable`; `index.html` is
  always `no-cache`.

## Source endpoint (`/api/source`)

Disabled by default (`403 source_disabled`). Enable with `--source`. When enabled,
one repo-relative `path` query is accepted and validated:

- rejects absolute / drive-letter / UNC paths and any `..` (`source_path_invalid`);
- canonicalizes the candidate and requires containment under the project root, so
  a symlink cannot escape (`source_outside_root`);
- requires a regular file (`source_not_file`) within a size limit
  (`source_too_large`) that is valid UTF-8 (`source_not_utf8`).

No response includes the resolved absolute path. There is no directory listing and
no line-range support. This is a strong containment check, **not** a
perfectly-atomic TOCTOU-proof sandbox.

## Ports

Default `7420`; without an explicit `--port` the server increments through
`7421..=7440` on conflict. An explicit `--port` fails immediately if occupied.
`--port 0` requests an OS-assigned ephemeral port. The server binds first, then
prints the actual URL.
