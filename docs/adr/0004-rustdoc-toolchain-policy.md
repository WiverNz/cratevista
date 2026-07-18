# ADR 0004 — rustdoc JSON compatibility tuple and toolchain policy

- Status: Accepted
- Date: 2026-07-13
- Related: PRD `PRD/issue_04_rustdoc_json.md`; ADR-0010 (Rust version policy).

## Context

`cratevista-rustdoc` consumes rustdoc JSON, which is **nightly-only**,
**unstable**, and **version-gated** by a `format_version` integer. CrateVista
must consume it without:

- leaking `rustdoc-types` into its public API,
- silently changing or installing the user's toolchain,
- making the *workspace* require nightly for build/fmt/clippy/unit tests, or
- producing non-deterministic or absolute-path-bearing output.

The workspace builds, tests, formats, and lints on the pinned **stable** Rust
1.97.1 (ADR-0010). `rustdoc-types` is an ordinary (de)serialization crate that
builds on stable; a **separately pinned nightly** is invoked only at runtime to
generate rustdoc JSON for a *target project*, and only gated tests use it.

## Decision — the compatibility tuple

CrateVista pins one **compatibility tuple**
`(pinned nightly, rustdoc JSON format version, rustdoc-types release, adapter version)`:

| component | value | source of truth |
|---|---|---|
| pinned nightly | **`nightly-2026-07-01`** (`rustc 1.98.0-nightly (f46ec5218 2026-06-30)`) | this ADR + `compat.rs` |
| rustdoc JSON `format_version` | **`60`** | `compat::EXPECTED_FORMAT_VERSION`, kept `== rustdoc_types::FORMAT_VERSION` |
| `rustdoc-types` release line | **`0.60`** (resolved `0.60.0`; `Cargo.lock` is authoritative for the exact patch) | `[workspace.dependencies]` + `Cargo.lock` |
| CrateVista adapter version | **`1`** | `compat::ADAPTER_VERSION` |

### Verification performed (2026-07-13, implementation time)

This tuple was **empirically verified**, not guessed:

1. Added `rustdoc-types = "0.60"`; it resolved to `0.60.0` and builds on stable
   Rust 1.97.0. Its source defines `pub const FORMAT_VERSION: u32 = 60;`.
2. Installed `nightly-2026-07-01` (`rustc 1.98.0-nightly (f46ec5218 2026-06-30)`).
3. Generated rustdoc JSON for the path-only fixture crate
   (`crates/cratevista-rustdoc/tests/fixtures/sample_lib`) with the verified
   command form (below). The output's `format_version` is **`60`**.
4. Deserialized that JSON with `rustdoc-types` 0.60.0
   (`serde_json::from_str::<rustdoc_types::Crate>`) successfully, and confirmed
   `crate.format_version == rustdoc_types::FORMAT_VERSION == 60`.
5. Smoke-normalized it through the adapter.

`rustdoc-types` 0.60.0 was released 2026-06-30 for format version 60 (format 59
was 2026-06-26); `nightly-2026-07-01` is the first nightly published on/after
that format-60 landing, so it emits exactly format 60.

### Verified command form

The **Cargo-level** form is used in production (`cargo rustdoc` respects
features/cfg/target resolution; direct `rustdoc` does not):

```
cargo +<nightly> rustdoc -Z unstable-options --output-format json \
    --manifest-path <manifest> -p <package> \
    <--lib | --bin <name>> \
    --target-dir <isolated-dir> \
    -- [--document-private-items]
```

- `--document-private-items` stays on the **rustdoc side** of the `--` separator.
- The JSON output flags (`-Z unstable-options --output-format json`) are on the
  **cargo side**, before `--`.
- Normal execution uses **exactly** this form and never silently retries an
  alternate syntax. The fixture crate carries a standalone `[workspace]` table so
  it is documented in isolation from the CrateVista workspace.

### Toolchain selection (never installs)

- `RustdocOptions::toolchain` (or `--toolchain`) overrides.
- Otherwise the pinned nightly (`compat::PINNED_NIGHTLY`) is used.
- CrateVista **never installs** a toolchain. A missing nightly is a fatal
  `nightly_missing` error carrying the exact remediation:
  `rustup toolchain install nightly-2026-07-01`.

### Runtime format gate

After deserializing, `crate.format_version` is compared to
`compat::EXPECTED_FORMAT_VERSION` (`60`). A mismatch is a fatal
`unsupported_format_version` error naming both versions and the nightly
remediation.

## Update process

When the nightly/format changes, bump **together**: `rustdoc-types`, the pinned
nightly, `compat.rs` constants, the checked-in fixtures, and this ADR; re-run the
fixture parse/normalize tests plus the gated live E2E; and record the new tuple.
`Cargo.lock` captures the resolved `rustdoc-types` patch. A tuple upgrade that
changes rustdoc's chosen canonical paths may require an architecture-ID migration
note in the changelog.

## Consequences

- The workspace never requires nightly; only rustdoc JSON generation and one
  gated `#[ignore]` live test do.
- rustdoc JSON instability is contained behind a single pinned format with a hard
  runtime gate and checked-in fixtures.
- `doctor` (issue 01) probes for the pinned nightly and points users at the exact
  `rustup` command; CrateVista never runs it.
