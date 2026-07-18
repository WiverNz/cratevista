# CrateVista product context

## Problem

Rust developers can generate rustdoc HTML and inspect Cargo dependencies, but it is still difficult to understand a non-trivial workspace as a system.

A new contributor often needs answers to questions such as:

- Which workspace crates exist and how do they depend on each other?
- Which modules, types, traits, and implementations define the architecture?
- Where is an item declared?
- Which types contain, accept, return, or implement other types?
- Which items are public, private, documented, or undocumented?
- How do manually documented runtime flows relate to discovered Rust items?
- How can this information be explored without installing a custom IDE extension?

CrateVista should answer those questions in an interactive browser UI similar to a system architecture map: searchable nodes, typed edges, tabs/views, filters, focus mode, step navigation, a legend, and a details inspector.

## Product statement

> CrateVista turns any Rust workspace into an interactive architecture and documentation explorer.

## User experience

Installation:

```bash
cargo install cargo-cratevista
```

Inside a Rust project:

```bash
cargo cratevista generate
cargo cratevista serve
cargo cratevista open
cargo cratevista build
```

Expected behavior:

- `generate` creates a stable CrateVista document from Cargo metadata, rustdoc JSON, and optional manual configuration.
- `serve` serves the generated document and embedded web UI locally.
- `open` generates, serves, and opens the browser.
- `build` creates a self-contained static site suitable for CI artifacts or Pages hosting.
- `serve --watch` regenerates data after relevant project changes and refreshes the UI.

## Primary personas

### New contributor

Needs a high-level map, search, source navigation, and understandable relationships.

### Maintainer

Needs architecture visibility, documentation coverage, stable generated artifacts, and CI publication.

### Reviewer

Needs to inspect the impact of changes and navigate between related modules, types, traits, and source files.

## Sources of truth

### Cargo metadata

Used for:

- workspace root
- packages
- workspace members
- targets
- features
- resolved package dependencies
- manifest and target source paths

### rustdoc JSON

Used for:

- modules
- structs
- enums
- unions
- traits
- implementations
- functions and methods
- fields and variants
- type signatures
- documentation
- visibility
- source spans
- canonical paths and re-exports where available

rustdoc JSON must be isolated behind an adapter because its format and required toolchain may change.

### Manual CrateVista configuration

Used for concepts that static API documentation cannot infer reliably:

- business flows
- runtime stages
- external systems
- infrastructure nodes
- edge labels such as HTTP, WebSocket, SQL, or Redis
- localized descriptions
- view composition
- category overrides
- hidden or promoted nodes

## Core domain vocabulary

### Explorer document

The stable JSON document consumed by the frontend.

### Entity

A node representing a workspace, package, target, module, type, trait, function, method, external system, stage, or manual architecture block.

### Relation

A typed directed connection between two entities.

### View

A named projection of the graph with filters, layout configuration, stages, and presentation metadata.

### Discovered entity

An entity derived automatically from Cargo or rustdoc data.

### Manual entity

An entity declared in CrateVista configuration.

### Override

Configuration that enriches or changes presentation of a discovered entity without replacing its discovered identity.

### Source location

A validated repository-relative file path and optional line/column range.

### Flow

A manually curated architecture or runtime view composed from discovered and manual entities.

## MVP generated views

- Workspace overview
- Crate dependency graph
- Module hierarchy
- Types
- Traits and implementations
- Type relationships
- Public API
- Documentation coverage
- Manual flows, when configured

## MVP UI characteristics

- Dark theme by default, with accessible contrast
- Top navigation for views
- Search
- Entity-kind filtering
- Fit, zoom, reset controls
- Focus mode
- Related-only mode
- Edge visibility controls
- Legend
- Right-side inspector
- Source link and source location
- Responsive behavior for a wide desktop layout
- Clear empty and error states
- English first; localization-ready data model

## Non-goals for MVP

- Precise runtime call graph
- Automatic sequence diagrams from function bodies
- Full type inference
- General-purpose language support
- Source editing
- IDE replacement
- Remote source hosting
- AI summaries
- Production telemetry collection

## Success criteria

A user can install one Cargo subcommand, run it in a representative Rust workspace, and explore useful automatically generated architecture views in a browser without installing Node.js or manually editing generated JSON.
