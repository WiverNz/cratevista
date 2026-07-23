# PRD — Issue 15: Explorer visual redesign

**Status:** Approved — safe to implement (approved 2026-07-23)
**Issue:** [`../ISSUES/issue_15_explorer_visual_redesign.md`](../ISSUES/issue_15_explorer_visual_redesign.md)
**Primary area:** `web/` (explorer SPA)
**Depends on:** Issues 06–10 (Implemented/Verified), Issue 14 (Implemented/Verified)
**Schema impact:** none. No `SchemaVersion` bump. No Rust/server/generated-type change.
**Follow-up:** [`issue_16_guided_flows_and_stage_navigation.md`](issue_16_guided_flows_and_stage_navigation.md) (draft placeholder; not approved) owns guided flows, stage navigation, transient motion and any schema change.

---

## 0. Purpose and repository audit

The CrateVista explorer is correct but visually under-developed: low hierarchy,
flat surfaces, cramped node cards, an undifferentiated inspector, and colour keyed
to Rust kind alone. This PRD delivers a **presentation-only** redesign that reuses
every existing seam, changes no semantics, and adds no schema. It is scoped
strictly to the visual system; all behavioural and motion work is deferred to
Issue 16.

**Current shell baseline (accurate).** The existing components already provide the
seams the redesign needs — project metadata, search, kind filters, `ViewTabs`,
graph controls, the legend and the inspector — but the current application shell
is organized primarily as **stacked full-width control rows**. Issue 15
**reorganizes those existing capabilities into the approved four-region shell**;
it does not add missing capability, and the redesigned shell does not exist yet.

The plan is grounded in an audit of the current code (2026-07-23):

- **Node cards.** `web/src/model/nodeCards.ts` computes a **deterministic layout
  box per kind-derived visual category** via `cardSize()` — currently
  `216×92` (workspace/package), `208×80` (target), `198×68` (module/manual),
  `190×62` (default). Density (`compact` / `normal` / `detailed`, `CardLevel`) is
  **zoom-driven and selection-forced**, and changes only *which parts render
  inside the fixed box* — the box itself never changes on zoom or interaction, and
  ELK receives exactly that box. This is the invariant the redesign keeps while
  enlarging the box to fit the new composition.
- **Kind vs role.** The existing `NodeCategory` type is **kind-derived**
  (workspace/package/type/trait/…), not the authored architectural role. The
  authored role is a separate concept carried in `attributes["category"]`. The
  redesign keeps these distinct: "kind visual category" (existing) vs
  "architectural role" (authored).
- **Authored role vocabulary.** Repository-wide, the only meaningful authored
  `category` value present today is `service` (one override fixture); the rest are
  duplicate-merge test values. The initial vocabulary is therefore a **product
  decision** (locked in D-ROLE), not a fact read off the fixtures.
- **Layout spacing.** `web/src/layout/elk.worker.ts` applies a **single spacing
  scalar** (`options.spacing`) to both `elk.spacing.nodeNode` and
  `elk.layered.spacing.nodeNodeBetweenLayers`, with deterministic ordering
  (`LAYER_SWEEP`, `considerModelOrder = NODES_AND_EDGES`, no random seed). Splitting
  or retuning this scalar is a one-time deterministic change (D-SPACING).
- **Controls & panels.** `web/src/components/Chrome.tsx` / `Panels.tsx` stack
  full-width control rows; `styles.css` uses 6px rectangular controls and flat
  panels. Native semantics (`<button>`, `<select>`, `<fieldset>`/`<legend>`,
  `role="tab"`, `aria-pressed`) are already correct and are preserved.

---

## 1. Goals / non-goals

### Goals

1. Reorganize the existing capabilities into a cohesive four-region application
   shell with clear information hierarchy.
2. Graph-local controls as compact overlays; the graph owns the viewport.
3. A redesigned, deterministic node card with room for role, description and cues.
4. A sectioned, scannable, responsive inspector built only from existing data.
5. A centralized, additive architectural-role presentation layer.
6. A centralized control/panel/token visual system.
7. Deterministic real-browser visual verification and one embedded rebuild.

### Non-goals

No new engine/3D/physics/WebGL. No graph-, focus- or motion-semantic change. **No
new continuous motion** (Issue 16). No stage/step navigation behaviour (Issue 16).
No auto-inferred roles. **No `backdrop-filter` blur.** No embedded `data:`-URI
font; no external asset; CSP unchanged. No schema change / no `SchemaVersion` bump.
No language selector or other unsupported capability. No source snippets beyond
what the server provides.

---

## 2. Approved decisions

All load-bearing decisions are **locked**. Changing any one requires a **PRD
amendment**. Exact final pixel dimensions and exact ELK spacing values are bounded
**implementation-tuning** details (selected from real-browser evidence within the
authorized ranges), **not** open approval questions.

- **D-SHELL — Four-region application shell (locked).** Adopt: (1) global header;
  (2) dedicated view-navigation row; (3) graph workspace with local overlays;
  (4) responsive inspector. The **global header** contains the project title, a
  compact search, entity-kind filters, and existing project-level actions. **View
  selection belongs only to the dedicated view-navigation row.** Graph-local
  edge/focus/zoom controls do **not** appear in the global header. (Section 3.)

- **D-CARD — Enlarged deterministic card policy (locked).** One deterministic
  layout box per node; box computed centrally in `nodeCards.ts`; ELK receives the
  exact rendered box; density changes content only; hover/selection/search/
  diagnostics/focus never change dimensions; interaction never requests relayout
  merely because of card presentation; a generated entity without an authored
  category still receives the redesigned polished kind-based card. **Authorized
  bounded dimension ranges** (implementation selects one exact deterministic value
  per category from real-browser evidence and records the final values in the
  Phase-3 report and tests; leaving these ranges requires a PRD amendment):

  | Visual category | Width | Height |
  |---|---|---|
  | workspace / package | 240–260px | 120–136px |
  | target | 228–244px | 108–124px |
  | module / manual | 216–232px | 100–116px |
  | code / default | 208–224px | 96–112px |

- **D-ROLE — Architectural-role vocabulary (locked).** The initial authored-role
  vocabulary is: `service`, `client`, `database`, `cache`, `observability`,
  `external`, `infra`, `shared`, `domain` (note: `database`, **not** `data-store`).
  Each known role receives a semantic token, a display label, a text badge and a
  non-colour decorative cue. **Unknown non-empty** values receive a neutral role
  badge/cue; **missing** values receive **no** role badge and retain the polished
  kind-based card. Roles are never inferred. (Section 5.)

- **D-TYPE — Typography/asset policy (locked).** Use a tuned `system-ui` stack. Do
  **not** embed a CSS `data:`-URI font. Do **not** add an external font request.
  (Section 7.)

- **D-BLUR — Overlay surface effect (locked).** Do **not** use `backdrop-filter`
  blur in Issue 15. Use restrained solid/gradient surfaces and bounded shadows. A
  future small-overlay blur may be considered separately; it is not part of this
  issue. (Section 6.)

- **D-SPACING — One deterministic layout retune (locked policy).** Horizontal/
  between-layer spacing and vertical/node-node spacing may be separated; final
  values are selected during the card/layout phase from real-browser evidence;
  values are centralized, deterministic and test-covered; sufficient space must
  exist for routed relation labels and parallel lanes; interaction state never
  changes spacing or requests relayout; old coordinates need not be preserved.
  Exact numeric spacing is implementation tuning, not an open decision. (Sections
  8–9.)

- **D-MOTION — Issue-14 compatibility (locked).** Issue 15 introduces **no** new
  continuous motion. Discovered and structural relations remain static; selection
  strengthens only static cues; `attributes.flow = "active"` remains the
  persistent-motion contract. Any future motion on selected discovered edges
  requires an explicit **Issue-14 amendment**, not this PRD. (Section 10.)

- **D-A11Y — Accessibility & determinism invariants (locked, hard gates).** Colour
  never the sole carrier (shape/cue + badge + text). Forced-colors keeps every
  distinction and the selected outline. Native control semantics preserved. No
  layout change on hover/selection/search/diagnostics/focus. Reduced-motion
  behaviour unaffected (nothing new moves).

---

## 3. Visual target — application shell

The explorer is four coordinated regions.

### A. Global header

- A visible project title (from the served document's project metadata).
- A compact search field.
- Global entity-kind filters (applied across the graph).
- Optional project-level actions already supported by CrateVista (e.g. repository
  link) — nothing new invented.
- **No** graph-local focus/edge/zoom controls, and **no** view selection, in this
  row.

### B. View navigation

- A dedicated row of compact **view tabs** (the generated views + any authored
  flow views), with a clear active state. **View selection lives only here.**
- Horizontal overflow (scroll) on narrow viewports — it must **not** wrap into an
  uncontrolled multi-line toolbar.
- Full keyboard navigation (roving tabindex; the existing model is preserved).

### C. Graph workspace

- The graph owns the **majority** of the viewport.
- Fit / zoom / reset controls are a **compact overlay at the upper-left**.
- Edge-visibility and focus controls are a **compact overlay at the upper-right**.
- The legend is a **compact overlay at the lower-left**.
- Controls do **not** consume a permanent full-width global row.
- Overlays must **not** cover a selected node after fit-to-view (fit padding
  accounts for overlay rectangles).
- The canvas has a **restrained** grid/wash, never a visually dominant texture.

### D. Responsive inspector (locked contract)

The inspector adapts to viewport width; **switching its presentation never causes
an ELK relayout**, and the selected entity stays selected across the change.

- **Wide — at or above 1200 CSS px:** a stable right-side **grid column**;
  default width ≈ 360px, clamped between **320px and 410px**; owns its vertical
  scroll.
- **Medium — 768–1199 CSS px:** a right-side **overlay drawer**; width
  `min(410px, 90vw)`; graph geometry is **not** recomputed merely because the
  drawer opens; the visible graph viewport may refit only through the existing
  explicit fit action.
- **Narrow — below 768 CSS px:** a **full-viewport panel**; closing restores the
  graph state and selection; no horizontal page overflow.
- **At all widths:** inspector focus management is keyboard-safe; the close control
  is accessible; the selected entity remains selected when the inspector
  presentation changes; **no ELK relayout is caused solely by switching inspector
  presentation.**
- No permanent mobile bottom navigation is introduced.

**Explicitly excluded:** a language selector or any reference-only capability
CrateVista does not currently support.

---

## 4. Node-card redesign

### Dimension policy (locked; replaces "identical dimensions")

- One deterministic layout box per node, computed centrally in `nodeCards.ts`
  (`cardSize`), sized to hold the fullest (`detailed`) composition.
- ELK receives exactly the dimensions the component renders.
- The box is **stable** across hover, selection, search, diagnostics and focus;
  density levels change only *content shown within* the box.
- Interaction never changes the box model and never requests relayout merely
  because of card presentation.
- The redesign **enlarges** the boxes into the authorized D-CARD ranges; the chosen
  exact per-category values become the layout-test baseline (a one-time
  deterministic change) and are recorded in the Phase-3 report.

### Composition

Each card renders, top to bottom, with consistent spacing and typography:

- title (truncated; full text in the accessible name);
- kind badge;
- an **optional** architectural-role badge (Section 5) when a role is authored;
- **one bounded description line** when the entity has a description;
- compact metrics/indicators (existing deterministic projections);
- source / diagnostic cues;
- a clear composition for the selected / search / diagnostic states (existing
  priority order preserved: selected → search → diagnostic → related → normal).

### Fallback for entities without an authored role

- Absence of `attributes.category` means: render the **new polished kind-based
  card** — not a revert to the pre-Issue-15 design.
- Every generated entity therefore benefits from the redesign; the role badge and
  role cue simply do not appear.

---

## 5. Architectural-role presentation

Keep a **centralized role registry** (mirroring the discipline of
`relationStyle.ts`), treated as an **additive** architectural layer on top of the
kind-based card.

- The registry returns, for a category string: a **semantic token**, a **display
  label**, a **decorative shape cue**, and a **known/fallback** flag, over the
  D-ROLE vocabulary (`service`, `client`, `database`, `cache`, `observability`,
  `external`, `infra`, `shared`, `domain`).
- Role presentation uses **three** redundant channels: colour, a text badge, and a
  non-colour decorative cue.
- The decorative cue must **not**: change the card's layout box; move React Flow
  handles; use a `clip-path` that reduces the clickable area; obscure text; or
  become the only semantic cue. (A safe cue lives inside the existing padding —
  e.g. a corner tab, an inset top band, or an inner border treatment.)
- **Unknown non-empty** categories use a neutral role badge/cue; **missing**
  categories show no role badge and keep the polished kind-based card.
- Roles are **never inferred** from names, modules or dependency patterns — only
  read from the authored `attributes.category`.

---

## 6. Control and panel visual system

Define centralized tokens/primitives (in `styles.css`, consumed by the
components) for:

- compact pill button; segmented control; tab; badge/chip;
- overlay surface; inspector surface;
- input / select; focus-visible ring;
- and the `active` / `hover` / `disabled` / `pressed` states of each.

Use restrained solid/gradient surfaces and bounded shadows. **`backdrop-filter`
blur is not used in Issue 15** (D-BLUR).

Preserve native semantics even when styling changes:

- real buttons stay `<button>`; selects stay accessible `<select>` unless an
  explicitly justified, fully-accessible custom control replaces one;
- `<fieldset>`/`<legend>`/`<label>` stay correctly associated.

## 7. Typography and asset policy

Issue 15 uses a **tuned `system-ui` stack** (D-TYPE). No embedded `data:`-URI
font; no external font request. A future self-hosted font is a separate decision
requiring an explicit open-source licence, a local hashed WOFF2 asset, a
bundle-size budget, CSP verification, a system-ui fallback, and no external
request.

Typography requirements specify a coherent scale:

- project-title scale; tab/control scale; node-title scale;
- inspector heading / body / code scale;
- line-height; letter-spacing;
- a minimum readable **graph-label** size (so relation and node labels stay legible
  at normal zoom).

## 8. Layout spacing

Issue 15 may deliberately adjust baseline spacing to support the redesigned cards
(D-SPACING). Lock:

- deterministic spacing tokens by view class (or a single justified scalar), with
  horizontal/between-layer and vertical/node-node spacing separable;
- sufficient **horizontal** distance for routed relation labels;
- sufficient **vertical** distance between parallel hierarchy lanes;
- stable output across reloads; no random placement;
- no relayout from interaction state.

A one-time change to deterministic node dimensions and/or ELK spacing is allowed
and must update the layout tests. The redesign does **not** need to preserve the
old coordinates — only that the new coordinates are deterministic and
interaction-stable. Exact numeric spacing is implementation tuning selected from
real-browser evidence.

## 9. Layout determinism note

The single deterministic-retune allowance in Section 8 is the only layout change.
All existing determinism guarantees (stable geometry across reload and a fresh
context; no random seed; identical output for identical input) continue to hold,
now against the new baseline. Interaction state (hover/selection/search/
diagnostics/focus/inspector presentation) never changes spacing or geometry.

## 10. Issue-14 motion compatibility

Issue 15 must **not** animate discovered/structural relations merely because an
entity is selected. Issue 14 is preserved exactly:

- discovered and structural relations remain **static**;
- selection does **not** make an ineligible relation animation-eligible;
- `attributes.flow = "active"` remains the only persistent motion contract.

In generated views, selection may strengthen **static** cues only — stroke width,
opacity, marker, static dash/highlight, z-index — and introduces **no** continuous
movement. All authored-flow / stage transient motion belongs to Issue 16. Motion
on selected discovered edges requires an explicit amendment to Issue 14; it must
not be introduced here under the guise of a "refinement".

---

## 11. Visual acceptance evidence

Deterministic real-browser captures at:

- **1600×900, zoom 100%, DPR 1** (primary);
- **1440×900**;
- **one narrow supported viewport**.

Required captures:

1. workspace overview — redesigned shell, no inspector;
2. workspace overview — selected package, inspector open;
3. a dense public-API / type view;
4. a generated view with **no** authored categories;
5. an authored-**role** view;
6. the diagnostics / search / selected composition;
7. dark theme;
8. maintained light theme;
9. forced-colors evidence;
10. static-site rendering.

Acceptance verifies:

- the project title is visible;
- global / header / view / graph controls have a clear hierarchy;
- graph-local controls are overlays (not a full-width row);
- the inspector is structured into sections;
- card text is readable without zooming; cards are not cramped or clipped;
- relation labels and arrow paths remain readable;
- no horizontal page overflow;
- no private paths or external-reference identifiers appear anywhere.

Screenshots remain **validation artifacts** unless a separate README task approves
tracking them in the repository.

---

## 12. Phase plan

Each phase is independently reviewable and leaves the explorer correct. Each phase
**may modify source and tests**. **Committed embedded assets are rebuilt once, in
Phase 6**; no partial generated bundle is ever committed, so `check:dist` is
expected stale only during the intervening phases and is made green once at the
end. **Approval itself performs no implementation or publication.**

1. **Token system and application shell** — centralized tokens/primitives
   (Section 6), typography (Section 7), and the four-region shell scaffolding
   (Section 3, structure only). Pure CSS/markup; no behaviour change.
2. **Graph overlays, responsive workspace and inspector shell** — move fit/zoom/
   reset, edge/focus and legend into compact overlays (Section 3.C); the locked
   responsive inspector contract (Section 3.D); responsive/overflow behaviour.
3. **Deterministic node-card redesign** — new composition + enlarged deterministic
   boxes handed to ELK (Section 4); select exact per-category values within the
   D-CARD ranges; update layout tests; record final values in the Phase-3 report.
4. **Role/category presentation** — the additive role registry and card role cue
   (Section 5); neutral fallback; forced-colors verified.
5. **Inspector content redesign and layout-spacing tuning** — sectioned inspector
   (Section 13) and the one-time deterministic ELK spacing retune (Sections 8–9).
6. **Real-browser verification, documentation, one embedded rebuild, bookkeeping**
   — the visual evidence set (Section 11), `docs/` updates (role authoring; the
   redesigned shell), the single embedded rebuild + `check:dist` green, and status
   bookkeeping.

Issue 16 separately owns guided flows, stage behaviour and any motion/schema work.

---

## 13. Inspector content

A first-class, sectioned inspector built **only** from data CrateVista already
produces:

- header with the entity title;
- kind, role/category and provenance chips;
- qualified name / id in secondary typography;
- description / explanation when present;
- repository / source actions (existing);
- parent / children summary;
- incoming and outgoing relation groups;
- diagnostics;
- examples / docs where already available (schema `1.1` `View`/entity docs);
- the existing local source-viewer entry point.

Presentation:

- section headings; cards or bordered groups;
- collapsible long relation groups; bounded lists with a "show more" affordance;
- clear empty states;
- a sticky header **only if** verified not to obscure content.

Constraints:

- No source snippets, and no pretence that code references exist when the current
  document/server cannot provide them.
- Preserve the existing local source viewer and the static-site privacy boundary
  (source *locations* visible; source *contents* opt-in, unchanged).

---

## 14. Frontend quality gates (per CLAUDE.md)

```
npm run check:types      # generated ExplorerDocument types unchanged (no schema change)
npm run check:dist        # committed embedded frontend == fresh production build (Phase 6)
npm run lint && npm run test && npm run e2e
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-features
```

The built CSS/HTML is asserted to make **no** external asset request and the CSP is
unchanged. Layout-test baselines are updated to the new deterministic dimensions/
spacing in the phases that change them.

---

## 15. Acceptance checklist (kept synchronized with implementation)

Application shell (Phases 1–2)
- [ ] Global header shows the project title, compact search, and entity-kind filters; it carries no graph-local focus/edge/zoom controls and no view selection.
- [ ] View navigation is a dedicated tab row (the only place view selection lives) with a clear active state, horizontal overflow on narrow viewports (no uncontrolled multi-line wrap), and keyboard navigation.
- [ ] The graph owns the majority of the viewport; fit/zoom/reset (upper-left), edge/focus (upper-right) and legend (lower-left) are compact overlays; overlays never cover a selected node after fit-to-view; the canvas wash is restrained.
- [ ] The responsive inspector meets the locked contract: wide grid column (≈360px, clamp 320–410px, own scroll); medium overlay drawer (`min(410px,90vw)`, no relayout on open); narrow full-viewport panel (restores state on close); keyboard-safe with an accessible close; selection preserved; no ELK relayout from switching presentation; no horizontal page overflow.

Node card (Phase 3)
- [ ] Card dimensions are new deterministic per-category boxes computed in `nodeCards.ts` within the authorized D-CARD ranges, handed to ELK verbatim, and stable across hover/selection/search/diagnostics/focus; interaction never changes the box or requests relayout; final values recorded in the Phase-3 report + tests.
- [ ] The card composition (title, kind badge, optional role badge, one bounded description line, metrics/cues, state composition) is readable without zooming and never cramped or clipped.
- [ ] A generated entity with no authored category renders the new polished kind-based card (no revert to the old design).

Architectural role (Phase 4)
- [ ] A centralized role registry returns token/label/cue/known-status over the locked D-ROLE vocabulary; role presentation uses colour **and** a text badge **and** a non-colour cue.
- [ ] The role cue does not change the box, move handles, reduce the clickable area, obscure text, or become the sole cue; unknown non-empty categories fall back neutrally; missing categories show no role badge.
- [ ] Roles are never inferred automatically; only read from `attributes.category`.

Inspector (Phase 5)
- [ ] The inspector is sectioned (header, chips, qualified name, description, source actions, parent/children, incoming/outgoing groups, diagnostics, examples/docs, source-viewer entry) using only existing data, with collapsible long groups, bounded "show more" lists, and clear empty states.
- [ ] No source snippets/fabricated references; the local source viewer and static-site privacy boundary are preserved.

Control/panel system + typography (Phases 1, 5)
- [ ] Centralized tokens/primitives (pill button, segmented control, tab, chip, overlay/inspector surface, input/select, focus-visible ring) with active/hover/disabled/pressed states; native control semantics preserved.
- [ ] Surfaces use restrained solid/gradient + bounded shadows; **no `backdrop-filter` blur**.
- [ ] Typography uses a tuned `system-ui` stack with a coherent scale and a minimum readable graph-label size; **no** embedded `data:`-URI font and no external asset; CSP unchanged.

Layout spacing (Phase 5)
- [ ] Any ELK spacing retune is deterministic, gives routed labels horizontal room and parallel lanes vertical room, is stable across reloads, and updates the layout tests; no relayout from interaction state.

Issue-14 compatibility (all phases)
- [ ] Discovered/structural relations stay static; selection strengthens only static cues (width/opacity/marker/dash/z-index) and introduces no continuous movement; `attributes.flow="active"` remains the only persistent motion contract.

Verification / bookkeeping (Phase 6)
- [ ] The deterministic visual-evidence set (Section 11) is captured across the specified viewports/themes and verified against the acceptance points; no private paths or external-reference identifiers appear.
- [ ] `docs/` documents the redesigned shell and role authoring; the committed embedded frontend under `crates/cratevista-server/embedded/` is rebuilt **once**, in this phase, and matches a fresh production build (`check:dist` green).
- [ ] All frontend and Rust gates pass; `PRD/INDEX.md` and this checklist are synchronized; status set to "Implemented and verified" only after all gates pass.
