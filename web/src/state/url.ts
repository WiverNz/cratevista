// Query-string URL state (shareable, back/forward-restorable). Only durable
// selection/filter state is serialized — never hover, viewport, inspector
// expansion, or transient loading state.
export type EdgeMode = "all" | "related" | "hidden";

/** Focus emphasis style. Only two values exist; "no focus" is the ABSENCE of a
 *  `focus` anchor, never a `focusmode` value. `"hide"` is the default and is never
 *  serialized (a bare `focus=<id>` means hide). */
export type FocusMode = "hide" | "dim";

export interface UrlState {
  view?: string;
  entity?: string;
  relation?: string;
  q?: string;
  kinds?: string[];
  focus?: string;
  /** Only ever `"dim"` here — `"hide"` is the omitted default and is never stored,
   *  and `focusmode` is meaningless (and dropped) without a `focus` anchor. */
  focusmode?: FocusMode;
  edges?: EdgeMode;
  stage?: string;
}

const EDGE_MODES: readonly EdgeMode[] = ["all", "related", "hidden"];

export function parseUrlState(search: string): UrlState {
  const p = new URLSearchParams(search);
  const state: UrlState = {};
  const view = p.get("view");
  if (view) state.view = view;
  // Only one of entity/relation may be active; prefer relation when valid.
  const relation = p.get("relation");
  const entity = p.get("entity");
  if (relation) state.relation = relation;
  else if (entity) state.entity = entity;
  const q = p.get("q");
  if (q) state.q = q;
  const kinds = p.get("kinds");
  if (kinds) state.kinds = kinds.split(",").filter(Boolean);
  const focus = p.get("focus");
  if (focus) state.focus = focus;
  // `focusmode` is meaningful ONLY with an anchor, and only `"dim"` is stored —
  // `"hide"`, `"all"`, and any unknown value all normalize to the hide default
  // (absent). A `focusmode` without a `focus` is dropped entirely.
  const focusmode = p.get("focusmode");
  if (focus && focusmode === "dim") state.focusmode = "dim";
  const edges = p.get("edges");
  if (edges && (EDGE_MODES as readonly string[]).includes(edges))
    state.edges = edges as EdgeMode;
  const stage = p.get("stage");
  if (stage) state.stage = stage;
  return state;
}

export function serializeUrlState(state: UrlState): string {
  const p = new URLSearchParams();
  if (state.view) p.set("view", state.view);
  // Enforce mutual exclusivity in the URL too (relation wins).
  if (state.relation) p.set("relation", state.relation);
  else if (state.entity) p.set("entity", state.entity);
  if (state.q) p.set("q", state.q);
  if (state.kinds && state.kinds.length > 0) p.set("kinds", state.kinds.join(","));
  if (state.focus) p.set("focus", state.focus);
  // Serialize `focusmode` only as `dim` and only alongside an anchor; the hide
  // default is omitted so existing `focus=<id>` URLs stay byte-for-byte identical.
  if (state.focus && state.focusmode === "dim") p.set("focusmode", "dim");
  if (state.edges && state.edges !== "all") p.set("edges", state.edges);
  if (state.stage) p.set("stage", state.stage);
  const s = p.toString();
  return s ? `?${s}` : "";
}
