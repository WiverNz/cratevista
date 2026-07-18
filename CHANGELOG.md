# Changelog

All notable changes to this project are documented here. The format is based on
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and this project
adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

<!-- At release, the maintainer transforms this heading to `## [0.1.0] - YYYY-MM-DD`
     and opens a fresh empty [Unreleased] above it (see docs/launch-checklist.md).
     0.1.0 has NOT been published yet. -->

### Added

- **`cargo cratevista build` — a self-contained static site** (PRD 10). Produces a
  portable directory (`index.html`, fingerprinted `assets/**`, and the three JSON
  artifacts) that any static HTTP host serves with **no running Rust server and no
  Node.js**, from a URL root or an arbitrary subpath (relative URLs + query-string
  routing; optional `--base-path` for an absolute `<base href>`). A produced site
  enters static mode via a CSP-safe `<meta name="cratevista-mode">`, opens **no**
  `EventSource`, and makes **zero** `/api/**` requests. It contains repository
  **links** (root always; per-file deep links only with an authoritative
  `default_branch`) but **no** copied source snippets and **no** absolute paths.
  `build` owns only directories it created (marker-first/finalize-last, transactional
  publish with rollback, per-output advisory lock, key-scoped recovery). `file://` is
  not supported.
- **crates.io publishability** (PRD 10). Internal dependency edges carry registry
  `version`s; the embedded frontend bundle lives inside `cratevista-server`
  (`embedded/`); `cargo-cratevista` ships a crate-local `README.md` and
  byte-identical `LICENSE-MIT`/`LICENSE-APACHE` copies. The nine crates install from
  their `.crate` files alone — **no Node, no workspace checkout, no network** —
  verified offline via a Cargo `local-registry` on Linux/macOS/Windows.
- **Release plumbing** (PRD 10). A tag-triggered `release.yml` builds four targets
  (`x86_64-unknown-linux-gnu`, `aarch64-apple-darwin`, `x86_64-apple-darwin`,
  `x86_64-pc-windows-msvc`), assembling checksummed (SHA-256) archives with one
  shared, deterministic cross-platform helper, and uploads them to the GitHub
  Release. A separate, manual, protected `publish.yml` (`workflow_dispatch` +
  `release` environment) runs `cargo publish` in dependency order. No signing/SLSA in
  the first release (deferred); binaries are **not** claimed byte-reproducible.
- **Dependency-licence reports** (PRD 10). Deterministic, path/timestamp-free reports
  for the Rust graph (pinned `cargo-about`, `about.toml` policy) and the web graph
  (a repository-owned Node script over `package-lock.json`), under `docs/licenses/`,
  with CI drift + policy checks.
- **Public documentation** (PRD 10): an expanded README (differentiation, install,
  first run, commands, supported inputs, stable-vs-nightly tuple, manual-flow and
  static-build examples, privacy, known limitations), `docs/hosting.md`,
  `docs/launch-checklist.md`, an expanded `SECURITY.md` static-site privacy/threat
  model, `docs/adr/0009-static-build-and-release.md`, and the
  `ISSUES/issue_13_static_source_snippets.md` specification shell for the deferred
  source-snippets follow-up.

- **The interactive explorer UI** (PRD 07). `cargo cratevista open` now opens a
  real architecture map instead of a placeholder: a React Flow graph canvas with
  an inspector, built from the generated document and shipped **prebuilt inside
  the binary** — end users still need no Node.js.
  - **All eight generated views** — workspace overview, crate dependencies,
    module hierarchy, types, traits and impls, type relationships, public API and
    documentation coverage — rendered from real generated documents, not
    hand-written fixtures.
  - **Search, filters and inspector.** Search by label or fully-qualified name;
    filter by entity kind with a legend that reflects only what is on screen;
    inspect an entity or a relation (docs, attributes, relations, diagnostics),
    with Markdown rendered through a sanitising pipeline.
  - **Shareable URL state.** `view`, `entity`/`relation`, `q`, `kinds`, `focus`,
    `edges` and `stage` live in the URL; Back/Forward restore durable state, and
    stale ids degrade gracefully instead of erroring.
  - **Source action**, off by default. The inspector shows an item's
    repo-relative location and only fetches contents when the server is started
    with `--source`, degrading honestly when it is not.
  - **ELK layout in a same-origin module Web Worker.** Layout runs off the main
    thread; results are tokened so stale layouts are discarded, and selection or
    inspector changes never trigger a relayout.
  - **Reduced mode for large graphs.** Above a configurable visible-node budget
    (default 1,500) the canvas renders a bounded neighbourhood around a focus and
    states how many of how many nodes are shown. Nothing is silently dropped: a
    complete, searchable, keyboard-navigable entity list remains available, and
    *Render full graph* is one click away. See
    [`docs/benchmarks/prd-07-large-graph.md`](docs/benchmarks/prd-07-large-graph.md).
  - **Accessibility baseline** — WCAG 2.1 AA *application baseline*, not
    certification: keyboard-operable tabs/search/entity list, visible focus,
    Escape handling, reduced-motion support, and state conveyed by text as well
    as colour. See [`docs/accessibility.md`](docs/accessibility.md).
  - **The production bundle is committed** at `web/dist` and embedded via
    `rust-embed`; `npm run check:dist` fails if it is stale.
  - **Real-server browser verification.** Playwright drives the actual
    `cargo cratevista serve` binary on a loopback port against the real embedded
    bundle, real same-origin APIs, real CSP headers and the real ELK worker —
    nothing mocked.

### Changed

- **Content-Security-Policy amended** (additively) for React Flow: the server now
  sends `style-src-attr 'unsafe-inline'` alongside `connect-src 'self'` and
  `worker-src 'self'`. There is exactly one `unsafe-inline` token and it belongs
  to `style-src-attr` — `script-src`/`style-src` remain `'self'`, with no
  `unsafe-eval`, no remote origins and no `blob:` worker.
- **Frontend toolchain**: stable **TypeScript 7** is the authoritative
  type-checker, with **TypeScript 6's compatibility API** installed side by side
  for typescript-eslint (no `--legacy-peer-deps`, no `--force`).

### Fixed

- **Build correctness for the embedded UI** (a PRD-06 amendment found while
  verifying PRD 07). `cratevista-server` embeds `web/dist` at compile time, but
  the directory was not a Cargo package input, so `npm run build && cargo build`
  could silently keep serving the previously embedded UI.
  `crates/cratevista-server/build.rs` now declares
  `rerun-if-changed=../../web/dist` — and nothing else. Cargo still never invokes
  npm, and no runtime watch behaviour is introduced.

- Adopted the pre-1.0 **latest-stable Rust policy** (ADR-0010): CrateVista now
  tracks the latest stable release. **MSRV raised to the Rust 1.97 line**
  (`rust-version = "1.97.1"`, `rust-toolchain.toml` pinned to `1.97.1`),
  superseding the earlier Rust 1.85 decision and the proposed 1.86 bump.
- Bumped the pinned stable toolchain **`1.97.0` → `1.97.1`** (2026-07-17): Rust
  1.97.1 fixes an LLVM miscompilation and supersedes the 1.97.0 pin under ADR-0010.
  Toolchain-only maintenance — no dependency was upgraded, and the rustdoc
  compatibility tuple (`nightly-2026-07-01` → `format_version 60` →
  `rustdoc-types 0.60.0`) is unchanged.
- Pinned `cargo_metadata` to `0.23` (workspace dependency), superseding the
  earlier `0.19`/`0.20` proposals tied to the old MSRV.
- Added `rustdoc-types = "0.60"` (resolved `0.60.0`) to `[workspace.dependencies]`
  for the rustdoc JSON adapter; it builds on stable Rust 1.97.1.

### Added

- `cratevista-server` + `cargo cratevista serve` / `open` are implemented: a
  **loopback** axum/tokio server that serves an **existing** artifact snapshot —
  `GET/HEAD /api/{document,generation,diagnostics,health}` (exact stored canonical
  bytes) and a guarded, off-by-default `/api/source` — plus the prebuilt SPA
  embedded via `rust-embed` (checked-in placeholder `web/dist`; no Node.js for
  end users). The snapshot loader is **hash-verified**: it requires the
  `generation.json` marker to be stable **and** the loaded `document.json` /
  `diagnostics.json` bytes to match the BLAKE3 `artifact_hashes` embedded in
  `generation.json`, with bounded retry, dual/cross-artifact schema-version
  validation, and stable codes (`snapshot_integrity_unavailable`,
  `invalid_artifact_hash`, `snapshot_hash_mismatch`, `schema_version_mismatch`, …);
  it never publishes a torn snapshot and never leaks a filesystem path. `serve`
  serves existing artifacts (missing → exit 3; no nightly, no network); `open` =
  generate + serve + a bounded loopback `/api/health` **readiness probe** + browser
  open (only after `200`; open failure is non-fatal; readiness timeout →
  `server_readiness_failed`, exit 1). Bind is loopback-first (default `7420`,
  increment `7421..=7440`, explicit-conflict fails, `0` = ephemeral); responses
  carry a strict CSP (no `unsafe-inline`) + `nosniff`/`DENY`/`same-origin` and no
  permissive CORS. `serve`/`open` no longer return exit 4. New flags: `serve`
  `--host`/`--port`/`--source`; `open` the `generate` flags plus
  `--host`/`--port`/`--source`. Adds `axum`, `tokio`, `tower`/`tower-http`,
  `rust-embed`, `mime_guess`, `arc-swap`, `opener`, and `docs/adr/0006-server-and-security.md`.
- `cratevista-schema`: added the additive optional `GenerationReport.artifact_hashes`
  (`ArtifactHashes { document_blake3, diagnostics_blake3 }`) — BLAKE3 of the exact
  canonical `document.json` / `diagnostics.json` bytes, lowercase hex, 64 chars.
  The `cratevista-core` artifact writer now computes and embeds these digests
  (`generation.json` committed last still carries them); `blake3` is added to
  `cratevista-core`. Backward-compatible: a pre-amendment `generation.json`
  without the field still deserializes, but the server refuses it with
  `snapshot_integrity_unavailable`. `SchemaVersion` is unchanged (`1.0`): the
  field lives on the unversioned `generation.json`, and neither versioned artifact
  changed.
- `cratevista-graph`: the **pure** graph builder. `build_rustdoc_plan` selects
  documentable metadata targets (lib + proc-macro; bins opt-in) into a
  `RustdocPlan`, and `build_document(GraphInput, &GraphBuildOptions)` merges
  `MetadataIngest` + `RustdocIngest` + an optional `GraphOverlay` into one
  deterministic `ExplorerDocument` — cross-source linking by `CrateSummary`
  identities, field-level merge by semantic ownership, exact structured
  cross-crate resolution (no fuzzy matching, no `references_type`), documentation
  coverage, the eight default filter-based views, and schema validation. Returns
  pure Rust values only (no JSON/clock/filesystem); depends on
  schema/metadata/rustdoc, never on core/CLI/server/config/watch.
- `cargo cratevista generate` is implemented: `cratevista-core::run_generate`
  orchestrates metadata → plan → rustdoc → graph and writes
  `target/cratevista/{document,generation,diagnostics}.json` via canonical
  serialization and a **prepare-then-commit** write (`generation.json` last as the
  completion marker). An empty default plan yields a metadata-only success
  (`no_documentable_rustdoc_targets`, exit 0). New flags: `--keep-going`,
  `--features`, `--all-features`, `--no-default-features`,
  `--document-private-items`, `--toolchain`, `--external-deps <exclude|direct|full>`,
  `--document-bins`. `generate` no longer returns exit code 4. Adds
  `docs/adr/0005-relation-reliability.md` and a `time` workspace dependency for
  the injected generation-timestamp clock.
- Cargo workspace bootstrap (edition 2024, dual `MIT OR Apache-2.0`).
- `cargo-cratevista` binary usable as the `cargo cratevista` external subcommand,
  with global options (`--manifest-path`, `-v/--verbose`, `-q/--quiet`,
  `--color`, `--format`) and a documented exit-code policy.
- `cratevista-core` orchestration/use-case layer with runtime scaffolding
  (diagnostics, exit codes, logging, process paths).
- Implemented `cargo cratevista init` (idempotent, non-overwriting) and
  `cargo cratevista doctor` (read-only prerequisite checks).
- Stub `serve`, `open`, and `build` commands that report "not implemented yet"
  (exit code 4). (`generate`, `serve`, and `open` are now implemented — see above;
  only `build` remains a stub.)
- Placeholder crate `cratevista-server` locking its name and the workspace graph.
  (Now implemented — see above.)
- `cratevista-rustdoc`: rustdoc JSON adapter — `ingest`/`normalize_json`/
  `load_and_normalize` execute a pre-resolved `RustdocPlan` (validated, but never
  package enumeration), invoke `cargo rustdoc … --output-format json` under a
  separately-pinned nightly, deserialize via `rustdoc-types` behind a
  format-version gate, and map items into deterministic schema entities/relations
  (`contains`/`implements`/`implemented_for`/`has_field_type`/`accepts_type`/
  `returns_type`/`error_type`/`re_exports`) plus `DocumentDiagnostic`s and a thin
  `CrateSummary` of unresolved cross-crate references. Raw `rustdoc-types` never
  appear in the public API; no absolute paths enter the output; default-fail with
  `--keep-going` partial mode; a cache-key computation for issue 09. Adds the
  **verified compatibility tuple** (nightly `nightly-2026-07-01` → format `60`,
  `rustdoc-types 0.60`, adapter `1`) and `docs/adr/0004-rustdoc-toolchain-policy.md`.
  A **PRD-05 bridge contract** carries stable schema identities for deterministic
  linking (`RustdocTarget`/`CrateSummary` gain `package_id`/`target_id`, plus
  `crate_name` and `root_module_id`; `TargetOutcome` gains `target_id`; plan
  validation rejects duplicate `target_id`) and replaces the display-only
  cross-crate hint with structured `UnresolvedTypeRef` evidence
  (`role: TypeReferenceRole`, `crate_name`/`canonical_path`/`item_kind` from
  rustdoc's `ItemSummary`/path map).
- `cratevista-metadata`: Cargo workspace metadata ingestion — `ingest`/`normalize`
  over `cargo metadata --format-version 1`, producing deterministic schema
  entities (workspace/packages/targets) and relations (`contains`, `depends_on`
  with per-role edges + BLAKE3 cfg discriminators) plus `DocumentDiagnostic`s;
  configurable selection, features, external-dependency modes, target kinds, and
  network mode; portable identities that never expose absolute paths.
- `cratevista-schema`: the canonical explorer document model — `ExplorerDocument`
  (`document.json`), `GenerationReport` (`generation.json`), and
  `DiagnosticsReport`/`DocumentDiagnostic` (`diagnostics.json`); open string-backed
  entity/relation kinds; stable ID newtypes with BLAKE3 semantic discriminators;
  validated repository-relative `SourceLocation`; a single canonical JSON
  serializer; and a schemars-generated, checked-in JSON Schema guarded by a drift
  test.
- Project docs (README, CONTRIBUTING, SECURITY), ADRs 0001–0003 and 0010, and CI.
