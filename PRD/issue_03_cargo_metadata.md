# PRD — Ingest Cargo workspace metadata

## Status

**Implemented / Verified** (2026-07-12). `cratevista-metadata` is implemented and all acceptance criteria are checked with verified evidence. Full workspace gates pass under the pinned stable toolchain (Rust 1.97.0): `cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets --all-features -- -D warnings`, `cargo test --workspace --all-features` (**70 tests**, 22 in `cratevista-metadata` + 1 gated live E2E), `cargo +1.97.0 check`. `cargo tree -i` confirms **no** dependency on core/rustdoc/graph/server. Resolved: `cargo_metadata 0.23.1` (in `Cargo.lock`).

**Deviations from the PRD (documented):** (1) the `entities`/`relations`/`select` logic is consolidated into `normalize.rs` rather than separate `entities.rs`/`relations.rs`/`select.rs` modules (cohesion; the module set is `lib, options, error, diagnostics, result, invoke, ids, source, normalize`); (2) the `bench` **bool** target attribute is omitted because `cargo_metadata::Target` exposes no such field (bench is a target *kind*); (3) a sixth fatal `MetadataError::InvalidOptions` (code `invalid_options`) was added for feature-option validation, a superset of the five PRD codes; (4) registry/git *full-metadata* fixtures are not generated offline — source classification/portability/collision are covered by unit tests (`ids.rs`, `normalize.rs`) plus the `external_path` fixture; fixtures are produced by a committed `examples/gen_fixtures.rs` (path-only, path-sanitized to a stable `/w` root).

### Approved decisions (recorded)

Finalized against the implemented workspace under the **latest-stable Rust policy (ADR-0010): Rust 1.97 / pinned `1.97.0`**. All material decisions recorded consistently:

- `cargo_metadata = "0.23"` (verified current release line; resolved `0.23.1`, `Cargo.lock` authoritative).
- Public API centered on `ingest(&MetadataOptions) -> Result<MetadataIngest, MetadataError>` and a **fallible** pure `normalize(&Metadata, &MetadataOptions) -> Result<MetadataIngest, MetadataError>`.
- `MetadataIngest { entities, relations, diagnostics: Vec<DocumentDiagnostic>, summary }`; compact `MetadataSummary` (counts + selection, no full feature map); feature detail on package entities with deterministic ordering; no feature-to-feature edges.
- `ExternalDepsMode::Exclude` default (`DirectOnly`/`FullGraph` opt-in, consistent boundary omission); default target kinds `{lib, bin, proc-macro}` (examples/tests/benches/build-scripts opt-in; build scripts never executed).
- One `RelationKind::DEPENDS_ON` per `normal`/`dev`/`build` role (+ BLAKE3 discriminator for differing `target_cfg`); distinct evidence never discarded.
- Opaque Cargo `Source::repr` (broad `source_kind` only); absolute paths never published/hashed; non-portable path identity → diagnostic + documented fallback; ID conflicts detected, never silently overwritten.
- Depends on `cratevista-schema` (+ `cargo_metadata`/serde); not on core/rustdoc/graph/server; writes no artifacts; reuses schema types/constructors.
- Library-level `MetadataOptions` only — no new CLI flags in this issue. **Do not implement metadata ingestion until scheduled.**

Reviewed (2026-07-12) against the **implemented** PRD 01 + PRD 02 repository state. It uses the real `cratevista-schema` public API (`Entity::new`, `Relation::new`, `EntityId::{workspace,package,external_package,external_package_disambiguated,target}`, `RelationId::{basic,with_role,with_role_and_discriminator}`, open `EntityKind`/`RelationKind` constants, `RepoRelativePath`, `SourceLocation`, `AttrValue = serde_json::Value`, `Provenance`, `DocumentDiagnostic`). Key corrections from the earlier draft: `cargo metadata` has **no `--package` flag** (package selection is in-process filtering); the crate returns a **named `MetadataIngest` result carrying `DocumentDiagnostic`s** (not a tuple with a generic `Diagnostic`); external path-dependency portability, git dependencies, and a fatal-vs-recoverable diagnostic split are now specified. No new CLI flags are wired in this issue (library-level options only).

## Source issue

`ISSUES/issue_03_cargo_metadata.md`

## Summary

Implement `cratevista-metadata`: invoke/consume `cargo metadata --format-version 1`, normalize Cargo workspace/package/target/dependency data, and map it into **deterministically-ordered** `cratevista-schema` entities and relations plus Cargo-specific `DocumentDiagnostic`s. The crate returns normalized Rust values via a named result type; it does **not** assemble or write `document.json`/`generation.json`/`diagnostics.json` (issue 05 / `cratevista-core` do that).

## Problem statement

CrateVista must know the workspace root, members, packages, targets, features, and the resolved dependency graph — from Cargo's machine-readable metadata, deterministically, with clear errors, without executing project code, and without leaking absolute machine paths.

## Goals

- Invoke `cargo metadata --format-version 1` with explicit options and consume its output.
- Produce deterministically-ordered schema entities (workspace, packages, targets) and relations (`contains`, `depends_on`) plus `DocumentDiagnostic`s, via a named `MetadataIngest` result.
- Portable, stable identities that survive version bumps and never embed absolute paths.
- Configurable external-dependency inclusion with a readable default.
- Robust handling of the enumerated edge cases with a clear fatal-vs-recoverable split.

## Non-goals

- rustdoc/item-level analysis (issue 04).
- Assembling/writing any artifact or building an `ExplorerDocument` (issue 05).
- Computing coordinates, views, or layout.
- Wiring new `generate` CLI flags (deferred to issue 05; this issue defines library-level options only).

## Current repository state

- `cratevista-metadata` is an empty placeholder crate (`#![forbid(unsafe_code)]`, no dependencies).
- `cratevista-schema` (implemented, issue 02) provides everything this crate maps into:
  - `Entity::new(id, kind, label: LocalizedText, qualified_name, provenance)` + public fields (`parent`, `source`, `docs`, `tags`, `attributes: BTreeMap<String, AttrValue>`, `description`).
  - `Relation::new(kind, from, to, provenance)` (sets the basic id) + public fields (`role: Option<String>`, `label`, `attributes`).
  - `EntityId::{workspace, package, external_package, external_package_disambiguated, target, module, item, manual, from_raw}`; `RelationId::{basic, with_role, with_role_and_discriminator, from_raw}`.
  - `EntityKind::{WORKSPACE, PACKAGE, TARGET, …}` / `RelationKind::{CONTAINS, DEPENDS_ON, …}` (open, string-backed; `new`, `as_str`, `is_known`).
  - `RepoRelativePath::new(&str) -> Result<Self, SourcePathError>` (rejects absolute/drive/UNC/`..`), `SourceLocation::new(path, Option<Span>)`, `Span`.
  - `DocumentDiagnostic::new(severity, code, message)` + `entities`/`relations` fields; `Severity::{Error, Warning, Info}`.
  - `Provenance::{Discovered, Manual}`; `AttrValue = serde_json::Value`.
- `cratevista-core` is the orchestration layer that will call this crate; the runtime `cratevista_core::Diagnostic` must **not** be used here.

## Required ownership boundary

`cratevista-metadata` owns: invoking/consuming `cargo metadata --format-version 1`; normalizing workspace/package/target/dependency data; mapping it into schema entities/relations; Cargo-specific diagnostics and discovery results.

- **Depends on**: `cratevista-schema`, `cargo_metadata`, `serde`/`serde_json`, and (optional) `tracing` for spans.
- **Must NOT depend on**: `cratevista-core`, `cratevista-rustdoc`, `cratevista-graph`, `cratevista-server`.
- **Must NOT** write `document.json`/`generation.json`/`diagnostics.json`, nor use `cratevista_core::Diagnostic`.

`camino::Utf8PathBuf` is used via `cargo_metadata` (its paths are already UTF-8), simplifying repo-relative mapping.

## Terminology

Per CONTEXT. **Workspace member** vs external **package**; **default members**; **target kind** (lib, bin, example, test, bench, proc-macro, custom-build); **dependency kind** (normal, dev, build).

## User-visible behavior

No new CLI surface in this issue. `cargo cratevista generate` remains the issue-01 stub until issue 05 wires it to `cratevista-core::run_generate`, which will consume this crate's `MetadataOptions`. The behaviors below describe the **library** semantics that issue 05 will surface.

## Functional requirements

1. Invoke `cargo metadata --format-version 1` via `cargo_metadata::MetadataCommand` (which pins the format version), configured from `MetadataOptions` (manifest path, cwd, features, network mode). **`cargo metadata` has no `--package`/`--workspace` flag** — package selection is performed **in-process** by filtering the returned `Metadata`.
2. Consume the result through the `cargo_metadata` crate (never hand-parse `Cargo.toml`).
3. Normalize into schema entities/relations + diagnostics; return a `MetadataIngest` (see API). A pure `normalize(&cargo_metadata::Metadata, &MetadataOptions) -> MetadataIngest` enables hermetic tests and caching (issue 09).
4. Apply package selection and external-dependency mode.
5. Map to schema: one `workspace` entity; `package` entities; `target` entities; `contains` relations (workspace→member package, package→target); `depends_on` relations from the **resolved** graph. Source paths become `SourceLocation` only when inside the selected workspace root.
6. **Deterministic ordering** independent of HashMap iteration, Cargo JSON ordering, filesystem traversal, OS, or process seed: entities sorted by id, relations sorted by id, diagnostics sorted (`DocumentDiagnostic` is `Ord`). The canonical serializer is used only later (issue 05); this crate returns normalized Rust values.
7. Do **not** execute project binaries, tests, examples, benches, or build-script *logic* for CrateVista's purposes. Document that `cargo metadata` itself performs Cargo's normal dependency resolution (and may compile build-script metadata) unless `--offline`/`--frozen` is requested; CrateVista never runs the project's own bins/tests.

## Technical design

### Module boundaries

```
crates/cratevista-metadata/src/
  lib.rs         # public API: ingest(), re-exports of the result/options types
  options.rs     # MetadataOptions + selection/feature/external/network/target-kind enums
  invoke.rs      # run cargo metadata via cargo_metadata::MetadataCommand; map failures to MetadataError
  select.rs      # apply PackageSelection + ExternalDepsMode -> the included package set
  source.rs      # workspace-relative SourceLocation mapping (strip_prefix + RepoRelativePath) + omission policy
  ids.rs         # metadata id construction wrapping schema EntityId/RelationId (incl. portability policy)
  entities.rs    # workspace/package/target -> Entity
  relations.rs   # contains + depends_on -> Relation (role/attributes)
  diagnostics.rs # stable diagnostic codes; helpers building DocumentDiagnostic and MetadataError
  result.rs      # MetadataIngest, MetadataSummary
```

Dependencies: `cratevista-schema`, `cargo_metadata`, `serde`, `serde_json`, optional `tracing`. Not on core/rustdoc/graph/server.

### Public ingestion API (`result.rs` / `options.rs` / `lib.rs`)

```rust
/// Options controlling Cargo metadata ingestion. Library-level; issue 05 maps
/// CLI flags / cratevista.toml onto this.
pub struct MetadataOptions {
    pub manifest_path: Option<PathBuf>,
    pub cwd: Option<PathBuf>,
    pub selection: PackageSelection,      // Default | Workspace | Packages(Vec<String>)
    pub features: FeatureSelection,       // { features: Vec<String>, all_features: bool, no_default_features: bool }
    pub external_deps: ExternalDepsMode,  // Exclude (default) | DirectOnly | FullGraph
    pub target_kinds: TargetKinds,        // which non-lib/bin targets to include (default: none)
    pub network: NetworkMode,             // Inherit (default) | Offline | Frozen | Locked
}

/// The deterministic, normalized result. NOT an ExplorerDocument.
///
/// Must NOT contain: an `ExplorerDocument`, a `cratevista_core::Diagnostic`,
/// serialized JSON, filesystem output paths, UI coordinates, or React Flow data.
pub struct MetadataIngest {
    pub entities: Vec<Entity>,            // sorted by id
    pub relations: Vec<Relation>,         // sorted by id
    pub diagnostics: Vec<DocumentDiagnostic>, // recoverable problems, sorted
    pub summary: MetadataSummary,
}

/// A compact ingestion summary: counts + selection context only (no full
/// feature map — feature detail lives on package entities).
pub struct MetadataSummary {
    pub workspace_root_repo_relative: Option<String>,
    pub selection: PackageSelection,
    pub external_deps_mode: ExternalDepsMode,
    pub workspace_package_count: usize,
    pub selected_package_count: usize,
    pub external_package_count: usize,
    pub target_count: usize,
    pub dependency_relation_count: usize,
    pub recoverable_diagnostic_count: usize,
    pub cargo_argv: Vec<String>,          // the exact invocation, for diagnostics/tests
}

/// Fatal ingestion errors (prevent producing a trustworthy MetadataIngest).
/// Each carries a stable code (see "Error handling").
#[derive(thiserror::Error, Debug)]
pub enum MetadataError {
    CargoNotFound,
    CargoMetadataFailed { argv: Vec<String>, stderr: String },
    MalformedMetadata(/* parse error */),
    PackageNotFound(String),
    InternalInvariant(String), // an invariant violation that prevents deterministic output
}

pub fn ingest(options: &MetadataOptions) -> Result<MetadataIngest, MetadataError>;

/// The pure conversion boundary. **Fallible**: duplicate ids that cannot be
/// safely resolved, an invalid selection state, or an internal invariant
/// violation are fatal `MetadataError`s; recoverable issues (unsupported target,
/// non-portable path identity, source outside workspace, safe duplicate-id
/// fallback, non-UTF-8 path, omitted external identity, incomplete optional
/// metadata) become `DocumentDiagnostic`s in the result.
pub fn normalize(
    metadata: &cargo_metadata::Metadata,
    options: &MetadataOptions,
) -> Result<MetadataIngest, MetadataError>;
```

`MetadataError` values carry stable codes (below) and can be converted to a `DocumentDiagnostic` by the caller (issue 05) when it wants to record the failure. This crate never constructs `cratevista_core::Diagnostic`. **Rationale for a fallible `normalize`:** duplicate generated ids with no safe deterministic fallback, invalid source-identity state, or an invalid selection cannot yield a trustworthy result, so they are fatal rather than silently degraded.

Feature information is preserved on **package entities** (not in the summary): attributes `declared_features` (sorted names), `enabled_features` (sorted names), and `default_features_enabled: bool`. All feature collections use deterministic (sorted) ordering. **No feature-to-feature graph edges are generated in issue 03** — that is out of MVP scope unless a later approved PRD adds it.

### Cargo command / options model

`cargo_metadata::MetadataCommand` is configured as:

- `--manifest-path <path>` from `manifest_path`; else run in `cwd` (or the process cwd) and let Cargo locate the nearest `Cargo.toml` upward.
- Features: `--features <a,b>` from `features.features`; `--all-features` from `all_features`; `--no-default-features` from `no_default_features`.
- Network: `Offline` → `--offline`; `Frozen` → `--frozen`; `Locked` → `--locked`; `Inherit` passes none. Forwarded via `MetadataCommand::other_options`.
- Resolve: run **with** the resolved graph (needed for `depends_on`); `--no-deps` is not used by default.
- Environment: inherit the parent environment (PATH, `CARGO_HOME`, `RUSTUP_TOOLCHAIN`, proxies) so Cargo/rustup resolve correctly.
- Cancellation/failure: run synchronously; a non-zero exit or unparsable output is a fatal `MetadataError` (with argv + stderr tail). Long-running cancellation is a watch-mode concern (issue 09); this crate just handles process success/failure cleanly.
- **Package selection is in-process**: `--package`/`--workspace` are not `cargo metadata` flags. `PackageSelection` filters `metadata.workspace_members` / `metadata.packages` after parsing.

Documented explicitly: `cargo metadata` performs Cargo's **normal dependency resolution** (and may fetch/compile build-script metadata) unless `Offline`/`Frozen` is set. CrateVista never runs the project's own binaries, tests, examples, or benches.

### Package and workspace identities

Grounded in the implemented `EntityId` API; `cargo_metadata::PackageId` stays internal mapping data only.

- **Workspace**: `EntityId::workspace()`, kind `EntityKind::WORKSPACE`.
- **Workspace member package**: `EntityId::package(name)` (no version), kind `PACKAGE`, `Provenance::Discovered`; version stored as attribute `version`. Manifest source → `SourceLocation` when inside the workspace root.
- **External package** (only when included): `EntityId::external_package(name, version)`, kind `PACKAGE`, `Provenance::Discovered`; attributes `version`, `source_kind` (registry/git/path); no repo-relative source.

Enumerated cases resolved:

1. **Packages that cannot share a public ID** (e.g. two members with the same crate name is impossible; but a member and an external of the same name): members always win `package:{name}`; externals use `package:{name}@{version}`; if that still collides across sources → `EntityId::external_package_disambiguated(name, version, source_kind, normalized_source)` (BLAKE3 over a portable, normalized source string) → emit `duplicate_generated_id` only if a genuine collision remains.
2. **Renamed dependencies**: the rename is a property of the *edge*, not the package identity — the target keeps its real `package:{name}`; the `rename` is stored on the `depends_on` relation (attribute + role influence). No separate entity id.
3. **Multiple versions of one external package**: distinct `package:{name}@{version}` entities (version is part of the id for externals).
4. **Registry dependencies**: `source_kind = "registry"`; `external_package(name, version)`; disambiguate only if the same name/version appears from multiple registries.
5. **Git dependencies (incl. revision)**: `source_kind = "git"`; `external_package(name, version)`, disambiguated via `external_package_disambiguated` with a normalized source = `git+<url>?<ref>` (canonicalized), so different revisions/urls stay distinct without absolute paths. Store the git url/ref as attributes.
6. **Path dependencies inside the workspace**: these are normally workspace members → `package:{name}` with a repo-relative `SourceLocation`. A path dep inside the root that is *not* a member is treated like a member-style package (`package:{name}`) with repo-relative source.
7. **Path dependencies outside the workspace root**: **portability policy** — never hash or expose the absolute path. Identify by `external_package(name, version)` with `source_kind = "path-external"` and **no** `SourceLocation` (outside root → omitted). If `name/version` is ambiguous across multiple external-path sources, attempt a **workspace-root-relative** normalized source for the discriminator; if no portable identity can be formed, emit `non_portable_path_identity` and fall back to `external_package(name, version)` (accepting possible ambiguity) rather than embedding an absolute path.
8. **Packages whose Cargo source is absent** (`source == None` and not a member): treat as `external_package(name, version)` with `source_kind = "unknown"`; if ambiguous, `non_portable_path_identity`/`duplicate_generated_id` as above.

Portability invariants: **absolute filesystem paths never appear in any id or artifact**; source locations outside the selected workspace root are omitted by default; a non-portable identity yields a diagnostic + documented fallback, never a fake-portable id.

**ID-conflict detection (never silent overwrite).** When two entities would generate the same `EntityId`, this is **detected** — the second is not silently dropped or overwritten. If a safe deterministic fallback exists (e.g. a portable source discriminator), it is applied and a `duplicate_generated_id` diagnostic is emitted; if none exists, it is a fatal `MetadataError::InternalInvariant`. This covers the (Cargo-normally-impossible) case of **duplicate workspace package names** should a fixture expose such a conflict.

### Cargo source identity (`cargo_metadata::Source::repr`)

`cargo_metadata::Source::repr` is treated as an **opaque** Cargo identity:

- It may be used **only** as input to a domain-separated BLAKE3 source discriminator (`EntityId::external_package_disambiguated`), never exposed raw in a public `EntityId`, and never used as a user-facing label.
- CrateVista does **not** depend on undocumented parsing details of `repr` and does not assume its internal representation is permanently stable.
- Documented consequence: a **source-disambiguated external ID may change** if Cargo changes its opaque source representation (acceptable — it affects only the rare cross-source collision case, never workspace-member or plain `name@version` ids).
- Only a **broad `source_kind`** — `registry`, `git`, `path`, or `unknown` — is derived, and only where that classification is reliable. Registry/Git source strings are **not** parsed more deeply than needed for this broad classification.

### Source locations

- Repo-relative base = `metadata.workspace_root` (a `Utf8Path`).
- For a member manifest path or a target `src_path`: `path.strip_prefix(workspace_root)` → the relative `Utf8Path` → `RepoRelativePath::new(rel.as_str())` → `SourceLocation::new(path, None)` (targets carry no span here; item spans come from issue 04).
- If `strip_prefix` fails (outside the root), **omit** the `SourceLocation` and (once per package, not per file) record a `source_outside_workspace` diagnostic. `RepoRelativePath` validation is never weakened; any residual rejection (e.g. an unexpected `..`) drops that source with a diagnostic and keeps the entity.

### Package and target entities

Target entity: `EntityId::target(package, target_kind, name)` → `target:{package}:{target-kind}:{target-name}` where `{package}` is the package's stable-id key (the member `name`, or external `name@version`), kind `EntityKind::TARGET`, `Provenance::Discovered`, `parent` = the owning package entity. Preserve useful target metadata as **deterministic** attributes where available: `crate_types` (sorted array), `target_kind` (sorted array), `required_features` (sorted array), `edition`, `doctest` (bool), `test` (bool), `bench` (bool), `doc` (bool). `SourceLocation` from `src_path` only when inside the selected workspace root (outside → omitted, no fake path). Build-script (`custom-build`) targets are opt-in and are **never executed** by CrateVista.

Default target-kind inclusion (keeps the map readable):

| Cargo target kind | default | rationale |
|---|---|---|
| `lib` (`lib`/`rlib`/`cdylib`/`staticlib`/`dylib`) | **included** | core architecture |
| `bin` | **included** | entry points |
| `proc-macro` | **included** | architecturally significant |
| `example` | opt-in (`TargetKinds`) | noise for architecture |
| `test` (integration) | opt-in | noise |
| `bench` | opt-in | noise |
| `custom-build` (build script) | opt-in | rarely part of the architecture map |

`TargetKinds` (in `MetadataOptions`) toggles the opt-in kinds. Unknown/future target kinds are included with a generic `target` entity + `unsupported_target` diagnostic (never a metadata-specific closed enum in the schema).

### Dependency relations

Built from the **resolved** graph (`metadata.resolve.nodes[].deps[].dep_kinds`), not just manifest declarations. All dependency edges use `RelationKind::DEPENDS_ON`. Encoding split:

- **relation kind**: `depends_on` (the only kind for dependencies).
- **relation role** (`Relation.role`): the dependency-kind category — `"normal"`, `"dev"`, `"build"` — so distinct dep-kind edges between the same packages do **not** collapse. One relation per `(from, to, dep_kind[, target-cfg])`.
- **relation attributes** (`AttrValue`): `rename` (renamed deps), `optional` (bool), `target` (platform `cfg(...)` string for target-specific deps), `resolved_version` (of the target), `source_kind`.
- **discriminator** (`RelationId::with_role_and_discriminator`): when two semantically distinct same-kind edges share `(from, to, role)` — e.g. two different platform `cfg` targets — a BLAKE3 discriminator over the normalized `cfg` string keeps them distinct. The builder MAY merge byte-identical evidence but MUST NOT discard semantically distinct edges.
- **intentionally omitted from MVP**: precise per-feature activation edges, weak-dependency (`dep?/feature`) semantics, and per-artifact (`artifact = "bin"`) dependency detail — recorded as attributes where cheap, not as separate relations.

Conceptual identity examples (the discriminator follows the domain-separated, length-framed BLAKE3 rules already established in `cratevista-schema`):

```text
rel:depends_on:package:app->package:core:normal
rel:depends_on:package:app->package:platform:normal:<cfg-discriminator>
```

Identical dependency evidence may be merged deterministically; semantically distinct evidence (a different `role`, or a different `target_cfg`) MUST NOT be discarded.

`contains` relations: workspace → member package; package → its (included) targets. No `contains` edge to external packages.

### External dependencies

`ExternalDepsMode`:

- `Exclude` (**default**): only workspace members + their targets + intra-workspace `depends_on`. Any edge whose endpoint is not included is **excluded consistently** (no dangling references; no boundary placeholder in MVP).
- `DirectOnly`: also include external packages that are direct dependencies of workspace members, and the workspace→external edges.
- `FullGraph`: include the entire resolved package graph.

The default keeps the architecture readable. When external packages are excluded, workspace→external edges are excluded consistently (documented boundary model: omission, not a synthetic node).

### Determinism

Equivalent Cargo metadata input + equivalent `MetadataOptions` must produce equivalent ordered output across repeated runs, Windows/Linux/macOS, different JSON object-field ordering, different package/node input ordering, and different process hash seeds. Concretely: sort **entities, relations, diagnostics, attribute maps, feature-name collections, target-kind collections, and summary collections** by documented stable rules (ids via `Ord`; names lexicographically); iterate Cargo maps through sorted keys; never rely on `resolve.nodes`/`packages` array order or HashMap/`DefaultHasher` iteration. `cratevista-metadata` does **not** serialize artifacts — canonical JSON serialization belongs to the later artifact-writing boundary (issue 05).

### Error handling — fatal vs recoverable

- **Fatal** (`MetadataError`, returned as `Err`, no trustworthy result): `cargo_not_found`, `cargo_metadata_failed` (process non-zero, with argv + stderr tail + remediation), `malformed_metadata` (parse failure), `package_not_found` (an explicitly selected package name is absent), `internal_invariant` (an invariant violation — e.g. an unresolvable duplicate id — that prevents deterministic output).
- **Recoverable** (`DocumentDiagnostic` in `MetadataIngest.diagnostics`, safe output still produced): `unsupported_target`, `non_portable_path_identity`, `source_outside_workspace`, `duplicate_generated_id` (only where a safe deterministic fallback exists), `non_utf8_path` (skip that source, keep the package), `omitted_external_identity`, and `incomplete_optional_metadata`. Diagnostics reference the affected entity ids where possible and are **sorted** for determinism.

Diagnostics are **never** added to `ExplorerDocument`; this crate does not build one.

### Security and privacy

Read-only; no project bins/tests executed. No absolute paths in ids or artifacts. Only workspace-internal paths become `SourceLocation`. Registry/cache paths are never exposed.

## CLI/API/configuration changes

None in the CLI. Defines the library-level `MetadataOptions` (and its enums) consumed by issue 05. Reserves the `cratevista.toml` `[metadata]` fields (`include_external_deps`, target-kind toggles) for issue 08 to bind; no TOML parsing happens here.

Adds **`cargo_metadata = "0.23"`** to `[workspace.dependencies]` (verified as the current release line — latest resolved `0.23.1`; `Cargo.lock` is the source of truth for the exact patch). The placeholder crate already declares it so the version is locked now, per the latest-stable Rust policy (ADR-0010). Not retaining `cargo_metadata 0.20` (which existed only for the superseded Rust 1.85 MSRV).

`cargo_metadata` compatibility notes:

- `cargo_metadata` may add support for newly exposed Cargo fields over time; CrateVista consumes only the fields it maps and tolerates additional ones.
- CrateVista still invokes Cargo metadata with an **explicit format version** (`--format-version 1`) regardless of the crate version.
- Updating `cargo_metadata` must **not** silently change CrateVista stable entity, relation, or ID semantics; any such change is a deliberate, documented decision.
- Every `cargo_metadata` update requires re-running the **fixture round-trip and determinism** tests before landing.

## Files and modules to create or modify

- `crates/cratevista-metadata/src/{lib,options,invoke,select,source,ids,entities,relations,diagnostics,result}.rs`
- `crates/cratevista-metadata/Cargo.toml`: add `cratevista-schema`, `cargo_metadata`, `serde`, `serde_json`, optional `tracing`.
- `Cargo.toml` (root): add `cargo_metadata` to `[workspace.dependencies]`.
- `crates/cratevista-metadata/fixtures/*.metadata.json` (captured `cargo metadata --format-version 1` outputs).
- `crates/cratevista-metadata/tests/{normalize,selection,determinism,dependencies,portability,failure}.rs`.
- Optional `docs/` metadata-ingestion note; `docs/configuration.md` `[metadata]` section coordinated with issue 08.

## Testing strategy

### Unit tests

- `select`: Default (members only, externals excluded), Workspace, Packages (subset + `package_not_found`).
- `ids`: member vs external ids; version-bump stability (member id unchanged across versions); git/registry/path-external disambiguation; no absolute path in any produced id.
- `source`: inside-root → `SourceLocation`; outside-root → omitted + diagnostic; `RepoRelativePath` validation preserved.
- `relations`: normal/dev/build role split; renamed dep attribute; optional dep; target-cfg discriminator prevents collapse.
- `entities`: target-kind default inclusion; version attribute; `unsupported_target` fallback.

### Integration tests

Deserialize checked-in `*.metadata.json` fixtures into `cargo_metadata::Metadata` and run `normalize` — **hermetic, no network, no live cargo**. Required coverage:

- single-package project; virtual workspace; workspace member dependencies;
- multiple target kinds; a proc-macro target;
- renamed dependencies; optional dependencies; **build** dependencies; **development** dependencies; target-specific (platform `cfg`) dependencies;
- **multiple semantic relations between the same package pair** (e.g. normal + build) that must remain distinct;
- multiple versions of one external package;
- **registry** source; **git** source; local workspace path dependency; external path dependency;
- malformed metadata; missing selected package; source path outside the workspace; duplicate-id handling; an **unknown/future-like target kind**;
- **reordered-JSON** determinism and **reordered package/node** determinism (identical `MetadataIngest`).

Failure cases: malformed/empty metadata JSON → `MalformedMetadata`; selected package absent → `PackageNotFound`.

### End-to-end tests (gated)

- One gated test builds a tiny **path-only** Cargo workspace in a `tempfile::tempdir()` and runs real `cargo metadata` in **offline** mode to validate `invoke` + argv; skipped by default so unit/integration need no network or live cargo. **Does not require crates.io or Git network access.** (Tempdirs preferred over committed nested manifests for isolation, per the PRD-01 precedent.)

### Fixtures

Captured `cargo metadata --format-version 1` JSON per scenario (hermetic). No committed nested Cargo workspaces are required for the core tests; the single gated test constructs a path-only workspace at runtime.

## Performance considerations

One `cargo metadata` call; parsing is fast. `normalize` is pure and cache-friendly (issue 09 keys on `Cargo.toml`/`Cargo.lock` hashes + options).

## Observability and diagnostics

Optional `tracing` span around `invoke` (argv + duration + counts). Recoverable diagnostics carry stable codes and affected entity ids; the exact `cargo_metadata` argv is captured in `MetadataSummary.cargo_argv` for tests and error messages.

## Documentation changes

`docs/` metadata-ingestion note (identity rules, target-kind defaults, external-dep modes, portability policy); `docs/configuration.md` `[metadata]` section (coordinated with issue 08).

## Rollout and migration

New crate; no migration. Fills the metadata half of the issue-05 pipeline inputs.

## Risks and mitigations

- **`cargo_metadata` schema changes** → pin the version; `--format-version 1` is explicit; fixtures catch regressions.
- **Absolute/non-portable paths leaking** → strip-prefix + `RepoRelativePath`; external/outside-root sources omitted; `non_portable_path_identity` fallback; test asserts no absolute path in any id/source.
- **Collapsing distinct dependency evidence** → role + discriminator per `(from,to,dep_kind,cfg)`; test asserts distinct edges survive.
- **Non-determinism** → sorted outputs + reordered-JSON determinism test.

## Alternatives considered

- Hand-parsing `Cargo.toml`: rejected (issue + CLAUDE.md; `cargo_metadata` only).
- Passing `--package`/`--workspace` to `cargo metadata`: rejected — those are not `cargo metadata` flags; selection is in-process filtering.
- Returning a partial `ExplorerDocument`: rejected — the crate returns normalized `Entity`/`Relation`/`DocumentDiagnostic` values; assembly is issue 05.
- Encoding dep-kind as separate `RelationKind`s: rejected — `depends_on` + `role` keeps the kind set small and open; distinct evidence preserved via role/discriminator.
- Wiring `generate` CLI flags now: rejected — would create temporary behavior rewritten in issue 05; library-level `MetadataOptions` only.

## Implementation sequence

1. `options` + `result` types.
2. `invoke` (via `cargo_metadata::MetadataCommand`) + `MetadataError` mapping.
3. `select` (package selection + external modes).
4. `source` (repo-relative mapping + omission) and `ids` (portability).
5. `entities` + `relations` (contains + depends_on with role/attributes).
6. `diagnostics` codes; deterministic ordering.
7. Fixtures + tests (incl. determinism, portability, failure).

## Acceptance criteria

- [x] Cargo metadata requested with explicit format version via `cargo_metadata::MetadataCommand`. *(verified: `invoke.rs`; `argv_pins_format_version` asserts `--format-version 1`; captured in `MetadataSummary.cargo_argv`)*
- [x] Manifests not parsed as primary source. *(verified: only `cargo_metadata`; no TOML parsing anywhere in the crate)*
- [x] Workspace and single-package fixtures covered. *(verified: `tests/normalize.rs` over `single_package` + `workspace_deps` fixtures)*
- [x] Package/dependency output deterministic, incl. reordered-JSON input. *(verified: `tests/determinism.rs` — reversed packages/nodes → identical output; repeated-normalization stable)*
- [x] External dependencies excludable/includable by option (`Exclude`/`DirectOnly`/`FullGraph`); default excludes with consistent boundary omission. *(verified: `tests/external_modes.rs` — Exclude omits externals + edges, no dangling refs; DirectOnly/FullGraph include)*
- [x] Default prioritizes workspace packages and readable target kinds (lib/bin/proc-macro). *(verified: `default_excludes_optin_target_kinds`; opt-in kinds excluded by default)*
- [x] No absolute paths in any id or `SourceLocation`; external/outside-root sources omitted; portability fallback diagnosed. *(verified: `assert_no_absolute_paths` in every integration test; ext dep has no source; `assign_external_group` non-portable-collapse test)*
- [x] Dependency kind/role/attribute split preserves distinct evidence (normal/dev/build, renamed, optional, target-cfg). *(verified: `workspace_deps_map_roles_attributes_and_targets` — normal/build/dev roles, rename attr, target_cfg + discriminator id)*
- [x] Result is a named `MetadataIngest` carrying `DocumentDiagnostic`s (no `cratevista_core::Diagnostic`, no `ExplorerDocument`, no artifact writing). *(verified: `result.rs`; the crate never references core; no file writes)*
- [x] Fatal vs recoverable split implemented with stable diagnostic codes. *(verified: `error.rs` codes; `maps_cargo_errors_to_stable_codes`; recoverable codes in `diagnostics.rs`)*
- [x] `cratevista-metadata` depends on `cratevista-schema` and not on core/rustdoc/graph/server. *(verified: `cargo tree -i` reports no path for all four)*
- [x] Errors include command context and remediation. *(verified: `MetadataError::CargoMetadataFailed{argv,stderr}` + `remediation()`)*
- [x] No project binaries/tests executed; only `cargo metadata`. *(verified: only `MetadataCommand::exec`; documented)*
- [x] Unit/integration tests require no network. *(verified: hermetic sanitized JSON fixtures; the one live test is `#[ignore]` + offline)*
- [x] `normalize` is fallible: unresolvable duplicate ids / invalid selection / internal-invariant → `MetadataError`; recoverable issues → `DocumentDiagnostic`. *(verified: signature `-> Result<_, MetadataError>`; `missing_selected_package_is_fatal`)*
- [x] Feature info (declared/enabled/default) is on package entities with deterministic ordering; no feature-to-feature edges; `MetadataSummary` stays counts+selection. *(verified: `add_feature_attributes` (sorted); summary has counts only; feature attrs asserted)*
- [x] Cargo `Source::repr` never appears raw in a public id; only broad `source_kind` derived; source-disambiguated ids documented as change-if-Cargo-changes. *(verified: `ids.rs` `classify_source`/`portable_source` tests; repr only feeds the BLAKE3 discriminator; documented in ADR-0003 + PRD)*
- [x] Target entities carry deterministic attributes (crate_types/required_features/edition/doctest/test/doc); build scripts opt-in and never executed. *(verified: `add_target_attributes`. NOTE: `bench` is a target **kind**, not a bool — `cargo_metadata::Target` exposes no `bench` field, so that attribute is omitted; see Deviations.)*
- [x] Quality gates pass under the pinned stable toolchain, incl. `cargo +1.97.0 check`. *(verified locally: fmt/clippy `-D warnings`/test (70 passed)/`cargo +1.97.0 check` all green)*

Verification (implementation must pass all of these under the pinned stable toolchain, ADR-0010):

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-features
cargo +1.97.0 check --workspace --all-features
cargo tree -p cratevista-metadata
# The tree must confirm cratevista-metadata has NO dependency on any of:
cargo tree -p cratevista-metadata -i cratevista-core     # must report no path
cargo tree -p cratevista-metadata -i cratevista-rustdoc  # must report no path
cargo tree -p cratevista-metadata -i cratevista-graph    # must report no path
cargo tree -p cratevista-metadata -i cratevista-server   # must report no path
```

### Dependency selection policy (new direct dependencies)

Use the latest compatible **stable** release of any new direct dependency at implementation time: verify its current release, confirm `rust-version` compatibility with Rust 1.97, prefer maintained stable releases, avoid prereleases unless explicitly justified, declare shared deps via `[workspace.dependencies]`, and let `Cargo.lock` record the exact resolved versions. Do not bump already-selected schema dependencies solely to chase a newer patch unless it is relevant to issue 03 or needed for compatibility/security.

## Open questions

**None blocking — all resolved for approval:**

- `cargo_metadata` pinned to **`0.23`** (latest-stable policy, ADR-0010).
- Default external-deps mode **`Exclude`**; default target kinds **`{lib, bin, proc-macro}`** (examples/tests/benches/build-scripts opt-in).
- Dependency-kind encoding: **one `depends_on` relation per `normal`/`dev`/`build` role** (+ BLAKE3 discriminator when the same `(from,to,role)` differs only by target-cfg).
- `MetadataSummary` stays **compact** (workspace root, counts, mode, `cargo_argv`); the full feature map is **deferred to issue 05** (stored as package-entity attributes where cheap).

## Traceability

Issue-03 checkboxes → tests above. `MetadataIngest` (entities/relations/diagnostics) is consumed by issue 05's `build_document`; `MetadataOptions` is surfaced by issue 05's `generate`; `[metadata]` config fields are bound by issue 08; `normalize` purity is reused by issue 09 caching.
