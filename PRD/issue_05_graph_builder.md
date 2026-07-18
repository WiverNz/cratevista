# PRD — Build the semantic graph and generated views

## Status

**Implemented / Verified** (2026-07-14). `cratevista-graph` (pure builder + `build_rustdoc_plan`) and the `cratevista-core` `run_generate` orchestration + prepare/commit artifact writing + `generate` CLI wiring are implemented exactly to the approved design below.

> **Follow-up amendment (documented 2026-07-14; implemented with PRD 06):** PRD 05 remains **Implemented / Verified**; the artifact writer gained one additive change so PRD 06 can load a *consistent* snapshot. `generation.json`-byte equality before/after a read does **not** by itself prove the `document.json`/`diagnostics.json` belong to that generation (a torn commit can leave an old `generation.json` observable both before and after the newer siblings are renamed). The fix: `cratevista-core::artifacts` computes **BLAKE3** over the exact canonical bytes of `document.json`/`diagnostics.json` and the writer embeds them in `generation.json` via the new optional `GenerationReport.artifact_hashes` field (PRD-02 additive amendment). This reorders the writer (build+validate → serialize document/diagnostics → hash → build `GenerationReport` with hashes → serialize `generation.json` → commit document/diagnostics/generation-last) and adds a `blake3` dependency to `cratevista-core`. Each digest is **lowercase hex, exactly 64 ASCII characters, no `0x` prefix, no whitespace** (PRD-02 encoding contract), computed over the **same canonical bytes the writer commits to disk**. It is an **additive, backward-compatible schema amendment; no breaking schema change**; the existing acceptance evidence stands, and the writer/`generate` **fixtures and tests** were updated so that every produced `generation.json` carries correct `artifact_hashes`. This was **delivered as Phase 0 of PRD 06** (now implemented and verified). See "Artifact commit semantics" for the sequence.

- **Gates:** `cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets --all-features -- -D warnings`, `cargo test --workspace --all-features`, and `cargo +1.97.0 check --workspace --all-features` pass on stable 1.97.0 (no nightly, no network for normal tests).
- **Dependency boundaries:** `cargo tree -p cratevista-graph -i {core, cargo-cratevista, server, config, watch}` report no path; `cargo tree -p {metadata, rustdoc} -i cratevista-graph` report no path.
- **Behavior verified:** metadata-only + merge + linking + resolution + coverage + overlay + views + validation are unit/integration-tested (stable); a **metadata-only bin-workspace** `run_generate` integration test asserts exit 0, a valid + deterministic `document.json`, separate `diagnostics.json` with `no_documentable_rustdoc_targets`, `generation.json.partial = false`, and no absolute paths; artifact prepare/commit and its failure-preservation are tested; a **gated `#[ignore]` live `generate`** runs the full metadata → plan → rustdoc (nightly `nightly-2026-07-01`) → graph → artifacts pipeline on a path-only lib crate and asserts the linked document. An **`#[ignore]` engineering benchmark** assembles ~20k entities / ~50k relations (indexed, near-linear).

Grounded in the **implemented** APIs of PRDs 01–04. **All material decisions are resolved** and implemented: the public API shapes (`GraphInput`, `GraphBuildOptions`, `GraphBuildResult`, `GraphError`, `GraphOverlay`), the pure `build_rustdoc_plan` API, cross-source linking, the cross-crate resolution algorithm, entity/relation merge and conflict policy, default views/stages, the overlay seam, empty-plan/partial/exit behavior, the artifact commit process, the external-dependency CLI (`--external-deps <exclude|direct|full>`), the cache stance (no `--no-cache` in PRD 05), and the performance benchmark. Minor repository-style names may differ from the sketches below; semantics and dependency boundaries are unchanged. One implementation note: the metadata workspace entity's `qualified_name` holds the absolute workspace path at runtime (a PRD-03 quirk), so the graph sanitizes it to the workspace label when assembling the public artifact — keeping absolute paths out of `document.json`.

The three former data blockers were resolved by the implemented **PRD-04 bridge amendment** (2026-07-13) and are recorded as history under "Resolved-decision history":

1. **Absolute workspace root** — runtime orchestration context resolved by `cratevista-core` and passed to `build_rustdoc_plan`; never added to `MetadataIngest`/`MetadataSummary`/`ExplorerDocument`/`GenerationReport`/public ids.
2. **Rustdoc-crate ↔ Cargo-target join** — `RustdocTarget`/`CrateSummary` carry `package_id`/`target_id` (+ `crate_name`/`root_module_id`); linking is by `target_id`. `PlannedTargetLink` is not used.
3. **Cross-crate resolution** — `UnresolvedTypeRef` carries structured `crate_name`/`canonical_path`/`item_kind`; resolution is exact and deterministic.

Aligned with the approved cross-cutting decisions: the `document.json` / `generation.json` / `diagnostics.json` split; deterministic-by-default `document.json` (no timestamps); open string-backed kinds; the `target/cratevista/` layout; `cratevista-core` as the orchestration layer; `--keep-going` partial status.

## Source issue

`ISSUES/issue_05_graph_builder.md`

## Summary

`cratevista-graph` is the **pure document-assembly** layer: it (a) builds an explicit `RustdocPlan` from `MetadataIngest`, and (b) merges `MetadataIngest` (PRD 03) and `RustdocIngest` (PRD 04) — plus an optional in-memory `GraphOverlay` (the issue-08 seam) — into one **deterministic** `cratevista_schema::ExplorerDocument` with the MVP views as projections, returning a `GraphBuildResult` of pure Rust values (document + diagnostics + summary + `partial`). It does **not** invoke Cargo/rustdoc, read the clock, touch the filesystem, or render CLI output.

`cratevista-core` provides the `run_generate` use case: it resolves paths, runs `cratevista_metadata::ingest`, calls `cratevista_graph::plan::build_rustdoc_plan`, runs `cratevista_rustdoc::ingest` (skipped for an empty plan), calls `cratevista_graph::build_document`, assembles the runtime `GenerationReport`/`DiagnosticsReport`, and **commits** `target/cratevista/{document.json, generation.json, diagnostics.json}` via the schema canonical serializer using a prepare-then-commit write (per-file atomic rename where supported, `generation.json` last as the completion marker — see "Artifact commit semantics"). `cargo cratevista generate` dispatches to it; it replaced the exit-code-4 stub.

## Problem statement

Metadata and rustdoc produce disjoint, partial inputs keyed by different identity schemes (`package:`/`target:` vs `module:`/`item:`/`impl:`). We need a single canonical, schema-valid document with stable identity, cross-source links, resolved relations *without invented edges*, deduplicated re-exports, byte-stable output, and view projections — the data contract for the entire frontend — assembled by a pure, deterministic, unit-testable function.

## Goals

- Build an explicit, pure `RustdocPlan` from `MetadataIngest` (no Cargo/rustdoc invocation).
- Merge metadata + rustdoc entities/relations into one valid `ExplorerDocument`; every relation references existing entities.
- Link the two identity schemes: workspace → package → target → rustdoc root module → items.
- Emit only **reliable** typed relations; preserve unresolved cross-crate references as diagnostics (never invented edges); `references_type` stays deferred (PRD 04).
- Build the MVP views as filtered projections over the same document (no coordinates).
- `document.json` byte-stable by default (no timestamps); runtime metadata → `generation.json`; diagnostics → `diagnostics.json`.
- Provide a stable `GraphOverlay` extension seam without `cratevista-graph` itself depending on `cratevista-config`. *(Still true: PRD 08 landed `cratevista-config`, and the direction is `config → graph`, never the reverse — `cratevista-core` wires them together. The graph knows nothing about TOML.)* *(Extended additively on 2026-07-16 with `EntityOverride::docs`; see "### Additive amendment (2026-07-16)".)*
- Wire `run_generate` end-to-end with a clean exit-code policy and injected clock (deterministic graph tests).

## Non-goals

- Invoking cargo/rustdoc (PRDs 03/04); reading the clock or filesystem inside `cratevista-graph`.
- UI coordinates/layout, viewport, or component names (PRD 07 via ELK).
- TOML manual-flow parsing (PRD 08); this crate defines and applies the `GraphOverlay` model only.
- Watch mode / persistent cache orchestration (PRD 09).
- Server / static-site behavior (PRDs 06/10).
- Approximate `references_type` edges (PRD 04 deferred them).

## Current repository state (implemented)

Verified against the real code (this PRD is implemented; the schema/metadata/rustdoc bullets describe the consumed foundation APIs):

- **`cratevista-graph`** is **implemented** (`#![forbid(unsafe_code)]`). Modules: `lib` (public API `build_document` + re-exports), `input` (`GraphInput`, `GraphBuildOptions`, `GraphOverlay`, `EntityOverride`), `plan` (`build_rustdoc_plan`, `RustdocPlanOptions`), `merge` (entity/relation merge), `link` (cross-source linking), `resolve` (cross-crate resolution), `views` (the eight default views), `coverage` (documentation coverage), `overlay` (overlay application), `diagnostics` (stable codes + builders), `validate` (dangling-drop + schema validation), `result` (`GraphBuildResult`, `GraphBuildSummary`), `error` (`GraphError`), and `test_support` (`#[cfg(test)]` fixtures). It **depends only on `cratevista-schema`, `cratevista-metadata`, `cratevista-rustdoc`, and `serde_json`** (dev: nothing extra) — never on core/CLI/server/config/watch.
- **`cratevista-core`** now has `generate.rs` (the implemented `run_generate` orchestration + `GenerateOptions`/`ExternalDepsChoice`), `artifacts.rs` (canonical serialization + prepare-then-commit write), and `clock.rs` (`Clock`/`SystemClock`/`FixedClock`), alongside the existing `usecase`/`diagnostic`/`exit`/`paths`/`logging`/`error`. `run_generate` is implemented and **no longer returns exit code 4** (the stub is gone). Core **depends on `cratevista-schema`, `cratevista-metadata`, `cratevista-rustdoc`, `cratevista-graph`, `cratevista-config`, `cratevista-server`, `time`, `blake3`, `tokio`, and `opener`** (+ serde/serde_json/thiserror/tracing) — `cratevista-server`, `tokio`, and `opener` are the implemented-PRD-06 `serve`/`open` additions; `blake3` computes the writer's embedded `artifact_hashes`; and **`cratevista-config` is the implemented-PRD-08 addition** (`config_diagnostics.rs` converts its diagnostics), which core calls **before** `build_document` so the resulting `GraphOverlay` is an input to it. (`cratevista-graph` itself still depends only on `cratevista-schema`, `cratevista-metadata`, `cratevista-rustdoc`, and `serde_json` — see the graph bullet above. The arrow runs `cratevista-config → cratevista-graph` for the overlay types, never the reverse, so the graph stays pure.)
- **`cargo-cratevista`** `Command::Generate` is implemented with the flags `--keep-going`, `--features <FEATURES>`, `--all-features`, `--no-default-features`, `--document-private-items`, `--toolchain <TOOLCHAIN>`, `--external-deps <exclude|direct|full>`, `--document-bins`, and **`--no-config`** (plus the global `--manifest-path`); it dispatches to `cratevista_core::generate::run_generate` with the real `SystemClock`. `--no-config` (implemented PRD 08) lives on the **shared `GenerateArgs`**, so **`generate` and `open` both accept it** and it skips configuration discovery entirely — no file under `.cratevista/` is opened and no configuration diagnostics are emitted. **`serve` does not accept it**: `serve` only replays artifacts that already exist and performs no generation, so there is nothing for it to configure.
- **Dependency-boundary checks pass:** `cargo tree -p cratevista-graph -i {core, cargo-cratevista, server, config, watch}` report no path, and `cargo tree -p {metadata, rustdoc} -i cratevista-graph` report no path.
- ADRs present: 0001–0007, 0010 (**ADR-0005 relation reliability exists**; 0006 server/security and **0007 config format = TOML** landed with PRDs 06 and 08).
- **`cratevista-schema`** provides: `ExplorerDocument::new(project, entities, relations, views)` (sorts by id) and `ExplorerDocument::validate() -> Result<(), Vec<SchemaError>>` (duplicate ids; dangling relation endpoints, entity parents, view `entity_ids`, view `default_focus`; unknown kinds are **not** errors); `Project { id, name, description, root: Option<SourceLocation>, repository_url, default_branch }`; `Entity`/`Relation` (open string-backed `EntityKind`/`RelationKind` with `KNOWN_*` constants); `EntityId::{workspace, package, external_package[_disambiguated], target, module, item, impl_block, manual, from_raw}`; `RelationId::{basic, with_role, with_role_and_discriminator, from_raw}` and `Relation::new`; `View { id: ViewId, title, description?, entity_kinds, relation_kinds, entity_ids: Option<Vec<EntityId>>, stages: Vec<Stage>, default_focus?, presentation, docs: Option<DocBlock>, examples: Vec<ViewExample> }` — `docs`/`examples` are the additive **PRD-08 Amendment A** fields (`SchemaVersion` 1.0 → 1.1) that carry authored flow documentation and **embedded** example contents, so they render without `/api/source`; the eight views this crate generates leave both empty; `ViewExample { id: String, title, language: Option<String>, content: String, description? }`; `Stage { id: StageId, title, order }`; `ViewId::view(name)` (`StageId` has only `from_raw`); `GenerationReport { generator: Generator{name,version}, generated_at: Timestamp, toolchain: Option<String>, rustdoc_format_version: Option<u32>, input_hashes: BTreeMap, counts: Counts{entities,relations,views,diagnostics}, durations_ms: BTreeMap, artifact_hashes: Option<ArtifactHashes>, partial: bool }` and `ArtifactHashes { document_blake3: String, diagnostics_blake3: String }` (BLAKE3 lowercase-hex, exactly 64 chars, of the exact canonical `document.json`/`diagnostics.json` bytes; **current `generate` runs always populate `artifact_hashes`** — the field is `Option` only for backward-deserialization compatibility with pre-amendment artifacts); `DiagnosticsReport::new(Vec<DocumentDiagnostic>)` (sorts); `DocumentDiagnostic::new(Severity, code, message)` + `.entities`/`.relations`; `canonical::to_canonical_string`; `SchemaVersion::current`.
- **`cratevista-metadata`** provides `ingest(&MetadataOptions) -> Result<MetadataIngest, MetadataError>` and pure `normalize(&Metadata, &MetadataOptions)`. `MetadataIngest { entities, relations, diagnostics: Vec<DocumentDiagnostic>, summary: MetadataSummary }`. Entities: `workspace`; `package:{name}` (members) / `package:{name}@{version}[:disc]` (externals); `target:{package}:{kind}:{name}` where `kind ∈ {lib, bin, proc-macro, example, test, bench, custom-build, other}`, `parent = package`, attributes `crate_types/required_features/edition/doctest/test/doc`. Package/target entities carry **repo-relative** `SourceLocation` (manifest / `src_path`); **no absolute path** is exposed. `MetadataSummary.workspace_root_repo_relative == Some(".")` (repo-relative — **not** absolute).
- **`cratevista-rustdoc`** provides `ingest(&RustdocPlan, &RustdocOptions) -> Result<RustdocIngest, RustdocError>` and `RustdocPlan { workspace_root: PathBuf, targets: Vec<RustdocTarget{package_id, target_id, package_name, target_name, crate_name, target_kind, manifest_path, package_root}> }`, `RustdocTargetKind::{Library, ProcMacro, Binary, Other(String)}`. `RustdocIngest { crates: Vec<CrateSummary>, entities, relations, diagnostics, summary: RustdocSummary }`. **`CrateSummary { package_id, target_id, root_module_id, crate_name, format_version, toolchain, entity_count, relation_count, unresolved_refs: Vec<UnresolvedTypeRef> }`** (PRD-04 bridge amendment — carries the stable identities for linking). **`UnresolvedTypeRef { from: EntityId, role: TypeReferenceRole, crate_name: Option<String>, canonical_path: Option<Vec<String>>, item_kind: Option<EntityKind>, display: String }`** (structured evidence from rustdoc's `ItemSummary`/path map). `RustdocSummary { …, partial, include_private, features, network, compat: CompatibilityTuple, targets: Vec<TargetOutcome{target_id, package_name, target_name, target_kind, succeeded}> }`. Rustdoc entity ids use the **crate name** (`RustdocTarget.crate_name`): root module `module:{crate}::{crate}`, items `item:{kind}:{crate}::{path}`, impls `impl:{crate}:{trait|inherent}:{for}:{disc}`. Rustdoc emits open kinds `field`/`variant`/`assoc_type`/`assoc_const` in addition to the known set.
- The exit-code policy is unchanged: `ExitCode::{SUCCESS(0), RUNTIME_ERROR(1), USAGE_ERROR(2), ENVIRONMENT_ERROR(3), NOT_IMPLEMENTED(4)}`; `generate` uses 0/1/2/3 and never 4.

### Follow-up (non-blocking) — metadata workspace `qualified_name`

`cratevista-metadata` currently places the **absolute** workspace path in the workspace entity's `qualified_name` (its `SourceLocation`s are already repo-relative; this one string field is not). `cratevista-graph` **sanitizes** it to the workspace label before assembling the public artifact, so **no absolute path enters `document.json`, `diagnostics.json`, or `generation.json`** (guarded by tests). This is a producer-boundary wart, not a leak: a future **PRD-03 maintenance amendment** should drop the absolute value at the source. It is **out of scope** for this PRD (no `cratevista-metadata` change here).

## Core responsibility split (dependency direction)

Preserved, verified by `cargo tree`:

- `cratevista-core` owns application orchestration and `run_generate` (paths, process execution, clock, artifact writing, CLI result rendering, error→exit mapping).
- `cratevista-graph` owns **pure** graph/domain transformations (plan building, merge, linking, resolve, views, coverage, overlay application, validation invocation).
- `cratevista-schema` owns the public artifact models + `validate()`.
- `cratevista-metadata` owns Cargo metadata ingestion; `cratevista-rustdoc` owns rustdoc invocation/normalization.
- `cargo-cratevista` stays a thin CLI.

`cratevista-graph` **may depend on**: `cratevista-schema`, `cratevista-metadata`, `cratevista-rustdoc`, and directly-justified support crates (none anticipated beyond `serde`/`serde_json` if a fixture helper needs it). It **must not depend on**: `cratevista-core`, `cargo-cratevista`, `cratevista-server`, `cratevista-watch`, or a future `cratevista-config`. No orchestration, process execution, CLI rendering, filesystem path resolution, clock, or logging may live in `cratevista-graph`.

## Terminology

**Cross-source link**: a `contains` edge (and `parent`) joining a metadata `target` entity to its rustdoc root module, using the stable identities on `CrateSummary`. **Resolved relation**: a reference whose target maps to a known entity id. **Reliable role**: `field`/`parameter`/`return`/`error`/`associated_type`/`impl_for`/`impl_trait` (`TypeReferenceRole`).

## Public graph API

Concrete, pure, and **approved**, grounded in the implemented schema. No serialized JSON is returned; no timestamp/clock/filesystem is touched. Minor repository-style renames during implementation must not change these semantics or the dependency boundaries.

```rust
// --- inputs -----------------------------------------------------------------

/// Everything the pure builder needs. Assembled by cratevista-core.
///
/// No separate `PlannedTargetLink` collection is needed: `RustdocIngest`'s
/// `CrateSummary` already carries `package_id`/`target_id`/`root_module_id` (PRD-04
/// bridge amendment), so the graph links each rustdoc crate to its Cargo target
/// directly. `CrateSummary` is the **single source of truth** for that join.
pub struct GraphInput {
    pub metadata: MetadataIngest,                 // PRD 03
    pub rustdoc: Option<RustdocIngest>,           // PRD 04; None = metadata-only (rustdoc disabled)
    pub overlay: GraphOverlay,                     // issue-08 seam; empty is a normal input
                                                   // (since PRD 08 step 6, core fills this from
                                                   //  cratevista_config::load_config; --no-config
                                                   //  and unconfigured projects pass it empty)
}

pub struct GraphBuildOptions {
    pub compute_coverage: bool,                   // attach doc-coverage attributes + view (default true)
    pub retain_empty_views: bool,                 // keep the stable view set even when empty (default true)
}
// NOTE: external-dependency selection is NOT a GraphBuildOptions field. Which
// external package entities / depends_on relations exist is owned by metadata
// ingestion (MetadataOptions.external_deps). The graph merges the supplied input
// deterministically and never reinterprets the Cargo external-dependency
// selection — there is no second, potentially-divergent filter inside the graph.

// --- overlay seam (issue 08 maps TOML into this; no cratevista-config here) --

#[derive(Default)]
pub struct GraphOverlay {
    pub entities: Vec<Entity>,                     // manual additions (forced Provenance::Manual)
    pub relations: Vec<Relation>,                  // manual relations (forced Provenance::Manual)
    pub overrides: BTreeMap<EntityId, EntityOverride>, // presentation-only overrides on discovered entities
    pub views: Vec<View>,                          // manual flow views (may use Stages)
}

/// Presentation-only override. Overlays never change discovered structural truth
/// (kind/parent/source); those fields are intentionally absent.
pub struct EntityOverride {
    pub label: Option<LocalizedText>,               // replaces
    pub description: Option<LocalizedText>,         // replaces
    pub add_tags: Vec<String>,
    pub set_attributes: BTreeMap<String, AttrValue>, // presentation attrs (e.g. category, hidden)
    pub hidden: Option<bool>,
    pub docs: Option<DocBlock>,                     // APPENDS; additive (2026-07-16), see amendment below
}

// --- outputs ----------------------------------------------------------------

pub struct GraphBuildResult {
    pub document: ExplorerDocument,               // schema-valid, deterministic, no timestamps
    pub diagnostics: Vec<DocumentDiagnostic>,     // sorted union: metadata + rustdoc + graph-produced
    pub summary: GraphBuildSummary,               // path-free counts for the CLI / GenerationReport counts
    pub partial: bool,                            // propagated from RustdocSummary.partial
}

pub struct GraphBuildSummary {
    pub entity_count: usize,
    pub relation_count: usize,
    pub view_count: usize,
    pub diagnostic_count: usize,
    pub documented_crate_count: usize,
    pub unresolved_reference_count: usize,
    pub resolved_cross_crate_count: usize,
    pub coverage_percent: Option<u8>,             // workspace-wide public-item doc coverage, if computed
}

pub fn build_document(
    input: GraphInput,
    options: &GraphBuildOptions,
) -> Result<GraphBuildResult, GraphError>;
```

`GraphError` is fatal (a trustworthy document cannot be produced); everything recoverable is a `DocumentDiagnostic`:

```rust
pub enum GraphError {
    EmptyInput(String),                 // no metadata entities at all
    DocumentValidationFailed(Vec<SchemaError>), // schema validate() failed — a builder bug; never write output
    Plan(String),                       // build_rustdoc_plan failure (see below)
    InternalInvariant(String),          // e.g. an entity-id collision the merge could not resolve
}
```

`GraphBuildResult` must never contain: a `cratevista_core::Diagnostic`, serialized JSON, UI coordinates, a `GenerationReport`, or any absolute machine path.

### Additive amendment (2026-07-16) — `EntityOverride::docs` required by PRD 08 (Amendment B)

> **Status: Implemented / Verified (PRD-08 Amendment B).** PRD 05 remains **Implemented / Verified**; this is an **additive, backward-compatible** change to the overlay seam only. No schema change, no `SchemaVersion` bump, no fixture regeneration.

Issue 08 requires an override to enrich a discovered entity with **extra documentation**, for which `EntityOverride` previously had no field. Added:

```rust
pub struct EntityOverride {
    // … unchanged …
    /// Manual documentation appended after whatever was discovered.
    pub docs: Option<DocBlock>,
}
```

Contract (implemented in `overlay::{append_docs, join_markdown}`, applied by `apply_overlay`):

- **`docs` appends; `label`/`description` still replace.** The asymmetry is deliberate: documentation is additive enrichment, a label is a substitution. A regression test pins the replace semantics so this amendment cannot have quietly made the other fields additive.
- **Merge table:**

  | discovered `docs` | override `docs` | result |
  | --- | --- | --- |
  | `Some(d)` | `Some(m)` | `markdown = join(d.markdown, m.markdown)`; `summary` = `d.summary`; `documented` = `d.documented` |
  | `None` | `Some(m)` | `Some(DocBlock { markdown: m.markdown, summary: None, documented: false })` |
  | any | `None`, or Markdown that is empty/newlines-only | discovered `docs` untouched, **byte-identical** |

- **The Markdown boundary is exact.** Only newline characters *immediately adjoining the junction* are normalized — the discovered text's trailing newlines and the manual text's leading newlines — then the sides are joined with `\n\n`, yielding **exactly one blank line**. `\r` is trimmed alongside `\n` at the junction so a CRLF-terminated discovered block cannot leave a stray carriage return mid-document. **Internal content is never rewritten**: indentation, trailing spaces, interior blank lines, and the manual text's **own trailing newline** all survive byte-for-byte.
- **`documented` is never written by an override**, so documentation **coverage cannot be moved from configuration**. `compute_coverage` reads `docs.map(|d| d.documented).unwrap_or(false)`, so the `None → Some(documented: false)` transition feeds it the identical value — coverage is invariant *by construction*, not by convention. A manual block claiming `documented: true` is ignored on that field; its prose still lands. This matters because coverage reports *Rust* documentation: a project must not be able to report coverage it does not have by writing prose in a TOML file.
- **`summary` is never taken from the manual block** — a manual paragraph must not silently become an item's summary line.
- **Deterministic**: `overrides` is a `BTreeMap<EntityId, _>` iterated in sorted id order with at most one override per id, and the join is pure string manipulation.

Ordering within `build_document` is unchanged: `apply_overlay` → `compute_coverage`, so manual docs are attached *before* coverage is computed — which is exactly why `documented` preservation is load-bearing rather than incidental.

## RustdocPlan construction — ownership and API

Plan construction is **pure domain planning**, so it lives in `cratevista-graph::plan` (not in `cratevista-rustdoc`, which only executes a plan, and not in `cratevista-core`, which is orchestration). `cratevista-core` calls the planner, then passes the resulting `RustdocPlan` to `cratevista_rustdoc::ingest`. **The planner never invokes Cargo or rustdoc.**

Because the PRD-04 bridge amendment put `package_id`/`target_id`/`crate_name` on `RustdocTarget` and `package_id`/`target_id`/`root_module_id` on `CrateSummary`, the planner no longer needs a separate `PlannedTargetLink` collection — the plan itself carries every identity the graph needs, and post-ingest linking reads them off `CrateSummary`. `build_rustdoc_plan` therefore returns a plain `RustdocPlan`:

```rust
pub struct RustdocPlanOptions {
    pub include_binaries: bool,   // default false; lib + proc-macro are always included
}

/// Pure: selects documentable targets from metadata and builds a concrete plan
/// whose RustdocTargets carry the stable package_id/target_id/crate_name.
/// `workspace_root` is the ABSOLUTE workspace root — **runtime orchestration
/// context** resolved by cratevista-core and passed in separately (it is never
/// added to MetadataIngest / MetadataSummary / ExplorerDocument / GenerationReport
/// / any public id). The planner joins it with each package's repo-relative
/// manifest source to produce absolute RustdocTarget paths. It performs no I/O.
pub fn build_rustdoc_plan(
    metadata: &MetadataIngest,
    workspace_root: &Path,
    options: &RustdocPlanOptions,
) -> Result<RustdocPlan, GraphError>;
```

### Exact planning behavior

Iterate the metadata **target** entities (kind `target`), reading their `target:{pkg}:{kind}:{name}` identity and their parent `package:{name}` entity. The planner sets each `RustdocTarget`'s `package_id`/`target_id` to those exact metadata entity ids and computes `crate_name` **once**, authoritatively, from the target metadata (a lib/proc-macro target's metadata name is already the crate name; a bin's crate name is its target name with `-`→`_`).

| case | behavior |
|---|---|
| `kind == lib` | select → `RustdocTargetKind::Library` |
| `kind == proc-macro` | select → `RustdocTargetKind::ProcMacro` |
| `kind == bin` | select only if `options.include_binaries` → `RustdocTargetKind::Binary` |
| `kind ∈ {example, test, bench, custom-build, other}` | never selected (MVP) |
| package with no documentable (lib/proc-macro) target and no opted-in bin | contributes **no** `RustdocTarget` (not an error) |
| **virtual workspace** (no root package) | fine — planning is per-member-target; the workspace entity still anchors the tree |
| **package name ≠ target/crate name** | `package_name` from the package entity; `target_name` + `target_id` from the target entity; `crate_name` from target metadata; all carried explicitly on the `RustdocTarget` |
| **duplicate targets** | cannot occur from metadata (target ids are unique); `RustdocPlan::validate` rejects any duplicate `target_id` even though a lib and bin may share a `crate_name` |
| **manifest / package-root propagation** | `manifest_path = workspace_root.join(<package entity repo-relative manifest source>)`; `package_root = manifest_path.parent()`; both absolute |
| **target ordering** | deterministic: sort by `(package_name, target_kind, target_name)` before building `plan.targets` |

**Default plan** (with `RustdocPlanOptions::default()`): all workspace-member **library** and **proc-macro** targets; **no** binaries; **no** examples/tests/benches; **no** external-dependency documentation. Selecting binaries is explicit opt-in.

A package selected for documentation whose manifest source location is missing/invalid → `GraphError::Plan` (a plan without a manifest path cannot be executed).

## Cross-source linking

After merging, join the two identity schemes using the **stable identities on `CrateSummary`** (`target_id`/`root_module_id`), never crate-name string guessing:

- **workspace → package**, **package → Cargo target**: already emitted by metadata as `contains`; passed through.
- **package/target → rustdoc root module**: for each `CrateSummary` in `rustdoc.crates`, read its **`target_id`** and **`root_module_id`** directly. Set the root-module entity's `parent = target_id` and emit `contains: {target_id} → {root_module_id}`. No crate-name matching is involved — the identities are authoritative (PRD-04 bridge amendment), so a lib and a bin that share a `crate_name` remain distinct by `target_id`.
- **root module → nested rustdoc items**: already emitted by rustdoc as `contains`; passed through.

Every rustdoc crate carries its own `target_id`, so linking is unambiguous. The graph still validates coherence:

- `CrateSummary.target_id` not present among the metadata target entities → `rustdoc_target_unlinked` diagnostic; the crate's entities remain valid (reachable via their own containment).
- `CrateSummary.root_module_id` not present among the crate's emitted entities → `rustdoc_target_unlinked` (an upstream invariant break; PRD 04 already fails `internal_invariant` for a missing root module, so this is defensive).

## Entity merge rules

In practice the metadata (`workspace`/`package:`/`target:`) and rustdoc (`module:`/`item:`/`impl:`) id spaces are **disjoint**, so most entities pass through unmerged. Merge still applies deterministically wherever two entities share an `EntityId` (overlay overrides, a `crate_name` collision, or a future producer). Group by id; for each group apply **field-level rules with semantic ownership**:

- **Semantic ownership**: metadata **owns** `workspace`/`package`/`target` structural facts; rustdoc **owns** `module`/`item`/`impl` structure, docs, signatures, and typed relations; overlays own **presentation only** (never discovered structural truth) unless a documented override field permits it.
- **kind**: identical → dedup. Different kinds for the same id → `conflicting_entity_kind` diagnostic; keep the **owner's** kind; if neither source owns the id space (a genuine invariant break) → `GraphError::InternalInvariant`.
- **label/title**: prefer the owner's non-empty label; if the owner's is empty, take the other's (complementary enrichment); overlay `label` overrides last.
- **parent**: identical → dedup. Different non-empty parents → `conflicting_entity_parent` diagnostic; keep the owner's; **never silently overwrite**. (Cross-source linking legitimately *sets* a previously-`None` parent — that is enrichment, not a conflict.)
- **source location**: enrich — if one has a `SourceLocation` and the other does not, take it; if both differ, keep the owner's.
- **documentation (`docs`)**: rustdoc owns; enrich a metadata-origin entity that lacks docs with rustdoc docs; if both present, prefer rustdoc.
- **attributes**: merge key-by-key. Identical value → dedup silently (`duplicate_entity_evidence` only if the whole entity is a redundant duplicate). Complementary keys → union. Same key, different value → keep the owner's + `duplicate_entity_evidence` diagnostic (conflict noted, distinct evidence never lost silently).
- **provenance**: `Discovered` + `Discovered` → `Discovered`. An overlay override on a discovered entity keeps `Discovered` (structure) and applies presentation; a purely manual overlay entity is `Manual`.
- **tags / presentation metadata**: union, sorted, deduped.

Principles: identical values deduplicate silently; complementary evidence merges deterministically; documentation/source enrich; **incompatible kind/parent conflicts are never silently overwritten** and always emit a stable `DocumentDiagnostic`; a conflict that leaves no trustworthy entity is a `GraphError`. Priority is defined **per field by semantic ownership**, never a blanket "metadata wins"/"rustdoc wins".

## Relation merge rules

Group by `RelationId` (which already encodes `kind:from->to[:role[:disc]]`, so distinct roles/cfg are distinct ids):

- identical relations (same id, same payload) → dedup.
- same endpoints, **different role** → distinct ids → both kept.
- **different cfg evidence** (metadata `depends_on` carries a BLAKE3 `target_cfg` discriminator) → distinct ids → both kept.
- same id, **incompatible payload** (e.g. differing attributes) → merge attributes like entities; unresolved value conflicts → `conflicting_relation_evidence` diagnostic; the relation is kept once (deterministically, owner's payload).
- **no relation is silently discarded.** A relation whose endpoint does not exist **after all merging/linking** cannot appear in a valid document; it is **dropped with a `dangling_relation` diagnostic** (dropped, but never silently — the diagnostic records it).

Never collapse: normal/build/dev `depends_on`; different `target_cfg` `depends_on`; `implements` vs `implemented_for`; `accepts_type`/`returns_type`/`error_type`; distinct re-export evidence. Relation kinds and direction use the implemented `RelationKind` constants (`CONTAINS`, `DEPENDS_ON`, `IMPLEMENTS`, `IMPLEMENTED_FOR`, `HAS_FIELD_TYPE`, `ACCEPTS_TYPE`, `RETURNS_TYPE`, `ERROR_TYPE`, `RE_EXPORTS`, `IMPORTS`).

## Cross-crate unresolved-reference resolution

PRD 04 preserves `UnresolvedTypeRef { from, role: TypeReferenceRole, crate_name, canonical_path, item_kind, display }` per crate (references whose target was not in that crate's own index), with **structured evidence** extracted from rustdoc's `ItemSummary`/path map (PRD-04 bridge amendment). PRD 05 resolves them **across the analyzed workspace crates** using only that structured evidence — **never re-parsing `display`, never fuzzy string matching** — and never turns arbitrary type mentions into `references_type` edges (deferred by PRD 04).

**Indexes** (keyed by the identities PRD 04 already produces):

- `by_target: BTreeMap<EntityId /*target_id*/, &CrateSummary>` and the set of analyzed `crate_name`s (each `CrateSummary` carries `crate_name`/`target_id`/`root_module_id`).
- `by_entity_id: BTreeSet<EntityId>` — every emitted entity id (relations must be valid).
- `by_canonical: BTreeMap<(crate_name, canonical_relative_path, EntityKind), EntityId>` over resolvable-kind entities. The rustdoc entity id is `item:{kind}:{crate}::{relative_path}`, so this index is reconstructed directly from emitted ids.

**Resolution** — for each `UnresolvedTypeRef` (all are already reliable roles):

1. Require **structured evidence**: `crate_name = Some(c)` where `c` is an analyzed crate, and `canonical_path = Some(p)`. Compute the crate-relative path (drop the leading crate segment of `p` when present). If evidence is absent (both `None`), leave it as an `unresolved_cross_crate_reference` diagnostic (no `display` parsing).
2. Look up `(c, relative_path, item_kind)` in `by_canonical` (when `item_kind` is `Some`; otherwise match on `(c, relative_path)` across kinds).
3. Outcomes:
   - **exactly one** candidate → emit the role-specific relation (`Field→has_field_type`, `Parameter→accepts_type`, `Return→returns_type`, `Error→error_type`, `ImplFor→implemented_for`, `ImplTrait→implements`, `AssociatedType→` reserved), direction per the implemented `RelationKind` constants.
   - **zero** candidates → `unresolved_cross_crate_reference` diagnostic; **no** edge.
   - **more than one** candidate → `ambiguous_cross_crate_reference` diagnostic; **no** edge, unless the reference's exact `target_id`-derived identity disambiguates to a single entity.

Because resolution keys on `crate_name` + `canonical_path` + `item_kind`, workspace-internal cross-crate references resolve **exactly and deterministically**; genuinely external (non-analyzed) crates stay diagnostics. `references_type` is not produced.

## Explorer views and stages

Build the MVP views as **filter-based projections** using the real `View` type (`entity_kinds` + `relation_kinds` filters; `entity_ids` left `None` so membership derives from filters and `validate()` has nothing to dangle). Stable `ViewId::view(<slug>)`:

| view | slug | entity_kinds | relation_kinds |
|---|---|---|---|
| Workspace overview | `workspace-overview` | workspace, package, target | contains, depends_on |
| Crate dependencies | `crate-dependencies` | package | depends_on |
| Module hierarchy | `module-hierarchy` | package, target, module | contains |
| Types | `types` | struct, enum, union, type_alias, constant, static | has_field_type, contains |
| Traits & implementations | `traits-and-impls` | trait, impl, struct, enum, union | implements, implemented_for, contains |
| Type relationships | `type-relationships` | struct, enum, union, trait, function, method, type_alias | has_field_type, accepts_type, returns_type, error_type |
| Public API | `public-api` | module, struct, enum, union, trait, function, method, type_alias, constant, static, macro | contains, re_exports, imports |
| Documentation coverage | `documentation-coverage` | package, module | contains |

Rules:

- **ViewId format**: `view:{slug}` (via `ViewId::view`). **StageId format**: `stage:{id}` via `StageId::from_raw`, where `id` is the stage id the author declares in a flow file — matching the canonical `manual_flow.document.json` fixture (`stage:client`, `stage:gateway`). This crate emits no stages itself; the eight generated views have none. The speculative `stage:{view-slug}:{n}` form originally reserved here was never implemented and contradicted the fixture; issue 08 shipped `stage:{id}` (stages nest inside a `View`, so ids need only be unique within their flow, which `cratevista-config` enforces).
- **Ordering**: views sorted by id (schema does this in `ExplorerDocument::new`); the 8 slugs are fixed and stable.
- **Membership**: derived from `entity_kinds`/`relation_kinds` filters (no explicit `entity_ids`); "Public API" additionally carries a `presentation` hint `visibility=public` for the frontend to filter (membership stays filter-based to keep `validate()` trivial).
- **Empty views**: **retained** by default (`retain_empty_views = true`) so the UI has a stable, predictable set even for a metadata-only or sparse workspace.
- **Partial rustdoc**: item-level views (Types/Traits/Type relationships/Public API/Documentation coverage) may be sparse; they are still emitted, and a diagnostic notes reduced coverage. Metadata-only runs still get Workspace overview / Crate dependencies fully.
- **Unknown/future kinds**: the graph emits only known kinds itself; open kinds present via overlay (or rustdoc's `field`/`variant`/`assoc_*`) round-trip and render via the frontend generic fallback (PRD 07). Views filter by explicit kind lists, so unknown kinds simply don't appear in a filtered view unless a view lists them.

The UI PRD (07) owns layout/rendering; no coordinates, viewport, or component names enter `View`/`ExplorerDocument`.

## Generation stages vs pipeline phases

The schema `Stage` is a **presentation grouping within a view** (an ordered lane, e.g. an issue-08 manual flow step) — it is **not** a generation-pipeline stage and carries no timing. MVP auto-generated views emit **no** `Stage`s (stages arrive with issue-08 flows). The **generation pipeline phases** (metadata → plan → rustdoc → merge → link → resolve → views → validate) are runtime concerns: their timings go into `GenerationReport.durations_ms` (owned by core) or logging — **never** into the deterministic `document.json`.

## Documentation coverage

When `compute_coverage` is set: for each **public** item entity (visibility attribute `public`) of a documentable kind, count `documented` (its `DocBlock.documented == true`). Attach a deterministic `doc_coverage` attribute (`{ documented, total, percent }`) to `package` and `module` entities (aggregated over descendants), and expose the workspace-wide percent in `GraphBuildSummary.coverage_percent`. The `documentation-coverage` view presents it. The algorithm (public-only, `DocBlock.documented`) is fixed and documented so coverage is reproducible.

## Diagnostics

`build_document` returns a single **sorted union** of `DocumentDiagnostic`s from: `metadata.diagnostics`, `rustdoc.diagnostics` (if present), and graph-produced diagnostics (merge, linking, cross-crate resolution, overlay, validation notes). They are serialized only into `DiagnosticsReport`/`diagnostics.json` and **never** embedded in `ExplorerDocument`. Stable graph diagnostic codes (in a `diagnostics::code` module mirroring metadata/rustdoc):

`duplicate_entity_evidence`, `conflicting_entity_kind`, `conflicting_entity_parent`, `conflicting_relation_evidence`, `dangling_relation`, `rustdoc_target_unlinked`, `unresolved_cross_crate_reference`, `ambiguous_cross_crate_reference`, `invalid_view_reference`, `overlay_target_missing`, and info-level `rustdoc_disabled` / `no_documentable_rustdoc_targets`.

Fatal `GraphError` is used only when no trustworthy document can be built: `EmptyInput`, `DocumentValidationFailed` (see below), `Plan`, `InternalInvariant`. `document_validation_failed` is reported as a fatal `GraphError::DocumentValidationFailed` (its per-`SchemaError` details may also be surfaced as diagnostics for the CLI), because invalid output must never be written.

## Partial-result semantics

`GraphBuildResult.partial` and eventually `GenerationReport.partial`:

| situation | outcome |
|---|---|
| **empty default plan** (no documentable lib/proc-macro target; bins not enabled) | **complete metadata-only success**; `partial = false`; info `no_documentable_rustdoc_targets` diagnostic; exit 0 (see "Empty RustdocPlan behavior") |
| metadata ok, rustdoc **partial** (`RustdocSummary.partial == true`; some targets failed under `--keep-going`) | `partial = true`; per-target `target_failed` diagnostics flow through; document is valid but item-sparse |
| a package has **no** rustdoc target (in a non-empty plan) | not partial; the package/target entities exist without a root module; no diagnostic required (expected) |
| rustdoc **intentionally disabled** (`rustdoc = None`) | `partial = false` (a complete metadata-only document by explicit choice); info `rustdoc_disabled` diagnostic |
| a **non-empty** plan where rustdoc execution fails, or an explicitly-requested target does not exist, or all selected targets fail | approved `RustdocError`/default-fail/`--keep-going` behavior: fatal (exit 1) without `--keep-going`; `NoTargetSucceeded` is fatal even under `--keep-going`. **Not** a metadata-only fallback. |
| an unresolved cross-crate reference remains | not partial by itself; recorded as `unresolved_cross_crate_reference` diagnostic |
| an optional overlay override targets a missing entity | not fatal; `overlay_target_missing` diagnostic; other overlay content still applied |

Three clearly distinguished results: **partial-but-valid** (`partial = true`, valid document, prominent diagnostics); **complete-with-recoverable-diagnostics** (`partial = false`, diagnostics present — including the empty-plan metadata-only case); **fatal** (`GraphError`/upstream error → no document written). A partial result never appears complete: `generation.json.partial = true` and the CLI prints the partial banner.

## Empty RustdocPlan behavior

An **empty default plan** — `build_rustdoc_plan` returns zero targets because the workspace has no default-documentable **library or proc-macro** target (e.g. a bin-only workspace with `--document-bins` off, or a workspace whose only targets are unsupported kinds) — is a **complete metadata-only success**, distinct from any rustdoc failure:

- `cratevista-core` **must not** call `cratevista_rustdoc::ingest` with an empty plan (that would return the fatal `NoTargetSucceeded`).
- Core passes **`rustdoc: None`** to `GraphInput`.
- `cratevista-graph` builds a **metadata-only `ExplorerDocument`** (workspace/package/target entities + Cargo dependency relations + the metadata-only views).
- The result is **not partial**: `GraphBuildResult.partial = false` and `GenerationReport.partial = false`.
- Core emits a stable informational `DocumentDiagnostic` **`no_documentable_rustdoc_targets`**.
- Generation **succeeds with exit code 0**.

This is explicitly **different** from: a non-empty plan whose rustdoc execution fails; an explicitly-requested target that does not exist (`RustdocError::TargetNotFound`); and a run where all selected targets fail (`RustdocError::NoTargetSucceeded`). Those retain the approved `RustdocError` / default-fail / `--keep-going` behavior (exit 1 unless a `--keep-going` partial applies).

## Determinism

Equivalent `MetadataIngest`, `RustdocIngest`, overlay, and options → **byte-identical `document.json`**. Never rely on `HashMap`/`HashSet` iteration, input `Vec` order, filesystem order, hash seed, absolute paths, the current time, rustdoc numeric ids, or `PackageId` string formatting. Documented stable sort keys:

- entities, relations, views → by `id` (schema `ExplorerDocument::new` enforces this; the builder also sorts before assembly).
- diagnostics → `DocumentDiagnostic`'s derived `Ord` (severity, code, message, entities, relations); `DiagnosticsReport::new` re-sorts.
- plan targets → `(package_name, target_kind, target_name)`.
- cross-crate `unresolved_refs` processing → PRD 04 already emits them sorted (derived `Ord` over `from`, `role`, structured fields); the resolver processes them in that order so emitted relations/diagnostics are order-independent.
- view membership is filter-derived (no per-view ordering to stabilize); `Stage`s (issue 08) by `order` then `id`.
- attribute maps are `BTreeMap` (already ordered); tag vectors sorted+deduped.
- `GraphBuildSummary` collections derived from already-sorted data.

## Schema validation

`build_document` calls `ExplorerDocument::validate()` on its assembled document **before returning**. On `Err(errors)` it returns `GraphError::DocumentValidationFailed(errors)` (a builder bug — the merge/linking should have prevented it). `cratevista-core` treats that as fatal (exit 1) and **never writes `document.json`**. Core performs no second, duplicate validator; it relies on the schema's `validate()`. Policy:

- structural schema-validation failure is **fatal**; invalid `document.json` is **never** written;
- on fatal failure, core writes **nothing** and surfaces the error via the CLI/exit code (see the approved artifact contract below) — it does not leave a stale or document-less artifact set.

## Artifact commit semantics (approved contract)

`cratevista-graph` builds **Rust values only** (`GraphBuildResult`); it never serializes or writes. `cratevista-core` (`generate.rs` + `artifacts.rs`) owns writing.

**On a successful or explicitly-accepted partial generation** — a **prepare** phase (which embeds sibling-artifact hashes into `generation.json`) followed by a **commit** phase:

1. **Build and validate** the `ExplorerDocument` (via `ExplorerDocument::validate()`).
2. Create the `DiagnosticsReport` (`DiagnosticsReport::new(result.diagnostics)`).
3. Canonically serialize `document.json` (`cratevista_schema::canonical::to_canonical_string`).
4. Canonically serialize `diagnostics.json`.
5. Compute **BLAKE3** over the exact canonical bytes of `document.json` and `diagnostics.json` (`cratevista-core` depends on `blake3` for this; the bytes hashed are content only — no absolute paths).
6. Construct the `GenerationReport` **with** `artifact_hashes = Some(ArtifactHashes { document_blake3, diagnostics_blake3 })` (PRD-02 amendment); partial output sets `GenerationReport.partial = true` and records the `target_failed`/diagnostics.
7. Canonically serialize `generation.json` (it now carries the sibling hashes; it never hashes itself).
8. Write all three to **completed temporary sibling files** in `target/cratevista/`.
9. Commit `document.json` by same-directory rename.
10. Commit `diagnostics.json` by same-directory rename.
11. Commit **`generation.json` last**, as the completion marker.

**Failure handling:**

- On a **fatal failure before the commit phase begins** (metadata/rustdoc/graph/validation/serialization error): replace **no** existing artifact; remove the temporary files; report through CLI human/JSON output; return the appropriate non-zero exit code.
- On a **rename/commit error** (a rename fails mid-commit): return a non-zero exit and perform **best-effort rollback/cleanup** where possible (remove leftover temp files; the already-renamed files may remain — see the honesty note).
- An **invalid `document.json` is never committed**, and `document.json` is never partially replaced.
- **No `last-failure.json`** is introduced in PRD 05.

**Honesty note (crash-atomicity limits).** Each individual rename is atomic **where the OS/filesystem supports it** (same-volume `fs::rename` on Linux/macOS/Windows). The **set of three files is not guaranteed to be a single crash-atomic transaction** across all supported operating systems: a crash between renames can leave a newer `document.json`/`diagnostics.json` with an older `generation.json` (or vice versa). Because that torn state is possible, **`generation.json`-byte equality before/after a read does NOT by itself prove the other two files belong to that generation** — an old `generation.json` can be observed both before and after the newer `document.json`/`diagnostics.json` are renamed. The **integrity mechanism** is therefore the `artifact_hashes` embedded in `generation.json` (step 6): a reader verifies that the `document.json`/`diagnostics.json` bytes it loaded hash to exactly those values. Robust **hash-verified snapshot reading with bounded retry** is the reader's responsibility and is implemented by **PRD 06** (the server). PRD 05 guarantees: `generation.json` is written **last**, and it carries the exact BLAKE3 hashes of the `document.json`/`diagnostics.json` bytes committed in the same run.

No server or static-site behavior; no `--output` override (that is PRD 10's `build`).

## run_generate orchestration (cratevista-core)

`run_generate` sequence (core owns steps 1, 3, 5, 8, 9, 10; core *calls* graph for 4, 6, 7):

1. **Resolve** project root + output dir (`<project>/target/cratevista/`) via `paths`; resolve the absolute workspace root (orchestration context).
2. Build `MetadataOptions` from CLI options (including `--external-deps` → `external_deps`).
3. Run `cratevista_metadata::ingest`.
4. Call `cratevista_graph::plan::build_rustdoc_plan(metadata, workspace_root, plan_options)` → `RustdocPlan` (skipped entirely if rustdoc is disabled).
5. If the plan has **≥1** target, run `cratevista_rustdoc::ingest(&plan, &rustdoc_options)` → `Some(RustdocIngest)`. If the plan is **empty** (or rustdoc disabled), **do not** call `ingest`; use `rustdoc = None` and record the `no_documentable_rustdoc_targets` info diagnostic (empty-plan case).
6. Call `cratevista_graph::build_document(GraphInput { metadata, rustdoc, overlay }, &build_options)` → `GraphBuildResult` (linking reads `target_id`/`root_module_id` off `CrateSummary`).
7. Validation happens **inside** `build_document`; a `GraphError::DocumentValidationFailed` aborts before writing.
8. Create the `DiagnosticsReport` from `GraphBuildResult`; canonically serialize `document.json` and `diagnostics.json`; compute their BLAKE3 hashes; then assemble the `GenerationReport` (inject the clock here) **with `artifact_hashes`** set to those hashes (so the marker file carries the sibling integrity hashes — PRD-02 amendment).
9. Canonically serialize `generation.json` and **commit** all artifacts (prepare complete temp files, then rename document → diagnostics → `generation.json` last — see "Artifact commit semantics").
10. Return a core use-case result (`CommandOutcome`) suitable for human or JSON CLI rendering (counts, coverage, diagnostic summary, `partial`).

`cratevista-graph` must **not** execute steps 1, 3, 5, 8, 9, or 10. Core maps upstream errors to `CommandFailure`: `MetadataError` (cargo missing → exit 3; other → exit 1), `RustdocError` (`nightly_missing`/`toolchain_not_found` → exit 3; other → exit 1), `GraphError` → exit 1.

## CLI generate wiring

Issue 05 replaces the `generate` stub (the source issue defines `generate` producing the document). Add flags to `Command::Generate`, grounded in the PRD 03/04 reserved contract (no conflicting flags):

- `--manifest-path` (already global).
- `--keep-going` (PRD 04): partial generation.
- feature selection: `--features <list>`, `--all-features`, `--no-default-features` (mapped into **both** `MetadataOptions.features` and `RustdocOptions.features`).
- `--document-private-items` (PRD 04).
- `--toolchain <name>` (PRD 04 nightly override).
- external-dependency mode: **`--external-deps <exclude|direct|full>`** (default `exclude`) → `MetadataOptions.external_deps` (`exclude → ExternalDepsMode::Exclude`, `direct → DirectOnly`, `full → FullGraph`). A tri-valued enum flag is required because a boolean cannot distinguish `DirectOnly` from `FullGraph`; the earlier `--include-external-deps` boolean is **rejected**. Metadata ingestion owns which external package entities and `depends_on` relations are present; the graph applies **no** second external-dependency filter.
- target selection / binary opt-in: `--document-bins` → `RustdocPlanOptions.include_binaries` (default off).
- **No `--no-cache` flag is added in PRD 05.** There is no persistent cache to bypass yet; adding and implementing `--no-cache` belongs to PRD 09. (There is currently no `--no-cache` in the CLI at all — no stale flag is introduced or "accepted" here.)

Exit behavior:

- **0**: complete success, **or** explicitly-accepted partial success under `--keep-going`.
- **1**: runtime/generation failure (metadata/rustdoc/graph fatal error, validation failure, write failure).
- **2**: invalid CLI usage (clap default).
- **3**: environment/toolchain failure (cargo missing, nightly missing).
- **4**: no longer used for `generate` once implemented.

**Partial under `--keep-going` exits 0** with `partial = true` and prominent diagnostics, because the user explicitly opted into partial generation. Without `--keep-going`, a failed target is fatal (exit 1). A **metadata-only success from an empty default `RustdocPlan`** is **complete, not partial**, and exits **0** (with the `no_documentable_rustdoc_targets` info diagnostic) — see "Empty RustdocPlan behavior".

## Cache behavior (before PRD 09)

PRD 04 implemented only the pure `cache_key` computation; PRD 09 owns the watcher and persistent cache. **Issue 05 does not implement watch mode and does not persist a cache**, and it **does not add a `--no-cache` flag** — there is no cache to bypass. Persistent-cache use, watcher invalidation, and the addition/implementation of `--no-cache` all belong to PRD 09. PRD 04's pure `cache_key` remains available for PRD 09 but is not required to be consumed by PRD 05; PRD 05 introduces **no** second cache-key format. MVP generation is simply uncached, and nothing pretends otherwise.

## GenerationReport population

Populated by **core** (not graph), using the real `GenerationReport` shape:

- `generator = { name: "cargo-cratevista", version: env!("CARGO_PKG_VERSION") }`.
- `generated_at`: from an **injected clock** (core owns time; graph never reads it), RFC-3339 `Timestamp`.
- `toolchain`: `rustdoc.summary.compat.nightly` (Some) or `None` when rustdoc disabled.
- `rustdoc_format_version`: `Some(rustdoc.summary.compat.format_version)` or `None`.
- `input_hashes`: optional; MVP may leave empty or record digests of the metadata/rustdoc inputs (not required for correctness).
- `counts`: from the document + diagnostics (`GraphBuildSummary`).
- `durations_ms`: per-phase timings recorded by core around each orchestration step.
- `partial`: `GraphBuildResult.partial`.

None of these values enter `ExplorerDocument`.

## Files and modules to create or modify

`cratevista-graph` (approved module layout; repository-style renames must not change semantics or boundaries):

```
crates/cratevista-graph/src/
  lib.rs         # public API: build_document; re-exports
  input.rs       # GraphInput, GraphBuildOptions, GraphOverlay, EntityOverride
  plan.rs        # build_rustdoc_plan, RustdocPlanOptions (returns RustdocPlan directly)
  merge.rs       # entity + relation merge (field-level ownership + conflict diagnostics)
  link.rs        # cross-source linking (workspace/package/target ↔ rustdoc root module)
  resolve.rs     # cross-crate unresolved-reference resolution
  views.rs       # the MVP default views
  coverage.rs    # documentation-coverage attributes + view
  overlay.rs     # apply GraphOverlay (presentation overrides + manual additions)
  diagnostics.rs # stable graph diagnostic codes + builders
  validate.rs    # invoke schema validate(); map failures to GraphError/diagnostics
  result.rs      # GraphBuildResult, GraphBuildSummary
  error.rs       # GraphError
```

`cratevista-graph/Cargo.toml`: add `cratevista-schema`, `cratevista-metadata`, `cratevista-rustdoc` (dev: `serde_json` for fixtures if needed). **Not** core/server/watch/config/CLI.

`cratevista-core` additions:

```
crates/cratevista-core/src/
  generate.rs    # run_generate orchestration (the 10 steps)
  artifacts.rs   # GenerationReport/DiagnosticsReport assembly + canonical + atomic write
  clock.rs       # injected clock trait (SystemClock + a fixed test clock)
```

`cratevista-core/Cargo.toml`: add `cratevista-schema`, `cratevista-metadata`, `cratevista-rustdoc`, `cratevista-graph`. `cargo-cratevista`: extend `Command::Generate` flags and dispatch to `run_generate`. `docs/adr/0005-relation-reliability.md` (reliable/approximate/excluded classification + cross-crate resolution policy).

## Testing strategy

### Pure graph unit/integration tests (stable, no nightly, no network)

Use **constructed** `MetadataIngest` / `RustdocIngest` values (public constructors or checked-in fixtures) so tests are stable-only:

- metadata-only document (rustdoc `None`); metadata + rustdoc merge.
- package/target/root-module linking; package name ≠ crate name; proc-macro target; explicitly-selected binary target.
- identical entity dedup; complementary evidence merge; kind conflict → `conflicting_entity_kind`; parent conflict → `conflicting_entity_parent`.
- identical relation dedup; same endpoints with distinct role/cfg kept; `dangling_relation` detection (drop + diagnostic).
- cross-crate unresolved refs: unique match → edge; none → diagnostic, no edge; ambiguous → diagnostic, no edge.
- re-exports (one canonical entity + `re_exports`, no duplicate node) pass through.
- partial rustdoc propagation (`partial = true`).
- plan building: default (lib+proc-macro), binary opt-in, package with no documentable target, virtual workspace, deterministic target ordering; empty/invalid plan behavior.
- **empty-plan / metadata-only behavior** (core integration): a **bin-only workspace with `--document-bins` off** → metadata-only success, `partial = false`, `no_documentable_rustdoc_targets`, exit 0; a **workspace whose only targets are unsupported kinds** → metadata-only success + `no_documentable_rustdoc_targets`; a **normal non-empty plan whose rustdoc fails** → fatal (exit 1) unless `--keep-going` applies (and `NoTargetSucceeded` fatal even under `--keep-going`).
- **external-deps CLI**: `--external-deps exclude|direct|full` maps to `MetadataOptions.external_deps`; the graph applies no second external filter.
- **artifact commit**: `generation.json` is written last; a fatal failure before commit replaces no existing artifact and leaves no temp files; no `last-failure.json`.
- default views + stages (filter-based; no coordinates; empty views retained).
- schema `validate()` passes on every produced document; no absolute paths anywhere; diagnostics never embedded in the document.
- determinism: reordered input vectors → byte-identical `document.json` (via canonical serializer).

### Core integration tests

- `run_generate` on local **path-only** fixture workspaces (no network): asserts the three artifacts are written, `document.json` validates and is deterministic across two runs (with an injected fixed clock so `generation.json` is also stable for the test), diagnostics are in `diagnostics.json` only, atomic replacement leaves no partial `document.json`, and exit codes follow the policy (0 / partial-0 / 1 / 3).
- A full **live** rustdoc pipeline test may be **gated/ignored** using the pinned nightly (`nightly-2026-07-01`); normal workspace tests stay stable-only.

Prefer checked-in `MetadataIngest`/`RustdocIngest` fixtures or public constructors; ordinary tests must not require network access, and artifact-output/atomic-replacement tests must not require the server or frontend.

## Performance considerations

This is an **initial engineering benchmark target, not a public product contract** — no hard wall-clock acceptance threshold is set before measuring the real implementation. The benchmark fixture is **~20,000 entities and ~50,000 relations**; the implementation must exhibit **no obvious O(n²)** merge/link/resolve behavior, achieved by **indexing** on: entity ids, relation ids, canonical paths, and unresolved-reference lookup (all `BTreeMap`/`BTreeSet`). Provide a synthetic fixture generator and a `criterion` bench (`benches/large_workspace.rs`) that measures assembly time and asserts the algorithmic shape (e.g. near-linear scaling across fixture sizes) rather than a fixed second-count. The UI's ~1,500 visible-node rendering benchmark is **separate and belongs to PRD 07**. Pure functions remain reusable by PRD 09 caching.

## Observability and diagnostics

`cratevista-graph` emits **no** logs (pure). `cratevista-core` may add `tracing` spans per orchestration phase with counts and durations; the CLI prints a summary (entity/relation/view/diagnostic counts, coverage %, `partial`). `diagnostics.json` (a `DiagnosticsReport`) carries unresolved references, conflicts, and excluded externals — never embedded in `document.json`.

## Documentation changes

`docs/adr/0005-relation-reliability.md` (reliable typed relations vs deferred `references_type` vs excluded cross-crate/macro edges; the cross-crate resolution policy). A schema/view reference listing the 8 views and their filters; the coverage algorithm. README/`generate` docs updated when the stub is replaced.

## Rollout and migration

New crate `cratevista-graph`; new `cratevista-core` `run_generate` + artifacts; `generate` stub replaced (exit 4 → real). Additive, backward-compatible; no breaking schema change. (The `artifact_hashes` writer follow-up amendment — see Status — was delivered as **Phase 0 of PRD 06** and is now implemented: `cratevista-core::artifacts` computes and embeds the two BLAKE3 digests, and every `generate` run emits `generation.json` with `artifact_hashes`.)

## Risks and mitigations

- **Invented edges from bad resolution** → only emit on an exact structured match; else diagnostic. Test enforces zero/one/many outcomes.
- **Non-determinism** → schema id-sorting + canonical serializer + determinism test; all runtime metadata isolated in `generation.json`; injected clock.
- **Absolute paths leaking** → graph consumes only repo-relative schema sources; absolute paths exist only transiently in core (invocation) and the plan; a no-absolute-path test guards artifacts.
- **Rustdoc↔target mis-linking** → each `CrateSummary` carries its own `target_id` (PRD-04 bridge amendment); linking is by identity, not crate-name guessing, so a lib and bin sharing a `crate_name` stay distinct.
- **Divergent external-dependency state** → external-dependency selection lives **only** in `MetadataOptions.external_deps`; the graph has no second filter, so the two cannot diverge.

## Alternatives considered

- Placing plan construction in `cratevista-rustdoc` or `cratevista-core`: rejected — it is pure domain planning; rustdoc only executes plans, core only orchestrates.
- A trait-object plugin overlay: rejected — a plain `GraphOverlay` struct of schema types is simpler and sufficient; no concrete MVP benefit to plugins.
- Resolving cross-crate references by fuzzy string matching: rejected — correctness over recall; unresolved stays a diagnostic.
- Building views as separate documents: rejected — the issue mandates projections over one canonical document.
- A second graph-specific validator: rejected — reuse `ExplorerDocument::validate()`.

## Implementation sequence

1. `plan` (`build_rustdoc_plan` → `RustdocPlan` with stable identities) with pure unit tests.
2. `input`/`result`/`error`/`diagnostics` scaffolding.
3. `merge` (entity + relation) + `link` (cross-source).
4. `resolve` (cross-crate) + `views` + `coverage` + `overlay`.
5. `build_document` assembly + `validate`; determinism tests.
6. `cratevista-core` `generate`/`artifacts`/`clock` + CLI wiring; integration tests; ADR-0005; bench.

## Acceptance criteria

- [x] Cargo and rustdoc inputs merge into one **schema-valid** document (`ExplorerDocument::validate()` passes). *(integration test)*
- [x] Every relation references existing entities; dangling relations are dropped **with** a `dangling_relation` diagnostic. *(validation + merge tests)*
- [x] Repeated generation of unchanged input yields byte-identical `document.json` by default (no flag; timestamps only in `generation.json`). *(determinism test)*
- [x] Runtime metadata → `generation.json`; diagnostics → `diagnostics.json`; neither embedded in `document.json`. *(output-split test; `--keep-going` sets `partial=true`)*
- [x] `build_rustdoc_plan` selects lib+proc-macro by default, bins only when opted in, is deterministic, and performs no Cargo/rustdoc I/O. *(plan tests)*
- [x] Cross-source linking joins workspace→package→target→rustdoc root module via `CrateSummary.target_id`/`root_module_id` (not crate-name guessing); unlinked crates diagnosed. *(linking tests)*
- [x] Trait implementations visible and navigable (`implements`/`implemented_for`). *(impls test)*
- [x] Function input/output/error types represented where resolvable; unresolved cross-crate refs → diagnostics, not edges (zero/one/many outcomes). *(resolve tests)*
- [x] Re-exports yield one canonical entity + a `re_exports` relation; no duplicate node. *(reexport test)*
- [x] The MVP views are filter-based projections with no UI coordinates; empty views retained. *(views test)*
- [x] Entity/relation merges follow per-field semantic ownership; kind/parent conflicts never silently overwritten (diagnostics emitted). *(merge tests)*
- [x] Default empty `GraphOverlay` is a normal supported input; overrides are presentation-only; missing override target → `overlay_target_missing`. *(overlay tests)*
- [x] `run_generate` commits the three artifacts via prepare-then-commit (per-file atomic rename, `generation.json` last); on fatal failure no existing artifact is replaced and no temp files or `last-failure.json` remain; `document.json` never partially replaced; `generate` no longer returns exit code 4; partial under `--keep-going` exits 0. *(core integration + exit-code tests)*
- [x] An empty default `RustdocPlan` (bin-only/off, or unsupported-kinds-only) yields a **complete metadata-only** document (`partial = false`) with `no_documentable_rustdoc_targets` and **exit 0**; a non-empty plan whose rustdoc fails is fatal unless `--keep-going` applies. *(empty-plan tests)*
- [x] `--external-deps <exclude|direct|full>` maps to `MetadataOptions.external_deps`; the graph applies no second external-dependency filter. *(CLI + graph tests)*
- [x] No `--no-cache` flag is added (deferred to PRD 09); generation is uncached and nothing pretends otherwise; no second cache-key format introduced. *(cache/CLI test)*
- [x] No absolute machine path appears in any artifact. *(no-absolute-path test)*
- [x] Engineering benchmark: ~20k entities / ~50k relations assembled with indexed merge/link/resolve and no obvious O(n²); no hard wall-clock threshold. *(bench asserting algorithmic shape)*

Verification:

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-features           # stable-only; no nightly, no network
cargo +1.97.0 check --workspace --all-features
cargo tree -p cratevista-graph -i cratevista-core     # no path
cargo tree -p cratevista-graph -i cargo-cratevista    # no path
cargo tree -p cratevista-graph -i cratevista-server   # no path
cargo tree -p cratevista-metadata -i cratevista-graph # no path (metadata must not depend on graph)
cargo tree -p cratevista-rustdoc  -i cratevista-graph # no path (rustdoc must not depend on graph)
cargo bench -p cratevista-graph
# gated: cargo test -p cratevista-core --test generate_live -- --ignored   # needs nightly, no network
```

Also verify: `generate` no longer returns exit 4; stable-only tests need no nightly; `document.json` validates and is deterministic; diagnostics are separate; no absolute path in public artifacts.

## Resolved-decision history

No open questions remain — **all material decisions are approved**. The items below are retained only as history of what was formerly open and how it was decided:

1. **Absolute workspace root** — *resolved*: runtime orchestration context; `cratevista-core` resolves it (e.g. `cargo locate-project --workspace` or its own metadata call) and passes it to `build_rustdoc_plan`. Never added to `MetadataIngest`/`MetadataSummary`/`ExplorerDocument`/`GenerationReport`/public ids (no PRD-03 code change).
2. **Rustdoc-crate ↔ Cargo-target join** — *resolved*: `RustdocTarget`/`CrateSummary` carry `package_id`/`target_id` (+ `crate_name`/`root_module_id`); the graph links via `target_id`. No `PlannedTargetLink`.
3. **Cross-crate resolution** — *resolved*: `UnresolvedTypeRef` carries structured `crate_name`/`canonical_path`/`item_kind`; resolution is exact and deterministic.
4. **External-dependency selection** — *resolved*: owned solely by `MetadataOptions.external_deps` via `--external-deps <exclude|direct|full>`; no second graph-level filter (boolean form rejected).
5. **Empty RustdocPlan** — *resolved*: a metadata-only complete success (`partial = false`, `no_documentable_rustdoc_targets`, exit 0), distinct from rustdoc failure.
6. **Performance** — *resolved as an engineering benchmark*: ~20k entities / ~50k relations, indexed merge/link/resolve, no obvious O(n²), **no hard wall-clock threshold** before measuring. (The UI ~1,500-node benchmark is PRD 07.)
7. **Fatal artifact contract** — *resolved*: prepare-then-commit; `generation.json` last as the completion marker **and carrying the BLAKE3 `artifact_hashes` of `document.json`/`diagnostics.json`** (PRD-02 additive amendment); on fatal failure replace no existing artifact and leave no temp files; no `last-failure.json`; each rename atomic where supported but the three-file set is not one crash-atomic transaction, so **hash verification (not marker equality alone)** is the reader's integrity mechanism, implemented by PRD 06.

**PRD 05 is Implemented / Verified.** All acceptance criteria have implementation evidence; no material decisions or implementation blockers remain.

## Traceability

Issue-05 checkboxes → tests above. Produces the `ExplorerDocument` served by issue 06, rendered by issue 07, extended by the issue-08 `GraphOverlay`, regenerated (with caching) by issue 09, and bundled by issue 10. The PRD-04 bridge amendment (stable `RustdocTarget`/`CrateSummary` identities + structured `UnresolvedTypeRef`) is implemented and verified.
