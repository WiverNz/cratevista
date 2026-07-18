# Contributing to CrateVista

Thanks for your interest in CrateVista!

## Workflow

CrateVista is developed issue-by-issue through PRDs. Never implement an issue
directly from `ISSUES/`:

1. Read `ISSUES/CONTEXT.md` and the relevant issue.
2. Create or refine the matching PRD under `PRD/`.
3. Implement only after the PRD has been approved.
4. Keep the PRD acceptance checklist synchronized with the code.

See `CLAUDE.md` for the full workflow and engineering principles.

## Prerequisites

- **Stable** Rust, edition 2024. During pre-1.0 development CrateVista tracks the
  latest stable release — currently **Rust 1.97.1**, pinned in
  `rust-toolchain.toml`. CrateVista itself builds, tests, and lints on stable —
  no nightly required.
- A **nightly** toolchain is only needed at runtime to generate rustdoc JSON for
  a target project (a later feature). It is never installed automatically.

## Rust version policy

CrateVista tracks the latest stable Rust release during pre-1.0 development (see
`docs/adr/0010-rust-version-policy.md`). When a new stable release is adopted,
update `rust-version`, `rust-toolchain.toml`, CI, dependencies, and `Cargo.lock`
together, and record the MSRV change in `CHANGELOG.md`. MSRV may move forward;
this is intentional before the first stable public release.

Check your environment:

```bash
cargo run -p cargo-cratevista -- cratevista doctor
```

## Quality gates

Before opening a pull request, all of these must pass:

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-features
```

Every new public Rust item should have useful rustdoc. Avoid `unsafe` unless a
PRD explicitly justifies it (crates use `#![forbid(unsafe_code)]`).

## Crate layout

See `docs/adr/0001-crate-boundaries.md`. In short: `cratevista-core` is the
orchestration/use-case layer, domain models live in `cratevista-schema`, and the
`cargo-cratevista` binary is a thin CLI adapter.
