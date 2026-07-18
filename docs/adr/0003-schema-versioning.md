# ADR 0003 — Explorer schema versioning, kinds, IDs, and artifacts

- Status: Accepted
- Date: 2026-07-12
- Related: `PRD/issue_02_explorer_schema.md`, `crates/cratevista-schema`

## Context

`cratevista-schema` is the single, frontend-independent contract between the
analyzers and the server/frontend. It must be deterministic, forward-compatible,
and self-validating, and its identifiers must survive regeneration.

## Decision

### Artifacts (three separate top-level files)

- `document.json` — an `ExplorerDocument` (project + entities + relations +
  views). Deterministic: **no timestamps, no runtime metadata, no embedded
  diagnostics**.
- `generation.json` — a `GenerationReport` (tool name/version, `generated_at`
  timestamp, toolchain, rustdoc format version, input hashes, counts,
  durations, `partial`). The sole home for runtime/non-deterministic metadata.
- `diagnostics.json` — a `DiagnosticsReport` (`schema_version` + a sorted list of
  `DocumentDiagnostic`). Diagnostics are surfaced separately (e.g.
  `/api/diagnostics`), never embedded in the document.

`cratevista_schema::DocumentDiagnostic` is distinct from the runtime
`cratevista_core::Diagnostic`; the `DocumentDiagnostic → core::Diagnostic`
conversion is owned by `cratevista-core` in a later issue. `cratevista-schema`
does not depend on `cratevista-core`.

### Canonical JSON

One serializer (`cratevista_schema::canonical::to_canonical_string`) is used for
every artifact and for the JSON Schema. It recursively orders object keys,
preserves array order, pretty-prints with two-space indent, writes UTF-8, and
ends the output with exactly one newline. This makes `document.json` /
`diagnostics.json` byte-stable and keeps the JSON Schema artifact and its drift
test in lockstep.

### Open kinds

`EntityKind` and `RelationKind` are open, string-backed newtypes with known-kind
constants. Unknown kinds deserialize, validate, serialize, and round-trip without
loss; the frontend renders them with a generic fallback. There is **no strict
mode** that rejects unknown kinds.

### Stable identifiers

IDs are deterministic strings derived from names / canonical paths — never from
array order, `DefaultHasher`, a process seed, absolute machine paths, timestamps,
or raw rustdoc numeric IDs.

- workspace: `workspace`
- package (workspace member): `package:{name}` (version excluded)
- package (external): `package:{name}@{version}`, optionally
  `package:{name}@{version}:{discriminator}` when identical name/version pairs
  from different sources must coexist (discriminator = BLAKE3 over the normalized
  Cargo source identity)
- target: `target:{package}:{kind}:{name}`
- module: `module:{crate}::{module_path}`
- item: `item:{kind}:{crate}::{canonical_path}`
- impl: `impl:{crate}:{trait_or_inherent}:{for_type}:{discriminator}` (the
  discriminator is always present)
- manual entity: `manual:{config_id}`
- relation (basic): `rel:{kind}:{from}->{to}`
- relation (disambiguated): `rel:{kind}:{from}->{to}:{role}:{discriminator}`
- view: `view:{name}`

### BLAKE3 semantic discriminators

Where a discriminator is required, it is the first **128 bits** (32 lowercase hex
chars) of a BLAKE3 hash over a domain-separated, length-framed input:

```text
"cratevista-id:v1:" <domain> ":" ( <byte_len> ":" <component_bytes> ":" )*
```

- `<domain>` names the use: `impl-sig`, `pkg-src`, `relation`.
- Each component is framed by its UTF-8 byte length, so different component
  splits cannot collide.
- Components are normalized semantic values only.

Changing this framing (or the `v1` version) changes IDs and is therefore a
**MAJOR** schema change.

### Compatibility policy

`schema_version` is `MAJOR.MINOR` (current: `1.0`).

- **Additive (MINOR)**: a new optional field; a new entity/relation kind
  (unknown kinds already round-trip and fall back generically).
- **Breaking (MAJOR)**: structural or semantic incompatibilities — removing,
  renaming, or retyping a field; changing a field's or an existing kind's
  meaning; changing the ID scheme; or changing a BLAKE3 discriminator input
  format.

Deserialization is lenient about unknown fields (forward-compatible reads).

### JSON Schema

The Rust types are the single source of truth. `schemars` (1.x) generates the
schema; the artifact is checked in at
`crates/cratevista-schema/schema/cratevista-document.schema.json` and guarded by
`tests/jsonschema_drift.rs`, which regenerates via the same functions as
`examples/gen_schema.rs` and byte-compares. Regenerate with:

```bash
cargo run -p cratevista-schema --example gen_schema \
  > crates/cratevista-schema/schema/cratevista-document.schema.json
```

## Consequences

- Reviewable, byte-stable diffs for `document.json`.
- New kinds ship without breaking existing consumers.
- The document is a pure structural model; runtime metadata and diagnostics are
  isolated in their own artifacts.
