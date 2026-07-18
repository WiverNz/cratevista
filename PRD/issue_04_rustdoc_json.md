# PRD — Generate and ingest rustdoc JSON

## Status

**Implemented / Verified** (2026-07-13). Implemented per the approved design below, with the compatibility tuple **empirically verified**:

- **Verified compatibility tuple (ADR-0004):** pinned nightly **`nightly-2026-07-01`** (`rustc 1.98.0-nightly (f46ec5218 2026-06-30)`) emits rustdoc JSON **`format_version = 60`**; `rustdoc-types` resolves to **`0.60.0`** (`Cargo.lock` authoritative; `rustdoc_types::FORMAT_VERSION == 60`, guarded by a compile-time assertion in `compat.rs`); adapter version **`1`**. Verified by generating JSON for the path-only `sample_lib` fixture, asserting `format_version == 60`, deserializing with `rustdoc-types 0.60.0`, and normalizing it.
- **Verified command form (ADR-0004):** `cargo +<nightly> rustdoc -Z unstable-options --output-format json --manifest-path <manifest> -p <package> <--lib|--bin name> --target-dir <dir> -- [--document-private-items]` (private-items flag on the rustdoc side of `--`; no silent syntax fallback).
- **Gates:** `cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets --all-features -- -D warnings`, `cargo test --workspace --all-features`, and `cargo +1.97.0 check --workspace --all-features` all pass on stable 1.97.0; `cargo tree -p cratevista-rustdoc -i {core,metadata,graph,server}` report no path. The gated `#[ignore]` live E2E (`--test live -- --ignored`) generates and normalizes real rustdoc JSON with the tuple (no network).

### Implementation amendment (2026-07-13) — PRD-05 bridge contract

Additive, backward-shaped changes so the (future) graph builder can link Cargo targets and resolve cross-crate types **deterministically, without string guessing**. PRD 04 remains **Implemented / Verified**; no rustdoc types entered the public API.

- **Stable target identity.** `RustdocTarget` now carries `package_id: EntityId`, `target_id: EntityId`, and an explicit `crate_name` (the target's actual Cargo crate name — not blindly `-`→`_`). Plan validation now checks id kind/prefix, non-empty names, no path separator in any public id, and **duplicate `target_id`** (so a package's lib and bin — which share a `crate_name` — stay distinct).
- **Stable crate identity in the companion.** `CrateSummary` now carries `package_id`, `target_id`, and `root_module_id` (the actual normalized root module); `TargetOutcome` carries `target_id`. `normalize` copies the ids from the `RustdocTarget` (via `NormalizeContext`) and fails `internal_invariant` if a successful crate has no root module — target ownership is **never** reconstructed from `crate_name` later.
- **Structured unresolved references.** `UnresolvedTypeRef` replaces the old `role: String` + `external_hint` with `role: TypeReferenceRole` and structured evidence `crate_name`/`canonical_path`/`item_kind`, extracted from rustdoc's `ItemSummary`/path map + `external_crates` (never by re-parsing `display`; no numeric id; no absolute path). Absent structured fields stay `None`.
- **Verification:** all stable gates pass; the gated live E2E passes with the same verified tuple and now asserts the stable identities.

Grounded in the implemented PRDs 01–03 and mirroring `cratevista-metadata`. All material choices resolved:

- **Explicit `RustdocPlan`** (workspace_root + concrete `RustdocTarget`s) is passed in by the orchestrator (issue 05/core); `cratevista-rustdoc` validates it but never enumerates packages or depends on `cratevista-metadata`. No `PackageSelection` in this crate.
- **`rustdoc-types` stays crate-private**: `pub(crate) load_raw`/`normalize_raw`; public API is `ingest(&RustdocPlan, &RustdocOptions)`, `normalize_json(&str, &NormalizeContext)`, `load_and_normalize(&Path, &NormalizeContext)` — no `rustdoc_types::Crate` in any public signature.
- **Candidate compatibility tuple** `rustdoc-types 0.60` / `format_version 60` / adapter `1`; exact nightly is implementation-time verified (generate fixture JSON → confirm `format_version == 60` → deserialize with 0.60.x → record nightly in ADR-0004; fail the task rather than approve an unverified tuple).
- **Command form** is verified at implementation and recorded in ADR-0004 (`--document-private-items` on the rustdoc side of `--`); no silent syntax fallback.
- **Safe summary**: `RustdocSummary` carries structured, path-free data; raw argv only in tracing/errors. No absolute paths in `RustdocIngest`/`GenerationReport`/artifacts.
- **Path derivation hierarchy** (path map → containment → parent → impl signature → skip) — items not skipped merely for absence from `Crate.paths`.
- **Spans**: `NormalizeContext` has both `workspace_root` and `package_root`; absolute and relative filenames handled; outside/generated omitted; absolute paths never exposed/hashed.
- **MVP scope**: `references_type` deferred (reliable typed relations only); per-target **sequential**; `document_dependencies` removed; binaries opt-in; examples/tests/benches unsupported; default plan = library + proc-macro.
- **Stable vs nightly**: workspace on pinned stable 1.97.0; `rustdoc-types` is an ordinary stable-build dep; nightly only as a separately invoked runtime toolchain; all normal tests stable-only; one gated live test needs the pinned nightly.

## Source issue

`ISSUES/issue_04_rustdoc_json.md`

## Summary

Implement `cratevista-rustdoc`: execute and validate a pre-resolved `RustdocPlan`, invoke `cargo rustdoc … --output-format json` under a separately-pinned nightly, discover and deserialize the rustdoc JSON via `rustdoc-types`, verify the compatibility tuple, and map items into deterministically-ordered `cratevista-schema` entities/relations + `DocumentDiagnostic`s (plus a thin per-crate normalized companion for cross-crate resolution). Raw `rustdoc-types` never appear in the public API. The result feeds the graph builder (issue 05); this crate writes no artifacts.

Package/target selection is **not** owned here: issue 05/core selects packages and targets from `MetadataIngest` and builds the `RustdocPlan`. `cratevista-rustdoc` owns `RustdocPlan` validation, per-target command construction/execution, JSON output discovery, compatibility validation, loading/normalization, deterministic aggregation, and rustdoc-specific diagnostics/cache inputs.

## Problem statement

rustdoc JSON is the richest source of Rust item structure but is nightly-only, unstable, and version-gated. CrateVista must consume it without leaking its types, without silently changing the user's toolchain, deterministically, with actionable diagnostics when nightly/format is unavailable or incompatible, and without ever putting absolute paths or nightly requirements into the ordinary workspace.

## Goals

- Invoke rustdoc JSON per target under a pinned nightly with explicit feature/private-item options.
- Deserialize via `rustdoc-types`, gate on the **compatibility tuple**, and map items into schema entities/relations + diagnostics.
- Convert spans to validated repo-relative `SourceLocation`s; never expose absolute paths.
- Be fully testable from checked-in fixtures on stable Rust 1.97.0 without nightly.
- Default-fail on a failed selected target; `--keep-going` yields explicitly-marked partial output.

## Non-goals

- Cross-crate type resolution, dedup across crates, view building, or artifact assembly (issue 05).
- Writing `document.json`/`generation.json`/`diagnostics.json` (issue 05 / `cratevista-core`).
- Serving HTTP; exposing raw rustdoc JSON as the frontend contract; parsing rustdoc **HTML**.
- A runtime call graph or sequence diagrams.
- Making the whole workspace require nightly for build/fmt/clippy/CLI/unit tests.

## Current repository state (post-implementation)

- `cratevista-rustdoc` is **implemented** (`#![forbid(unsafe_code)]`). It depends on `cratevista-schema`, `rustdoc-types`, `serde`/`serde_json`, and `blake3` (dev: `tempfile`); `cargo tree -i` confirms **no** dependency on `cratevista-core`, `cratevista-metadata`, `cratevista-graph`, or `cratevista-server`.
- `rustdoc-types` is resolved to **`0.60.0`** in `Cargo.lock` (workspace dependency line `rustdoc-types = "0.60"`), and builds on stable Rust 1.97.0.
- The compatibility constants are implemented in `src/compat.rs` (`PINNED_NIGHTLY`, `EXPECTED_FORMAT_VERSION = 60`, `RUSTDOC_TYPES_RELEASE = "0.60"`, `ADAPTER_VERSION = 1`) with a compile-time `assert!(EXPECTED_FORMAT_VERSION == rustdoc_types::FORMAT_VERSION)`.
- **ADR-0004** (`docs/adr/0004-rustdoc-toolchain-policy.md`) exists and records the verified compatibility tuple and command form.
- Checked-in fixtures exist under `crates/cratevista-rustdoc/tests/fixtures/` (`sample_lib.rustdoc.json`, `sample_lib_private.rustdoc.json`, and the path-only `sample_lib/` crate + `examples/gen_fixtures.rs` regenerator). Stable, no-nightly tests and one gated `#[ignore]` live test are implemented and passing.
- `cratevista-schema` (implemented) provides everything this crate maps into: `Entity::new`, `Relation::new`, `EntityId::{module, item, impl_block, from_raw}`, `discriminator(domain, components)`, open `EntityKind`/`RelationKind` constants, `RepoRelativePath::new`, `SourceLocation`, `Span`, `DocBlock`, `LocalizedText`, `AttrValue = serde_json::Value`, `Provenance`, `DocumentDiagnostic::new`, `Severity`.
- `cratevista-metadata` (implemented) was the model for this crate's structure (`ingest`/pure `normalize`/`Ingest`/`Error`/`Summary`, `diagnostics::code`, deterministic sorting) — but is **not** a dependency here.
- Workspace: pinned stable **Rust 1.97.0** (`rust-toolchain.toml`, ADR-0010). ADRs present: 0001–0004, 0010.

## Ownership boundary

`cratevista-rustdoc` owns: `RustdocPlan` validation; per-target `cargo rustdoc` command construction/execution; JSON output discovery; compatibility verification; `rustdoc-types` deserialization; normalization; mapping rustdoc items into schema entities/relations/source-locations/diagnostics for the structure it reliably owns; rustdoc-specific cache inputs and diagnostics. Package/target **selection** (building the plan from `MetadataIngest`) is owned by issue 05/core, not here.

- **Depends on**: `cratevista-schema`, `rustdoc-types`, `serde`/`serde_json`, `blake3` (dev: `tempfile`). (`tracing` is a planned follow-up, not an MVP dependency.)
- **Must NOT depend on**: `cratevista-core`, `cratevista-metadata`, `cratevista-graph`, `cratevista-server`.
- **Must NOT**: assemble `ExplorerDocument`; write any artifact; serve HTTP; expose raw rustdoc JSON as the frontend contract; parse rustdoc HTML; require nightly for ordinary build/fmt/clippy/unit tests; use `cratevista_core::Diagnostic`; depend on `cratevista-metadata` to enumerate packages/targets.
- **`rustdoc-types` must not appear in any public signature.** Raw parsing/normalization (`load_raw`, `normalize_raw`) are `pub(crate)`; the public surface is JSON/path + CrateVista-owned types (`normalize_json`, `load_and_normalize`, `ingest`).

## Terminology

**Compatibility tuple**: `(pinned nightly toolchain, rustdoc JSON format version, rustdoc-types release, CrateVista adapter version)`. **Normalized companion**: the thin per-crate data (`CrateSummary` + unresolved references) issue 05 needs for cross-crate resolution. **Private-item mode**: `--document-private-items`.

## Stable vs nightly responsibilities

- The CrateVista application/CLI/libraries build, test, format, and lint on the **pinned stable** toolchain (Rust 1.97.0, ADR-0010) and never require nightly. `cratevista-rustdoc`'s library, its pure normalization path (`normalize_json`/`load_and_normalize`, backed by crate-private `normalize_raw`), and all non-gated tests compile and run on stable.
- A **separate, pinned nightly** executable is invoked **only** to generate rustdoc JSON for a *target project*. It is never installed automatically and never made a workspace-wide requirement. `doctor` (issue 01) probes it; only gated tests use it.

## Public API

Mirrors `cratevista-metadata`'s shape. Structured process args only — **no shell command strings**. The caller passes an **explicit execution plan** (`RustdocPlan`); this crate does **not** enumerate packages or reconstruct workspace topology.

```rust
/// The kind of a target to document. Open-ended (unknown kinds handled safely).
pub enum RustdocTargetKind { Library, ProcMacro, Binary, Other(String) }

/// One concrete target the orchestrator (issue 05/core) selected to document.
/// Carries the stable cratevista-schema identities the planner already knows, so
/// the graph can link back to exactly one Cargo target without string guessing.
pub struct RustdocTarget {
    pub package_id: EntityId,     // metadata package entity id (`package:{name}`)
    pub target_id: EntityId,      // metadata target entity id (`target:{package}:{kind}:{name}`)
    pub package_name: String,
    pub target_name: String,
    pub crate_name: String,       // the ACTUAL Cargo target crate name (not blindly `-`→`_`)
    pub target_kind: RustdocTargetKind,
    pub manifest_path: PathBuf,   // the package manifest (used for the cargo invocation)
    pub package_root: PathBuf,    // the package directory (used to resolve relative spans)
}

/// The concrete, pre-resolved plan. Prepared by issue 05/core from MetadataIngest.
pub struct RustdocPlan {
    pub workspace_root: PathBuf,
    pub targets: Vec<RustdocTarget>,
}

/// Options that are not per-target (features, private mode, toolchain, network).
pub struct RustdocOptions {
    pub features: FeatureSelection,   // { features: Vec<String>, all_features: bool, no_default_features: bool }
    pub include_private: bool,        // documents private items
    pub keep_going: bool,
    pub toolchain: Option<String>,    // Some(x) uses x; None uses the pinned nightly (compat::PINNED_NIGHTLY)
    pub target_dir: Option<PathBuf>,  // isolated rustdoc output dir
    pub network: NetworkMode,         // Inherit | Offline | Frozen | Locked
}

/// The context needed to purely normalize one crate's rustdoc JSON. Carries
/// both roots so spans resolve without any absolute path escaping.
pub struct NormalizeContext {
    pub workspace_root: PathBuf,
    pub package_root: PathBuf,
    pub package_id: EntityId,         // recorded on CrateSummary for graph linking
    pub target_id: EntityId,          // recorded on CrateSummary for graph linking
    pub package_name: String,
    pub crate_name: String,           // the actual Cargo target crate name
    pub target_name: String,
    pub target_kind: RustdocTargetKind,
    pub toolchain: String,
}

pub struct RustdocIngest {
    pub crates: Vec<CrateSummary>,             // thin per-crate normalized companion (unresolved refs, format, toolchain)
    pub entities: Vec<Entity>,                 // schema entities, sorted by id
    pub relations: Vec<Relation>,              // schema relations, sorted by id
    pub diagnostics: Vec<DocumentDiagnostic>,  // recoverable, sorted
    pub summary: RustdocSummary,
}

/// A **safe** structured summary — no absolute machine paths.
pub struct RustdocSummary {
    pub documented_crate_count: usize,
    pub entity_count: usize,
    pub relation_count: usize,
    pub succeeded_target_count: usize,
    pub failed_target_count: usize,
    pub partial: bool,                         // true when keep_going skipped ≥1 target
    pub include_private: bool,
    pub features: Vec<String>,                 // normalized (sorted) feature names
    pub network: NetworkMode,
    pub compat: CompatibilityTuple,            // recorded versions used
    pub targets: Vec<TargetOutcome>,
}

/// One per plan target.
pub struct TargetOutcome {
    pub target_id: EntityId,          // coherent with CrateSummary.target_id
    pub package_name: String,
    pub target_name: String,
    pub target_kind: RustdocTargetKind,
    pub succeeded: bool,
}

/// The verified compatibility tuple used for a run.
pub struct CompatibilityTuple {
    pub nightly: String,
    pub format_version: u32,
    pub rustdoc_types: String,
    pub adapter: u32,
}

/// Fatal errors; each carries a stable code (see "Diagnostics and errors").
pub enum RustdocError {
    NightlyMissing,
    ToolchainNotFound(String),
    RustdocInvocationFailed { argv: Vec<String>, stderr: String },
    UnsupportedFormatVersion { found: u32, supported: u32 },
    MalformedRustdocJson(String),
    OutputFileMissing(String),
    TargetNotFound(String),
    UnsupportedTargetKind(String),
    NoTargetSucceeded,
    InvalidPlan(String),
    InternalInvariant(String),
}
```

Entry point and layered APIs (public API is JSON/path + CrateVista-owned types only — **`rustdoc_types` never appears in any public signature**):

- **Plan ingestion**: `ingest(plan: &RustdocPlan, options: &RustdocOptions) -> Result<RustdocIngest, RustdocError>` — validates the plan (paths under `workspace_root`; no duplicate targets), then per target: invoke → load → normalize → aggregate. It validates the plan but does **not** reconstruct Cargo workspace topology.
- **Public normalization from JSON**: `normalize_json(json: &str, context: &NormalizeContext) -> Result<CrateIngest, RustdocError>` — **testable on stable, no nightly**.
- **Public load + normalize**: `load_and_normalize(path: &Path, context: &NormalizeContext) -> Result<CrateIngest, RustdocError>`.
- **Crate-private raw layer** (never public): `pub(crate) fn load_raw(json: &str) -> Result<rustdoc_types::Crate, RustdocError>` (deserialize + compat gate) and `pub(crate) fn normalize_raw(&rustdoc_types::Crate, &NormalizeContext) -> Result<CrateIngest, RustdocError>`. In-crate tests may call these directly; nothing outside the crate can.

`RustdocIngest` must **not** contain: an `ExplorerDocument`, a `cratevista_core::Diagnostic`, a `cratevista_metadata::MetadataIngest`, serialized artifact JSON, UI layout values, or any absolute machine path. Raw argv (which may contain absolute manifest/target/`CARGO_HOME`/workspace paths) lives **only** in local `tracing` and in runtime `RustdocError` messages (subject to the diagnostic privacy policy) — never in `RustdocSummary`, `RustdocIngest`, `GenerationReport`, or public artifacts.

## Normalized-model boundary decision

**Chosen: Option B — return schema `Entity`/`Relation` directly, plus a thin normalized companion.** `cratevista-rustdoc` maps rustdoc items straight into `cratevista-schema` entities/relations (consistent with `cratevista-metadata`, which returns `Entity`/`Relation`), emitting only relations it can resolve **within a single crate's rustdoc index**. A thin `CrateSummary` carries what cannot yet be expressed — per-crate metadata and **preserved unresolved type references** — for issue 05's cross-crate resolution.

Justification: matching issue 03's `Entity`/`Relation` output lets issue 05 uniformly concatenate metadata + rustdoc entities/relations, resolve cross-crate references, dedup, and assemble the document, without re-mapping a second full model. It also avoids duplicating the `rustdoc-types` model. The full item set the issue enumerates (module hierarchy, structs/enums/unions/traits/impls/functions/methods/fields/variants/aliases/consts/statics/macros, visibility, docs, attributes, spans, canonical paths, re-exports, fn inputs/outputs, implemented trait, target type) is expressed as entities + attributes + intra-crate relations; unresolved pieces are preserved (not invented).

## Target selection — the explicit `RustdocPlan`

`cratevista-rustdoc` does **not** enumerate packages or targets. The orchestrator (issue 05/`cratevista-core`) uses `cratevista-metadata`'s `MetadataIngest` to build a concrete `RustdocPlan` and passes it in. This crate documents exactly the targets present in the plan (after validating it), and never reconstructs Cargo topology.

**How the orchestrator normally builds the plan (issue 05, documented here for context):** by default it includes each workspace member's **library** and **proc-macro** targets (a proc-macro crate's target is a library with `proc-macro = true`); **binaries are explicit opt-in**; **examples/tests/benches are unsupported in MVP** and excluded. A member with no documentable library/proc-macro target simply contributes no `RustdocTarget`.

Behavior of `cratevista-rustdoc` given a plan:

| case | behavior |
|---|---|
| `RustdocTargetKind::Library` / `ProcMacro` | documented via `--lib` |
| `RustdocTargetKind::Binary` | documented via `--bin <name>` (only if the orchestrator opted it in) |
| `RustdocTargetKind::Other(_)` | fail-fast: fatal `unsupported_target_kind`; keep-going: recoverable `unsupported_rustdoc_item` + `target_failed`; never a crash |
| a target whose `cargo rustdoc` reports the target does not exist | fatal `TargetNotFound` |
| plan path outside `workspace_root`, or duplicate targets | fatal `InvalidPlan` (plan validation) |
| empty plan | `NoTargetSucceeded` (nothing to document is a fatal caller error) |

`cratevista-rustdoc` **validates** the plan (every `manifest_path`/`package_root` under `workspace_root`; no duplicate `(package, target)`), but performs **no** package discovery. Targets are **never** silently omitted: anything in the plan is either documented or produces a diagnostic/error.

Removed from MVP (decisions #5, #9): `document_dependencies` (documenting dependency crates requires the orchestrator to expand the plan from Cargo metadata — deferred to issue 05 or later); examples/tests/benches (unsupported); approximate `references_type` (deferred). Per-target execution is **sequential** in MVP; parallelism may be added later after measurement.

## Command construction

Structured `std::process::Command` args (no shell strings). **The verified command form is the Cargo-level form recorded in ADR-0004** (`src/invoke.rs::build_argv`). Normal execution uses exactly this form and never silently tries an alternate syntax:

```
cargo +<nightly> rustdoc -Z unstable-options --output-format json \
    --manifest-path <manifest> -p <package> \
    <--lib | --bin <name>> \
    --target-dir <dir> [feature/network flags] \
    -- [--document-private-items]
```

- The JSON output flags (`-Z unstable-options --output-format json`) and feature/network flags are on the **cargo side**, before `--`.
- **`--document-private-items` stays on the rustdoc side of the `--` separator** (only when `include_private` is set).
- Library/proc-macro targets use `--lib`; binary targets use `--bin <name>`.

The direct-`rustdoc` form (historically shown by the rustdoc-types docs, with the JSON flags after `--`) was **rejected** in favor of the Cargo-level form so feature/cfg/target resolution is respected — see "Alternatives considered".

- **Nightly selection** (`src/toolchain.rs`): `cargo +<toolchain> rustdoc …`. `RustdocOptions::toolchain = Some(x)` uses exactly `x`; otherwise exactly the **pinned nightly** (`compat::PINNED_NIGHTLY`, ADR-0004). CrateVista does **not** scan `rustup toolchain list` or read `RUSTUP_TOOLCHAIN` to pick a toolchain (that would silently break the format-version guarantee), and **never installs** one. Either way the parsed `format_version` is gated against `60`.
- **Target dir**: dedicated `--target-dir` (default `<workspace>/target/cratevista/rustdoc`) to isolate output and enable caching.
- **Environment**: the child process inherits the parent environment (PATH, `CARGO_HOME`, proxies, etc.). Toolchain selection is nonetheless explicit via `+<toolchain>`, so an inherited `RUSTUP_TOOLCHAIN` does **not** change which toolchain runs.
- **Offline/frozen/locked**: forwarded from `NetworkMode`.
- **Cancellation / capture**: run synchronously (per-target sequential in MVP); capture stdout+stderr; a non-zero exit or missing output is a `RustdocError` (with argv + stderr tail in the error message only). A stderr indicating a missing library/bin target → `TargetNotFound`; a missing toolchain → `NightlyMissing` (pinned) or `ToolchainNotFound` (explicit override).
- **Output discovery**: `<target-dir>/doc/<crate_name>.json` where `<crate_name>` is `RustdocTarget.crate_name` (the target's actual Cargo crate name, supplied by the planner). If absent after a success exit → `OutputFileMissing`.
- **Missing nightly**: `NightlyMissing` fatal with the **exact** actionable `rustup toolchain install <pinned-nightly>` command (in `RustdocError::remediation`); CrateVista never runs it.

## Compatibility tuple

A 4-tuple `CompatibilityTuple { nightly: String, format_version: u32, rustdoc_types: String, adapter: u32 }`.

**Verified tuple (empirically confirmed at implementation, ADR-0004):**

- pinned **nightly = `nightly-2026-07-01`** (`rustc 1.98.0-nightly (f46ec5218 2026-06-30)`).
- rustdoc JSON `format_version` = **`60`**.
- `rustdoc-types` release line = **`0.60`** (resolved **`0.60.0`** in `Cargo.lock`).
- adapter version = **`1`**.

These are implemented as constants in `src/compat.rs` (`PINNED_NIGHTLY`, `EXPECTED_FORMAT_VERSION`, `RUSTDOC_TYPES_RELEASE`, `ADAPTER_VERSION`).

1. **How it was verified (implementation-time):** added `rustdoc-types = "0.60"` (resolved `0.60.0`, builds on stable Rust 1.97.0); generated JSON for the `sample_lib` fixture crate under `nightly-2026-07-01`; confirmed `Crate.format_version == 60`; deserialized it with `rustdoc-types 0.60.0`; and normalized it. A compile-time `assert!(EXPECTED_FORMAT_VERSION == rustdoc_types::FORMAT_VERSION)` locks the constant to the linked crate.
2. **Where recorded:** `docs/adr/0004-rustdoc-toolchain-policy.md` (authoritative tuple + verified command form) + the `src/compat.rs` constants; mirrored in the PRD Status.
3. **Runtime check:** after reading the format version (a lightweight probe before the full parse), `compat::check_format_version` compares it to `compat::EXPECTED_FORMAT_VERSION` (= `60`). Mismatch → fatal `UnsupportedFormatVersion { found, supported }`.
4. **Failure message:** `rustdoc JSON format version {found} is not supported (adapter expects 60); install the supported nightly: rustup toolchain install nightly-2026-07-01` — names both sides of the tuple and the exact remediation.
5. **Update procedure (future maintenance):** to move to a new nightly/format, bump `rustdoc-types` + `compat.rs` constants + pinned nightly + fixtures + ADR-0004 **together**; re-run the fixture parse/normalize tests + the gated live E2E; record the new tuple. `Cargo.lock` captures the resolved patch. A tuple upgrade that changes rustdoc's chosen canonical paths may require an architecture-ID migration note.

## Technical design

### Module boundaries

```
crates/cratevista-rustdoc/src/
  lib.rs         # public API: ingest(); re-exports; per-target orchestration + aggregation
  options.rs     # RustdocPlan/RustdocTarget/RustdocTargetKind, RustdocOptions, NormalizeContext,
                 #   FeatureSelection/NetworkMode; plan + option validation
  error.rs       # RustdocError + stable codes + remediation
  diagnostics.rs # recoverable DocumentDiagnostic codes + builder
  result.rs      # RustdocIngest, RustdocSummary, CrateIngest, CrateSummary, CompatibilityTuple,
                 #   TargetOutcome, UnresolvedTypeRef
  compat.rs      # compatibility-tuple constants + format-version gate (compile-time assert)
  toolchain.rs   # nightly selection: explicit override else pinned nightly (no scan, never installs)
  invoke.rs      # cargo rustdoc command construction, execution, output discovery
  load.rs        # deserialize rustdoc JSON (crate-private load_raw/normalize_raw) + public normalize_json/load_and_normalize
  ids.rs         # rustdoc item → schema entity-kind + impl signature (canonical paths, impl discriminators)
  spans.rs       # rustdoc Span → RepoRelativePath / SourceLocation (+ omission policy)
  types.rs       # rustdoc type → normalized TypeRef + intra-crate resolution/unresolved refs
  normalize.rs   # pure: &rustdoc_types::Crate + context → CrateIngest (entities/relations/diagnostics)
  cache.rs       # cache_key(target, options, compat, input_digest) over all semantic inputs (BLAKE3)
```

Depends on `cratevista-schema`, `rustdoc-types`, `serde`/`serde_json`, and `blake3` (dev: `tempfile`). Not on core/metadata/graph/server. (No `tracing` dependency is wired in the MVP — see "Observability and diagnostics".)

### Data model (thin normalized companion)

```
CrateSummary {
  package_id: EntityId,       // metadata package entity id (from the RustdocTarget)
  target_id: EntityId,        // metadata target entity id (from the RustdocTarget)
  root_module_id: EntityId,   // the actual normalized root-module entity id
  crate_name: String,
  format_version: u32,
  toolchain: String,
  entity_count: usize,
  relation_count: usize,
  unresolved_refs: Vec<UnresolvedTypeRef>,   // for issue-05 cross-crate resolution
}

enum TypeReferenceRole { Field, Parameter, Return, Error, AssociatedType, ImplFor, ImplTrait }

// Structured cross-crate evidence extracted from rustdoc's ItemSummary/path map
// (never by re-parsing `display`). No numeric id, no absolute path.
UnresolvedTypeRef {
  from: EntityId,
  role: TypeReferenceRole,
  crate_name: Option<String>,        // the referenced Rust crate, when rustdoc records it
  canonical_path: Option<Vec<String>>, // ItemSummary.path components (crate segment first)
  item_kind: Option<EntityKind>,     // the referenced item's kind, when known
  display: String,                   // presentation/debug only, not the resolver key
}
```

The heavy structure is expressed directly as schema `Entity`/`Relation` (below); `CrateSummary` carries the stable identities the graph needs to link this crate to exactly one Cargo target, plus structured unresolved references. During aggregation the adapter copies `package_id`/`target_id` from the explicit `RustdocTarget`, sets `root_module_id` to the actual normalized root module, and fails with `internal_invariant` if a successful crate has no trustworthy root module; it never reconstructs target ownership from `crate_name` alone.

### Stable identity mapping

Raw `rustdoc_types::Id` (numeric) are **internal lookup keys only** (a per-crate `BTreeMap<Id, EntityId>`), never public. Public ids use schema constructors:

- module: `EntityId::module(crate, canonical_path)`
- item: `EntityId::item(kind, crate, canonical_path)` (kind from the open `EntityKind` set: struct/enum/union/trait/function/method/type_alias/constant/static/macro)
- impl: `EntityId::impl_block(crate, trait_or_inherent, for_type, normalized_signature)` — the normalized-signature BLAKE3 discriminator (via schema `discriminator`) is **always present**, so multiple/inherent/blanket impls for one type never collide.

Enumerated resolutions:

- **anonymous / unnamed impls, inherent impls, multiple impls for one type, blanket impls**: `impl:{crate}:{"inherent"|trait_path}:{for_type}:{disc}` where `disc = discriminator("impl-sig", &[normalized_signature])` (generics + where-clauses + trait + self type). Blanket impls use the generic self type in the signature and are marked `attributes.synthetic = "blanket"` (excluded from default views by issue 07).
- **methods with identical names in different impls**: the method's parent is its impl entity, so its `canonical_path` includes the impl-scoped path → distinct ids (and the impl discriminator disambiguates).
- **associated types / consts**: child items of the trait/impl, with `item:{assoc_type|assoc_const}:{crate}::{path}`.
- **re-exported items**: one **canonical** entity at the item's true canonical path; the re-export site yields a `re_exports` relation (not a duplicate entity). See "Re-exports".
- **macros**: `item:macro:{crate}::{path}` where available.
- Ids never derive from rustdoc index/HashMap iteration order or numeric id order.

**Canonical-path derivation hierarchy (deterministic).** Do **not** skip every local item merely absent from `Crate.paths` (important in `include_private` mode). For each item, derive its path by the first applicable rule:

1. Use the rustdoc **path map** (`Crate.paths`) when it has an entry.
2. Otherwise derive a **named local item's** path through **module/trait/impl containment** (walk parents to build the canonical path).
3. Derive **associated-item** paths (assoc type/const/method) from the **parent** entity's path.
4. Derive **impl** ids from the normalized semantic signature (`impl_block` discriminator).
5. **Only** when no deterministic semantic path can be reconstructed → emit `missing_canonical_path` and skip that item (recoverable, not fatal).

The chosen rustdoc path is **implementation-defined and may change when the pinned compatibility tuple changes**. Ids are expected to be **stable for unchanged source under the same tuple**; a **tuple upgrade may require an architecture-ID migration note** (documented in ADR-0004 / the changelog when it happens).

### Paths and source spans

`rustdoc_types::Span { filename, begin, end }`. `filename` may be **absolute** or **relative** depending on the rustdoc invocation. `NormalizeContext` therefore carries **both** `workspace_root` and `package_root`, and `spans.rs` maps:

- **absolute** filenames: if contained under `workspace_root`, strip that prefix → `RepoRelativePath::new(...)` → `SourceLocation`; else outside-workspace (below).
- **relative** filenames: resolve from `package_root`, normalize, then strip `workspace_root` → `SourceLocation`.
- **outside the workspace root** (e.g. a dependency source) → omit `SourceLocation` + recoverable `source_outside_workspace`.
- **generated / macro-expanded / synthetic** (no real file, or a `<...>` pseudo-path) → omit + recoverable `generated_source_omitted`.
- `RepoRelativePath` validation is never weakened; a residual invalid path drops the source with a diagnostic and keeps the entity. **Absolute paths never appear or are hashed into any entity, relation, diagnostic, id, or summary field.**

### Entities and relations (what issue 04 owns)

Entities (`Provenance::Discovered`): module, struct, enum, union, trait, impl, function, method, type_alias, constant, static, macro. Attributes carry visibility (`visibility`), presentation attrs (deprecated, doc(hidden), doc aliases, `synthetic`), generics (display), and — for functions/methods — the signature (`inputs`, `output`, `is_result`) as attributes. `docs` → `DocBlock { markdown, summary?, documented }`.

Relations, **only when the target resolves within the same crate's rustdoc index** (else preserved as an unresolved ref/diagnostic, never invented):

- `contains` — module→child item, package/crate→root module, impl→method/assoc item.
- `implements` / `implemented_for` — impl→trait, impl→self type.
- `has_field_type` — struct/union field → field type.
- `accepts_type` / `returns_type` / `error_type` — function/method → param / return / `Result` error type.
- `re_exports` / `imports` — re-export site → canonical entity.

Only **reliable typed relations** above are emitted. The approximate `references_type` relation (dyn-trait mentions, generic-arg references) is **deferred** (decision #9) — those references are preserved in `CrateSummary.unresolved_refs` / diagnostics for a future issue, never emitted as approximate edges in MVP.

### Type relationships — issue 04 vs issue 05

- **Issue 04 extracts reliably (intra-crate):** struct/union field types, fn param/return types, `Result` ok/err decomposition, trait-impl and inherent-impl relations, associated types, generic arguments (as attributes), `dyn` trait references, references/pointers/arrays/tuples (structurally, via `TypeRef`).
- **Issue 05 constructs:** cross-crate type resolution (using multiple crates' indices), dedup across crates, external-type edges, view projections.
- **Unresolved** (target not in this crate's index) → preserved `UnresolvedTypeRef` + `unresolved_type_reference` diagnostic; **never an invented edge**. No runtime call graph.

### Re-exports and canonical entities

One **canonical** entity per rustdoc item at its true canonical path. A re-export produces exactly one `re_exports` relation from the exporting module to the canonical entity; alias names are stored as `attributes.aliases`. No duplicate entity is created merely because an item is reachable through multiple paths. If a re-export target is missing from the index → recoverable `reexport_target_missing`.

### Documentation and Markdown

Docs preserved as Markdown in `DocBlock.markdown` (`documented = false` for empty docs, `summary` = first paragraph). Intra-doc links and code blocks preserved verbatim (as text). Handling: `#[doc(hidden)]` → `attributes.doc_hidden = true` (kept only in `include_private` mode, else the item is stripped like a private item); deprecated → `attributes.deprecated`; doc aliases → `attributes.aliases`; stripped private items respected by `include_private`; synthetic blanket/auto-trait impls → `attributes.synthetic` (excluded from default views by issue 07). **Doctests are never executed.**

### Diagnostics and errors

**Fatal `RustdocError`** (stable codes): `nightly_missing`, `toolchain_not_found`, `rustdoc_invocation_failed`, `unsupported_format_version`, `malformed_rustdoc_json`, `output_file_missing`, `target_not_found`, `unsupported_target_kind`, `no_target_succeeded`, `invalid_plan`, `internal_invariant`.

**Recoverable `DocumentDiagnostic`** (stable codes, sorted): `source_outside_workspace`, `generated_source_omitted`, `missing_canonical_path`, `unresolved_type_reference`, `duplicate_item_identity`, `unsupported_rustdoc_item`, `incomplete_item_metadata`, `target_failed` (under keep-going), `reexport_target_missing`. Never `cratevista_core::Diagnostic`; never embedded in an `ExplorerDocument`.

### Failure and partial-result policy

- **Default:** any failed selected target is **fatal** (`RustdocError` → the caller exits 1).
- **`keep_going = true`:** failed targets become `target_failed` diagnostics; successful targets are returned; `RustdocSummary.partial = true`.
- A run where **no** target succeeds is fatal even under keep-going (`NoTargetSucceeded`).
- Partial output is always marked (`RustdocSummary.partial`), and issue 05 propagates it into `GenerationReport.partial`. Incomplete output never looks complete.

### Determinism

Equivalent rustdoc JSON + options → equivalent ordered output. Never rely on rustdoc index/HashMap order, numeric id order, JSON object order, filesystem order, hash seed, absolute paths, or output timestamps. Sort entities, relations, `unresolved_refs`, diagnostics, attribute maps, and summary collections by documented stable keys (ids via `Ord`; names lexicographically).

### Caching boundary

Cacheable stages: raw rustdoc JSON (per target), parsed `rustdoc_types::Crate`, and the normalized `CrateIngest`. A pure `cache_key(&RustdocTarget, &RustdocOptions, &CompatibilityTuple, inputs) -> String` includes: package/target, selected features, `include_private`, nightly toolchain, rustdoc format version, and relevant Cargo inputs (source file hashes + `Cargo.lock` hash). Issue 04 defines the key computation + cache metadata only; **watch-mode orchestration is issue 09**.

### Security and privacy

Read-only w.r.t. project runtime: rustdoc runs the compiler front-end on project source (as rustdoc always does) but CrateVista executes no project bins/tests/doctests. No absolute paths in public data. Raw JSON stays internal (issue 06 exposes no raw endpoint). rustdoc HTML is never parsed.

## CLI/API/configuration changes

Adds `--document-private-items`, `--toolchain`, `--keep-going`, and feature-flag semantics to `generate` (surfaced by the CLI in issue 05 wiring; not wired in this issue). The chosen toolchain, format version, compatibility tuple, and `partial` flag are recorded in `generation.json` (issue 05 writes it). `cratevista.toml` reserves `[rustdoc] toolchain=…, document_private_items=false` (issue 08 binds). `document_dependencies` is **not** an MVP option (dependency documentation is deferred). Adds `rustdoc-types = "0.60"` (+ `serde`/`serde_json`) to `[workspace.dependencies]` at implementation time; `Cargo.lock` records the resolved patch.

## Files and modules to create or modify

As implemented:

- `crates/cratevista-rustdoc/src/{lib,options,error,diagnostics,result,compat,toolchain,invoke,load,ids,spans,types,normalize,cache}.rs`
- `crates/cratevista-rustdoc/Cargo.toml`: `cratevista-schema`, `rustdoc-types`, `serde`, `serde_json`, `blake3` (dev: `tempfile`).
- `Cargo.toml` (root): `rustdoc-types = "0.60"` in `[workspace.dependencies]` (resolved `0.60.0` in `Cargo.lock`).
- `crates/cratevista-rustdoc/tests/fixtures/`: checked-in `sample_lib.rustdoc.json` + `sample_lib_private.rustdoc.json` (path-sanitized), the path-only `sample_lib/` fixture crate, and `examples/gen_fixtures.rs` (regenerator recording the exact nightly used).
- `crates/cratevista-rustdoc/tests/{normalize,ids_and_impls,private_items,plan_and_failure,determinism,live}.rs` + `tests/common/mod.rs`. (`compat`/`spans`/`types`/`cache`/`toolchain`/`load` unit tests live inline in their `src/*.rs` modules.)
- `docs/adr/0004-rustdoc-toolchain-policy.md` (the verified compatibility tuple + command form + update procedure).

## Testing strategy

### Unit tests (stable, no nightly)

- `compat`: matching format version accepted; mismatch → `UnsupportedFormatVersion` with both versions.
- `spans`: inside-root → `SourceLocation`; outside-root → omitted + `source_outside_workspace`; generated → omitted + `generated_source_omitted`; validation preserved.
- `ids`: module/item/impl ids; impl discriminator distinguishes multiple/inherent/blanket impls; methods with same name in different impls get distinct ids; missing canonical path → skip + diagnostic.
- `types`: field/param/return/Result decomposition; unresolved ref preserved (no invented edge).

### Integration tests (checked-in fixtures, no nightly, no network)

- `normalize_json` over fixtures covering: modules; structs+fields; enums+variants; unions; traits; inherent impls; trait impls; multiple impls for one type; functions+methods; params+returns; `Result` error type; generics; `dyn` traits; type aliases; consts/statics; macros; re-exports; deprecated/hidden/documented/undocumented items; source spans (absolute + relative); missing canonical paths; unknown/future-like item forms (where practical); malformed JSON → `MalformedRustdocJson`; incompatible format version → `UnsupportedFormatVersion`. (In-crate unit tests may call `pub(crate) normalize_raw`/`load_raw`; external integration tests use the public `normalize_json`.)
- **Plan validation**: paths outside `workspace_root` / duplicate targets → `InvalidPlan`; empty plan → `NoTargetSucceeded`.
- **Determinism**: a fixture with reordered index entries → identical `RustdocIngest`.
- Assert no absolute path in any entity/relation/diagnostic and in `RustdocSummary`.

### End-to-end tests (gated; nightly + a tiny fixture crate)

- `#[ignore]` live test: run real `cargo +<nightly> rustdoc … --output-format json` on the small fixture crate with the approved tuple, `load` + `normalize`, and assert the shape. Skipped by default; needs nightly but **no network** (path-only). Run with `--ignored`.

### Fixtures

Checked-in `*.rustdoc.json` for a small crate exercising all kinds; produced by `examples/gen_fixtures.rs` (or a documented script) with the pinned nightly, path-sanitized (as metadata's fixtures are) so no absolute path is committed.

## Performance considerations

rustdoc invocation dominates; run per target (bounded parallelism allowed later). `normalize` is pure and cacheable; the cache key computation is defined here.

## Observability and diagnostics

Recoverable diagnostics carry stable codes + affected entity ids and are sorted. Toolchain, format version, compatibility tuple, and `partial` flag are surfaced via `RustdocSummary` and recorded in `generation.json` by issue 05 — never in the deterministic `document.json`. The raw `cargo rustdoc` argv (which may contain absolute paths) is confined to `RustdocError` messages only.

> **MVP note:** the crate does **not** yet depend on or emit `tracing`. Structured per-target `tracing` spans (argv + duration + item counts) are a planned follow-up; add the `tracing` dependency and instrument `invoke.rs`/`ingest` when wiring observability with the CLI in issue 05.

## Documentation changes

`docs/adr/0004-rustdoc-toolchain-policy.md` (compatibility tuple + process); README "rustdoc/nightly requirement" section (coordinated with issue 10); `doctor` documents how to install the pinned nightly.

## Rollout and migration

New crate. When the nightly/format changes, bump `rustdoc-types` + pinned nightly + fixtures + ADR-0004 together and re-run tests.

## Risks and mitigations

- **rustdoc JSON instability** → single pinned format via the compatibility tuple + hard gate + fixtures.
- **Accidental nightly requirement for the workspace** → library + pure normalization (`normalize_json`/`load_and_normalize`/`normalize_raw`) + all non-gated tests build/run on stable 1.97.0; nightly only in gated tests and at runtime.
- **Absolute paths leaking** → strip-prefix + `RepoRelativePath`; outside/generated omitted; test asserts no absolute path.
- **Unstable ids** → canonical-path + always-present impl discriminator; determinism test; no iteration-order ids.
- **Invented cross-crate edges** → intra-crate only; unresolved preserved as refs/diagnostics.

## Alternatives considered

- Depending on `cratevista-metadata` for target enumeration: rejected — violates the ownership boundary; selection is resolved by issue 05/core and passed as a `RustdocPlan`.
- Invoking `rustdoc` directly: rejected — `cargo rustdoc` respects features/cfg/target resolution.
- Returning only a rustdoc-specific normalized model (Option A): rejected — returning schema `Entity`/`Relation` (Option B) matches issue 03 and avoids a second mapping in issue 05; a thin `CrateSummary` covers unresolved cross-crate refs.
- Supporting multiple format versions or a `--force` past mismatches: rejected for MVP.
- Documenting dependencies: rejected for MVP entirely (expensive; requires expanding the plan from Cargo metadata). `document_dependencies` is **not** an MVP option — deferred to a later issue, not opt-in.

## Implementation sequence

1. `options` + `error` + `diagnostics` + `result` + `compat` (tuple constant).
2. `ids` + `spans` + `types` (pure helpers, unit-tested).
3. `normalize` against checked-in fixtures (no nightly).
4. `load` (deserialize + gate).
5. `toolchain` + `invoke` + `ingest`; gated live E2E; `examples/gen_fixtures.rs`.
6. ADR-0004 + workspace dep + README/doctor notes.

## Acceptance criteria

- [x] `cratevista-rustdoc` depends on `cratevista-schema` and **not** on core/metadata/graph/server. *(`cargo tree -i` for all four reports no path)*
- [x] Library + pure `normalize` + all non-gated tests build and run on **stable 1.97.0** (no nightly). *(CI stable job; `cargo +1.97.0 check`)*
- [x] A representative rustdoc JSON fixture deserializes and normalizes into schema entities/relations. *(integration tests)*
- [x] Compatibility tuple defined + enforced; incompatible format version → actionable fatal error naming both versions + the nightly remediation. *(compat test)*
- [x] Missing nightly → actionable error with the exact `rustup` command; **never** auto-installs. *(error test; no install code path)*
- [x] Public and private-item modes handled. *(two fixtures / two normalize contexts)*
- [x] Source spans → validated repo-relative `SourceLocation`; outside/generated omitted + diagnostic; no absolute path anywhere. *(spans + no-absolute-path tests)*
- [x] Stable ids from canonical paths; multiple/inherent/blanket impls and same-named methods never collide; missing-path items skipped with a diagnostic (recoverable). *(ids tests)*
- [x] Re-exports yield one canonical entity + a `re_exports` relation, no duplicate entity. *(reexports test)*
- [x] Intra-crate type relations extracted (reliable typed relations only; `references_type` deferred); unresolved references preserved (never invented edges). *(types test)*
- [x] `rustdoc-types` absent from **every** public signature; `load_raw`/`normalize_raw` are `pub(crate)`; public surface is `ingest`/`normalize_json`/`load_and_normalize`. No rustdoc HTML parsed; no artifact written. *(API review + `pub` audit)*
- [x] `cratevista-rustdoc` takes an explicit `RustdocPlan`, validates it, and does **not** depend on `cratevista-metadata` or enumerate packages. *(`cargo tree -i cratevista-metadata` no path; `InvalidPlan` test)*
- [x] `RustdocSummary`/`RustdocIngest` contain no absolute machine paths; raw argv lives only in tracing/error messages. *(no-absolute-path test on the summary + entities)*
- [x] Canonical-path derivation follows the documented hierarchy (path map → containment → parent → impl signature → skip); items are not skipped merely for being absent from `Crate.paths`. *(ids/private-mode tests)*
- [x] Default-fail on a failed target; `--keep-going` returns successful targets, marks `partial`, and diagnoses each failure; a fully-failed run is fatal. *(failure tests)*
- [x] Deterministic output under reordered index input. *(determinism test)*
- [x] Cache-key computation includes all semantic inputs. *(cache-key test)*
- [x] Gated live E2E generates and normalizes real rustdoc JSON with the approved tuple (no network). *(`--ignored` test)*

Verification (implementation must pass under the pinned stable toolchain, ADR-0010):

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-features                 # no nightly required
cargo +1.97.0 check --workspace --all-features
cargo tree -p cratevista-rustdoc -i cratevista-core     # no path
cargo tree -p cratevista-rustdoc -i cratevista-metadata # no path
cargo tree -p cratevista-rustdoc -i cratevista-graph    # no path
cargo tree -p cratevista-rustdoc -i cratevista-server   # no path
# gated: cargo test -p cratevista-rustdoc --test live -- --ignored   # needs nightly, no network
```

## Open questions

**None — all material choices are resolved and implemented.** The compatibility tuple was empirically verified (`rustdoc-types 0.60.0` / `format_version 60` / adapter `1` / nightly `nightly-2026-07-01`): the implementation generated the fixture JSON, confirmed `format_version == 60`, deserialized with `0.60.0`, and recorded the exact nightly in ADR-0004. (Resolved: `references_type` deferred; per-target sequential; `document_dependencies` removed from MVP; binaries opt-in; examples/tests/benches unsupported.)

## Traceability

Issue-04 checkboxes → tests above. `RustdocIngest` (entities/relations/diagnostics + `CrateSummary` unresolved refs) is consumed by issue 05's `build_document`; `partial` propagates into `GenerationReport.partial`; the compatibility tuple is surfaced by `doctor` (01) and the README (10); the cache-key computation is reused by issue 09.
