# PRD — Define the stable CrateVista explorer schema

## Status

**Approved — implemented and verified** (2026-07-12). Implemented in `crates/cratevista-schema`. All acceptance criteria are checked with verified evidence; the full workspace gates pass on Windows (Rust 1.97.0 stable): `cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets --all-features -- -D warnings`, `cargo test --workspace --all-features`. The checked-in JSON Schema regenerates with no diff. Resolved dependency versions: `schemars` 1.2.1, `blake3` 1.8.5, dev `jsonschema` 0.30.0.

**Two additive amendments have landed since; the status is unchanged (Implemented / Verified):**

1. **2026-07-14 — artifact-integrity hashes** (`ArtifactHashes` + optional `GenerationReport.artifact_hashes`), required by PRD 06. **No `SchemaVersion` bump** — the field lives on the unversioned `generation.json`.
2. **2026-07-16 — view docs/examples** (`View::docs`, `View::examples`, `ViewExample`), required by PRD 08 (Amendment A). **`SchemaVersion` 1.0 → 1.1** — this one changes the versioned `ExplorerDocument`, so ADR-0003 makes an added optional field a MINOR bump. Backward-compatible: 1.0 artifacts still load and validate, and major-version rejection is unchanged.

See the two "### Additive amendment" sections below.

Reviewed against the real workspace created by PRD 01 and finalized. Key decisions: `cratevista-schema` must **not** depend on `cratevista-core` (avoids a cycle); the root type is `ExplorerDocument`; the generated-document diagnostic is `DocumentDiagnostic` (a **separate** type from the runtime `cratevista_core::Diagnostic`, with a schema→core conversion owned by core in a later issue); `document.json`, `generation.json`, and `diagnostics.json` are **three separate top-level artifacts** (diagnostics are **not** embedded in `ExplorerDocument`); `SourceLocation` is a domain-only repo-relative type; semantic ID discriminators use **blake3** (first 128 bits) over a normalized, domain/version-prefixed, UTF-8 input; JSON Schema uses **schemars 1.x** with a single **canonical JSON serializer** shared by the regenerator and the drift test; no `--strict` mode in this issue.

### Approved decisions (recorded)

1. **JSON Schema dependency**: `schemars` **1.x** via `[workspace.dependencies]`; the implementation picks the current compatible 1.x release and `Cargo.lock` captures it. Only fall back to 0.8 if a concrete 1.x incompatibility is discovered and reported first.
2. **Stable hash**: use the **`blake3`** crate for deterministic semantic ID discriminators (no ad-hoc FNV). Hash input must be UTF-8, carry an explicit domain/version prefix, use unambiguous separators or length framing, be built from normalized semantic values, and never depend on HashMap ordering / `DefaultHasher` / process seed / absolute machine paths / timestamps / raw rustdoc numeric IDs. Use the first **128 bits** as the displayed discriminator unless tests justify more. The input format is documented so future versions do not silently change IDs.
3. **Snapshot strategy**: no `insta` in issue 02 — use checked-in JSON fixtures, direct round-trip assertions, canonical byte comparisons, and standard test diffs. Reconsider a snapshot library later if fixtures grow unwieldy.
4. **Unknown kinds / strict mode**: no `--strict` mode in issue 02. Unknown entity/relation kinds remain valid during deserialization and schema validation, and are preserved through round-trip; issue 07 provides the generic frontend fallback. First-party emitters use documented known-kind constants; the schema stays open for extensions.
5. **Diagnostic output boundary**: `cratevista_schema::DocumentDiagnostic` is the serialized diagnostic contract used by `diagnostics.json`; `ExplorerDocument` does **not** embed diagnostics; the three artifacts stay separate; `cratevista_core::Diagnostic` remains the runtime CLI/process diagnostic; the `DocumentDiagnostic → core::Diagnostic` conversion is owned by `cratevista-core` in a later issue.
6. **Canonical JSON & drift**: one reusable canonical JSON serializer (recursively orders object keys; preserves array order; stable pretty formatting; UTF-8; exactly one terminating newline). Both `examples/gen_schema.rs` and `tests/jsonschema_drift.rs` call the same schema-generation and canonical-serialization functions. Checked-in artifact: `crates/cratevista-schema/schema/cratevista-document.schema.json`. Rust types are the single source of truth.
7. **Package ID disambiguation**: workspace packages use `package:{name}` (no version); external packages may use `package:{name}@{version}`; identical name/version pairs from different sources append a deterministic source discriminator derived from normalized Cargo source identity (never an absolute path); raw `cargo_metadata::PackageId` is analyzer input only.
8. **Relation & impl identities**: a basic relation ID derives from relation kind + source entity ID + target entity ID → `rel:{kind}:{from}->{to}`. When multiple semantically distinct same-kind relations can exist between the same endpoints, include the optional semantic role and, when still necessary, a deterministic BLAKE3 discriminator → `rel:{kind}:{from}->{to}:{role}:{discriminator}`. No `DefaultHasher`, randomized hashing, iteration order, timestamps, absolute paths, raw rustdoc IDs, or process-specific data. The builder may merge identical evidence but must not discard semantically distinct relations. Impl IDs include a semantic discriminator derived from the normalized impl signature so multiple inherent impl blocks for one type cannot collide.

## Source issue

`ISSUES/issue_02_explorer_schema.md`

## Summary

Define `cratevista-schema`: the canonical, versioned, frontend-independent Rust data model for the explorer document (`ExplorerDocument`), plus its stable identifier scheme, one reusable canonical JSON serializer, forward-compatible kind handling, compatibility policy, and a generated JSON Schema artifact. There are **three separate top-level artifacts**: `document.json` (the deterministic `ExplorerDocument`, no timestamps/runtime metadata and no embedded diagnostics), `generation.json` (a `GenerationReport` of runtime metadata), and `diagnostics.json` (a `DiagnosticsReport` of `DocumentDiagnostic`s). This model is the single contract between analyzers (issues 03–05, 08) and the server/frontend (issues 06–07).

## Problem statement

All analyzers and the frontend need one shared vocabulary. Coupling it to rustdoc JSON or React Flow would violate CLAUDE.md and make evolution impossible. We need a stable, deterministic, self-validating model whose identifiers survive regeneration and whose kind set can grow without breaking existing consumers.

## Goals

- Canonical Rust types for `ExplorerDocument`, Entity, Relation, View, Stage, SourceLocation, DocBlock, tags/attributes, provenance, and localization-ready labels.
- Separate `GenerationReport` (→ `generation.json`) and `DiagnosticsReport`/`DocumentDiagnostic` (→ `diagnostics.json`) types; diagnostics are **not** embedded in `ExplorerDocument`.
- Stable ID scheme independent of rustdoc numeric IDs and array indices, with **blake3**-based semantic discriminators.
- One reusable **canonical JSON serializer** giving deterministic, reviewable `document.json` (no opt-in flag needed).
- Forward-compatible entity/relation kinds: unknown kinds are preserved (deser + validation + round-trip) and rendered via a generic frontend fallback (issue 07). No `--strict` mode in this issue.
- Documented schema version + compatibility/evolution policy.
- Generated JSON Schema artifact (schemars 1.x, checked in, drift-tested) and reference fixtures covering every MVP entity/relation kind.
- Reference validation that detects dangling references.

## Non-goals

- Producing documents from real data (issues 03–05).
- UI coordinates or layout (never in schema; layout is issue 07 via ELK).
- Manual flow *loading* (issue 08 produces schema entities/views; format lives there).

## Current repository state

`cratevista-schema` exists as a compiling placeholder lib crate with `#![forbid(unsafe_code)]` and no dependencies (created in issue 01). PRD 01 also shipped, concretely:

- `cratevista-core::diagnostic::Diagnostic` (re-exported as `cratevista_core::Diagnostic`) — a **runtime CLI** diagnostic with fields `severity`, `code`, `message`, `remediation`, `context`, and its own `Severity { Error, Warning, Info }`; derives `Serialize` and implements `Display`.
- `cratevista-core::paths` — **process/OS** path helpers (`resolve_project_root`, `find_cargo_manifest`, `checked_utf8`) that resolve the working directory / manifest. These are process-level, not domain-level.
- `cratevista-core::error::CoreError` (thiserror), `ExitCode`, `logging`.

`cratevista-core` is the **high** orchestration layer; it currently does not depend on `cratevista-schema`, and it will in issues 05/06. Consequently `cratevista-schema` (the **low** domain layer) must not depend on `cratevista-core`. The validated repository-relative source-path type is defined **here**, self-contained, with no reuse of `cratevista-core::paths`. Domain vocabulary is fixed by `ISSUES/CONTEXT.md`. MVP entity/relation kinds are enumerated in issue 05.

## Terminology

Per `ISSUES/CONTEXT.md`. Key: **Discovered** vs **Manual** provenance; **Override** enriches a discovered entity without changing identity; **View** is a projection (filters + presentation, no coordinates); **Kind** is an open, string-backed classifier for entities/relations.

## User-visible behavior

Indirect: determines the shape of `/api/document` (issue 06) and what the frontend renders (issue 07). Directly, contributors get a `crates/cratevista-schema/schema/cratevista-document.schema.json` artifact and fixtures under `crates/cratevista-schema/fixtures/`. `document.json` diffs are stable and reviewable because output goes through the canonical serializer and is timestamp-free.

## Functional requirements

1. Root `ExplorerDocument` with `schema_version`, `project`, `entities`, `relations`, and `views`. It contains **no timestamps, no runtime metadata, and no embedded diagnostics**.
2. A separate `GenerationReport` type holds runtime metadata (tool name/version, `generated_at` timestamp, toolchain, rustdoc format version, input hashes, durations, counts, and a `partial` flag), serialized to `generation.json` (issue 05 writes it).
3. A separate `DiagnosticsReport` (`schema_version` + `Vec<DocumentDiagnostic>`) is serialized to `diagnostics.json` (issue 05 writes it). `DocumentDiagnostic` carries `severity`, `code`, `message`, and `entities`/`relations` ID references. The three artifacts (`document.json`, `generation.json`, `diagnostics.json`) are independent top-level files.
4. Every entity/relation/view has a stable `id`.
5. `serde` round-trip (serialize→deserialize→serialize) via the canonical serializer for each artifact is lossless and **byte-identical** (`document.json` and `diagnostics.json` have no timestamp; `generation.json` is expected to vary between runs).
6. One reusable **canonical JSON serializer** (module `canonical`) is used for every serialized artifact and for the JSON Schema: it recursively orders object keys, preserves array order, uses stable pretty formatting, writes UTF-8, and terminates the file with exactly one newline. `document.json` uses it and is deterministic by default (no reproducibility flag).
7. Entity/relation **kinds are open**: known kinds are constants but the type preserves any unknown kind string. Unknown kinds remain valid during deserialization and validation and are preserved through round-trip; the frontend renders them with a generic fallback (issue 07). No `--strict` mode is added in this issue.
8. Explicit unknown-**field** policy for forward compatibility (lenient read).
9. `ExplorerDocument::validate()` returns structured errors for dangling `from/to/parent/entity_ids` references, duplicate ids, and invalid ids/source paths, and collects all errors (not fail-fast). Unknown kinds are **not** errors.
10. A test regenerates the JSON Schema (via the shared functions) and fails if it drifts from the checked-in artifact.

## Technical design

### Layering and the two diagnostic types

**Dependency direction (no cycle).** `cratevista-schema` is the **low** domain layer; `cratevista-core` is the **high** orchestration layer and will depend on `cratevista-schema` in issues 05/06 (to assemble and read `document.json`). Therefore `cratevista-schema` **must not depend on `cratevista-core`** — that would be a circular dependency. Schema is self-contained: its `source` path validation is implemented here and does **not** reuse `cratevista-core::paths` (whose cwd/manifest resolution is a process concern, not a domain one).

**Two separate diagnostic types (decision).** Two diagnostics with overlapping intent live at different layers and are kept as **separate, independent types** with **distinct names** to remove ambiguity:

- `cratevista_core::diagnostic::Diagnostic` — the **runtime CLI/process diagnostic** (`severity`, `code`, `message`, `remediation`, `context`; `Display` + `Serialize`). Belongs to the orchestration/runtime layer; already exists (issue 01).
- `cratevista_schema::DocumentDiagnostic` — the **serialized document-diagnostic contract** written to `diagnostics.json` (`severity`, `code`, `message`, `entities: Vec<EntityId>`, `relations: Vec<RelationId>`). Belongs to the domain model. It is **not** embedded in `ExplorerDocument`; `diagnostics.json` is its own top-level artifact (a `DiagnosticsReport`).

They are **field-aligned by convention** (both carry `severity`/`code`/`message`) but do **not** share a Rust type, and each defines its **own** `Severity` (a 3-variant enum). Rationale: a shared type would force either schema→core (forbidden — cycle) or core→schema for the CLI's runtime diagnostic (couples the runtime layer to the domain model). The `DocumentDiagnostic` name is chosen (over `GenerationDiagnostic`) because it describes diagnostics *about the analyzed document/graph*; `GenerationReport` (runtime metadata) is a distinct concept in `generation.json`.

**Conversion direction.** When a later issue (05/06) surfaces *document* diagnostics on the CLI, the conversion `DocumentDiagnostic → cratevista_core::Diagnostic` lives in `cratevista-core` (which may depend on schema), folding the entity/relation references into the runtime diagnostic's `context`. The reverse never exists, and **no orchestration concern moves into `cratevista-schema`** to enable reuse.

### Module boundaries

`cratevista-schema` modules: `document` (`ExplorerDocument`), `entity`, `relation`, `view`, `source` (validated repo-relative path + traversal safety; reused by server/config), `docs` (DocBlock), `diagnostic` (`DocumentDiagnostic` + `DiagnosticsReport` + its own `Severity`), `ids` (id construction/newtypes; blake3 discriminators), `kind` (open kind types + known constants), `version` (schema version + policy consts), `generation` (`GenerationReport`), `canonical` (the single canonical JSON serializer), `validate`, `jsonschema` (schemars 1.x integration). Depends on `serde`, `serde_json`, `thiserror`, `schemars` (1.x), and `blake3` — and **not** on `cratevista-core`. `schemars` (1.x), `blake3`, and the dev-only `jsonschema` validator must be added to `[workspace.dependencies]`; `serde`/`serde_json`/`thiserror` are already declared there.

### Data model

Newtypes (transparent `String`): `EntityId`, `RelationId`, `ViewId`, `StageId`, `SchemaVersion`.

Open kinds (`kind` module):

```
EntityKind(String)   // serialized as a snake_case string
RelationKind(String) // serialized as a snake_case string
// Known values exposed as associated constants, e.g.:
EntityKind::MODULE, EntityKind::STRUCT, EntityKind::TRAIT, EntityKind::EXTERNAL_SYSTEM, …
RelationKind::CONTAINS, RelationKind::IMPLEMENTS, RelationKind::MANUAL, …
// Any other string is preserved verbatim (round-trips losslessly).
// Helper: `is_known()` for validation/telemetry; the frontend uses a generic
// fallback style whenever a kind is not in its recognized set.
```

Rationale for string-backed open kinds over a closed Rust `enum`: adding a kind must be an additive (MINOR) change, and existing frontends/backends must tolerate documents that use kinds they predate. A closed enum with `#[serde(other)]` would lose the original string; a string newtype with known constants preserves it and gives a clean generic-fallback path.

```
ExplorerDocument {            // -> document.json (deterministic; no timestamps, no diagnostics)
  schema_version: SchemaVersion,
  project: Project { id, name, description, root: SourceLocation?, repository_url?, default_branch? },
  entities: Vec<Entity>,      // sorted by id
  relations: Vec<Relation>,   // sorted by id
  views: Vec<View>,           // sorted by id
}

GenerationReport {            // -> generation.json, NOT in document.json
  generator: { name, version },
  generated_at: Timestamp,
  toolchain: Option<String>,
  rustdoc_format_version: Option<u32>,
  input_hashes: BTreeMap<String,String>,
  counts: { entities, relations, views, diagnostics },
  durations_ms: BTreeMap<String,u64>,
  partial: bool,              // true when produced under --keep-going (issue 04/05)
  artifact_hashes: Option<ArtifactHashes>,  // additive (2026-07-14); see amendment below
}

ArtifactHashes {              // BLAKE3 (hex) over the exact canonical bytes of the
  document_blake3: String,    //   sibling artifacts, so a reader can prove the
  diagnostics_blake3: String, //   document/diagnostics belong to this generation.
}

DiagnosticsReport {           // -> diagnostics.json, a separate top-level artifact
  schema_version: SchemaVersion,
  diagnostics: Vec<DocumentDiagnostic>,   // sorted deterministically
}
DocumentDiagnostic { severity, code, message,
                     entities: Vec<EntityId>, relations: Vec<RelationId> }

Entity {
  id: EntityId, kind: EntityKind, label: LocalizedText, qualified_name: String,
  provenance: Provenance /* Discovered | Manual */, parent: Option<EntityId>,
  source: Option<SourceLocation>, docs: Option<DocBlock>,
  tags: Vec<String>, attributes: BTreeMap<String, AttrValue>,
  description: Option<LocalizedText>,
}
Relation { id, kind: RelationKind, from: EntityId, to: EntityId,
           role: Option<String> /* semantic discriminator */,
           label: Option<LocalizedText>, provenance, attributes: BTreeMap<String,AttrValue> }
View { id, title: LocalizedText, description: Option<LocalizedText>,
       entity_kinds: Vec<EntityKind>, relation_kinds: Vec<RelationKind>,
       entity_ids: Option<Vec<EntityId>>, stages: Vec<Stage>,
       default_focus: Option<EntityId>, presentation: BTreeMap<String,AttrValue>, /* NO coordinates */
       docs: Option<DocBlock>,          // additive (2026-07-16, schema 1.1); see amendment below
       examples: Vec<ViewExample> }     // additive (2026-07-16, schema 1.1); see amendment below
SourceLocation { path: RepoRelativePath, span: Option<Span{start_line,start_col,end_line,end_col}> }
LocalizedText { default: String, translations: BTreeMap<Lang,String> }
```

- MVP known `EntityKind`s: workspace, package, target, module, struct, enum, union, trait, function, method, impl, type_alias, constant, static, macro, external_system, infrastructure, stage, manual_block.
- MVP known `RelationKind`s: contains, depends_on, imports, re_exports, implements, implemented_for, has_field_type, accepts_type, returns_type, error_type, references_type, manual.
- `AttrValue`: a small JSON-value newtype (string|number|bool|array|object) for extensible attributes.
- No React Flow type, no rustdoc numeric id, appears anywhere.
- `SourceLocation` is a **domain** location, not a process path. `path` is a `RepoRelativePath` newtype validated at construction to be repository-relative and normalized: forward-slash separators; no leading `/`, no drive letter, no UNC/`\\?\` prefix; no `..` component (no traversal escape); valid UTF-8. `span` is optional and 1-based (`start_line`, `start_col`, `end_line`, `end_col`). **No absolute machine paths and no current-working-directory semantics ever appear in `document.json`.** Turning an absolute/OS span into a `RepoRelativePath` (relative to the workspace root) is the caller's job in issues 04/05; construction here *rejects* anything that is not already a safe repo-relative path (this is exactly the traversal guard the server reuses in issue 06).

### Additive amendment (2026-07-14) — artifact-integrity hashes required by PRD 06

> **Status: Implemented / Verified (with PRD 06).** `ArtifactHashes` and the optional `GenerationReport.artifact_hashes` field ship in `cratevista-schema`; the `cratevista-core` writer computes and embeds the digests; the `full_mvp.generation.json` fixture and round-trip/backward-compat tests are updated. See the "actual implemented contract" notes below.

PRD 02 remains **Approved / Implemented / Verified**; this is an **additive, backward-compatible** schema amendment required so PRD 06 can load a *consistent* three-file snapshot (the three artifacts are committed by per-file rename with `generation.json` last, and are **not** one crash-atomic transaction — see PRD 05). Comparing `generation.json` bytes before/after reading the others is **insufficient** (an old `generation.json` can be observed both before and after the newer `document.json`/`diagnostics.json` are renamed). The fix is to embed the sibling artifacts' content hashes **inside** `generation.json`:

```rust
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ArtifactHashes {
    /// BLAKE3 digest of the exact canonical UTF-8 bytes of `document.json`,
    /// encoded as lowercase hexadecimal: exactly 64 ASCII characters, no `0x`
    /// prefix, no whitespace.
    pub document_blake3: String,
    /// BLAKE3 digest of the exact canonical UTF-8 bytes of `diagnostics.json`,
    /// encoded as lowercase hexadecimal: exactly 64 ASCII characters, no `0x`
    /// prefix, no whitespace.
    pub diagnostics_blake3: String,
}

// In GenerationReport, an additive optional field:
#[serde(default, skip_serializing_if = "Option::is_none")]
pub artifact_hashes: Option<ArtifactHashes>,
```

Contract:

- **Additive & optional** so a pre-amendment `generation.json` still deserializes (`artifact_hashes = None`).
- The **current generator always populates it** (PRD 05 writer amendment); the **server requires it** for snapshot-integrity verification (PRD 06). An artifact set produced *before* the amendment (hashes absent) fails PRD 06 with the stable error `snapshot_integrity_unavailable`, whose remediation is to run `cargo cratevista generate`.
- Hashes are **BLAKE3 over the exact canonical bytes** written to `document.json` / `diagnostics.json` (already available: `cratevista-schema` depends on `blake3`). They hash **content only** — no absolute paths. `generation.json` does **not** hash itself (that would be circular); its own consistency is covered by the marker (before/after byte equality).
- **Encoding contract (exact):** each digest is BLAKE3 of the exact canonical UTF-8 artifact bytes, serialized as **lowercase hexadecimal**, **exactly 64 ASCII characters**, **no `0x` prefix**, **no whitespace**. The writer hashes the *same* canonical bytes it writes to disk; the PRD-06 server **validates this encoding before comparison** and rejects a malformed digest with the dedicated `invalid_artifact_hash` error (not `malformed_generation`), so the diagnostic is actionable.
- This is an **additive, backward-compatible schema amendment; no breaking schema change**. **Actual implemented `SchemaVersion` decision: no bump (stays `1.0`).** `SchemaVersion` versions `document.json` (`ExplorerDocument`) and `diagnostics.json` (`DiagnosticsReport`); `generation.json` (`GenerationReport`) carries **no `schema_version`**, and neither versioned artifact changed structurally. Under ADR-0003 an added optional field is additive (MINOR-eligible), but since the field lives on the unversioned artifact and reads stay forward-compatible (old files deserialize; `serde(default)` tolerates absence), no version change is warranted — bumping to `1.1` would have churned every `document`/`diagnostics` fixture for no compatibility benefit. Implementation updated, in one coordinated change: **(1)** the Rust schema types (`ArtifactHashes` + the optional `GenerationReport.artifact_hashes` field); **(2)** the `GenerationReport` fixture (`full_mvp.generation.json`) and its round-trip/backward-compat tests. **The checked-in JSON Schema (`cratevista-document.schema.json`) is generated from `ExplorerDocument` only and therefore does not cover `GenerationReport`; adding this field does not change it, and the drift test passes unchanged** — there is no separate checked-in schema for `generation.json`.

### Additive amendment (2026-07-16) — view docs/examples required by PRD 08 (`SchemaVersion` 1.0 → 1.1)

> **Status: Implemented / Verified (PRD-08 Amendment A).** `View::docs`, `View::examples` and `ViewExample` ship in `cratevista-schema`; `SchemaVersion::CURRENT` is now `"1.1"`; the checked-in JSON Schema and the frontend `ExplorerDocument` types are regenerated and committed.

PRD 02 remains **Approved / Implemented / Verified**; this is an **additive, backward-compatible** schema amendment required so PRD 08 can express a manual flow's documentation and worked examples, which the issue-08 source issue requires and for which `View` previously had no home:

```rust
pub struct View {
    // … unchanged …
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub docs: Option<DocBlock>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub examples: Vec<ViewExample>,
}

pub struct ViewExample {
    pub id: String,
    pub title: LocalizedText,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub language: Option<String>,
    pub content: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<LocalizedText>,
}
```

Contract:

- **Additive & optional**: a pre-amendment (`1.0`) document has neither key and still deserializes and validates; a view without them serializes with neither key (`skip_serializing_if`), so the eight generated views are unchanged in shape.
- **Example contents are embedded, not referenced.** `ViewExample::content` carries the text itself, so the explorer renders examples **without the guarded `/api/source`** endpoint and a static export (issue 10) is self-contained. Producers embed only files a maintainer named explicitly in configuration; `language` is a display hint that is never interpreted or executed.
- **`examples` is author-ordered, not sorted.** The canonical serializer sorts entities/relations/views by id, but an example sequence is a narrative (request, then response); a test pins that order survives serialization.
- **Why this bumps `SchemaVersion` when the 2026-07-14 amendment did not.** That amendment added a field to `generation.json`, which carries **no `schema_version`**, so a bump would have churned fixtures for no compatibility benefit. This one changes **`ExplorerDocument`**, a *versioned* artifact, so under ADR-0003 an added optional field is a **MINOR** bump: **`1.0` → `1.1`**.
- **Verified backward-compatible, against the code rather than by assertion:**
  - the PRD-06 server gates on the **major** (`require_supported_major`) and additionally requires `document.schema_version == diagnostics.schema_version`; both derive from `CURRENT`, so a 1.1 run emits a matching pair, and a pre-bump 1.0/1.0 snapshot is self-consistent and still loads;
  - the PRD-07 frontend gates on the major (`SUPPORTED_SCHEMA_MAJOR = 1`);
  - the checked-in JSON Schema types `SchemaVersion` as an unconstrained `{"type": "string"}` (no `const`/`pattern`), so both 1.0 and 1.1 validate;
  - **major-version rejection is unchanged**: a `2.x` artifact still fails with `schema_version_unsupported`, and a mixed 1.x pair still fails with `schema_version_mismatch`.
- **No fixture regeneration was forced.** The committed schema fixtures and all five E2E/benchmark snapshots remain `1.0` and keep loading; they now double as real backward-compatibility evidence (`crates/cratevista-server/tests/e2e_fixtures.rs::committed_1_0_snapshots_still_load_after_the_1_1_bump`).
- **Unlike the 2026-07-14 amendment, the checked-in JSON Schema *is* affected**, because it is generated from `ExplorerDocument`, which now transitively includes `ViewExample`. It was regenerated and committed, and `tests/jsonschema_drift.rs` passes. The frontend types (`web/src/types/generated/explorer-document.ts`) were regenerated and committed; `npm run check:types` passes. **`web/dist` is unaffected** — TypeScript types are erased at compile time and no runtime code changed, so `check:dist` stays green.
- **Latent trap fixed while landing this:** three `cratevista-server` snapshot tests and one router test hard-coded the literal `"1.0"`. The bump turned their `.replace(…)` rewrites into silent no-ops (the router test failed outright). All four are now anchored on `SchemaVersion::CURRENT`, and the rewrite helper asserts the marker is present, so the next bump cannot silently void them.
- **Rendering is deliberately out of scope here.** These fields are data-only until PRD-08 **Amendment C** adds the explorer rendering increment; until then a document may carry them and the UI will ignore them.

### Stable identifier scheme (`ids` module)

Deterministic strings derived from canonical source identity, not runtime order or rustdoc numeric ids:

- workspace: `workspace`
- package: `package:{name}` (workspace members; version excluded so version bumps don't invalidate references)
- target: `target:{package}:{kind}:{name}`
- module: `module:{crate}::{module_path}`
- item: `item:{kind}:{crate}::{canonical_path}`
- impl: `impl:{crate}:{trait_or_inherent}:{for_type}:{sig_discriminator}` (semantic discriminator always present — see below)
- manual entity: `manual:{config_id}`
- relation (basic form): `rel:{kind}:{from}->{to}` — derived from relation kind + source entity ID + target entity ID
- relation (disambiguated form): `rel:{kind}:{from}->{to}:{role}:{discriminator}` — used only when multiple semantically distinct same-kind relations can exist between the same endpoints; the `role` is the optional semantic role and the `discriminator` is a deterministic BLAKE3 discriminator appended only when the role alone is still insufficient
- view: `view:{name}`

Constructor helpers ensure producers never hand-build ids. Collisions are a validation error. Documented rule: "ids remain stable while canonical path + kind (+ semantic discriminator) are unchanged."

#### blake3 semantic discriminators

Where an id needs a discriminator (impl signatures, external package source disambiguation), use the **`blake3`** crate over a **normalized, UTF-8** input string, and take the first **128 bits** (16 bytes → 32 lowercase hex chars) unless a test shows a reason to keep more. The hash input MUST:

- carry an explicit **domain + version prefix** (e.g. `cratevista-id:v1:impl-sig:` / `cratevista-id:v1:pkg-src:`);
- use **unambiguous separators or explicit length framing** between components (e.g. length-prefixed segments) so different component splits can't collide;
- be built from **normalized semantic values** (canonical type/path strings), never from HashMap iteration order, `DefaultHasher`, a process seed, absolute machine paths, timestamps, or raw rustdoc numeric IDs.

The exact input format for each discriminator is documented in the `ids` module and in ADR-0003, so future versions don't silently produce different IDs (changing the format is a MAJOR change).

#### Robustness against the actual future inputs (issues 03/04/08)

- **Cargo package IDs** (`cargo_metadata::PackageId`) are opaque and embed source URLs / absolute paths / versions; they are **analyzer input only**, never the public identity. Workspace members use `package:{name}` (no version). External packages may use `package:{name}@{version}`. If identical `name/version` pairs from **different sources** must coexist, append a deterministic source discriminator = blake3 (per above) over the **normalized Cargo source identity** (e.g. registry/git/path *kind* + canonical source string) — **never an absolute filesystem path**.
- **Package and target names** back `package:{name}` and `target:{package}:{kind}:{name}` (unique within their scope).
- **rustdoc numeric item IDs** are per-crate and unstable across runs/toolchains; they are **never** exposed as public identity. `cratevista-rustdoc` keeps them as internal opaque keys (issue 04); issue 05 maps each item to a schema id via its **canonical path**. This satisfies "the schema must not expose raw rustdoc item IDs as its only stable public identity."
- **Canonical paths** are the primary basis for module/item ids.
- **Impls** frequently lack a clean path: `impl:{crate}:{trait_or_inherent}:{for_type}:{sig_discriminator}`, where `sig_discriminator` is a blake3 (per above) over the **normalized impl signature** (generics, where-clauses, trait + self type). This is **always** included so multiple inherent impl blocks for one type cannot collide.
- **Manual entities** use `manual:{config_id}` from the user-declared id (issue 08).
- **Relations** derive their id from **relation kind + source entity id + target entity id**, giving the basic public form `rel:{kind}:{from}->{to}`. When multiple semantically distinct same-kind relations can exist between the same endpoints, include the optional semantic `role` and, when the role alone is still insufficient, a deterministic BLAKE3 discriminator, giving `rel:{kind}:{from}->{to}:{role}:{discriminator}`. IDs must never depend on `DefaultHasher`, randomized hashing, collection/iteration order, timestamps, absolute paths, raw rustdoc IDs, or any process-specific data. The graph builder MAY merge identical relation evidence, but MUST NOT silently discard semantically distinct relations between the same endpoints — those carry a distinct `role` (and discriminator) and therefore a distinct id. Id collisions are a validation error.

### Control flow

Producers build typed `Entity`/`Relation`/`View` via constructors → assemble `ExplorerDocument` via a builder that sorts and dedups → `validate()` → serialize via the shared `canonical` serializer. Runtime metadata is accumulated separately into a `GenerationReport` (→ `generation.json`) and diagnostics into a `DiagnosticsReport` (→ `diagnostics.json`); all three artifacts serialize through the same `canonical` serializer.

### Error handling

`SchemaError` (thiserror): DanglingReference, DuplicateId, InvalidId, InvalidSourcePath. `validate()` collects **all** errors, not just the first. Unknown kinds are **not** errors.

### Compatibility

- `schema_version` is `MAJOR.MINOR`.
- **Additive (MINOR)**: adding a new optional field; **adding a new entity or relation kind** (kinds are open strings; unknown kinds remain valid in deser + validation, round-trip losslessly, and render via the generic frontend fallback).
- **Breaking (MAJOR)**: structural or semantic incompatibilities — removing/renaming a field, changing a field's meaning or type, changing the id scheme, changing a blake3 discriminator input format, or changing the meaning of an existing kind.
- Deserialization is lenient about unknown **fields** by default (forward-compatible reads). There is **no `--strict` mode in this issue**.
- `document.json` and `diagnostics.json` are deterministic by default (no reproducibility flag) because they carry no timestamps/runtime metadata (those live in `generation.json`).
- Policy documented in `docs/adr/0003-schema-versioning.md` (including the blake3 discriminator input formats).

### JSON Schema generation

**Single source of truth: the Rust types.** The JSON Schema artifact is *generated from* the Rust model via **`schemars` 1.x**, **and** the generated artifact is checked into the repository at `crates/cratevista-schema/schema/cratevista-document.schema.json` (so the frontend and external consumers can use it without a Rust build). "Generated **and** checked in", Rust types authoritative.

Both the regenerator and the drift test call the **same two functions**: `cratevista_schema::jsonschema::document_schema()` (build the schema) and `cratevista_schema::canonical::to_canonical_string(..)` (serialize it). `examples/gen_schema.rs` writes the artifact via those functions; `tests/jsonschema_drift.rs` regenerates in memory via the same functions and asserts byte-equality with the checked-in file (failing with the regeneration command on mismatch). **No `xtask` crate and no nightly.** Because the drift test runs under the standard `cargo test --workspace` gate, any divergence breaks CI.

### Conventions (aligned with the PRD-01 workspace)

Follow the conventions established by issue 01:

- Crate `Cargo.toml` uses `version.workspace`/`edition.workspace`/`rust-version.workspace`/`license.workspace` (edition 2024, latest-stable Rust — currently 1.97, `MIT OR Apache-2.0`) and pulls dependencies from `[workspace.dependencies]`. `serde`/`serde_json`/`thiserror` are already declared there; **add `schemars` 1.x and `blake3`** (and the dev-only `jsonschema`). `Cargo.lock` must capture the resolved `schemars` 1.x version.
- `#![forbid(unsafe_code)]` at the crate root.
- Every public item carries useful rustdoc (clippy runs with `-D warnings`).
- Errors use `thiserror` enums (mirroring `cratevista-core::CoreError`); `validate()` collects all errors rather than failing fast.
- Tests: inline `#[cfg(test)] mod tests` for unit tests, `tests/*.rs` for integration/fixture tests (mirroring `crates/cargo-cratevista/tests/cli.rs`).
- Must pass the existing gates unchanged: `cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets --all-features -- -D warnings`, `cargo test --workspace --all-features`.

### Security and privacy

Source paths are repo-relative and validated in `source` (this crate) — absolute paths, `..` escapes, and non-UTF-8 are rejected at construction; the server reuses this type for traversal safety. No secrets/env in the model.

## CLI/API/configuration changes

None directly. Adds `crates/cratevista-schema/schema/cratevista-document.schema.json` (generated from the Rust types via schemars 1.x, checked in) plus a drift test and an `examples/gen_schema.rs` regenerator (no `xtask`, no nightly). Establishes the `document.json` / `generation.json` / `diagnostics.json` three-artifact split consumed by issues 05/06/10. Adds `schemars` 1.x, `blake3`, and dev-only `jsonschema` to `[workspace.dependencies]`.

## Files and modules to create or modify

- `crates/cratevista-schema/src/{lib,document,entity,relation,view,source,docs,diagnostic,ids,kind,version,generation,canonical,validate,jsonschema}.rs`
- `crates/cratevista-schema/examples/gen_schema.rs` (writes the JSON Schema artifact via the shared functions; the regeneration entry point)
- `crates/cratevista-schema/fixtures/{minimal,full_mvp,manual_flow,unknown_kind}.document.json`
- `crates/cratevista-schema/fixtures/full_mvp.generation.json`
- `crates/cratevista-schema/fixtures/full_mvp.diagnostics.json`
- `crates/cratevista-schema/schema/cratevista-document.schema.json` (generated from Rust types via schemars 1.x, checked in)
- `Cargo.toml` (root): add `schemars` 1.x, `blake3`, and dev-only `jsonschema` to `[workspace.dependencies]`
- `crates/cratevista-schema/Cargo.toml`: `serde`/`serde_json`/`thiserror`/`schemars`/`blake3`; dev `jsonschema`
- `docs/adr/0003-schema-versioning.md` (incl. the blake3 discriminator input formats)
- `crates/cratevista-schema/tests/{roundtrip,determinism,validation,jsonschema_drift,unknown_kind}.rs`

## Testing strategy

### Unit tests

- ID constructors produce expected stable strings; collisions detected.
- `validate()` flags dangling/duplicate references.
- Open kinds: known constants serialize as expected; an unknown kind string round-trips losslessly and reports `is_known() == false`.
- Localized text default/translation resolution.

### Integration tests

- Round-trip every fixture through the canonical serializer (serialize→deserialize→serialize byte-equal). Covers `document.json`, `generation.json`, and `diagnostics.json` fixtures.
- Determinism: building the same logical document twice yields byte-identical `document.json` (canonical byte comparison; no `insta`).
- `unknown_kind.document.json`: a document using entity/relation kinds outside the MVP set **validates** and round-trips (unknown kinds are not errors).
- JSON Schema drift: regenerate via the shared functions and compare to the checked-in artifact; each fixture validates against the schema (via the dev `jsonschema` crate).
- Canonical serializer: object keys recursively ordered, array order preserved, exactly one trailing newline (unit tests on nested inputs).
- blake3 discriminators: a fixed normalized input produces a fixed 128-bit hex discriminator (golden test guarding the documented input format).

### End-to-end tests

Deferred (schema has no runtime surface until issue 06).

### Fixtures

`full_mvp.document.json` contains at least one entity of every known MVP `EntityKind` and one relation of every known `RelationKind`, plus a view with stages and a manual+discovered mix. `unknown_kind.document.json` exercises forward compatibility. `full_mvp.generation.json` demonstrates the runtime-metadata split.

## Performance considerations

Entities reference each other by id (no nesting/duplication). `Vec` + id lookups; `ExplorerDocument::index()` (id→&Entity) is a non-serialized helper.

## Observability and diagnostics

Diagnostics are a **separate** `diagnostics.json` artifact (`DiagnosticsReport` of `DocumentDiagnostic`s), not embedded in `ExplorerDocument`; the frontend fetches them separately (issue 06 exposes `/api/diagnostics`). `validate()` errors feed tests and the pipeline (issue 05). Non-deterministic runtime metadata is isolated in `generation.json`.

## Documentation changes

`docs/adr/0003-schema-versioning.md` (compatibility policy, open-kind fallback rule, id scheme, blake3 discriminator input formats, and the `document.json` / `generation.json` / `diagnostics.json` three-artifact split); a schema reference section describing the entity/relation kinds and the canonical serializer.

## Rollout and migration

Initial version `1.0`. Future changes follow ADR-0003 (kinds additive, structural/semantic changes are breaking).

## Risks and mitigations

- **Open kinds allowing typos to pass silently** → `is_known()` + a lint/diagnostic when producers emit unknown kinds unexpectedly; tests assert MVP producers only emit known kinds.
- **Non-determinism** → the single `canonical` serializer (recursive key ordering) + `BTreeMap`/sorted builders + determinism test; runtime metadata kept out of `document.json`/`diagnostics.json`.
- **ID instability across versions** → exclude volatile data (versions, spans) from ids; document the blake3 discriminator input formats (changing them is MAJOR); test with a "version bump" fixture pair and a golden discriminator test.

## Alternatives considered

- Closed Rust `enum` kinds (earlier draft): rejected per decision — forces a MAJOR bump to add a kind and loses unknown strings; open string-backed kinds with known constants + generic fallback chosen instead.
- Timestamps inside `document.json` with a `--reproducible` opt-out (earlier draft): rejected per decision — `document.json` is deterministic by default and timestamp-free; runtime metadata moves to `generation.json`.
- Content-hash-only ids: rejected (opaque, changes on trivial edits); path-based ids preferred, with blake3 discriminators only for signature/source disambiguation.
- Embedding `diagnostics` inside `document.json` (earlier draft): rejected per decision — diagnostics are a **separate `diagnostics.json`** artifact (`DiagnosticsReport`), keeping the document a pure structural model and matching the `/api/diagnostics` endpoint (issue 06).
- Naming the schema diagnostic `Diagnostic` (colliding with `cratevista_core::Diagnostic`) or `GenerationDiagnostic`: rejected — `DocumentDiagnostic` removes ambiguity and does not imply it lives in `generation.json`.

## Implementation sequence

1. Newtypes + `kind` (open kinds) + `version` + `ids` (incl. `blake3` discriminators + documented input formats).
2. Core types (Entity/Relation/View/`ExplorerDocument`) + `source` + serde.
3. `canonical` serializer; `generation` (`GenerationReport`); `diagnostic` (`DocumentDiagnostic` + `DiagnosticsReport`).
4. `validate` + builder (sort/dedup).
5. `jsonschema` (schemars 1.x) via shared functions + `examples/gen_schema.rs` + fixtures (incl. `unknown_kind` + `diagnostics`).
6. Tests (roundtrip/determinism/validation/unknown-kind/drift/canonical/blake3) + ADR-0003.

## Acceptance criteria

- [x] `cratevista-schema` contains the canonical Rust model (`ExplorerDocument`, `GenerationReport`, `DiagnosticsReport`/`DocumentDiagnostic`). *(verified: crate compiles; all types present and re-exported from `lib.rs`)*
- [x] The model round-trips through JSON via the canonical serializer. *(verified: `tests/roundtrip.rs` — every fixture deserializes and re-serializes byte-identically)*
- [x] Example fixtures demonstrate every MVP entity and relation kind. *(verified: `full_mvp.document.json` has one entity of every `KNOWN_ENTITY_KIND` and one relation of every `KNOWN_RELATION_KIND`; built by `examples/gen_fixtures.rs`)*
- [x] Serialization output is deterministic. *(verified: `tests/determinism.rs` — identical bytes regardless of input order; document is timestamp-free; regenerating fixtures produces no diff)*
- [x] Documented compatibility and versioning policy. *(verified: `docs/adr/0003-schema-versioning.md`)*
- [x] React Flow concepts absent from the Rust schema. *(verified: no coordinate/react-flow fields; views carry filters + `presentation` hints only)*
- [x] rustdoc JSON IDs do not leak as the only public stable identity. *(verified: `ids.rs` derives from names/canonical paths; unit tests assert formats)*
- [x] Invalid references detected by schema validation; unknown kinds are not errors. *(verified: `tests/validation.rs` + `tests/unknown_kind.rs`)*
- [x] Source paths repository-relative and validated. *(verified: `source.rs` rejects absolute/drive/UNC/`..`; deserialize validates; unit tests)*
- [x] Unit and fixture tests cover representative documents (no `insta`). *(verified: checked-in fixtures + canonical byte comparison; no `insta` dependency)*
- [x] Unknown entity/relation kinds round-trip and are frontend-fallback-friendly. *(verified: `tests/unknown_kind.rs`; `is_known()` false; string preserved)*
- [x] `document.json`/`diagnostics.json` are timestamp-free; runtime metadata is only in `generation.json`; the three artifacts are separate and diagnostics are not embedded in `ExplorerDocument`. *(verified: `ExplorerDocument` has no diagnostics/timestamp field; `tests/determinism.rs` asserts no timestamp; separate `DiagnosticsReport`/`GenerationReport` fixtures)*
- [x] `cratevista-schema` does not depend on `cratevista-core`. *(verified: schema `Cargo.toml` has no `cratevista-core` dep; only serde/serde_json/thiserror/schemars/blake3)*
- [x] The document diagnostic (`DocumentDiagnostic`) and the runtime CLI diagnostic (`cratevista_core::Diagnostic`) are distinct types; the schema crate does not reference the core type. *(verified: distinct types with distinct fields; schema crate does not import core)*
- [x] One canonical JSON serializer is used by the regenerator and the drift test, and the checked-in JSON Schema (schemars 1.x) does not drift. *(verified: `tests/jsonschema_drift.rs` passes; `example gen_schema` regeneration produces no diff; both call `document_schema_json()`)*
- [x] blake3 semantic discriminators are deterministic and their input format is documented. *(verified: `ids.rs` golden discriminator tests — deterministic, 128-bit, length-framed, domain-separated; format in ADR-0003)*

Verification:

```bash
cargo test -p cratevista-schema --all-features
cargo run -p cratevista-schema --example gen_schema > /tmp/schema.json && diff crates/cratevista-schema/schema/cratevista-document.schema.json /tmp/schema.json   # regeneration matches (also covered by the drift test)
cargo tree -p cratevista-schema -i cratevista-core   # must report no path from schema to core
```

## Open questions

**None blocking.** All prior questions are resolved by the approved decisions (schemars 1.x; blake3 discriminators; checked-in JSON fixtures + canonical byte comparison, no `insta`; no `--strict` mode; `DocumentDiagnostic` name + separate `diagnostics.json`; single canonical serializer + checked-in schema at `crates/cratevista-schema/schema/…`; refined package/relation/impl ID rules; no schema→core dependency).

Cross-PRD follow-up (tracked in INDEX, not blocking issue 02): PRDs **05** and **07** still say generation diagnostics appear "in the document"; they need a minor wording update to reflect that diagnostics are a **separate `diagnostics.json` artifact** and are surfaced via `/api/diagnostics` (issue 06 already provides that endpoint), not embedded in `ExplorerDocument`.

## Traceability

Issue-02 checkboxes → tests above. Consumed by: issue 03 (metadata→entities), 04 (rustdoc→normalized→entities + partial flag), 05 (assembles `ExplorerDocument` → `document.json`; writes `GenerationReport` → `generation.json` and `DiagnosticsReport` → `diagnostics.json`, all via the canonical serializer), 06 (`/api/document`, `/api/diagnostics`), 07 (frontend adapter + generic unknown-kind fallback; fetches diagnostics separately), 08 (manual entities/views + `manual` kind/provenance).
