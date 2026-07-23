# Issue 16 — Guided flows and stage navigation

**Type:** enhancement (frontend behaviour + optional additive schema)
**Raised:** 2026-07-23
**Status:** draft — placeholder, not approved, not scheduled

This issue holds the **behavioural** capabilities split out of the Issue 15 visual
redesign. It is a draft placeholder: it is **not** approved and **not** to be
implemented. It exists so the moved scope has a home and so the architectural
boundaries it must respect are recorded before any design work begins.

**Depends on:** [Issue 15 — Explorer visual redesign](issue_15_explorer_visual_redesign.md) — which may prepare reusable
visual primitives (dim tokens, a shared membership helper, the role registry) that
this issue consumes without changing their semantics.

**PRD:** [`../PRD/issue_16_guided_flows_and_stage_navigation.md`](../PRD/issue_16_guided_flows_and_stage_navigation.md) (draft placeholder)

## Scope (moved from Issue 15)

- **Guided stage timeline behaviour** — a configured flow's ordered stages become a
  walkable, numbered progression, with keyboard navigation and clear active state.
- **Step-driven dim and fit** — selecting a step dims to and fits to that step's
  members, with **no relayout** (positions identical across step changes).
- **Interaction-driven edge motion** — any transient motion of the active
  selection/step set, layered on Issue 14's persistent contract.
- **Optional `Stage.description`** — a possible additive schema field for a
  per-step explanation (deferred; see below).
- **Any schema version change** required by the above.

## Architectural boundaries this issue must respect (recorded now)

These are load-bearing constraints, captured at split time so the eventual design
honours them:

- **Focus and stage are independent state dimensions.** The focus anchor /
  `focusmode` and the active stage are separate. **Stage selection never writes or
  rewrites `focus` / `focusmode`.**
- **Dim reuse is visual only.** Stage dim may reuse the visual dim **tokens** and a
  shared membership helper, but it must **not** reuse the anchor-based focus
  *semantics*.
- **Explicit visual priority.** Selection, search, diagnostics, focus and stage
  have an explicit, documented visual priority order.
- **No relayout on step change.** Step changes never request relayout; node
  positions are identical across step changes.
- **Independent URL/history.** Stage state and focus state are independent in the
  URL and in Back/Forward behaviour.
- **Stage description source.** A stage description is **not** to be derived from
  unrelated view docs or member descriptions. For the initial design, use the
  stage **title, ordinal and member list** only, and **defer `Stage.description`**.
  A future optional `Stage.description` requires its own explicit schema decision
  and versioning evidence (additive, back-compatible, its own `SchemaVersion`
  bump).

## Motion boundary vs Issue 14 (recorded now)

- Issue 14 remains authoritative: discovered/structural relations are static, and
  `attributes.flow = "active"` is the persistent motion contract.
- Any transient motion introduced here (e.g. animating the active step's members)
  is this issue's to design **and to justify**. Motion on **selected discovered
  edges** in particular requires an **explicit amendment to Issue 14**, not an
  implicit "refinement" folded into this issue.

## Status

Draft placeholder. Do not approve. Do not implement. A full PRD is written only
when this issue is explicitly selected for design.
