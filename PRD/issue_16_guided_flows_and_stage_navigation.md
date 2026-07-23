# PRD — Issue 16: Guided flows and stage navigation

**Status:** Draft — placeholder, not approved, not scheduled
**Issue:** [`../ISSUES/issue_16_guided_flows_and_stage_navigation.md`](../ISSUES/issue_16_guided_flows_and_stage_navigation.md)
**Primary area:** `web/` (explorer SPA) + optional additive `cratevista-schema` / `cratevista-config`
**Depends on:** [Issue 15 — Explorer visual redesign](issue_15_explorer_visual_redesign.md)

> This is a **placeholder** capturing the scope moved out of Issue 15 and the
> boundaries that scope must respect. It is intentionally not implementation-ready.
> A full, phased PRD is written only when Issue 16 is explicitly selected for
> design. Nothing here is approved.

---

## 1. Scope (moved from Issue 15)

1. **Guided stage timeline.** Render a configured flow's ordered `Stage`s as a
   walkable, numbered progression (e.g. `1 → 2 → 3`), keyboard-navigable, with a
   clear active state and a "clear step" affordance.
2. **Step-driven dim and fit.** Selecting a step dims to and fits to that step's
   members (membership via the existing `attributes.stage` → `GraphNode.stage`
   seam), with **no relayout**.
3. **Interaction-driven / transient edge motion.** Any motion of the active
   selection or active-step set, layered on Issue 14's persistent contract and its
   view-wide suppression, reduced-motion, zoom-floor and count fail-safe.
4. **Optional `Stage.description`.** A possible additive schema field for a
   per-step explanation — **deferred** (Section 3).

## 2. Architectural boundaries (locked at split time; carried from the issue)

- Focus (`focus` / `focusmode`) and active stage are **independent** state
  dimensions. Stage selection **never** writes or rewrites `focus` / `focusmode`.
- Stage dim reuses the visual dim **tokens** and a shared membership helper only —
  **not** the anchor-based focus semantics.
- Selection, search, diagnostics, focus and stage have an explicit, documented
  visual **priority order**.
- Step changes never request relayout; positions are identical across steps.
- Stage and focus state are **independent** in the URL and in Back/Forward.
- No continuous motion on discovered/structural edges without an explicit **Issue
  14 amendment**; `attributes.flow = "active"` stays the persistent contract.

## 3. Stage description policy (deferred)

- The initial design uses stage **title, ordinal and member list** only.
- A stage description is **not** derived from unrelated view docs or member
  descriptions.
- A dedicated `Stage.description` (+ `[[flow.stage]].description` + overlay
  mapping) is a **future, optional** additive field requiring its own explicit
  schema decision, back-compatible additive design, and its own `SchemaVersion`
  bump with versioning evidence. It is not assumed by this placeholder.

## 4. Reused seams (no semantic change)

Issue 16 is expected to consume — without changing — the seams that already exist:
schema `Stage` / config `[[flow.stage]]` / `attributes.stage` membership; the
Issue-14 motion decision point (`relationStyle.ts`) and its policy/threshold; the
Issue-14 dim **tokens**; the deterministic ELK layout (no per-step relayout); and
any visual primitives Issue 15 introduces (role registry, control primitives).

## 5. Status

Draft placeholder. Do not approve. Do not implement. When selected, expand this
into a phased, implementation-ready PRD with its own load-bearing decisions and an
acceptance checklist, and enumerate any schema/versioning change explicitly.
