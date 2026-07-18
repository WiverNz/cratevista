# Issue 02 — Define the stable CrateVista explorer schema

## Goal

Create the versioned, frontend-independent document model that all analyzers and renderers use.

The schema must not expose React Flow types and must not mirror rustdoc JSON directly.

## Required concepts

- Document metadata and schema version
- Project/workspace metadata
- Entities
- Relations
- Views
- Stages/groups
- Source locations
- Documentation blocks
- Tags and attributes
- Diagnostics
- Generation metadata
- Optional localization-ready labels and descriptions
- Manual-vs-discovered provenance

## Identity requirements

Define stable identifiers for:

- workspace
- Cargo package
- target
- module
- rustdoc item
- manual entity
- relation
- view

Identifiers must remain stable across repeated generation when the corresponding source item has not meaningfully changed.

Do not rely only on array indexes or unstable runtime ordering.

## Serialization requirements

- JSON is the canonical generated interchange format.
- Unknown fields should be handled according to an explicit compatibility policy.
- Schema evolution rules must be documented.
- A JSON Schema artifact should be generated or maintained.
- Large documents should avoid needless duplication.
- Ordering should be deterministic to produce reviewable diffs.

## Acceptance criteria

- [ ] `cratevista-schema` contains the canonical Rust model.
- [ ] The model round-trips through JSON.
- [ ] Example fixtures demonstrate every MVP entity and relation kind.
- [ ] Serialization output is deterministic.
- [ ] The schema has a documented compatibility and versioning policy.
- [ ] React Flow concepts do not appear in the Rust schema.
- [ ] rustdoc JSON IDs do not leak as the only public stable identity.
- [ ] Invalid references are detected by schema validation.
- [ ] Source paths are repository-relative and validated.
- [ ] Unit and snapshot tests cover representative documents.

## PRD requirement

Do not implement this issue directly.

First create:

```text
PRD/issue_02_explorer_schema.md
```

The PRD must map every acceptance criterion to concrete modules, tests, and verification commands.
