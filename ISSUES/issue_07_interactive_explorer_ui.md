# Issue 07 — Build the interactive architecture explorer UI

## Goal

Create a polished browser UI comparable to the supplied architecture explorer reference: a wide interactive graph, top-level views, flow steps, filtering controls, legend, and a details inspector.

## Core layout

- Header with project title
- Search input
- Entity-kind filter
- Language-ready selector
- Reset action
- View tabs
- Optional stage/flow step navigation
- Main graph canvas
- Canvas controls
- Legend
- Right-side inspector

## Graph behavior

- Pan and zoom
- Fit view
- Reset
- Automatic ELK-based layout
- Focus selected path
- Related-only mode
- Show all edges / related edges / hide edges
- Visible selection state
- Clear directional edge styling
- Grouping or stage lanes where configured
- Graceful rendering of large graphs
- Stable layout during inspector interaction

## Inspector behavior

For a selected entity, show:

- label and qualified name
- kind
- tags
- description/rustdoc
- source location
- parent/container
- related entities grouped by relation kind
- relevant attributes
- documentation status
- link or action to open source where supported

For a selected relation, show:

- relation kind
- source and target
- label
- provenance
- attributes

## Data boundary

The frontend consumes only the CrateVista explorer schema.

A dedicated adapter converts schema entities and relations to React Flow nodes and edges.

Do not expose rustdoc JSON directly to React components.

## Accessibility

- Keyboard navigation for primary actions
- Visible focus states
- Accessible labels
- Sufficient contrast
- Reduced-motion consideration
- Inspector usable without precise pointer interaction

## Acceptance criteria

- [ ] The UI loads a fixture document and renders every MVP generated view.
- [ ] Search can locate entities by label and qualified name.
- [ ] Entity-kind filters update the visible graph predictably.
- [ ] Selecting a node populates the inspector.
- [ ] Fit/reset/focus/related controls work.
- [ ] The legend reflects only categories present in the active view.
- [ ] Layout is deterministic enough for repeatable tests.
- [ ] Large graph handling has an explicit threshold and fallback behavior.
- [ ] TypeScript strict mode passes.
- [ ] Unit/component tests cover the schema adapter and major interactions.
- [ ] End-to-end smoke tests cover opening a view, selecting a node, searching, and changing filters.

## PRD requirement

Do not implement this issue directly.

First create:

```text
PRD/issue_07_interactive_explorer_ui.md
```

The PRD must map every acceptance criterion to concrete modules, tests, and verification commands.
