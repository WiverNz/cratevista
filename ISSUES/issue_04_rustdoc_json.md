# Issue 04 — Generate and ingest rustdoc JSON

## Goal

Build a robust adapter that invokes rustdoc JSON generation for selected workspace targets and converts its output into an internal normalized representation.

## Required extraction

Where supported by rustdoc JSON:

- Modules
- Structs
- Enums
- Unions
- Traits
- Implementations
- Functions
- Methods
- Fields
- Variants
- Type aliases
- Constants and statics
- Macros
- Visibility
- Documentation
- Attributes relevant to presentation
- Source spans
- Canonical item paths
- Re-exports/imports
- Function inputs, outputs, and error/result structure
- Implemented trait and target type

## Compatibility requirements

rustdoc JSON is not the CrateVista public schema.

The implementation must:

- isolate rustdoc-specific types in `cratevista-rustdoc`;
- verify supported format/toolchain compatibility;
- produce clear diagnostics for unsupported versions;
- document whether a pinned nightly is required;
- avoid silently downloading or switching toolchains;
- support cached fixture-based tests;
- retain enough raw context for useful diagnostics without exposing raw JSON to the frontend.

## Invocation requirements

Define:

- target selection;
- feature selection;
- private-item support;
- dependency documentation behavior;
- output discovery;
- caching;
- cancellation and failure handling;
- behavior when only stable Rust is installed.

## Acceptance criteria

- [ ] A representative rustdoc JSON fixture is deserialized and normalized.
- [ ] The adapter detects incompatible format versions.
- [ ] Missing nightly/toolchain errors are actionable.
- [ ] The user is not surprised by automatic global toolchain changes.
- [ ] Public and optional private-item modes are tested.
- [ ] Source spans become validated repository-relative source locations.
- [ ] The adapter is independently testable without running rustdoc.
- [ ] Integration tests cover at least structs, enums, traits, impls, methods, generics, and re-exports.
- [ ] Generated raw rustdoc data is kept out of the frontend contract.

## PRD requirement

Do not implement this issue directly.

First create:

```text
PRD/issue_04_rustdoc_json.md
```

The PRD must map every acceptance criterion to concrete modules, tests, and verification commands.
