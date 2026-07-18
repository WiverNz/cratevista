# ADR 0002 — CLI surface and exit-code policy

- Status: Accepted
- Date: 2026-07-12
- Related: `PRD/issue_01_workspace_and_cli.md`

## Context

CrateVista installs as a Cargo external subcommand (`cargo cratevista`). It needs
a stable CLI surface and a single, documented exit-code policy that later issues
build on.

## Decision

### Invocation

The binary is `cargo-cratevista`. When invoked as `cargo cratevista <args>`,
Cargo runs `cargo-cratevista cratevista <args>`; the leading `cratevista` token
is stripped so the same parser serves both that form and a direct
`cargo-cratevista <args>` invocation.

### Global options

- `--manifest-path <PATH>`
- `-v/--verbose` (repeatable)
- `-q/--quiet`
- `--color <auto|always|never>`
- `--format <human|json>` (machine-readable diagnostics)

### Commands

`init`, `doctor`, `generate`, `serve`, `open`, `build`. In this bootstrap,
`generate`/`serve`/`open`/`build` are stubs that report "not implemented yet".

### Exit-code policy (single source of truth)

| code | meaning                                |
|------|----------------------------------------|
| 0    | success                                |
| 1    | runtime / generation error             |
| 2    | usage / argument error (clap default)  |
| 3    | prerequisite / environment error       |
| 4    | not implemented yet (bootstrap stub)   |

`doctor` returns 0 when only warnings/info are present and 3 when any fatal
prerequisite check fails (e.g. no Cargo project detected).

### Logging & output

Logs go to stderr via `tracing`; verbosity sets the level (`quiet` → ERROR,
`-v` → INFO, `-vv` → DEBUG, `-vvv` → TRACE). Failure diagnostics render as human
text (stderr) or, with `--format json`, as a JSON object (stdout).

## Consequences

- Later issues fill in the stub commands without changing the CLI contract.
- Tools and scripts can rely on the exit codes and `--format json` output.
