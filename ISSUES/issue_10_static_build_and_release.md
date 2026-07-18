# Issue 10 — Static site build, packaging, documentation, and public release

## Goal

Make CrateVista usable as a public open-source tool and easy to discover, install, evaluate, and publish in CI.

## Static build

```bash
cargo cratevista build
```

The output should be a self-contained directory containing:

- application assets
- generated explorer document
- diagnostics or generation summary, according to policy
- no dependency on a running Rust server

Define behavior for:

- base paths
- GitHub Pages
- GitLab Pages
- CI artifacts
- source links
- optional source snippets
- privacy-sensitive paths

## Release packaging

- crates.io-ready metadata
- reproducible frontend embedding
- release workflow
- checksums or provenance where practical
- Linux, macOS, and Windows validation
- minimum supported Rust version policy
- pinned/supported rustdoc toolchain policy
- changelog and semantic versioning

## Public documentation

The root README should include:

- one-sentence value proposition
- screenshot/GIF placeholder
- installation
- first run
- commands
- supported inputs
- rustdoc/nightly requirement
- manual flow example
- static build example
- privacy statement
- known limitations
- contribution links

## Discoverability

Use the full descriptive title consistently:

> CrateVista — Interactive Rust Architecture & Documentation Explorer

Recommended repository topics and keywords:

```text
rust
cargo
rustdoc
architecture
documentation
visualization
code-explorer
dependency-graph
react-flow
developer-tools
```

Prepare launch material for:

- crates.io description
- GitHub About text
- Rust users forum announcement
- Reddit r/rust announcement
- This Week in Rust submission
- awesome-rust submission, once mature
- short demo video/GIF

Do not claim capabilities that are not implemented.

## Acceptance criteria

- [ ] `cargo cratevista build` produces a static site that works from the documented hosting targets.
- [ ] The crates.io package can be installed and exposes `cargo cratevista`.
- [ ] Release CI verifies Rust, frontend, fixtures, and packaged assets.
- [ ] README first-run instructions are tested from a clean environment.
- [ ] The public description clearly distinguishes CrateVista from rustdoc HTML, static dependency graphs, and terminal-only inspectors.
- [ ] Licensing for Rust and frontend dependencies is documented.
- [ ] Security and privacy behavior is documented.
- [ ] A launch checklist exists.
- [ ] Name availability is rechecked immediately before publication.

## PRD requirement

Do not implement this issue directly.

First create:

```text
PRD/issue_10_static_build_and_release.md
```

The PRD must map every acceptance criterion to concrete modules, tests, and verification commands.
