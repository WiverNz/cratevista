# PRD — Graph motion, routed relations, and visual depth

## Status

**Implemented and verified** (2026-07-20). All four capabilities were delivered
across five phases and verified against the real, freshly-built embedded bundle:
Phase 1 routed ELK geometry (bend-point consumption, smooth-step fallback,
self-loops, collision-safe parallel-edge separation); Phase 2 semantic manual-flow
animation (`attributes.flow = "active"`, width-scaled dashes, `prefers-reduced-motion`,
`EDGE_FLOW_MAX_ANIMATED = 60`); Phase 3 token-derived node depth/elevation; Phase 4
context-preserving dim focus (`focusmode=dim`) with URL/history compatibility. The
six locked decisions were not reopened.

**Verification (2026-07-20):** frontend `check:types`/`typecheck` (TS7 + TS6)/`lint`
(0 errors) green; **529 component/unit tests** pass; the single official embedded
rebuild produced `index.4d30a5fe.js` / `index.2445e87b.css` / `elk.worker.af90117a.js`,
`check:dist` **green** (committed bundle matches a fresh production build) and
`check:embed-rebuild` **pass**; **92 real-browser Playwright E2E tests** pass against
the compiled `cargo cratevista serve` binary and the new embedded frontend
(routed geometry, node depth, dim/hide focus, deterministic layout, static-export
parity, security). Rust gates green (`fmt`/`clippy -D warnings`/`test --workspace`/
`+1.97.1 check`/`docs_integrity`). FlightTrace regenerated twice byte-identically
(2418 entities / 4264 relations / 25 records / 2119 occurrences / 1171 source
locations / 0 animation-eligible relations); live FlightTrace in a real browser
showed title `CV · FlightTrace`, routed edges, node depth, working dim/hide focus,
and **zero** falsely-animated edges. Flow-animation motion mechanics (keyframe +
reduced-motion rule) are present in the shipped stylesheet and exercised by the
component suite rendering the real edge component; a served flow-eligible E2E
snapshot was not added (see Deferred). This PRD performs **no** crates.io
publication, GitHub release, or tag.

## Source issue

`ISSUES/issue_14_graph_motion_and_visual_depth.md`

## Summary

A bounded frontend visual-polish phase that improves how the explorer graph reads
and navigates, delivered as four independently-reviewable capabilities that
**extend** existing systems rather than replace them:

1. **Routed relation paths** — draw edges along the orthogonal routes ELK already
   computes, with a typed, tested fallback, instead of straight diagonals.
2. **Animated flow relations** — let a small, semantically-eligible set of
   relations animate direction of travel; everything structural stays static;
   reduced motion yields a complete static representation.
3. **Node-card depth and elevation** — a restrained, token-derived depth system
   that strengthens hierarchy without changing card dimensions.
4. **Context-preserving focus mode** — an additive "dim unrelated" option beside
   the unchanged "hide unrelated" behaviour, with a shareable URL contract.

Framing is deliberate: these are **navigation, hierarchy and comprehension**
improvements. They do not improve architectural correctness, and they must never
imply runtime activity or infer roles from styling.

## Problem statement

- Edges are rendered with `getStraightPath` (`web/src/components/Graph.tsx`), so
  dense views (workspace overview, `public-api`, `type-relationships`) become a
  field of crossing diagonals — even though the ELK worker is configured with
  `edgeRouting: "ORTHOGONAL"` and already returns `RoutedEdge.points`, which the
  renderer discards.
- Node cards are flat single surfaces with a left accent bar; the visual weight
  of an overview package card and a leaf function card is too similar to the
  hierarchy they encode.
- Relation kinds are already visually differentiated — the registry gives each
  kind a distinct colour token, line pattern, width and directional marker. The
  gap is confined to the manual-flow family: manual-flow edges render statically,
  and there is no explicit visual distinction between an ordinary manual relation
  and a semantically eligible active/ordered flow relation, so an ordered manual
  flow cannot communicate direction of travel or membership in the walked path.
- The only focus mode ("related only") reduces the projection to a neighbourhood,
  discarding spatial context that some users depend on.

None of this is a correctness defect; it is a readability and navigation deficit.
Animation and gradients are explicitly **not** claimed to improve correctness.

## Current capabilities (authoritative baseline — extend, do not duplicate)

Recorded from the implemented repository so this PRD builds on fact:

- **Centralized typed relation-style registry** — `web/src/adapter/relationStyle.ts`.
  Each relation kind resolves to a `RelationStyle`: a `--rel-*` **stroke colour
  token**, **line pattern** (`solid` | `dashed` | `dotted`), **width**,
  **directional marker** (`arrow` | `arrow-closed` | `none`), an **emphasis**
  rank, and explicit **per-state visuals** for `normal | related | selected |
  faded`. Meaning is encoded through *colour token + pattern + width + marker*
  together, never colour alone, and colour is a token so dark/light both resolve.
  `dashArrayFor`, `edgeVisual`, `markerId`, `edgeZIndex`, `shouldShowEdgeLabel`
  are all pure and shared by the graph and the legend.
- **Relation legend** — `web/src/components/Panels.tsx` (`Legend`) consumes the
  same registry; there is no relation styling outside the registry.
- **Selected / related / faded edges** — computed in `Graph.tsx` from the anchor
  (`selectedEntity ?? focusId`): the selected relation is `selected`, edges
  touching the anchor are `related`, the rest are `faded`; `edgeZIndex` keeps
  strong/selected above quiet/faded.
- **Progressive node-card density** — `web/src/model/nodeCards.ts` precomputes an
  immutable `NodeCard` (category, deterministic `width`/`height`, metrics with a
  `minLevel`, diagnostic badge); `web/src/components/NodeCard.tsx` only picks a
  `CardLevel` (`compact | normal | detailed`) from zoom + selection and one
  dominant visual state. No aggregation happens at render.
- **Design tokens** — `web/src/styles.css` defines `--node-*` (per category) and
  `--rel-*` (per relation) for **both** dark (default `:root`) and light
  (`@media (prefers-color-scheme: light)` **and** `:root[data-theme="light"]`),
  plus `--node-bg`, `--border`, `--focus`, `--sev-*`.
- **Diagnostics UI** — `web/src/components/DiagnosticsExplorer.tsx` (unchanged by
  this PRD).
- **React Flow rendering + ELK layout** — `@xyflow/react` 12 with a same-origin
  module Web Worker running `elkjs`; `web/src/layout/types.ts` already defines
  `RoutedEdge { id, points: Point[] }` and `LayoutResult.edges`; the worker
  (`elk.worker.ts`) already fills `points` from `section.startPoint +
  bendPoints + endPoint`. **`web/src/app/AppContext.tsx` (`LayoutState`) keeps
  only node `positions` and drops `result.edges`** — this is the missing seam.
- **Reduced-motion behaviour** — `styles.css` has a global
  `@media (prefers-reduced-motion: reduce) { * { animation: none !important;
  transition: none !important; } }` kill-switch.
- **Manual-flow presentation surface** — `cratevista-config` `RawRelation`
  already carries `kind` (open, defaults to `manual`), `role`, `label`, and an
  open **`attributes` bag documented as "presentation attributes"**, which flows
  through to `cratevista_schema::Relation.attributes` (a
  `BTreeMap<String, AttrValue>`). `Relation` also carries
  `provenance` (`Discovered | Manual`). This means animation eligibility can be
  expressed **additively** with no new required schema field and no schema
  version bump.
- **URL / query-state model** — `web/src/state/url.ts` serializes only durable
  state: `view`, `entity`/`relation` (mutually exclusive), `q`, `kinds`, `focus`,
  `edges` (`all | related | hidden`), `stage`. Unknown `edges` values are
  dropped. `web/src/state/store.ts` (`toUrlState`) writes it back; history is
  covered by `tests/history.test.tsx`.
- **Embedded bundle workflow** — `web/dist` is committed and drift-guarded by
  `scripts/check-dist.mjs` (`check:dist`) and `tests/dist-compare.test.ts`; CI
  builds `dist` before the server binary that embeds it. No frontend change is
  "done" until `dist` is rebuilt and matches source — but rebuilding `dist` is a
  **separate, final** step, never bundled into an intermediate phase commit.

This PRD must not introduce a second relation-style source, a second layout
engine, a per-component gradient literal, or a parallel URL model.

## Design principles / architecture boundaries (locked)

- **One relation pipeline.** Geometry, style and animation-presentation are three
  separable concerns behind one path: registry → `edgeVisual`/geometry → edge
  component. The legend and the graph consume the same semantic tokens.
- **Routing and style stay separate.** Edge *geometry* (routing) never reads
  relation *kind*; relation *style* never reads geometry.
- **Animation state is separate from relation identity.** Animation is never
  encoded by overloading `RelationKind`. It is a derived UI flag (`animated:
  boolean`) computed from provenance + an explicit, additive presentation
  attribute, resolved into the same per-state visual pipeline.
- **Depth tokens live in the token system.** No component defines a unique
  hardcoded gradient; depth derives from existing `--node-*` tokens via
  `color-mix`.
- **Static and live render identically.** Nothing here depends on the server; a
  statically-built site and a live `serve` session look the same.
- **One embedded bundle; no server/API dependency introduced.**
- **No animation state is serialized into generated architecture artifacts**
  unless the additive manual-flow presentation contract in Capability 2 is
  explicitly adopted; that contract is the *only* sanctioned schema-adjacent
  change, and it is additive and optional.

---

## Capability 1 — Routed relation paths

### Decision (approved)

**ELK route sections are authoritative for edge geometry when present; a
`getSmoothStepPath` computation is the typed fallback when they are absent or
malformed.** ELK already produces orthogonal routes for the current layered
layout; consuming them keeps the rendered edge consistent with the geometry the
layout actually reserved space for. `getSmoothStepPath` (rounded orthogonal
corners between the source and target handles) is a deterministic, dependency-free
fallback that visually matches ELK's orthogonal style, so the two sources are not
jarringly different when a fallback occurs.

Rejected alternatives: (a) `getSmoothStepPath` for **all** edges — discards ELK's
collision-aware routing and reintroduces overlaps ELK already solved; (b)
straight paths — the current problem.

### Requirements

- **Geometry source of truth.** Surface `LayoutResult.edges` (`RoutedEdge`)
  through `LayoutState` (new `routes: Map<string, Point[]>` alongside
  `positions`), memoized by the same layout token so selection/inspector changes
  never rebuild it. The `RelationEdge` component builds its SVG path from the
  routed points when available.
- **Fallback.** When a route is missing, empty, has fewer than two points, or
  contains non-finite coordinates, fall back to `getSmoothStepPath` between the
  resolved handle anchors. The fallback is a single typed function with unit
  tests; it never silently returns arbitrary geometry.
- **Handles and arrow direction.** Keep the existing `source` (right) / `target`
  (left) handles and the registry's directional marker. Arrow direction always
  follows `from → to`, independent of routing source. The end marker sits at the
  final routed point, oriented along the last segment.
- **Rounded corners.** Orthogonal corners are rounded with a bounded radius
  (proposed token `--edge-corner-radius`, default ~6px, clamped so it never
  exceeds half the shorter adjacent segment). Applies to both routed and fallback
  paths for a consistent look.
- **Self-loops.** A relation whose `from === to` is drawn as a small rounded
  self-loop on one side of the node (never a zero-length degenerate path). ELK
  may not route these; the fallback owns them deterministically.
- **Parallel edges.** Multiple relations between the same ordered pair (distinct
  `role`/kind) must remain individually visible and clickable. When ELK routes
  them to overlapping polylines, apply a deterministic, index-based perpendicular
  offset (stable ordering by relation id) so they separate without overlapping;
  labels attach per edge.
- **Layout pending.** While `layoutState.status !== "ok"`, edges render with the
  fallback geometry against placeholder positions and are visually subdued (the
  existing "Computing layout…" status remains); no route is read until positions
  and routes for the current token are both present.
- **Determinism.** For a given document + view + edge-visibility set, node
  positions, routes, offsets and the resulting `d` attributes are byte-stable
  across reloads and a fresh browsing context (consistent with the PRD-07
  deterministic-layout guarantee). No randomness, no time input.
- **Edge labels.** `shouldShowEdgeLabel` is unchanged. The label anchor moves to
  the **midpoint of the routed polyline by arc length** (not the straight
  midpoint) so labels sit on the drawn path; ties broken deterministically.
- **Malformed data.** Any malformed routing payload is treated as "route absent"
  → fallback, and is counted (dev-only) so a systemic layout regression is
  observable in tests rather than silently degrading every edge.

### Proposed types

```ts
// web/src/layout/types.ts (already present): RoutedEdge { id; points: Point[] }

// web/src/app/AppContext.tsx — LayoutState gains routed geometry:
export type LayoutState = {
  status: "idle" | "loading" | "ok" | "error";
  positions: Map<string, PositionedNode>;
  routes: Map<string, Point[]>; // NEW — empty when a layout produced no sections
  // …existing fields (retry, error) unchanged
};

// web/src/components/edgeGeometry.ts — one geometry seam, style-agnostic:
export interface EdgePathInput {
  route?: Point[];                 // ELK section points for this edge, if any
  source: Point; target: Point;    // resolved handle anchors
  cornerRadius: number;
  parallelIndex: number;           // 0 for a lone edge; ± offset rank otherwise
  selfLoop: boolean;
}
export interface EdgePath { d: string; labelX: number; labelY: number; }
export function edgePath(input: EdgePathInput): EdgePath; // deterministic, pure
```

### Non-goals (Capability 1)

No new layout algorithm, no per-edge manual routing UI, no draggable waypoints,
no curve/bezier styling of structural edges.

---

## Capability 2 — Animated flow relations

### Decision (approved)

Animation is **opt-in and semantic**, expressed as a **derived UI flag**, never
by relation kind:

- **Structural relations stay static, always** — `contains`, `depends_on`,
  `implements`/`implemented_by`/`implemented_for`, `calls`, `uses`,
  `has_field_type`, `accepts_type`, `returns_type`, `error_type`, `re_exports`,
  `imports`, `references_type`, and the **unknown** fallback. Unknown relation
  kinds never animate.
- **Eligible = manual relations that explicitly declare active flow intent
  through the locked presentation contract** — the exact attribute
  **`attributes.flow = "active"`** on the relation (see the Presentation contract
  below). A relation is animation-eligible **only** when its provenance is
  `Manual` **and** it carries exactly that attribute/value. Absence ⇒ static.
- **Selection, hover and stage membership alone never make an otherwise
  ineligible relation animate.** These states change emphasis/label visibility
  only; they can never introduce motion on a relation that is not eligible by the
  presentation contract. (An eligible relation keeps animating while selected,
  hovered, or on the active stage, subject to the state/zoom/fail-safe rules
  below.)

*Rejected alternatives (kept for the record):* overloading `RelationKind` with an
animation state (breaks the "kind is identity, not presentation" boundary); a
boolean `attributes.animated = true` (rejected in favour of the single locked
spelling `attributes.flow = "active"` so there is exactly one public contract);
animating discovered/structural edges (would falsely imply runtime activity).

Animation must **never** imply runtime activity when the document contains only
static architecture data; that is why default eligibility is empty for every
discovered relation.

### Presentation contract (locked public configuration)

The public, additive configuration contract for animation intent is the exact
attribute **`attributes.flow = "active"`** on a manual relation, carried by the
existing open `attributes` bag (`RawRelation.attributes` →
`cratevista_schema::Relation.attributes`). Locked rules:

- **Optional and additive** — its absence means the relation is static.
- **Manual-only intent** — it is valid as animation intent **only when the
  relation's provenance is `Manual`**. A **discovered** relation carrying the same
  raw `flow = "active"` attribute (e.g. echoed from some producer) **remains
  static** — provenance gates the intent.
- **Safe unknown values** — any other value of `flow` (or any other attribute) is
  ignored for animation and renders static; parsing never throws.
- **Single typed helper** — parsing/resolution is centralized in one typed helper
  (`isAnimationEligible`, below); no component reads the attribute directly.
- **No schema version bump** — the bag already exists; nothing in the versioned
  schema changes.
- **Documentation + validation are implementation deliverables** — the
  configuration reference (`docs/configuration.md`) documents the contract, and
  validation/parsing tests are required when the contract is implemented.

The boolean `attributes.animated` is explicitly **not** part of the contract and
must not be introduced.

### Requirements

- **Eligibility resolution.** A single pure function decides eligibility:

  ```ts
  // web/src/adapter/relationStyle.ts (extends the registry, same module)
  export function isAnimationEligible(rel: {
    provenance: "discovered" | "manual";
    kind: string;
    attributes?: Record<string, unknown>;
  }): boolean; // true iff provenance === "manual" AND attributes.flow === "active"
  ```

  It is the only place eligibility is decided; the legend and the graph call it.
  It returns `true` **only** when `provenance === "manual"` and
  `attributes.flow === "active"`; every other input (discovered provenance, a
  missing attribute, or any other `flow` value) returns `false` and renders
  static. It never throws on malformed attributes.
- **Direction of dash movement.** Animated edges use a moving `stroke-dashoffset`
  along the path in the `from → to` direction (dashes travel toward the arrow),
  reinforcing the marker rather than contradicting it.
- **Speed tokens, not literals.** Motion parameters live in tokens
  (proposed `--edge-flow-duration`, default ~0.6s; `--edge-flow-dash` the
  dash/gap geometry). No literal durations scattered in components. The dash
  geometry derives from the edge's current stroke width so it stays legible at
  the animated state's width.
- **Interaction with states.** Animation is orthogonal to `normal | related |
  selected | faded`. An eligible edge animates in `normal`, `related`, and
  `selected`; a **`faded`** eligible edge does **not** animate (it is
  de-emphasised context, so continuous motion would fight the focus). Selected
  eligible edges keep the strongest static width/opacity **and** animate.
- **Hover and focus.** Hover/keyboard-focus of an edge never *starts* animation
  on an ineligible edge; it only affects label visibility and the existing
  emphasis. Eligible edges keep animating under hover/focus.
- **Zoom thresholds.** Below `LABEL_ZOOM_MIN` (0.4) animation is suppressed
  (motion at tiny scale is noise, not signal); the edge stays visible and static
  with its pattern/marker intact.
- **High-edge-count behaviour / fail-safe (locked at 60).** Animation is limited
  to a bounded active subset. The **locked initial threshold is 60 eligible
  animated relations per active view**: 60 or fewer may animate; **above 60,
  continuous motion is disabled for the whole active view** and eligible relations
  retain the distinct **static** "flow" treatment (the flow dash pattern held
  still), with the legend communicating that motion is suppressed. Semantics never
  depend on motion. The threshold is a single **named centralized constant/token**
  (proposed `EDGE_FLOW_MAX_ANIMATED = 60`), never a scattered literal; it is
  **not** user-configurable in this PRD, must be benchmarked and tested, and may be
  adjusted later only through a separately reviewed change.
- **Unknown relations.** Never animate by default (they are `provenance =
  Discovered` and carry no presentation attribute).
- **Reduced motion (mandatory).** Under `prefers-reduced-motion: reduce` the
  global kill-switch stops all movement. Eligible edges must remain fully
  distinguishable through their **static** flow treatment (dash pattern + width +
  marker + label). No essential meaning may exist only in motion — a reduced-motion
  user must be able to identify every flow relation and its direction from static
  cues alone.
- **CSS/SVG only.** Motion is a CSS keyframe animation on the SVG path
  (`@keyframes cv-edge-flow { to { stroke-dashoffset: 0 } }`), not a JS
  requestAnimationFrame loop and not React state updated per frame.

### Legend

The relation legend gains a small **animated flow sample** for the "active flow"
treatment, shown **only when the current view actually contains eligible
relations**. Under `prefers-reduced-motion` (and above the fail-safe threshold)
the sample renders in its static flow form. The legend must state, in text, the
difference between animated flow and static structural relations so the
distinction is never colour/motion-only.

### Non-goals (Capability 2)

No animation of discovered/structural edges; no per-frame JS animation; no
inferred traffic/throughput; no encoding of animation in `RelationKind`; no
required schema field.

---

## Capability 3 — Node-card depth and elevation

### Decision (approved)

A **restrained** depth system layered onto the existing category-token + density
model, entirely token-derived:

- **Category-aware background.** Replace the flat `--node-bg` fill with a subtle
  vertical gradient computed from the existing category `--accent` and
  `--node-bg` via `color-mix` (e.g. top = `color-mix(in srgb, var(--accent) ~6%,
  var(--node-bg))`, bottom = `--node-bg`). One rule parameterised by `--accent`
  — **no per-category gradient literals**.
- **Border/accent hierarchy retained.** Keep the left accent bar and the
  workspace/package top-border heading treatment; depth augments, not replaces,
  the non-colour structural cue.
- **Bounded elevation.** A single small drop shadow token
  (proposed `--node-elevation`, e.g. `0 6px 18px rgba(0,0,0,.28)` dark / a lighter
  value light) applied to cards, scaled subtly by hierarchy (overview cards read
  slightly raised). No glow, no coloured shadow that could read as a status.
- **Stronger, non-jumping selected state.** Selection keeps the current focus
  ring/box-shadow but must **not** change the card's box model — no border-width
  or padding change that alters `width`/`height` (dimensions are precomputed in
  `nodeCards.ts` and drive layout). Elevation may increase on selection; geometry
  may not.
- **Themes.** Depth is defined for dark (default) and preserves the existing
  light-theme tokens (both `prefers-color-scheme: light` and
  `:root[data-theme="light"]`). Gradients use `color-mix` on tokens so both
  themes resolve without new literals.
- **Forced-colors fallback.** Under `@media (forced-colors: active)` gradients
  and shadows are dropped; cards fall back to system colours with the existing
  selected `outline` rule; hierarchy is still conveyed by border/heading cues.

### Prevented failure modes (must be tested)

- No unique hardcoded gradient per component/category.
- No excessive glow or status-like coloured shadow.
- No text-contrast regression against the tinted background in either theme.
- No card-dimension change caused by selection or hover.
- No inference of business role from styling.
- No importation of a separate/foreign design system — depth is expressed only in
  CrateVista's existing token vocabulary.

Applies uniformly to `workspace`, `package`, `target`, `module`, `type`,
`trait`, `function`, `impl`, `manual`, and the `unknown` fallback categories.

### Proposed tokens

```css
/* styles.css — additive, per theme */
--node-elevation: 0 6px 18px rgba(0,0,0,.28);      /* bounded; lighter in light theme */
--node-tint: 6%;                                   /* accent mix strength for the gradient top */
--edge-corner-radius: 6px;                          /* shared with Capability 1 */
```

### Non-goals (Capability 3)

No redesign of card content/metrics; no new density level; no animated cards; no
size/shape change on interaction.

---

## Capability 4 — Context-preserving focus mode

### Decision (approved)

Keep the existing **hide** behaviour unchanged and add **dim** as an additive,
explicit option. Focus is modelled as two independent facts — whether an **anchor**
is present, and the **style** of emphasis around it — and the URL is the single
source of truth. `focusmode` has exactly two serialized values (`hide | dim`);
there is **no** serialized `all` value — "no focus" is simply the absence of a
`focus` anchor.

The complete, locked state model (URL → behaviour):

| URL state | Behaviour | Relationship to today |
|-----------|-----------|-----------------------|
| no `focus` param | Normal complete graph; conceptually "all"; **no focus mode serialized** | Unchanged default |
| `focus=<entity>` (no `focusmode`) | Legacy **hide-unrelated**: projection reduced to the neighbourhood | **Exactly today's "related only"**; preserves existing shared URLs byte-for-byte |
| `focus=<entity>&focusmode=dim` | Full projection retained; unrelated content **dimmed** | **New** |
| `focus=<entity>&focusmode=<unknown>` | Discard the unknown value; fall back to legacy **hide** while the anchor exists | Safe fallback |

Because today's behaviour couples "related only" to `focusMode` (which sets the
projection's `relatedOnly`), the PRD represents the *style* of focus separately
from the *anchor*:

- **Anchor** stays as today: `focus=<entityId>` in the URL (plus selection).
  Removing the anchor is the only way to reach the "all" (no-focus) state.
- **Focus style** is a new, additive URL key **`focusmode`** with values
  `hide | dim`. It is meaningful **only when a `focus` anchor is present**;
  `focusmode` is never serialized without a `focus` (a `focusmode` with no anchor
  is ignored and omitted when normalized), and `all` is never a `focusmode` value.
  Absent `focusmode` (with an anchor) ⇒ `hide`, so every existing shared URL is
  byte-identical. This URL contract is locked (see **Approved decisions**).

The UI exposes three focus controls that map onto this model without ever writing
an `all` value:

- **Clear focus** — removes the `focus` anchor (and any `focusmode`), returning to
  the normal complete graph. It does **not** write `focusmode=all`.
- **Hide unrelated** — sets `focus=<entity>` with no `focusmode` (legacy hide).
- **Dim unrelated** — sets `focus=<entity>&focusmode=dim`.

For **dim mode** specifically:

- **Preserve node positions and context.** The projection is **not** reduced;
  every node stays in its laid-out position. Unrelated nodes and edges are faded
  (nodes to a bounded opacity, edges to the existing `faded` state).
- **Related path stays prominent.** The anchor, its `related` neighbours, and the
  connecting edges keep full emphasis and paint above the dimmed set
  (`edgeZIndex` already de-prioritises `faded`).
- **Selectability of dimmed content.** Recommended **locked** decision: dimmed
  nodes/edges **remain selectable** (dim is a de-emphasis, not a removal);
  selecting a dimmed node re-anchors focus around it. This must be an explicit
  locked decision, not incidental behaviour.
- **Keyboard navigation.** Tab order and the keyboard GraphList remain complete
  and understandable; dimmed items are still reachable and announce their normal
  role. Dim is a visual state, not a disabled state.
- **Screen readers.** Dimmed content must **not** be exposed as "hidden" and must
  not receive `aria-hidden`; opacity is presentational only. (Hide mode, which
  truly removes nodes, legitimately omits them from the tree.)
- **Contrast floor.** Dimming must not drop node **text** below minimum
  readability. The recommended approach dims the card **container/background and
  edges** while keeping text at/above the contrast floor (i.e. opacity is applied
  so that essential text still meets contrast, or text is exempted from the
  dimming), verified in both themes.

### Interactions to specify (locked)

- **Edge-visibility controls (`edges`).** `edges` (`all | related | hidden`) and
  the focus style are independent axes. `edges=hidden` + `focusmode=dim` shows
  dimmed nodes with no edges; `edges=related` shows only anchor edges among dimmed
  nodes. No mode silently overrides another.
- **Search matches.** A search match on an otherwise-unrelated node keeps its
  search emphasis **above** the dim (a match must never be invisible); the
  existing `state-search` ring wins over the dim treatment.
- **Diagnostics.** Node diagnostic badges/severity rings remain visible above the
  dim so problems are never hidden by focus.
- **Selection.** Selecting a node sets the anchor; in dim mode the graph
  re-emphasises around the new anchor over the unchanged full projection and
  **must not request a new layout** (see the relayout contract under Performance).
- **Manual-flow stage highlighting.** When a `stage` is active, stage membership
  emphasis composes with dim: on-stage related content stays prominent; off-stage
  unrelated content dims. Stage and focus are independent and both round-trip in
  the URL.
- **Back/Forward + shareable URL.** `focusmode` round-trips through
  `parseUrlState`/`serializeUrlState`, is covered by history tests, and is absent
  from the URL when at its default so existing links are byte-identical.
- **Stale/unknown values.** With a `focus` anchor present, an unknown `focusmode`
  value is discarded and the legacy `hide` behaviour is used, mirroring how an
  unknown `edges` value is dropped today. A `focusmode` with no `focus` anchor is
  ignored entirely (there is nothing to focus).

### Proposed types

```ts
// web/src/state/url.ts
export type EdgeMode = "all" | "related" | "hidden"; // unchanged
// Serialized focus style. Only two values exist; "all"/no-focus is the ABSENCE
// of a `focus` anchor, never a `focusmode` value.
export type FocusMode = "hide" | "dim";
export interface UrlState {
  /* …existing… */
  focus?: string;        // anchor entity id (unchanged). Absent ⇒ no focus ("all").
  focusmode?: FocusMode; // NEW, additive. Serialized only WITH a `focus` anchor;
                         //   absent-with-anchor ⇒ "hide"; unknown ⇒ "hide".
}
```

### Non-goals (Capability 4)

No removal/rename of the existing hide behaviour; no multi-anchor focus; no
persistence beyond the URL; no change to how the projection is built for hide
mode.

---

## Accessibility (acceptance requirements)

- `prefers-reduced-motion: reduce` disables all continuous motion; every flow
  relation stays identifiable and directional from static cues (pattern + width +
  marker + label). No meaning is motion-only.
- Keyboard focus reaches every node and edge control in all modes; dim mode never
  removes items from tab order or the keyboard GraphList.
- The visible focus state remains the existing high-contrast ring and is intact
  under `forced-colors`.
- Screen-reader relation descriptions are unchanged in wording and are never
  gated on animation; dimmed content is not announced as hidden.
- No meaning conveyed **only** by animation, colour, gradient or shadow — each
  carries a redundant non-colour/non-motion cue.
- Text contrast meets the project minimum against tinted card backgrounds and
  under dimming, in dark and light themes.
- No flashing; animation is a smooth continuous translate with a bounded,
  tokenised speed (no strobing, well under 3 flashes/second).
- Selecting a node never resizes or reflows its **card** (dimensions are fixed);
  emphasis changes over an unchanged projection request no relayout. (Changing the
  hide-mode anchor may relayout the reduced projection — see the relayout contract
  under Performance — but that is projection change, not card motion.)
- The legend describes animated-vs-static flow in text and shows an animated
  sample that respects reduced motion and the fail-safe threshold.

## Performance (budgets and fail-safes)

Measured against synthetic and existing fixtures; consistent with the PRD-07
1,500-node budget and benchmark harness (`scripts/gen-benchmark-workspaces.mjs`,
`playwright.bench.config.ts`).

- **Scale targets.** 500 nodes; ≥1,000 edges; many parallel relations between
  shared pairs.
- **Bounded animation.** Only the eligible active subset animates; above the
  locked threshold of **60** (`EDGE_FLOW_MAX_ANIMATED`) continuous motion is
  disabled for the view with the static flow fallback retained.
- **No per-frame React.** Motion is CSS/SVG; no React state update per animation
  frame; no JS rAF loop for edges.
- **Memoized geometry.** Routes and edge `d` strings are memoized by layout token
  + visibility set. Pure emphasis changes over an unchanged projection (dim
  toggle, anchor change in dim mode, hover, selection) reuse the memoized
  geometry; a projection change (entering/changing hide) recomputes it.
- **Relayout contract (locked).** Layout requests are governed by these rules:
  - toggling visual emphasis within an **unchanged full projection** (e.g.
    `dim`→`all` with the same nodes, or turning dim on) **must not** request a new
    layout;
  - **selecting a different anchor while in dim mode must not** request a new
    layout (the full projection is unchanged; only emphasis moves);
  - **entering or changing hide mode may legitimately require a layout** for the
    reduced projection — this is expected, not a regression;
  - **returning from hide to dim/all may reuse a cached full-layout result** when
    one is available, but such caching is an **optimization, not an acceptance
    requirement**;
  - no **continuous** or otherwise **unnecessary** relayout may occur in any mode.
- **Stable projection identities.** Node-card projections keep referential
  stability across emphasis changes (as today), so React reconciliation stays
  cheap.
- **Background tab.** Animations pause/no-op when the tab is hidden (CSS
  animations already throttle; verify no busy work).
- **Browser zoom / HiDPI.** Geometry and motion render crisply at browser zoom
  100% / DPR 1 and at HiDPI; captures for visual evidence use zoom 100% / DPR 1.

The fail-safe threshold must be explained and tested, not asserted arbitrarily:
the value is chosen so that the animated set stays a readable "flow," and the
test proves motion is disabled above it while static semantics persist.

## Testing strategy

Unit/component tests use Vitest + the jsdom React Flow stub
(`tests/support/xyflow.tsx`); E2E uses the real Chromium Playwright suite against
the real server binary; visual evidence uses deterministic captures. **No
external reference repository, fixture or screenshot is used** — synthetic and
existing CrateVista fixtures only.

**Routing** (extends `tests/layout.test.ts`, `tests/relation-edge.test.tsx`):
- routed path built from ELK points; orthogonal/rounded corners;
- ELK bend-point consumption end-to-end (worker → `LayoutState.routes` → `d`);
- typed fallback when routes absent/malformed (each malformed shape);
- self-loop path is non-degenerate;
- parallel edges separate and stay individually clickable;
- arrow direction always `from → to` regardless of geometry source;
- deterministic `d` across reload and a fresh context.

**Animation** (extends `tests/relation-style.test.ts`,
`tests/relation-legend.test.tsx`):
- only eligible (manual + presentation attribute) relations animate;
- every structural/unknown relation stays static;
- dash movement direction matches `from → to`;
- reduced-motion disables movement while static flow cues remain;
- `faded` eligible edge does not animate; `selected` eligible edge animates at
  strongest static width;
- fail-safe disables motion above the threshold and keeps static semantics;
- legend shows the animated sample only when eligible relations exist and honours
  reduced motion.

**Node depth** (extends `tests/node-card-view.test.tsx`,
`tests/node-cards.test.tsx`):
- background/gradient derives from `--accent`/`--node-bg` tokens (no literal);
- no duplicated per-category gradient literals (guard test);
- selected/hover state does not change `width`/`height`;
- contrast holds against tinted background (dark + light) and non-colour cue
  present;
- forced-colors fallback drops gradient/shadow and keeps the selected outline.

**Dim focus** (extends `tests/store.test.ts`, `tests/url-normalize.test.ts`,
`tests/history.test.tsx`, `tests/a11y.test.tsx`, `tests/reduced-mode.test.tsx`):
- no `focus` param ⇒ complete graph, no focus mode serialized;
- `focus=<entity>` with no `focusmode` ⇒ legacy hide (projection reduced), and
  the serialized URL is byte-for-byte identical to today's shared links;
- `focus=<entity>&focusmode=dim` ⇒ full projection retained, unrelated nodes
  remain in layout and are dimmed; related path/anchor stay prominent above them;
- "Clear focus" removes the `focus` anchor (and any `focusmode`) and never writes
  `focusmode=all`; `all` never appears as a serialized value;
- `focusmode` round-trips through parse/serialize; Back/Forward restores it;
  `focusmode` is omitted when there is no anchor and when at the `hide` default;
- unknown `focusmode` **with** an anchor falls back to `hide`; `focusmode`
  **without** an anchor is ignored;
- dimmed content stays keyboard-reachable and is not `aria-hidden`;
- search match, diagnostic badge, and selection all stay visible over dim.

**Performance** (extends the PRD-07 benchmark harness):
- 500-node / 1,000-edge synthetic graph within budget;
- bounded animated-edge count / fail-safe engaged above threshold;
- no layout request on a dim toggle over an unchanged projection;
- no layout request when selecting a different anchor while in dim mode;
- entering/changing hide mode may relayout the reduced projection (expected), and
  no continuous or repeated relayout occurs;
- stable projection/edge-geometry identities across pure emphasis changes.

**Visual evidence** (deterministic captures, dark theme, zoom 100% / DPR 1):
workspace overview (routed); a dense `type`/`public-api` view (routed, readable);
a manual flow with eligible animated edges; a selected node in dim mode; a
reduced-motion capture; a light-theme capture. Captures are internal test
evidence, not README assets, and add nothing to the release archive.

## Migration and compatibility

- Existing documents and configuration render unchanged; absence of a presentation
  attribute ⇒ static edges.
- Old shared URLs continue to work; existing `edges` (`related`/`hidden`) and
  `focus` state remain valid. A `focus=<entity>` link with no `focusmode` keeps
  its legacy hide behaviour, and `focusmode` is serialized only alongside an
  anchor and only when not at its `hide` default, so previously-shared links are
  byte-identical. `all` is never written as a `focusmode` value.
- Unknown relation kinds keep the neutral static fallback (no animation).
- The static-site privacy contract is unchanged: no source snippets introduced,
  zero `/api/**` in static mode, no new network dependency.
- Release archive contents are unchanged except for the rebuilt embedded asset
  bytes (`web/dist`) once the frontend ships — and that rebuild is a single final
  step, never a partial-phase commit.

## Implementation plan (phased; each phase independently green)

The plan remains split into five independently-reviewable phases. Implementation
phases **may edit source and tests**; `web/dist` is **rebuilt once, in the final
phase**; **no partially-rebuilt bundle may be committed**; each phase keeps
`fmt`/`clippy`/type/lint/test green. Approving this PRD does **not** itself perform
implementation, an asset rebuild, or publication — it authorizes these phases.

1. **Routed geometry + tests.** Surface `RoutedEdge.points` through
   `LayoutState.routes`; add the pure `edgePath` seam with the smooth-step
   fallback, self-loop and parallel-edge handling; switch `RelationEdge` to it.
   Rollback boundary: revert the edge component to `getStraightPath`; no other
   system touched.
2. **Semantic animation + reduced motion.** Add `isAnimationEligible` (manual +
   `attributes.flow = "active"`), the derived `animated` flag, the flow keyframe +
   speed tokens, the `EDGE_FLOW_MAX_ANIMATED = 60` fail-safe, and the legend
   sample; document the presentation contract and add its validation tests.
   Rollback boundary: eligibility returns `false` for everything ⇒ fully static,
   everything else inert.
3. **Node depth.** Add depth tokens + the token-derived gradient/elevation and the
   non-jumping selected state. Rollback boundary: remove the depth tokens/rules;
   cards revert to the flat surface.
4. **Dim-focus URL/state.** Add `FocusMode`, the dim emphasis path (no projection
   reduction), and the URL round-trip/normalization. Rollback boundary: drop
   `focusmode`; hide remains exactly as today.
5. **E2E, performance and final embedded-bundle rebuild.** Real-browser E2E,
   benchmark budgets, then the single `web/dist` rebuild + `check:dist`.

Each phase is independently reviewable and independently revertible at its stated
rollback boundary.

## Acceptance criteria (outcome-based)

- [x] Dense views (`workspace-overview`, `public-api`, `type-relationships`) are
      drawn with routed, orthogonal, rounded paths that follow ELK's geometry when
      present and a typed smooth-step fallback otherwise; the rendered `d` is
      deterministic across reloads and a fresh context.
- [x] Self-loops and parallel edges are individually visible and clickable.
- [x] Structural/discovered/unknown relations never animate and never read as
      "active"; a discovered relation carrying `flow = "active"` stays static.
- [x] Only manual relations carrying exactly `attributes.flow = "active"` animate,
      and they show direction through motion **plus** static
      pattern/width/marker/label; selection, hover or stage membership alone never
      introduces motion on an ineligible relation. `attributes.animated` is absent
      from the codebase.
- [x] Under `prefers-reduced-motion`, every flow relation is fully identifiable and
      directional from static cues alone; no meaning is motion-only.
- [x] The fail-safe disables continuous motion above **60** eligible animated
      relations per active view (a named centralized constant) while keeping the
      static flow treatment, semantics, and the legend's motion-suppressed note.
- [x] Node cards gain token-derived depth that strengthens hierarchy with **no**
      change to card dimensions on selection or hover, correct in dark and light
      themes, and degraded safely under forced-colors.
- [x] No duplicated hardcoded per-category gradient exists (guarded by a test).
- [x] Dim focus preserves node positions and spatial context; the related path
      stays prominent; dimmed content stays selectable, keyboard-reachable, and is
      not announced as hidden; text stays above the contrast floor.
- [x] Hide focus remains available and behaves exactly as today: `focus=<entity>`
      with no `focusmode` reduces the projection and serializes byte-identically to
      previously-shared links.
- [x] The focus state model holds: no `focus` ⇒ complete graph (no serialized focus
      mode); "Clear focus" removes the anchor and never writes `focusmode=all`;
      `all` never appears as a serialized `focusmode` value; `focusmode` is
      serialized only alongside an anchor and only when not at the `hide` default.
- [x] `focusmode` URL state is shareable and Back/Forward-safe; an unknown
      `focusmode` with an anchor falls back to `hide`, and a `focusmode` without an
      anchor is ignored; all previously-shared URLs still work.
- [x] Search matches, diagnostic badges and selection remain visible over dim.
- [x] Accessibility checks (keyboard, focus ring, forced-colors, contrast, no
      flashing, no selection-induced layout motion) pass.
- [x] Performance budgets pass (500 nodes / ≥1,000 edges; bounded animated set;
      stable geometry identities). The relayout contract holds: no layout request
      on a dim toggle over an unchanged projection or on an anchor change in dim
      mode; entering/changing hide may relayout the reduced projection; no
      continuous or unnecessary relayout.
- [x] Static and live modes render identically; the static-site privacy contract
      is unchanged.
- [x] The committed `web/dist` matches frontend source (`check:dist` green) and is
      rebuilt only once, in the final phase.
- [x] No external reference-project name, path, URL, screenshot, identifier, CSS
      literal or domain label appears anywhere in tracked content.

## Approved decisions

All six load-bearing decisions have explicit maintainer approval and are **locked**
(2026-07-19). Implementation must follow them and **must not reopen any of them
without a PRD amendment**. The rejected alternatives and trade-offs are retained
so the reasoning is not lost.

1. **Edge geometry — ELK sections authoritative, smooth-step fallback.** ELK route
   sections are authoritative for edge geometry when valid; `getSmoothStepPath` is
   the deterministic typed fallback when a route is absent or malformed. Routing
   geometry and relation styling remain separate concerns. Self-loops, parallel
   edges, malformed routes and label placement follow the Capability 1
   requirements. *Rejected:* smooth-step for all edges (discards ELK's
   collision-aware routing) and straight paths (the original problem). *Trade-off
   accepted:* a small amount of section-parsing complexity in exchange for
   geometry consistent with the space the layout reserved.

2. **Animation eligibility — manual + explicit intent only.** Discovered and
   structural relations never animate; unknown relation kinds never animate by
   default. Only **manual** relations with explicit presentation intent are
   eligible. Selection, hover or stage membership alone must **not** make an
   otherwise ineligible relation animate. Reduced-motion support and static
   semantic cues remain mandatory. *Rejected:* animating discovered/structural
   edges (would imply runtime activity); default-on flow. *Trade-off accepted:*
   less "alive by default" in exchange for never implying activity that a static
   architecture document does not contain.

3. **Presentation contract — `attributes.flow = "active"`.** The single, locked
   public configuration contract is the exact attribute **`attributes.flow =
   "active"`** on a relation, carried by the existing open `attributes` bag. It is
   optional and additive (absent ⇒ static); valid as animation intent **only when
   provenance is `Manual`**; a discovered relation carrying the same raw attribute
   remains static; unknown values are ignored and render static; parsing is
   centralized in one typed helper; there is **no schema version bump**; and
   configuration documentation plus validation tests are implementation
   deliverables. *Rejected:* the boolean `attributes.animated` (would create a
   second spelling) and a typed first-class schema field (a schema change; the
   attribute bag is already the sanctioned presentation surface). **`attributes.animated`
   must not be introduced.**

4. **Focus URL contract.** Locked: no `focus` ⇒ complete graph with no focus mode
   serialized; `focus=<entity>` without `focusmode` ⇒ legacy hide-unrelated;
   `focus=<entity>&focusmode=dim` ⇒ full projection retained with unrelated content
   dimmed; an unknown `focusmode` is ignored and the legacy hide behaviour applies
   while a focus anchor exists; a `focusmode` without `focus` is ignored and
   omitted when normalized. **`all` is not a serialized `focusmode` value.** UI
   concepts: **Clear focus** (removes both the anchor and any focus-mode state),
   **Hide unrelated**, **Dim unrelated**. *Rejected:* a serialized `all` value;
   coupling the style into the anchor. *Trade-off accepted:* a separate,
   normalized key keeps every previously-shared URL byte-identical.

5. **Animation fail-safe — 60 eligible animated relations per active view.** 60 or
   fewer may animate; above 60, continuous motion is disabled for the whole active
   view while eligible relations keep the static flow treatment and the legend
   communicates that motion is suppressed. The value is a **named centralized
   constant/token**, not a scattered literal; it is **not** user-configurable in
   this PRD; it must be benchmarked and tested; future adjustment is permitted only
   through a separately reviewed change. *Trade-off accepted:* a fixed, tested
   ceiling over per-user tuning, chosen so the animated set stays a readable flow.

6. **Node visual depth.** Category tint strength ≈ 6%, derived from the existing
   category accent and node-background tokens; one bounded elevation token per
   theme; no per-category gradient literals; no coloured glow; selected elevation
   may strengthen without changing dimensions; forced-colors removes
   gradients/shadows and retains structural cues. Exact final CSS colour values may
   be tuned during implementation while preserving these semantic limits and
   passing contrast/visual tests. *Rejected:* richer gradients/glow (risk of
   contrast regressions and a status-like read). *Trade-off accepted:* a restrained
   depth ceiling over maximal richness.

No locked decision may be changed by implementation; changing one requires a PRD
amendment.
