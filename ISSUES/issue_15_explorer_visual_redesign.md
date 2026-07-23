# Issue 15 — Explorer visual redesign

**Type:** enhancement (frontend visual redesign phase)
**Raised:** 2026-07-23
**Status:** PRD approved — ready for phased implementation

**Intent:** a focused, end-to-end **visual redesign** of the CrateVista explorer
so it reads as one deliberate, modern, accessible product rather than a
utilitarian tool. The redesign covers the application shell and information
hierarchy, global navigation, graph-local controls, the node card, the inspector,
the legend/overlay panels, and the presentation of authored architectural role —
plus the responsive and accessibility behaviour, deterministic layout, visual
verification, and the single embedded-frontend rebuild that ship them.

This issue is **presentation only**. It does not change graph semantics, focus
semantics, motion eligibility, or the schema. It may prepare centralized visual
primitives (tokens, control primitives, a role registry) that a later issue
consumes, but it introduces no new behavioural state.

**Primary area:** `web/` (explorer SPA). No Rust, schema, generated-type or
server change. One deliberate, **one-time** deterministic change to node-card
dimensions and/or ELK layout spacing is in scope (it updates the layout tests) —
this is a determinism-preserving retune, not a behavioural change.

**Scope split:** guided stage-timeline behaviour, step-driven dim/fit,
interaction-driven edge motion, any transient motion, an optional
`Stage.description`, and **any schema version change** are **out of scope** and are
proposed separately in Issue 16.

**PRD:** [`../PRD/issue_15_explorer_visual_redesign.md`](../PRD/issue_15_explorer_visual_redesign.md)
**Follow-up:** [`issue_16_guided_flows_and_stage_navigation.md`](issue_16_guided_flows_and_stage_navigation.md) (draft placeholder; not approved)

## Summary

The explorer is structurally complete and correct (Issues 06–10, 14) but visually
under-developed. Its controls are small rectangles, its panels are flat, its node
cards are cramped single surfaces, its inspector is an undifferentiated field
list, and it colours nodes by Rust kind alone while ignoring the architectural
**role** an author can already assign (`[[override]].category` →
`attributes["category"]`). The result is low visual hierarchy and poor
first-glance comprehension.

**Current shell baseline (accurate).** The existing components already provide the
seams the redesign needs — project metadata, search, kind filters, `ViewTabs`,
graph controls, the legend and the inspector. However, the current application
shell is organized primarily as **stacked full-width control rows**. Issue 15
**reorganizes those existing capabilities into the approved four-region shell**;
it does not add missing capability, and the redesigned shell does not exist yet.

This issue delivers a cohesive visual system without weakening any existing
guarantee:

- **Application shell.** Reorganize the existing capabilities into a clear
  four-region hierarchy — a global header, a dedicated view-navigation row, a
  graph workspace that owns the viewport with compact control **overlays**, and a
  responsive right-side inspector — replacing the current stack of full-width
  control rows.
- **Node card.** A redesigned, deterministic card that has room for a title, kind
  badge, an optional architectural-role badge, one bounded description line, and
  compact metrics/cues — with new deterministic dimensions computed centrally and
  handed to ELK verbatim, stable across every interaction state.
- **Inspector.** A sectioned, scannable, responsive inspector built only from data
  CrateVista already produces.
- **Architectural role presentation.** A centralized, additive role registry that
  gives an authored `attributes.category` a colour, a text badge and a non-colour
  decorative cue, with a neutral fallback — never inferred automatically.
- **Control & panel system.** Centralized tokens and primitives (pill button,
  segmented control, tab, chip, overlay/inspector surface, input/select,
  focus-visible ring) with restrained solid/gradient surfaces and bounded shadows.
- **Verification.** Deterministic real-browser visual evidence across viewports
  and themes, plus the single embedded rebuild and documentation.

## What already exists (extend, do not duplicate or regress)

- Progressive-disclosure node cards and deterministic per-category card boxes
  (`web/src/model/nodeCards.ts`: `CardLevel`, `cardSize`, `nodeCategory`), the
  kind style registry (`web/src/adapter/kindStyle.ts`), and the design tokens +
  depth/forced-colors/reduced-motion handling in `web/src/styles.css`.
- The existing (stacked) shell and its seams: `ViewTabs`, `StageBar`, focus
  controls, search and kind filters (`web/src/components/Chrome.tsx`,
  `Panels.tsx`).
- The deterministic ELK worker with a single spacing scalar
  (`web/src/layout/elk.worker.ts`, `web/src/layout/client.ts`).
- The authored-role seam already reaching the frontend: `[[override]].category`
  → `attributes["category"]` (repo audit: the only meaningful value present today
  is `service`).
- The real-browser E2E harness and the committed embedded frontend + `check:dist`
  drift guard.

## Non-goals

No new graph engine, 3D, physics, particles or WebGL. No change to graph
semantics, relation meaning, focus/anchor semantics, or motion eligibility. **No
new continuous motion of any kind** (all motion work is Issue 16). No stage/step
navigation behaviour (Issue 16). No auto-inference of role from names, modules or
dependencies. **No `backdrop-filter` blur.** No embedded CSS `data:`-URI font. No
external font, script, style or asset (CSP stays exactly as shipped). No schema
change and no `SchemaVersion` bump. No language selector or other capability
CrateVista does not already support. No source snippets or fabricated code
references beyond what the document/server already provides.

## Acceptance (high level; the PRD owns the concrete checklist)

- The existing capabilities are reorganized into a clear four-region shell with a
  visible project title, graph-local controls as compact overlays (not full-width
  rows), and a responsive sectioned inspector; no horizontal page overflow at
  supported viewports.
- Node cards are readable without zooming, never cramped or clipped, with new
  deterministic dimensions handed to ELK verbatim and stable across hover,
  selection, search, diagnostics and focus.
- Authored architectural role renders by colour **and** badge **and** a non-colour
  cue, with a neutral fallback; a generated entity with no authored role still
  gets the new polished kind-based card.
- Issue 14 is preserved exactly: discovered/structural relations stay static;
  selection strengthens only static cues (width/opacity/marker/dash/z-index) and
  introduces **no** continuous movement; `attributes.flow="active"` remains the
  only persistent motion contract.
- Determinism, accessibility (colour never sole carrier; forced-colors; native
  control semantics), and static/live parity all hold; the committed embedded
  frontend matches a fresh build.

## Deliverable

`PRD/issue_15_explorer_visual_redesign.md` — the implementation-ready PRD, **Approved
— safe to implement**, with all load-bearing decisions locked. Implementation
proceeds through the PRD's six phased steps; this issue does not itself perform
implementation.
