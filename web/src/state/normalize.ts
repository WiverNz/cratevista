// The single URL-state normalization boundary. Applied on initial load AND on
// popstate, so the store can only ever hold valid durable state.
//
// Never normalizes transient state (viewport, hover, inspector expansion, source
// loading, reduced-mode BFS internals) — those are not part of `UrlState`.
import type { DocumentModel } from "../model/model.ts";
import type { EdgeMode, UrlState } from "./url.ts";

const EDGE_MODES: readonly string[] = ["all", "related", "hidden"];

/** View resolution: valid requested → `view:workspace-overview` → first view. */
export function chooseView(requested: string | undefined, model: DocumentModel): string {
  if (requested && model.viewById.has(requested)) return requested;
  if (model.viewById.has("view:workspace-overview")) return "view:workspace-overview";
  return model.views[0]?.id ?? "";
}

/** Returns only valid durable state for the given document. */
export function normalizeUrlState(raw: UrlState, model: DocumentModel): UrlState {
  const view = chooseView(raw.view, model);
  const out: UrlState = {};
  if (view) out.view = view;

  // Selection: relation wins when valid; else a valid entity; else none.
  const relationValid = !!raw.relation && model.relationById.has(raw.relation);
  const entityValid = !!raw.entity && model.entityById.has(raw.entity);
  if (relationValid) out.relation = raw.relation;
  else if (entityValid) out.entity = raw.entity;

  // Stage: keep only when the selected view defines it.
  const activeView = view ? model.viewById.get(view) : undefined;
  const stageIds = new Set((activeView?.stages ?? []).map((s) => s.id));
  if (raw.stage && stageIds.has(raw.stage)) out.stage = raw.stage;

  // Kinds: drop unknown, dedupe, deterministic order.
  if (raw.kinds && raw.kinds.length > 0) {
    const known = model.entitiesByKind;
    const kinds = [...new Set(raw.kinds.filter((k) => known.has(k)))].sort();
    if (kinds.length > 0) out.kinds = kinds;
  }

  // Focus: only an existing entity. `focusmode` is carried ONLY as `dim` and ONLY
  // with a valid anchor — a stale/missing focus drops any focus mode with it, an
  // unknown mode (incl. `all`) degrades to the omitted hide default, and a
  // `focusmode` without a focus is never kept.
  if (raw.focus && model.entityById.has(raw.focus)) {
    out.focus = raw.focus;
    if (raw.focusmode === "dim") out.focusmode = "dim";
  }

  // Edges: only a known mode; `all` is the default and is not serialized.
  if (raw.edges && EDGE_MODES.includes(raw.edges) && raw.edges !== "all") {
    out.edges = raw.edges as EdgeMode;
  }

  // Query: drop whitespace-only.
  if (raw.q && raw.q.trim() !== "") out.q = raw.q;

  return out;
}

/** True when two durable states differ ONLY by the search query (→ replaceState
 *  rather than a new history entry for high-frequency typing). */
export function differsOnlyBySearch(a: UrlState, b: UrlState): boolean {
  if ((a.q ?? "") === (b.q ?? "")) return false;
  const rest = (s: UrlState) =>
    JSON.stringify({
      view: s.view ?? null,
      entity: s.entity ?? null,
      relation: s.relation ?? null,
      kinds: [...(s.kinds ?? [])].sort(),
      focus: s.focus ?? null,
      focusmode: s.focusmode ?? null,
      edges: s.edges ?? null,
      stage: s.stage ?? null,
    });
  return rest(a) === rest(b);
}
