# CrateVista web UI

The interactive architecture explorer: a React + TypeScript + Vite single-page
app, served by `cargo cratevista serve`/`open` from a bundle **embedded in the
Rust binary**.

**End users never need Node.js.** Node is required only to *rebuild* this UI.

## Requirements

- **Node 24** and npm 11 (`engines` pins `node >= 24`).
- No global installs. Everything runs from `node_modules`.

### TypeScript: 7 compiler, 6 compatibility API

The authoritative type-check uses **stable TypeScript 7**; typescript-eslint
still needs TypeScript 6's programmatic API, so both are installed side by side
following the official transition model:

| package      | resolves to                | used by                              |
| ------------ | -------------------------- | ------------------------------------ |
| `typescript` | `@typescript/typescript6`  | typescript-eslint, `npm run typecheck:compat` (`tsc6`) |
| `typescript-7` | `typescript@7.0.2`       | `npm run typecheck` (authoritative)  |

No `--legacy-peer-deps`, no `--force`, no peer overrides. `npm run typecheck`
invokes the TS7 binary by explicit path, because the compat package bundles its
own `tsc` which would otherwise win the bare-name collision.

## Everyday commands

```bash
npm ci                    # exact, lockfile-pinned install
npm run dev               # Vite dev server (hot reload, not the embedded bundle)
npm run build             # production bundle -> web/dist
npm run test              # unit + component tests (Vitest, jsdom)
npm run lint              # ESLint (flat config)
npm run typecheck         # TypeScript 7 (authoritative)
npm run typecheck:compat  # TypeScript 6 compatibility API
```

## The committed bundle

`web/dist` is **checked in**. `cratevista-server` embeds it at compile time, so
the installed binary is self-contained.

**Cargo never runs npm.** If you change anything under `web/src`, rebuild and
commit the bundle yourself:

```bash
npm run build
git add web/dist
```

Two guards keep that honest:

```bash
npm run check:dist           # committed dist == a fresh build? (byte-exact)
npm run check:embed-rebuild  # does Cargo re-embed dist when it changes?
```

`check:dist` builds into an isolated temp directory and compares recursively,
byte for byte, without touching the committed baseline and without using Git.

`check:embed-rebuild` guards the build-correctness amendment
(`crates/cratevista-server/build.rs`, which only emits
`rerun-if-changed=../../web/dist`). It modifies an asset, rebuilds *without*
`cargo clean`, and asserts the **served bytes** changed; then it adds a new asset
and asserts it is served. The add-file probe is the one that matters: rust-embed's
`include_bytes!` expansion already tracks modifications, but not additions —
which is what a rebuild produces whenever a content hash changes a filename. It
restores the checkout even if an assertion fails.

```bash
npm run check:types          # committed generated types == a fresh generation?
npm run generate:types       # regenerate them from the JSON Schema
```

## Browser tests

```bash
npm run e2e                  # Playwright, real server + real embedded bundle
npx playwright install chromium   # first time only
```

The suite runs the **actual** `cargo-cratevista serve` binary on an ephemeral
loopback port against committed snapshots, and mocks nothing: real same-origin
APIs, real CSP headers, real ELK worker. Unit tests may mock; Playwright may not.

Build ordering matters — the binary embeds the dist at compile time:

```bash
npm run check:dist && npm run build
cargo build -p cargo-cratevista   # AFTER the build above
npm run e2e
```

The suite also byte-compares every served asset against `web/dist`, so a stale
binary fails loudly rather than silently testing an old UI.

## Fixtures

Committed, real generated snapshots under `e2e/fixtures/` — running the tests
needs **no nightly toolchain**. Refreshing them does (pinned
`nightly-2026-07-01`):

```bash
npm run gen:benchmark-workspaces   # regenerate the benchmark Rust sources
npm run refresh:e2e-snapshots      # GATED: regenerates every snapshot
npm run refresh:fixtures           # GATED: the all_views component fixture
```

Snapshots are never hand-edited: `generation.json` embeds BLAKE3 digests over
the exact bytes of `document.json`/`diagnostics.json`, so the refresh pipeline
re-commits them through the production writer, which recomputes the digests.
`cargo test -p cratevista-server --test e2e_fixtures` validates every committed
fixture against the real loader.

## Benchmark

```bash
npm run benchmark   # writes docs/benchmarks/prd-07-large-graph.json
```

Runs the production bundle in the pinned Chromium across the sample and the
three `bench-*` fixtures. Timings come from the app's own local instrumentation
(`src/app/perf.ts`, User Timing API) — in-memory only, no telemetry, no network,
no paths. The narrative report and the node-budget decision live in
`docs/benchmarks/prd-07-large-graph.md`.

## Constraints worth knowing

- **CSP.** The server sends `default-src 'self'; script-src 'self'; style-src
  'self'; style-src-attr 'unsafe-inline'; connect-src 'self'; worker-src 'self';
  base-uri 'self'; object-src 'none'; frame-ancestors 'none'`. The single
  `unsafe-inline` is for React Flow's inline `style` attribute. Do not add inline
  scripts, `eval`, remote origins, or CDN assets — everything is self-hosted.
- **The layout worker** is a same-origin ES module asset, never a `blob:` URL.
  It imports `elkjs/lib/elk-api.js` plus elkjs's own worker; importing
  `elk.bundled.js` here silently breaks in the browser (its Node variant
  hijacks `self.onmessage` and its worker factory resolves to `undefined`).
- **Asset hashes must be hex** (`hashCharacters: "hex"` in `vite.config.ts`).
  The server only grants `immutable` caching to filenames with a ≥8 hex-char
  segment; Vite's default base64url hashes would silently degrade to `no-cache`.
- **No runtime CSS-in-JS**, no Redux, no styled-components/Emotion.
