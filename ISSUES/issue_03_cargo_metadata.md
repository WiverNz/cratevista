# Issue 03 — Ingest Cargo workspace metadata

## Goal

Use Cargo's machine-readable metadata to discover workspace structure without parsing `Cargo.toml` manually.

## Required discovery

- Workspace root
- Workspace members
- Packages
- Package versions
- Manifest paths
- Targets and target kinds
- Target source paths
- Features
- Resolved package dependency graph
- Dependency kinds where available
- Workspace-default members
- Selected package filtering

## Command behavior

The generator must support:

```bash
cargo cratevista generate
cargo cratevista generate --manifest-path path/to/Cargo.toml
cargo cratevista generate --package package-name
cargo cratevista generate --workspace
```

Exact flags may be refined in the PRD, but ambiguity must be resolved explicitly.

## Generated graph contribution

This component should produce normalized intermediate data or schema entities for:

- workspace
- packages
- targets
- package dependency relations
- containment relations

It must not calculate UI coordinates.

## Edge cases

- Virtual workspaces
- Single-package projects
- Renamed dependencies
- Multiple dependency kinds
- Path dependencies
- Optional dependencies and features
- Binary-only packages
- Examples, benches, integration tests, and proc macros
- Non-UTF-8 paths, with an explicit support policy
- Metadata command failure
- Unsupported or missing Cargo

## Acceptance criteria

- [ ] Cargo metadata is requested with an explicit format version.
- [ ] The implementation does not parse manifests as the primary metadata source.
- [ ] Workspace and single-package fixtures are covered.
- [ ] Package dependency output is deterministic.
- [ ] External dependencies can be included or excluded by configuration.
- [ ] The default view prioritizes workspace packages.
- [ ] Errors include the command context and actionable remediation.
- [ ] No project binaries or tests are executed.
- [ ] Unit tests do not require network access.

## PRD requirement

Do not implement this issue directly.

First create:

```text
PRD/issue_03_cargo_metadata.md
```

The PRD must map every acceptance criterion to concrete modules, tests, and verification commands.
