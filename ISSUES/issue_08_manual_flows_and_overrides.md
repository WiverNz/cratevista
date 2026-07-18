# Issue 08 — Add manual architecture flows and presentation overrides

## Goal

Allow maintainers to enrich automatically discovered Rust structure with business and runtime architecture that rustdoc cannot infer reliably.

## Configuration location

Define a clear project-local convention, for example:

```text
cratevista.toml
.cratevista/
  flows.yaml
  overrides.yaml
  docs/
```

The exact format must be selected and justified in the PRD.

## Required capabilities

### Manual entities

Examples:

- Web client
- Mobile client
- Redis
- PostgreSQL
- message broker
- external API
- observability stack
- deployment environment

### Flows

A flow can define:

- id
- localized title and description
- ordered stages
- selected discovered entities by stable reference
- manual entities
- relations
- edge labels
- default focus
- examples of output data
- documentation blocks

### Overrides

An override can enrich a discovered entity with:

- display label
- category
- tags
- description
- stage
- hidden/promoted state
- extra documentation
- presentation hints

Overrides must not silently change semantic identity.

## Validation

Configuration errors must report:

- file
- line/column where possible
- invalid reference
- duplicate id
- unsupported kind
- missing required field
- type mismatch

Broken references must not crash the server.

## Acceptance criteria

- [ ] A sample flow reproduces the pattern Clients → Gateway → Services → Infrastructure.
- [ ] Manual and discovered entities coexist in one view.
- [ ] Overrides preserve discovered stable IDs.
- [ ] Invalid references produce actionable diagnostics.
- [ ] Configuration supports comments and reviewable diffs.
- [ ] The schema is documented with complete examples.
- [ ] Localization-ready labels are supported without requiring full UI translation in this issue.
- [ ] Configuration loading is deterministic.
- [ ] Tests cover merge precedence and conflict behavior.

## PRD requirement

Do not implement this issue directly.

First create:

```text
PRD/issue_08_manual_flows_and_overrides.md
```

The PRD must map every acceptance criterion to concrete modules, tests, and verification commands.
