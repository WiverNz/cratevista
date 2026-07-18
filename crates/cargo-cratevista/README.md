# cargo-cratevista

**Turn any Rust workspace into an interactive architecture map.**

CrateVista is a standalone Cargo subcommand that turns a Rust workspace into an
interactive, browser-based map of its crates, modules, types, traits and the typed
relationships between them — built from your real `cargo metadata` and rustdoc JSON,
locally and source-private. It can run as a live loopback server or produce a
self-contained static site.

## Install

```bash
cargo install cargo-cratevista
```

This installs the `cargo cratevista` subcommand. The installed CLI serves a
**prebuilt, embedded** web application — end users need **no Node.js**.

## Use

```bash
# Serve the interactive explorer for the current workspace and open a browser.
cargo cratevista open

# Generate the explorer document artifacts only.
cargo cratevista generate

# Serve an already-generated document.
cargo cratevista serve

# Build a self-contained static site you can host anywhere (URL root or a subpath).
cargo cratevista build --output dist
cargo cratevista build --output dist --base-path /cratevista/
```

`cargo cratevista build` writes a portable directory (`index.html`, the frontend
assets, and the three JSON artifacts) that renders from any static HTTP host, from a
URL root or a subpath, with no running server. It opens no `/api` connection and
copies no source snippets. `file://` is not supported — serve over HTTP. When the
analyzed workspace declares a `repository`, the explorer links back to it.

## Toolchain: stable vs nightly

The CrateVista CLI itself builds, installs and runs on **stable** Rust (1.97.1) with
no nightly. Generating **rustdoc JSON** for your workspace requires a **nightly**
toolchain (rustdoc JSON is a nightly-only format): CrateVista pins
`nightly-2026-07-01` and reports the exact `rustup` command when it is missing. Only
the rustdoc-JSON step needs nightly; `--help`, `doctor`, `serve`, and a metadata-only
`build` do not. CrateVista never installs a toolchain automatically.

## Documentation

Full documentation, hosting guide and configuration reference live in the repository:
<https://github.com/cratevista/cratevista>.

## Licence

Dual-licensed under either of

- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE))
- MIT license ([LICENSE-MIT](LICENSE-MIT))

at your option.

Authored by Aleksandr Skibin.
