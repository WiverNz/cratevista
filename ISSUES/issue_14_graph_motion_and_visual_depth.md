# Issue 14 — Graph motion, routed relations, and visual depth

**Type:** enhancement (frontend visual-polish phase)
**Raised:** 2026-07-19
**Status:** implemented and verified (2026-07-20)

**Delivered (four capabilities):** (1) **routed relation paths** — edges follow the
ELK orthogonal routes (bend points consumed) with a typed smooth-step fallback,
deterministic geometry, non-degenerate self-loops and collision-safe parallel-edge
separation; (2) **semantic active-flow animation** — only manual relations carrying
`attributes.flow = "active"` animate (width-scaled dashes travelling source→target),
with `prefers-reduced-motion` static fallback and a view-wide `EDGE_FLOW_MAX_ANIMATED
= 60` fail-safe; (3) **node depth & elevation** — a restrained, token-derived
gradient + bounded shadow that strengthens hierarchy with no card-dimension change,
correct in dark/light and safe under forced-colors; (4) **context-preserving dim
focus** — `focus=<id>&focusmode=dim` keeps the full projection and dims unrelated
content (no relayout on anchor moves), alongside the unchanged legacy hide focus,
with a shareable, Back/Forward-safe URL contract. Verified against the freshly-built
embedded bundle: 529 component tests, 92 real-browser E2E tests, `check:dist` green,
Rust gates green, and live FlightTrace showing the routed/depth/dim behaviour with
zero falsely-animated edges.
**PRD:** [`../PRD/issue_14_graph_motion_and_visual_depth.md`](../PRD/issue_14_graph_motion_and_visual_depth.md)
**Primary area:** `web/` (explorer SPA); one deliberately-scoped, additive
manual-flow presentation contract may touch `cratevista-config` / the schema
**docs**, but no core generation semantics.

## Summary

The explorer renders structurally-correct graphs, but four comprehension gaps
remain in dense and manual-flow views. This issue asks for a bounded
visual-polish phase that improves **navigation, hierarchy and readability**
without weakening relation semantics, accessibility, performance or deterministic
rendering.

The work extends existing systems — it must not replace or duplicate them:

- the centralized, typed relation-style registry
  (`web/src/adapter/relationStyle.ts`): colour token + line pattern + width +
  directional marker + per-state visuals;
- the relation legend and the selected / related / faded edge states;
- the progressive node-card density model (`web/src/model/nodeCards.ts`,
  `web/src/components/NodeCard.tsx`) and the `--node-*` / `--rel-*` design
  tokens (dark + light) in `web/src/styles.css`;
- React Flow rendering and the ELK layout worker
  (`web/src/layout/*`, `edgeRouting: "ORTHOGONAL"`, which already emits
  `RoutedEdge.points`);
- the diagnostics UI and the reduced-motion behaviour
  (`@media (prefers-reduced-motion: reduce)` global kill-switch).

## Problem statement

1. **Straight diagonal edges are hard to follow in dense views.** Edges are
   rendered with `getStraightPath`, so the workspace/public-API/type views become
   a starburst of crossing diagonals even though ELK already computes orthogonal
   routes that are then discarded at render time.
2. **Structurally correct graphs can still read as flat.** Node cards are single
   flat surfaces with a left accent bar; the visual hierarchy between an
   overview-level package card and a leaf code card is weaker than the
   information hierarchy it represents.
3. **Manual-flow edges cannot signal an active or ordered process.** Relation
   kinds are already visually differentiated (the registry gives each kind a
   distinct colour token, line pattern, width and directional marker). What is
   missing is *within* the manual-flow family: manual-flow edges are drawn
   statically, and there is no explicit visual distinction between an ordinary
   manual relation and a semantically eligible active/ordered flow relation, so a
   reader cannot see direction of travel or which relations are part of the
   walked flow.
4. **"Related only" removes context.** The existing focus mode reduces the graph
   to a neighbourhood, which is the right default for some users but discards
   spatial context others rely on; there is no "keep everything, dim the rest"
   option.

These are comprehension and navigation improvements. **Motion and depth do not
make an architecture more correct** — they must never imply runtime activity,
infer business roles, or become the sole carrier of any meaning.

## Goals

- Render dense graphs with deterministic, readable **routed** edge paths.
- Add a restrained, token-derived **visual-depth** treatment to node cards that
  strengthens hierarchy without changing card dimensions.
- Allow **eligible** relations (manual-flow / selected flow steps) to animate
  direction of travel, always backed by a complete static representation.
- Add an **additive** context-preserving focus mode (dim unrelated) alongside the
  existing hide behaviour, with a shareable, Back/Forward-safe URL contract.

## Non-goals

No new graph engine; no 3D, physics, particles or WebGL; no animation of every
dependency edge; no inferred runtime traffic; no automatic business-domain
categorization; no redesign of diagnostics or node-card content/metrics; no
launcher changes; no schema changes beyond a single, explicitly-justified
additive presentation contract; no server/API dependency; no change to the static
site privacy contract.

## Acceptance (high level; the PRD owns the concrete checklist)

- Dense views are routed and readable; geometry is deterministic across reloads.
- Structural dependencies never appear "active".
- Eligible flow relations show direction through animation **plus** static cues.
- `prefers-reduced-motion` users get a complete static representation.
- Node depth improves hierarchy with no card-dimension change on selection.
- Dim focus preserves layout/context; hide focus stays available and compatible.
- URL state is shareable and Back/Forward-safe with safe fallbacks.
- Accessibility and performance budgets pass; static/live parity holds; the
  committed embedded bundle matches frontend source.

## Deliverable

`PRD/issue_14_graph_motion_and_visual_depth.md` — an implementation-ready PRD,
**Approved — safe to implement**, with all six load-bearing decisions locked.
Implementation proceeds through the PRD's five phased steps; this issue does not
itself perform implementation.
