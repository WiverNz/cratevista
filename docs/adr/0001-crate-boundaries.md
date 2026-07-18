# ADR 0001 — Crate boundaries

- Status: Accepted
- Date: 2026-07-12
- Related: `PRD/issue_01_workspace_and_cli.md`, `CLAUDE.md`

## Context

CrateVista must keep source discovery, semantic conversion, graph generation,
serving, and presentation as separate, independently testable concerns. The
initial technology direction in `CLAUDE.md` lists a set of crates but permits
adjustment via an approved PRD.

## Decision

The workspace uses these crates:

```
crates/
  cratevista-core       # orchestration / use-case layer + application runtime scaffolding
  cratevista-schema     # canonical domain model (entities, relations, views, source paths, kinds)
  cratevista-metadata   # cargo metadata ingestion
  cratevista-rustdoc    # rustdoc JSON invocation + normalization adapter
  cratevista-graph      # document assembly and generated views
  cratevista-server     # local HTTP server + embedded UI
  cargo-cratevista      # CLI binary (thin adapter over cratevista-core)
web/                    # frontend (added in issue 07)
```

Two later crates are introduced **only by their own issues**, not during
bootstrap:

- `cratevista-config` (issue 08) — manual flows and overrides.
- `cratevista-watch` (issue 09) — watch mode, caching, live reload.

### `cratevista-core` is an orchestration/use-case crate only

`cratevista-core` coordinates generation workflows and defines application-level
use cases (`init`, `doctor`, and later `generate`, `serve`, `build`), connecting
the analyzer, graph, schema, and server crates. It must **not**:

- own shared domain models — those belong in `cratevista-schema`; nor
- become a generic utilities crate.

During bootstrap it also hosts thin application-runtime scaffolding used by the
CLI (diagnostics rendering, exit-code policy, logging init, process/OS path
resolution). Domain path validation (validated repository-relative source paths,
traversal safety) is a `cratevista-schema` concern, not a `cratevista-core` one.

### Dependency layering

`schema` → {`metadata`, `rustdoc`, `config`} → `graph` → `server`/`watch` →
`cratevista-core` → `cargo-cratevista`. The analyzer, graph, and server crates do
not depend on `cratevista-core`.

## Consequences

- Adding `core`/`config`/`watch` beyond the `CLAUDE.md` list is deliberate and
  recorded here.
- The CLI stays thin; testable logic lives in library crates.
- Bootstrap creates only `core`, `cargo-cratevista`, and compiling placeholders
  for `schema`/`metadata`/`rustdoc`/`graph`/`server`.
