# Announcement drafts (DRAFT — not published)

**Status: draft only.** Nothing here has been posted. These drafts are prepared for a
future launch; publication is a manual maintainer action (see
[`../launch-checklist.md`](../launch-checklist.md)). Every claim below maps to an
implemented and tested feature. When editing, keep to that rule — do **not** claim
publication has happened, that source deep links always exist, that snippets exist,
that binaries are byte-reproducible, that `file://` works, or that any remote/hosted
analysis exists.

---

## crates.io short description

> Turn any Rust workspace into an interactive architecture map — an offline,
> source-private explorer of crates, modules, types, traits and their relationships,
> built from cargo metadata and rustdoc JSON.

## GitHub *About* text

> Interactive Rust architecture & documentation explorer. `cargo cratevista open`
> turns a workspace into a browser-based map; `cargo cratevista build` produces a
> self-contained static site. Local-first, no Node.js for end users.

Suggested topics: `rust`, `cargo`, `cargo-subcommand`, `rustdoc`, `documentation`,
`architecture`, `visualization`, `developer-tools`.

## Demo GIF placeholder

<!-- DEMO GIF PLACEHOLDER — record `cargo cratevista open` on a real workspace using
     CrateVista's OWN UI (graph canvas, search, inspector, view tabs). No GIF exists
     yet; this is a placeholder, not a claim. -->

_A demo GIF of the CrateVista explorer will go here._

## Rust users forum (users.rust-lang.org) draft

**Title:** CrateVista — an interactive architecture & documentation explorer for Rust
workspaces

> I've been building **CrateVista**, a Cargo subcommand that turns a Rust workspace
> into an interactive, browser-based architecture map — crates, modules, types,
> traits and the typed relationships between them, built from your real
> `cargo metadata` and rustdoc JSON.
>
> It's local-first and source-private: nothing is uploaded. `cargo cratevista open`
> generates the document and opens a graph explorer with search, kind filters, focus
> modes and an inspector across eight generated views. `cargo cratevista build`
> produces a self-contained static site you can host anywhere (URL root or subpath) —
> no running server, and no Node.js for end users (the UI ships prebuilt in the
> binary).
>
> CrateVista itself builds and installs on stable Rust; a pinned nightly is used only
> at runtime to produce rustdoc JSON for the workspace you're exploring. Static sites
> contain repository links but no copied source snippets and no absolute paths.
>
> Feedback welcome — especially on the generated views and the static-hosting flow.

## r/rust draft

**Title:** CrateVista: turn any Rust workspace into an interactive architecture map

> CrateVista is a `cargo cratevista` subcommand that builds an interactive,
> browser-based map of a Rust workspace (crates → modules → types/traits →
> relationships) from `cargo metadata` + rustdoc JSON. `open` runs a local explorer;
> `build` emits a self-contained static site for any host. Local-first, no data
> uploaded, no Node.js for end users, dual MIT/Apache-2.0. Details and screenshots in
> the README.

## This Week in Rust submission draft

> **CrateVista** — an interactive Rust architecture & documentation explorer.
> `cargo cratevista open` turns a workspace into a browser-based map of crates,
> modules, types, traits and their relationships; `cargo cratevista build` produces a
> self-contained static site. Local-first and source-private.

## awesome-rust submission draft

Category: *Development tools → Build system / Cargo plugins* (or *Visualization*).

> - [CrateVista](https://github.com/cratevista/cratevista) — Interactive architecture
>   & documentation explorer for Rust workspaces; `cargo cratevista open` (live) and
>   `cargo cratevista build` (static site) from cargo metadata + rustdoc JSON.
