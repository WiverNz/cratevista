# Issue 09 — Add watch mode, caching, and live reload

## Goal

Make local development comfortable:

```bash
cargo cratevista serve --watch
```

Relevant changes should regenerate project data and refresh the browser without restarting the command manually.

## Required behavior

Watch relevant inputs:

- `Cargo.toml`
- `Cargo.lock`
- workspace member manifests
- Rust source files
- CrateVista configuration
- manual documentation included by configuration

Ignore:

- `target/`
- `.git/`
- generated CrateVista output
- frontend dependencies
- configured ignore patterns

## Regeneration behavior

- Debounce bursts of filesystem events.
- Do not run overlapping generations.
- Preserve the last valid document when regeneration fails.
- Publish diagnostics for the failed generation.
- Notify the frontend when a new valid document is available.
- Avoid reloading for output files created by CrateVista itself.
- Support clean cancellation and shutdown.

## Caching

The PRD must define safe caching boundaries for:

- Cargo metadata
- raw rustdoc JSON
- normalized rustdoc data
- final explorer document
- frontend layout state, if any

Cache invalidation must favor correctness over cleverness.

## Acceptance criteria

- [ ] Editing a relevant `.rs` file triggers one debounced regeneration.
- [ ] Editing ignored output does not create a regeneration loop.
- [ ] Browser data refreshes after successful generation.
- [ ] A failed generation leaves the previous valid graph visible and shows diagnostics.
- [ ] Concurrent regeneration is prevented.
- [ ] Watch mode shuts down cleanly.
- [ ] Tests use deterministic synthetic filesystem events where possible.
- [ ] Cache keys include all inputs that affect output.
- [ ] `--no-cache` behavior is defined and tested.

## PRD requirement

Do not implement this issue directly.

First create:

```text
PRD/issue_09_watch_and_live_reload.md
```

The PRD must map every acceptance criterion to concrete modules, tests, and verification commands.
