# ADR 0010 — Rust version policy (pre-1.0: track latest stable)

- Status: Accepted
- Date: 2026-07-12
- Supersedes: the earlier "MSRV = Rust 1.85 (bump only if a dependency requires
  it)" decision recorded in PRD 01 / ADR-0001, and the proposed Rust 1.86 bump.

> **Amendment (2026-07-17): pinned stable Rust 1.97.0 → 1.97.1.** Rust **1.97.1**
> supersedes the 1.97.0 pin because it fixes an **LLVM miscompilation**; under this
> policy CrateVista tracks the latest stable patch. `rust-toolchain.toml` now pins
> `channel = "1.97.1"` and `Cargo.toml` sets `rust-version = "1.97.1"` (full patch,
> so CrateVista does not claim 1.97.0 support). Toolchain-only maintenance — no
> dependency was upgraded and the rustdoc compatibility tuple (`nightly-2026-07-01`
> → `format_version 60` → `rustdoc-types 0.60.0`) is unchanged. Statements below
> are updated to 1.97.1; the 2026-07-12 policy itself is otherwise unchanged.

## Context

During pre-1.0 development, CrateVista benefits from using the newest language,
standard-library, Cargo, and dependency features. A conservative MSRV adds
friction with no user-facing benefit before there is a public release.

## Decision

> CrateVista tracks the latest stable Rust release so the project can use the
> latest stable language, standard-library, Cargo, and dependency features.

Concretely:

- The workspace pins the current latest stable release. As of this ADR that is
  **Rust 1.97.1**: `Cargo.toml` sets `rust-version = "1.97.1"`, and
  `rust-toolchain.toml` pins `channel = "1.97.1"` (explicit, not `stable`) with
  `components = ["rustfmt", "clippy"]` and `profile = "minimal"` for
  reproducible local/CI builds.
- When a new stable Rust release is adopted, update **together**:
  `rust-version`, `rust-toolchain.toml`, CI, dependencies, and `Cargo.lock`.
- MSRV changes are allowed during pre-1.0 development and **must be recorded in
  `CHANGELOG.md`**.
- Before the first stable public release, reconsider adopting a wider
  compatibility window (e.g. the latest two or three stable releases).

### Stable vs nightly

- The CrateVista application, CLI, libraries, formatting, Clippy, and ordinary
  tests use the **pinned stable** toolchain (1.97.1). Normal CLI help, schema
  work, metadata ingestion, unit tests, and linting must work **without**
  nightly.
- rustdoc JSON generation uses a **separately pinned nightly** toolchain defined
  by PRD 04. The workspace as a whole must never require nightly.

## Consequences

- The MSRV moves forward with stable releases; this is intentional pre-1.0.
- CI runs the gates on the pinned stable and additionally runs
  `cargo +1.97.1 check --workspace --all-features`. No older-MSRV CI job.
- Historical approval records (PRD 01, ADR-0001, INDEX) remain but are labelled
  as superseded by this policy rather than deleted.
