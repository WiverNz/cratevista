# CrateVista project instructions

## Product

CrateVista is a standalone open-source Cargo subcommand that generates and serves an interactive architecture and documentation explorer for Rust workspaces.

Canonical public description:

> Turn any Rust workspace into an interactive architecture map.

Canonical command:

```bash
cargo cratevista open
```

Canonical package and binary:

```text
package: cargo-cratevista
binary:  cargo-cratevista
command: cargo cratevista
```

## Interactive explorer UI direction

The UI-related PRDs — `PRD/issue_06_server_and_embedded_ui.md`,
`PRD/issue_07_interactive_explorer_ui.md`, and
`PRD/issue_08_manual_flows_and_overrides.md` — build a polished, accessible,
browser-based architecture explorer. Its design is defined directly by these
requirements:

- Render the interactive map with **React Flow**, laid out with **ELK**.
- Provide an inspector, a toolbar, search, kind/relation filters, focus/related
  modes, and a dynamic legend.
- Render open entity and relation kinds generically, with a safe fallback for
  kinds the UI does not special-case.
- Drive the whole UI from the generated `ExplorerDocument` data via a pure
  schema→React Flow adapter — never from hand-authored UI data.

## Required workflow

Never implement an issue directly from `ISSUES/`.

For every issue:

1. Read `ISSUES/CONTEXT.md`.
2. Read the selected issue.
3. Explore the current repository and relevant ADRs.
4. Create or update a detailed PRD under `PRD/`.
5. Stop and report the PRD path.
6. Implement only after the PRD has been explicitly selected for implementation.
7. Keep the PRD acceptance checklist synchronized with the implementation.

Use these skills:

```text
/create-prd ISSUES/issue_XX_name.md
/create-all-prds
/review-prd PRD/issue_XX_name.md
/implement-prd PRD/issue_XX_name.md
```

## Engineering principles

- Prefer a small number of deep, independently testable modules.
- Keep the generated graph schema independent from React Flow and rustdoc JSON.
- The frontend must not depend directly on a specific rustdoc JSON version.
- Keep source discovery, semantic conversion, graph generation, layout, serving, and presentation as separate concerns.
- Do not parse generated rustdoc HTML.
- Use `cargo metadata --format-version 1` for workspace and package metadata.
- Use rustdoc JSON behind an explicit adapter boundary.
- Treat rustdoc JSON as versioned and potentially incompatible.
- Pin the supported nightly toolchain or provide a clear compatibility mechanism.
- Do not install toolchains, Node.js, or other global software silently.
- The installed CLI must serve a prebuilt embedded web application; end users must not need Node.js.
- Bind the development server to `127.0.0.1` by default.
- Do not expose arbitrary local files over HTTP.
- Never execute project code as part of visualization.
- Avoid `unsafe` unless a PRD explicitly justifies it.
- Keep platform support in mind: Linux, macOS, Windows, and WSL.

## Initial technology direction

These are starting constraints, not permission to skip analysis:

### Rust workspace

Expected crates:

```text
crates/
  cratevista-schema
  cratevista-metadata
  cratevista-rustdoc
  cratevista-graph
  cratevista-server
  cargo-cratevista
web/
```

The final boundaries may be adjusted by an approved PRD.

### Backend candidates

- `cargo_metadata`
- `rustdoc-types`
- `serde` / `serde_json`
- `clap`
- `axum`
- `tokio`
- `notify`
- `rust-embed`
- `tracing`
- `thiserror` / `anyhow`

Do not add a dependency only because it appears here. Justify it in the relevant PRD.

### Frontend direction

- React
- TypeScript
- Vite
- React Flow
- ELK.js for complex layouts
- A thin adapter from the CrateVista schema to React Flow nodes and edges

## Quality gates

Before marking any implementation complete:

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-features
```

For frontend work, define and run equivalent format, lint, type-check, and test commands.

Every new public Rust item must have useful rustdoc unless the PRD explicitly excludes it.

## Documentation rules

- Keep `README.md` focused on installation, first run, screenshots, and common commands.
- Put design decisions in `docs/adr/`.
- Put implementation plans in `PRD/`.
- Put user configuration reference in `docs/configuration.md`.
- Use the terms defined in `ISSUES/CONTEXT.md`.
- Avoid using “UML” as the sole description. The product is an interactive Rust architecture and documentation explorer.

## Security and privacy

- The default workflow is fully local.
- Do not upload source code or generated project data.
- Source snippets must be opt-in or constrained to explicit source locations.
- Do not include environment variables, secrets, target build outputs, or ignored files in generated documents.
- Validate all paths derived from HTTP parameters.
- Make generated static sites explicit about whether source paths or snippets are included.

## Scope discipline

The MVP is Rust-only.

Do not add:

- AI-generated explanations
- cloud accounts
- hosted source-code ingestion
- support for Go, C++, Python, or JavaScript analysis
- a full compiler frontend
- a guaranteed function call graph
- automatic sequence diagrams inferred from function bodies

Those may be proposed later, but are not part of the initial implementation.
