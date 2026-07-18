# Issue 05 — Build the semantic graph and generated views

## Goal

Merge Cargo metadata and normalized rustdoc data into one deterministic CrateVista explorer document.

## Required entity kinds

At minimum:

- workspace
- package/crate
- target
- module
- struct
- enum
- union
- trait
- function
- method
- implementation
- type alias
- constant
- macro

## Required relation kinds

At minimum:

- contains
- depends_on
- imports
- re_exports
- implements
- implemented_for
- has_field_type
- accepts_type
- returns_type
- error_type
- references_type

The PRD must define which relationships are reliable, approximate, or intentionally excluded.

## Required generated views

- Workspace overview
- Crate dependencies
- Module hierarchy
- Types
- Traits and implementations
- Type relationships
- Public API
- Documentation coverage

Each view must be a projection over the same canonical document, not a separate incompatible data format.

## Important behavior

- Resolve type references where reliable.
- Preserve unresolved references as diagnostics rather than inventing connections.
- Avoid duplicate nodes caused by re-exports.
- Keep stable identity separate from display labels.
- Generate readable descriptions from rustdoc without losing Markdown semantics.
- Compute documentation coverage consistently.
- Produce deterministic ordering and stable output.

## Acceptance criteria

- [ ] Cargo and rustdoc inputs merge into one valid document.
- [ ] Every relation references existing entities.
- [ ] Repeated generation of unchanged input produces byte-stable JSON, excluding explicitly documented timestamps.
- [ ] Trait implementations are visible and navigable.
- [ ] Function input and output types are represented where resolvable.
- [ ] Unresolved types produce diagnostics, not incorrect edges.
- [ ] Generated views contain filters and presentation metadata but no fixed UI coordinates.
- [ ] Large-workspace performance has a benchmark fixture and an explicit target.
- [ ] Graph validation tests detect cycles only where cycles are forbidden, not globally.

## PRD requirement

Do not implement this issue directly.

First create:

```text
PRD/issue_05_graph_builder.md
```

The PRD must map every acceptance criterion to concrete modules, tests, and verification commands.
