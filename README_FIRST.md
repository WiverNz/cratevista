# CrateVista planning pack for Claude Code

This pack describes a new standalone project:

> **CrateVista — Interactive Rust Architecture & Documentation Explorer**

CrateVista is installed as a Cargo subcommand and turns a Rust workspace into an interactive browser-based architecture explorer.

```bash
cargo install cargo-cratevista
cargo cratevista generate
cargo cratevista serve
cargo cratevista open
```

## Why this name

- **Crate** immediately signals the Rust ecosystem.
- **Vista** communicates an overview, map, and visual explorer.
- The full public title should always include searchable terms:

  **CrateVista — Interactive Rust Architecture & Documentation Explorer**

- Recommended package and repository names:
  - crates.io package: `cargo-cratevista`
  - binary: `cargo-cratevista`
  - Cargo command: `cargo cratevista`
  - GitHub repository: `cratevista`
  - website title: `CrateVista | Rust Architecture Explorer`

Before publishing, perform a final exact-name check on crates.io, GitHub, package managers, and domain registries.

## How to use this pack

1. Copy this directory structure into a new empty Git repository.
2. Start Claude Code in the repository root.
3. Ask Claude to read `CLAUDE.md`.
4. Generate all implementation PRDs:

```text
/create-all-prds
```

5. Review the generated files under `PRD/`.
6. Implement one approved PRD at a time:

```text
/implement-prd PRD/issue_01_workspace_and_cli.md
```

7. Review the result before moving to the next issue.

## Intended delivery order

1. Workspace and Cargo subcommand
2. Stable explorer schema
3. Cargo metadata ingestion
4. rustdoc JSON ingestion
5. Graph builder and generated views
6. Local server and embedded frontend
7. Interactive React Flow explorer
8. Manual architecture flows and overrides
9. Watch mode and live reload
10. Static build, release, and project launch

Do not implement directly from `ISSUES/`. Every issue must first be converted into an implementation-ready PRD.
