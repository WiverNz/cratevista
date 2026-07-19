# CrateVista

**Interactive Rust Architecture & Documentation Explorer.**

> Turn any Rust workspace into an interactive architecture map.

CrateVista is a standalone Cargo subcommand that turns a Rust workspace into an
interactive, browser-based map of its crates, modules, types, traits and the typed
relationships between them — built from your real `cargo metadata` and rustdoc JSON,
**locally** and **source-private**. Run it as a live loopback server, or `build` a
self-contained static site you can host anywhere.

## What makes it different

CrateVista is not another view of the same data:

- **Not rustdoc HTML.** rustdoc renders per-item API pages. CrateVista renders the
  **architecture**: an interactive graph of crates, modules, types, traits and their
  relationships, with search, filters, focus modes and an inspector — a map you
  navigate, not a page you scroll.
- **Not a static dependency graph.** Crate-dependency images (e.g. `cargo depgraph`)
  stop at the crate edge. CrateVista goes **inside** — modules, types, trait impls,
  public API surface, documentation coverage — across eight generated views, and
  lets you enrich it with manual flows.
- **Not a terminal inspector.** Terminal tools answer one query at a time.
  CrateVista is a real browser UI: layout runs off-thread, state lives in the URL so
  any view is shareable, and it works fully offline.

## Screenshots

<!-- SCREENSHOT/GIF PLACEHOLDER — add a capture of CrateVista's OWN UI here
     (`cargo cratevista open` on a real workspace). No screenshot ships yet; this is
     a placeholder, not a claim that one exists. -->

_A screenshot/GIF of the CrateVista explorer will go here._

## The explorer

`cargo cratevista open` builds the document and opens a real interactive map of your
workspace — a graph canvas with an inspector, driven entirely from the generated
document.

**Eight generated views**, each a different question about your code:

| View                     | Shows                                              |
| ------------------------ | -------------------------------------------------- |
| Workspace overview       | the workspace, its packages and their targets       |
| Crate dependencies       | how your crates depend on each other                |
| Module hierarchy         | packages, targets and their module trees            |
| Types                    | structs, enums, unions, aliases, constants, statics |
| Traits and impls         | traits and what implements them                     |
| Type relationships       | types, traits and the functions/methods relating them |
| Public API               | the public surface of every crate                   |
| Documentation coverage   | which modules are documented, and which are not     |

What you can do:

- **Search** by label or fully-qualified name (`cvcore::model::Widget`).
- **Filter** by entity kind; the legend reflects only what is on screen.
- **Select** an entity or relation to open the inspector (docs, attributes,
  relations, diagnostics).
- **Focus** on a node's neighbourhood ("related only"), or switch edges between
  all / related / hidden.
- **Fit, zoom and reset** the canvas; layout runs off the main thread in a
  same-origin Web Worker (ELK).
- **Share the view**: `view`, `entity`/`relation`, `q`, `kinds`, `focus`, `edges`
  and `stage` live in the URL, so any state is a link, and Back/Forward work. Stale
  ids degrade gracefully rather than erroring.

**No Node.js required.** The UI ships prebuilt inside the binary. Node is needed only
to rebuild the UI itself — see [`web/README.md`](web/README.md).

## Installation

### From source (available today)

```bash
cargo install --path crates/cargo-cratevista
```

This installs the `cargo cratevista` subcommand. It builds on **stable** Rust (edition
2024, currently **1.97.1**) and needs **no Node.js**.

### From crates.io (after publication)

> **Not published yet.** Once CrateVista is released to crates.io, this will work:
>
> ```bash
> cargo install cargo-cratevista
> ```
>
> Until then, use the from-source command above. This section is documentation for
> the future release, not a claim that the crate is already on crates.io.

### From a binary release (after publication)

Tagged releases will attach checksummed archives — `.tar.gz` (Linux/macOS) and
`.zip` (Windows) — each containing the `cargo-cratevista` binary plus the licences,
README and changelog, with a `.sha256` beside it. Download the archive for your
target, verify the checksum, extract it, and put `cargo-cratevista` on your `PATH`;
then `cargo cratevista …` works. Binaries are **not** claimed byte-reproducible; the
checksums verify archive integrity.

## First run

```bash
# In your Rust workspace:
cargo cratevista doctor     # check toolchain + prerequisites (read-only)
cargo cratevista open       # generate the document, serve it, open your browser
```

`open` generates `target/cratevista/{document,generation,diagnostics}.json`, serves
them on a loopback port, and opens the explorer. If your workspace has a documentable
library/proc-macro target, generation uses the pinned nightly for rustdoc JSON (see
below); a workspace with no such target still produces a **metadata-only** document
with no nightly needed.

## Quick launcher

A small cross-platform launcher opens the explorer for any local project in one
command:

```powershell
# Windows
./scripts/open-project.ps1 D:\Projects\MyRustProject
```

```bash
# Linux/macOS
./scripts/open-project.sh /path/to/my-rust-project
```

It accepts either a workspace directory or a path to its `Cargo.toml`.

- **Default = live explorer.** It runs `cargo cratevista open` with **local source
  access** and **watch mode**, bound to loopback, letting CrateVista pick a free
  port and open your browser. Live mode keeps the Rust server process running in the
  foreground — leave it running to use the explorer; press **Ctrl+C** to stop it.
- **Static mode** (`-Static` on Windows, `--static` on Linux/macOS) builds a
  self-contained **snapshot** under `<project>/target/cratevista/site` instead. A
  snapshot has **no `/api/**`**, **no source viewer** and **no live reload** —
  rebuild it after changes. Serving a snapshot is not automated by the launcher
  (CrateVista ships no static-file server and the launcher adds no extra
  dependency); serve the built directory with any static HTTP host.
- **Both modes require HTTP; `file://` is unsupported.**

The launcher prefers an installed `cargo cratevista`; run from the CrateVista
repository, it falls back to a local build. It never modifies the target project or
its Git state, and it never installs a toolchain — if the pinned nightly is missing
it prints the exact `rustup toolchain install` command for you to run.

## Commands

```bash
cargo cratevista --help       # show all commands
cargo cratevista init         # create a minimal cratevista.toml (never overwrites without --force)
cargo cratevista doctor       # report toolchain and project prerequisites (read-only)
cargo cratevista generate     # build the explorer document → target/cratevista/*.json
cargo cratevista serve        # serve the existing document and embedded UI (loopback)
cargo cratevista open         # generate, serve, and open the explorer in a browser
cargo cratevista open --watch # ...and regenerate whenever the workspace changes
cargo cratevista build        # produce a self-contained static site (no server needed)
```

Global options: `--manifest-path <PATH>`, `-v/--verbose` (repeatable), `-q/--quiet`,
`--color <auto|always|never>`, `--format <human|json>`.

`generate` options: `--keep-going`, `--features <a,b>`, `--all-features`,
`--no-default-features`, `--document-private-items`, `--toolchain <name>`,
`--external-deps <exclude|direct|full>`, `--document-bins`.

`serve` / `open` options: `--host <IP>` (defaults to `127.0.0.1`; a non-loopback host
prints a warning), `--port <PORT>` (defaults to `7420`, increment-on-conflict through
`7440`), `--source` (enable the guarded, off-by-default `/api/source` endpoint).
`open` also accepts all `generate` options and `--watch`.

`build` options: `--output <DIR>` (default `target/cratevista/site`), `--base-path
<PATH>` (optional absolute `<base href>`), plus all `generate` options. See the
[static build](#static-build) section.

The HTTP API and security model are documented in [`docs/server.md`](docs/server.md).

## Supported inputs

CrateVista analyzes:

- **`cargo metadata`** — the workspace, its packages, targets and dependency edges
  (always available on stable Rust).
- **rustdoc JSON** — types, traits, impls, function signatures and documentation,
  produced through the **pinned nightly** (see below). Optional: a workspace with no
  documentable target still produces a metadata-only document.
- **Optional CrateVista TOML** — manual flows, entities and overrides in
  `cratevista.toml` and `.cratevista/` that enrich the generated document. See
  [`docs/configuration.md`](docs/configuration.md).

## Stable vs nightly

This distinction is deliberate and load-bearing:

- **CrateVista itself builds, tests, lints and installs on stable Rust 1.97.1.** No
  nightly is required to compile or install the CLI, run `--help`, `doctor`, `serve`,
  `build` on a metadata-only workspace, or host a produced static site.
- **Nightly is required only at *runtime*, and only to generate rustdoc JSON** for a
  workspace with a documentable target — rustdoc JSON is a nightly-only format.
  CrateVista pins one supported nightly and verifies it against an exact
  compatibility tuple:

  | | |
  | --- | --- |
  | toolchain | `nightly-2026-07-01` |
  | rustdoc JSON `format_version` | `60` |
  | `rustdoc-types` | `0.60.0` |
  | adapter version | `1` |

  CrateVista **never installs a toolchain automatically**. If the pinned nightly is
  missing it reports the exact command:

  ```bash
  rustup toolchain install nightly-2026-07-01
  ```

Ordinary rustdoc-JSON generation does **not** work on stable — CrateVista never
pretends otherwise. See [`docs/adr/0004-rustdoc-toolchain-policy.md`](docs/adr/0004-rustdoc-toolchain-policy.md).

## Manual flows

You can enrich the generated document with architecture that rustdoc cannot infer —
external systems, request flows and typed edges — in project-local TOML. A minimal
`.cratevista/flows/system.toml`:

```toml
[[entity]]
id = "gateway"
label = "API Gateway"
kind = "service"

[[flow]]
id = "request"
title = "Incoming request"
entity_ids = ["gateway"]

  [[flow.relation]]
  from = "gateway"
  to = "gateway"
  kind = "manual"
  label = "HTTP"
```

Manual entities and flows appear as first-class views alongside the generated ones.
Overrides adjust presentation (labels, categories, tags) of **discovered** entities
without changing their identity. Full reference:
[`docs/configuration.md`](docs/configuration.md).

## Static build

`cargo cratevista build` produces a self-contained static site — `index.html`, the
frontend assets, and the three JSON artifacts — that any static HTTP host serves with
**no running Rust server and no Node.js**:

```bash
# Default output: target/cratevista/site
cargo cratevista build

# A custom directory:
cargo cratevista build --output dist
```

The site uses **relative URLs** and query-string routing, so it works unchanged from
a URL root or an arbitrary subpath (GitHub/GitLab Pages, CI artifact directories).
`--base-path` is optional — only for a host that needs an absolute `<base href>`:

```bash
# Hosting under https://user.github.io/cratevista/
cargo cratevista build --output dist --base-path /cratevista/
```

A produced site opens **no** `EventSource` and makes **zero** requests to any
`/api/**` route. `file://` is **not** supported (browsers block `fetch` of the
sibling JSON over `file://`) — serve it over HTTP. Full hosting guide:
[`docs/hosting.md`](docs/hosting.md).

## Repository cleanup

Before archiving a checkout, preview the repository-local generated output that can
be removed:

```powershell
# Windows preview
./scripts/clean-project.ps1

# Windows cleanup
./scripts/clean-project.ps1 -Apply
```

```bash
# Linux/macOS preview
./scripts/clean-project.sh

# Linux/macOS cleanup
./scripts/clean-project.sh --apply
```

Both scripts default to dry-run, read the same explicit allowlist from
[`scripts/clean-project-paths.txt`](scripts/clean-project-paths.txt), print every
path they would remove, print the total removable size, and finish with
`git status --short` for manual review. The cleanup is intentionally limited to
reproducible repository-local output; it does not use `git clean` or modify Git
state.

## Privacy

CrateVista is local-first. It does not upload your source code or generated data.

A produced static site contains exactly the three JSON artifacts, the app shell and
its assets. It contains repository **links** when the analyzed workspace declares a
`repository` (and safe metadata is available), **no** copied source snippets, and
**no** absolute paths — every `SourceLocation` is repository-relative by design, and
this is tested. In static mode the site performs no `/api/**` request and opens no
`EventSource`. See [SECURITY.md](SECURITY.md) and [`docs/hosting.md`](docs/hosting.md).

## Known limitations

- **No `file://` support** — a produced site must be served over HTTP.
- **No source snippets** — the explorer shows repo-relative *locations*, never copies
  file contents into a produced site. (A guarded, opt-in `/api/source` endpoint
  exists only in live `serve --source` mode.) Snippets in static sites are a deferred
  follow-up ([`ISSUES/issue_13_static_source_snippets.md`](ISSUES/issue_13_static_source_snippets.md)).
- **Repository *deep* links need a branch.** A repository-root link renders whenever a
  safe `repository_url` is present; per-file deep links require an authoritative
  `default_branch`, and none is rendered while that is absent.
- **The rustdoc nightly is pinned** to `nightly-2026-07-01` and its compatibility
  tuple; other nightlies are not supported.
- **No persistent cache** — deferred to a follow-up
  ([`ISSUES/issue_12_persistent_cache.md`](ISSUES/issue_12_persistent_cache.md)).
- **Not yet published.** At the time this documentation lands, CrateVista has not been
  released to crates.io and no binary release exists yet.

## Contributing, security, licence

- **Contributing:** [CONTRIBUTING.md](CONTRIBUTING.md). Design decisions live under
  [`docs/adr/`](docs/adr); implementation plans under [`PRD/`](PRD).
- **Security & privacy:** [SECURITY.md](SECURITY.md).
- **Dependency licences:** [`docs/licenses/`](docs/licenses).
- **Licence:** dual-licensed under [Apache License, Version 2.0](LICENSE-APACHE) or
  [MIT license](LICENSE-MIT) at your option.

Authored by Aleksandr Skibin.
