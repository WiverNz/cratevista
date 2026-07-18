# PRD — Bootstrap the workspace and Cargo subcommand

## Status

**Approved — implemented and verified** (2026-07-12). Approved by the user with the decisions recorded below, then implemented. All acceptance criteria are checked with verified results; quality gates (`fmt --check`, `clippy -D warnings`, `test --workspace`) pass locally on Windows (Rust 1.97.0 stable) and the external subcommand installs and runs. See "Deviations" note under Fixtures.

### Approved decisions (recorded)

1. **Crate layout** — `cratevista-core` is approved as an **orchestration/use-case crate only**: it coordinates generation workflows, defines application-level use cases, and connects metadata/rustdoc/graph/schema/server (and later config/watch); it must **not** own shared domain models (those live in `cratevista-schema`) and must **not** become a generic utilities crate. `cratevista-config` and `cratevista-watch` are **not** created in issue 01 — they are introduced only by issues 08 and 09.
2. **Rust version** — `edition = "2024"`; `rust-version = "1.85"` **as originally approved (2026-07-12)**, now **superseded by the latest-stable Rust policy (ADR-0010)**: the workspace tracks the latest stable release, currently **`rust-version = "1.97"`** with `rust-toolchain.toml` pinned to **`1.97.0`**. The CrateVista application builds/tests/lints on **stable** Rust; the nightly requirement for rustdoc JSON belongs to issue 04 and must not make the workspace require nightly for `cargo fmt`, `cargo clippy`, unit tests, or `--help`.
3. **License** — dual **`MIT OR Apache-2.0`**, with `LICENSE-MIT` and `LICENSE-APACHE` files and matching Cargo package metadata.
4. **Generated output** — default `target/cratevista/{document.json, generation.json, diagnostics.json, cache/, site/}`; `cargo cratevista build` supports `--output <path>`; generated output is git-ignored and never committed by default.
5. **This PRD** is Approved; do not implement yet.

## Source issue

`ISSUES/issue_01_workspace_and_cli.md`

## Summary

Establish the CrateVista Cargo workspace, the `cargo-cratevista` binary that behaves as the `cargo cratevista` external subcommand, and the shared application scaffolding (top-level error/result, terminal diagnostics rendering, logging, exit-code policy, CLI contracts, use-case entry points) that every later issue depends on. Non-bootstrap subcommands parse and return explicit "not implemented yet" diagnostics; their contracts are fixed here so later PRDs only fill in behavior.

## Problem statement

The repository currently contains only planning material (`CLAUDE.md`, `ISSUES/`, `PRD/`, `docs/NAME_AND_POSITIONING.md`, `README_FIRST.md`). There is no Rust code, no `Cargo.toml`, no CI. Before any analyzer, schema, or server can be built, the workspace layout, CLI surface, and shared conventions must exist and be stable so parallel later work does not conflict.

## Goals

- A Cargo workspace with clear, documented crate boundaries.
- A `cargo-cratevista` binary invocable as `cargo cratevista ...`.
- Global CLI options, subcommand contracts, exit-code policy, and help text.
- Application scaffolding: structured logging, human + machine-readable diagnostics, cross-platform process/OS path resolution.
- `init` and `doctor` fully implemented (they do not depend on later issues).
- CI running fmt, clippy, and tests on Linux/macOS/Windows.
- Baseline project docs (README, licenses, CONTRIBUTING, SECURITY) and an ADR fixing crate boundaries.

## Non-goals

- Implementing `generate`, `serve`, `open`, or `build` behavior (later issues). They exist as parsing stubs returning a structured "not implemented" diagnostic with a distinct exit code.
- Defining the explorer schema or the domain path/source-location types (issue 02).
- Embedding the frontend (issue 06).
- Creating the `cratevista-config` or `cratevista-watch` crates. Those are owned by issues 08 and 09 respectively and must not be scaffolded prematurely here.

## Current repository state

- No `Cargo.toml`, no `crates/`, no `web/`, no `docs/adr/`, no CI.
- Product/vocabulary fixed by `ISSUES/CONTEXT.md`; canonical names fixed by `docs/NAME_AND_POSITIONING.md` and `CLAUDE.md` (`package/binary: cargo-cratevista`, command `cargo cratevista`).

## Terminology

Uses `ISSUES/CONTEXT.md` vocabulary (Explorer document, Entity, Relation, View, Source location, Flow). Adds:

- **Subcommand contract**: the fixed name, arguments, and exit codes of a `cargo cratevista` subcommand.
- **Diagnostic**: a structured, user-facing message with severity, code, message, and optional remediation; renderable as human text or JSON.
- **Use case / orchestration**: an application-level operation (`generate`, `serve`, `build`, `doctor`, `init`) composed from lower-level crates. Owned by `cratevista-core`.

## User-visible behavior

These parse successfully and show help:

```bash
cargo cratevista --help
cargo cratevista init
cargo cratevista generate
cargo cratevista serve
cargo cratevista open
cargo cratevista build
cargo cratevista doctor
```

- `cargo cratevista` with no subcommand prints help and exits non-zero (usage error).
- `init` creates minimal TOML configuration if absent; never overwrites without `--force`; idempotent.
- `doctor` reports toolchain/project prerequisites (Cargo present, nightly-with-rustdoc-JSON availability, workspace detected, write access to the output directory) and distinguishes warnings from fatal errors; it never modifies the machine.
- `generate|serve|open|build` print a structured "not implemented yet" diagnostic and exit with the reserved "unimplemented" code.
- Outside a Cargo project, commands needing a workspace fail with an actionable diagnostic; `--help`/`doctor` still work.

## Functional requirements

1. Binary `cargo-cratevista` supports Cargo external-subcommand dispatch: when invoked as `cargo cratevista X`, argv is `["cargo-cratevista", "cratevista", "X", ...]`; the leading `cratevista` token must be tolerated/stripped. Also works when invoked directly as `cargo-cratevista X`.
2. Global options (before or after the subcommand where clap allows): `--manifest-path <PATH>`, `-v/--verbose` (repeatable), `-q/--quiet`, `--color <auto|always|never>`, `--format <human|json>`.
3. Exit-code policy (single source of truth):
   - `0` success
   - `1` runtime/generation error
   - `2` usage/argument error (clap default)
   - `3` prerequisite/environment error (e.g. no Cargo, no nightly)
   - `4` not-implemented-yet (bootstrap stubs)
4. Logging via `tracing` + `tracing-subscriber`, level from verbosity, respecting `--color`; logs to stderr, machine output (`--format json`) to stdout.
5. Process/OS path resolution centralized in one module (`cratevista-core::paths`): resolve `--manifest-path`, resolve and create the output directory, cross-platform separators, and an explicit non-UTF-8 path policy (reject with diagnostic, documented). NOTE: the **domain** validated repository-relative `SourceLocation` type and its traversal-safety validation are a schema concern (issue 02) and are NOT defined here; server/config reuse the schema type. This split keeps `cratevista-core` an orchestration layer, not a model/utility dumping ground.
6. `init` writes a minimal commented `cratevista.toml` (see "Configuration") without clobbering; `--force` overwrites; it does not create `.cratevista/` subfiles (those are optional, documented in issue 08).
7. `doctor` runs read-only checks and prints a categorized report; exit `0` if only warnings, `3` if any fatal check fails.

## Technical design

### Module boundaries

Proposed workspace (adjusts CLAUDE.md's list; additions justified in ADR-0001):

```
crates/
  cratevista-core       # orchestration/use-case layer + application runtime scaffolding   [01, filled in 03–06/10]
  cratevista-schema     # canonical domain model incl. validated source paths               [02]
  cratevista-metadata   # cargo metadata ingestion                                          [03]
  cratevista-rustdoc    # rustdoc JSON invoke + adapter                                     [04]
  cratevista-graph      # merge + views + document assembly (pipeline)                      [05]
  cratevista-server     # axum server + embedded assets                                     [06]
  cargo-cratevista      # CLI: arg parsing, subcommand dispatch, thin adapter over core     [01]
web/                    # frontend                                                          [07]
```

Created in this issue: `cratevista-core` (scaffolding) and `cargo-cratevista` (CLI). Placeholder lib crates are created for `cratevista-{schema,metadata,rustdoc,graph,server}` to lock names and the workspace graph (each is a compiling `//! TODO(issue NN)` stub). **`cratevista-config` (issue 08) and `cratevista-watch` (issue 09) are deliberately NOT created here** — their boundaries are owned by their issues.

**`cratevista-core` scope guardrail (decision):** `cratevista-core` is the application/use-case layer. It orchestrates operations by composing the analyzer/graph/server crates and hosts application-runtime concerns only (top-level error, terminal diagnostic rendering, logging init, exit-code policy, process/OS path resolution, and the use-case entry-point signatures). It must **not** accumulate shared domain model types — those belong in `cratevista-schema`. In this issue the use-case functions (`run_generate`, `run_serve`, `run_build`) exist as stub signatures returning the "unimplemented" diagnostic; later issues implement them.

`cargo-cratevista` internal modules: `cli` (clap definitions), `commands/{init,doctor,generate,serve,open,build}` (thin adapters that call `cratevista-core` use cases), `dispatch`, `main`.

`cratevista-core` modules (this issue): `error`, `diagnostic` (terminal/JSON rendering), `paths` (process/OS resolution), `logging`, `exit`, `usecase` (stub entry points).

### Data model

- `Diagnostic { severity: Severity(Error|Warning|Info), code: String, message: String, remediation: Option<String>, context: Vec<(String,String)> }` with `Display` (human) and `Serialize` (json). This is the *CLI/runtime* diagnostic; the schema-embedded diagnostic (issue 02) is a separate but field-aligned type (`severity/code/message` shared). Runtime diagnostics are rendered to the terminal; schema diagnostics are written into `diagnostics.json`/the document by later issues.
- `CoreError` (thiserror enum) convertible into a `Diagnostic`; `anyhow` used only at the `main` boundary.
- `ExitCode` newtype mapping the policy above.

### Control flow

`main` → parse args (strip external-subcommand token) → init logging → dispatch to a `commands/*` adapter → adapter calls a `cratevista-core::usecase` function → returns `Result<ExitCode, CoreError>` → render error as diagnostic in the chosen format → `process::exit`.

### Error handling

All user-facing failures become a `Diagnostic` with actionable remediation. Panics are treated as bugs; a panic hook prints a "please report" message. `--format json` emits a single JSON diagnostic object on failure.

### Compatibility

- Cargo external subcommand mechanics (argv token).
- **`edition = "2024"`** across the workspace. Per the latest-stable Rust policy (ADR-0010), `rust-version` tracks the current latest stable release — now **`1.97`** (supersedes the originally-approved `1.85` and the proposed `1.86`). Edition 2024 requires Rust ≥ 1.85, so this stays consistent.
- `rust-toolchain.toml` pins the **stable** toolchain used to build CrateVista, explicitly at **`1.97.0`** (not `stable`) for reproducible local/CI builds. The **nightly** required to emit rustdoc JSON for target projects is a runtime concern of issue 04, documented and probed by `doctor`, never installed here.

### Security and privacy

- No network access. `doctor` and `init` never mutate the machine or arbitrary files. `cratevista-core::paths` rejects non-UTF-8 process paths per policy; domain traversal safety is added with the schema path type in issue 02 and reused by server/config.

## CLI/API/configuration changes

Establishes the full CLI surface and global options above. Introduces the configuration file convention (finalized in issue 08):

```
cratevista.toml                    # tool config + [[override]] entries
.cratevista/
  flows/*.toml                     # manual flows       (issue 08)
  overrides/*.toml                 # presentation overrides (issue 08)
```

`init` writes only a minimal, commented `cratevista.toml` stub; the `.cratevista/` tree is optional and documented by issue 08.

Establishes the **generated output layout** (default root `target/cratevista/`, git-ignored):

```
target/cratevista/
  document.json                    # explorer document (deterministic)      [05]
  generation.json                  # timestamps + runtime generation metadata [05]
  diagnostics.json                 # generation diagnostics                  [05]
  cache/                           # cached intermediate artifacts           [09]
  site/                            # static build output                     [10]
```

The `build` command must support an explicit `--output <DIR>` to override the default site directory (defined in issue 10; the flag name `--output` is reserved here for consistency).

## Files and modules to create or modify

- `Cargo.toml` (workspace, `edition="2024"`, `rust-version="1.97"`), `rust-toolchain.toml` (pinned `1.97.0`), `.gitignore` (already present: ignores `target/`, `target/cratevista/`, `web/dist/`, and `web/node_modules/`).
- `crates/cratevista-core/{Cargo.toml,src/lib.rs,src/error.rs,src/diagnostic.rs,src/paths.rs,src/logging.rs,src/exit.rs,src/usecase.rs}`.
- `crates/cargo-cratevista/{Cargo.toml,src/main.rs,src/cli.rs,src/dispatch.rs,src/commands/*.rs}`.
- Placeholder crates: `crates/cratevista-{schema,metadata,rustdoc,graph,server}/{Cargo.toml,src/lib.rs}` (no config/watch).
- `README.md`, `LICENSE-MIT`, `LICENSE-APACHE`, `CONTRIBUTING.md`, `SECURITY.md`, `CHANGELOG.md`.
- `docs/adr/0001-crate-boundaries.md`, `docs/adr/0002-cli-and-exit-code-policy.md`.
- `.github/workflows/ci.yml`.

## Testing strategy

### Unit tests

- `cratevista-core`: process/OS path resolution, non-UTF-8 rejection, diagnostic human/json rendering, exit-code mapping.
- CLI arg parsing per subcommand incl. external-subcommand token stripping and global option placement.

### Integration tests

- `assert_cmd` + `predicates` in `crates/cargo-cratevista/tests/`: `--help` lists all seven commands; `init` idempotency and non-overwrite; `init --force` overwrites; `doctor` warning vs fatal exit codes; stub commands return exit `4` with a diagnostic; running outside a Cargo project yields exit `3` with remediation.

### End-to-end tests

- Installability: documented `cargo install --path crates/cargo-cratevista` then `cargo cratevista --help` (verified in CI on all three OSes).

### Fixtures

- **Deviation (as implemented):** instead of committed `tests/fixtures/empty_dir/` and `tests/fixtures/single_package/` directories, the `doctor` and `init` tests construct isolated `tempfile::tempdir()` directories at runtime. This is more robust because a committed fixture directory would sit *inside* the CrateVista workspace, so `doctor`'s walk-up manifest detection would find the workspace `Cargo.toml` and never exercise the "outside a Cargo project" path. A system-temp directory is genuinely outside any Cargo workspace, so the exit-3 case is tested correctly. No committed test fixtures are needed.

## Performance considerations

Negligible; startup must remain fast (no heavy work in bootstrap). No blocking network calls.

## Observability and diagnostics

`tracing` spans around dispatch; `-vv` shows debug. `doctor` is the primary diagnostic surface for environment problems.

## Documentation changes

README (installation, first run, command list), CONTRIBUTING (build/test/quality gates), SECURITY (local-first, no upload), ADR-0001 (crate boundaries incl. justification for the `core` orchestration crate and for deferring `config`/`watch`), ADR-0002 (CLI + exit codes).

## Rollout and migration

Greenfield; no migration. Establishes conventions later PRDs must follow.

## Risks and mitigations

- **External-subcommand argv quirks** → explicit token-stripping + tests invoking both `cargo-cratevista X` and simulated `cargo cratevista X`.
- **Windows path/CI differences** → path module + CI matrix from day one.
- **`cratevista-core` becoming a dumping ground** → explicit scope guardrail in ADR-0001 + code review; domain types live in schema.
- **Latest-stable MSRV moving forward** → intentional pre-1.0 (ADR-0010); on each stable adoption, update `rust-version` / `rust-toolchain.toml` / CI / dependencies / `Cargo.lock` together and record the bump in `CHANGELOG.md`.

## Alternatives considered

- Single binary crate (no workspace): rejected — CLAUDE.md mandates deep separable modules.
- `cratevista-core` as a low-level "shared utilities" crate: rejected per decision — core is the orchestration/use-case layer; domain model + validated paths live in schema.
- Scaffolding all future crates (incl. config/watch) now: rejected per decision — config/watch are created by issues 08/09.

## Implementation sequence

1. Workspace `Cargo.toml` (edition 2024, rust-version 1.97), `rust-toolchain.toml` (pinned 1.97.0), confirm `.gitignore`.
2. `cratevista-core` (error, diagnostic, paths, logging, exit, usecase stubs) + tests.
3. Placeholder crates (schema, metadata, rustdoc, graph, server).
4. `cargo-cratevista` CLI, dispatch, `init`, `doctor`, stubs + tests.
5. Project docs + ADRs.
6. CI workflow.

## Acceptance criteria

- [x] `cargo install --path crates/cargo-cratevista` installs the external subcommand. *(verified locally on Windows: installed `cargo-cratevista.exe`, then `cargo cratevista --help` runs. CI install job on 3 OSes authored in `.github/workflows/ci.yml` but not yet executed on GitHub Actions.)*
- [x] `cargo cratevista --help` shows all MVP commands. *(verified: help lists init/doctor/generate/serve/open/build; integration test `help_lists_all_mvp_commands`)*
- [x] `cargo cratevista init` is idempotent. *(verified: unit + integration `init_creates_config_and_is_idempotent`)*
- [x] Existing configuration never overwritten without a flag. *(verified: `init_does_not_overwrite_without_force`; `--force` overwrites)*
- [x] `doctor` distinguishes warnings from fatal errors. *(verified: exit 0 warnings-only in-project, exit 3 fatal outside a project; tests `doctor_succeeds_inside_a_cargo_project` / `doctor_is_fatal_outside_a_cargo_project`)*
- [x] CLI tests cover argument parsing and representative failure modes. *(verified: 13 core unit tests + 10 CLI integration tests pass, incl. token-stripping, usage errors, stub exit 4, JSON format)*
- [x] CI runs formatting, clippy, and workspace tests. *(workflow authored; the three gate commands verified green locally on Windows: `cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets --all-features -- -D warnings`, `cargo test --workspace --all-features`. GitHub Actions execution pending first push.)*
- [x] No dependency on globally installed Node.js for end-user commands. *(verified: no Node in the dependency tree; pure-Rust runtime)*
- [x] Architectural crate boundaries documented in an ADR. *(verified: `docs/adr/0001-crate-boundaries.md` present)*

Verification commands:

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-features
cargo run -p cargo-cratevista -- cratevista --help
```

## Open questions

None blocking. All prior issue-01 questions are resolved by the approved decisions above:

- License dual `MIT OR Apache-2.0` — **approved**.
- `cratevista-core` = orchestration/use-case layer (not a shared-utilities crate); config/watch deferred to issues 08/09 — **approved**.
- `edition = "2024"`, stable build toolchain — **approved**. `rust-version` originally approved at `1.85`, **superseded by the latest-stable Rust policy (ADR-0010): now `1.97`, pinned `1.97.0`**.
- `target/cratevista/{document.json, generation.json, diagnostics.json, cache/, site/}` output layout + `build --output` — **approved**.

## Traceability

Every issue-01 acceptance checkbox maps to a test/verification above. Downstream: fixes CLI contracts consumed by issues 03, 06, 09, 10; the crate graph consumed by all; the output layout consumed by issues 05, 09, 10; the `cratevista-core` use-case seam filled by issues 03–06 and 10.
