# Issue 01 — Bootstrap the workspace and Cargo subcommand

## Goal

Create the initial CrateVista repository structure and a working Cargo external subcommand.

The following commands must parse successfully:

```bash
cargo cratevista --help
cargo cratevista init
cargo cratevista generate
cargo cratevista serve
cargo cratevista open
cargo cratevista build
cargo cratevista doctor
```

At this stage, non-bootstrap commands may return explicit “not implemented yet” diagnostics, but command contracts and shared infrastructure must be established.

## Required outcomes

- A Rust workspace with clear crate boundaries.
- Binary package named `cargo-cratevista`.
- Cargo external-subcommand argument handling compatible with `cargo cratevista ...`.
- Structured logging and human-readable errors.
- Cross-platform path handling.
- Initial CI for formatting, linting, and tests.
- Initial `README.md`, license files, contribution guide, and security policy.
- `rust-toolchain.toml` decision documented.
- `cargo cratevista init` creates a minimal configuration without overwriting existing files.
- `cargo cratevista doctor` reports toolchain and project prerequisites without modifying the machine.

## CLI behavior to define

- Global `--manifest-path`
- Global verbosity control
- Machine-readable diagnostics option, if justified
- Exit-code policy
- Help text and examples
- Behavior outside a Cargo project
- Behavior for a virtual workspace
- Behavior on Windows and WSL

## Acceptance criteria

- [ ] `cargo install --path crates/cargo-cratevista` installs the external subcommand.
- [ ] `cargo cratevista --help` shows all MVP commands.
- [ ] `cargo cratevista init` is idempotent.
- [ ] Existing configuration is never overwritten without an explicit flag.
- [ ] `doctor` distinguishes warnings from fatal errors.
- [ ] CLI tests cover argument parsing and representative failure modes.
- [ ] CI runs formatting, clippy, and workspace tests.
- [ ] The repository has no dependency on a globally installed Node.js runtime for end-user commands.
- [ ] Architectural crate boundaries are documented in an ADR.

## PRD requirement

Do not implement this issue directly.

First create:

```text
PRD/issue_01_workspace_and_cli.md
```

The PRD must map every acceptance criterion to concrete modules, tests, and verification commands.
