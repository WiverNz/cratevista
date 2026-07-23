// Global header, view tabs, workspace controls, search, kind filters, stage bar.
//
// Four-region shell (Issue 15, Phase 1): the global header carries only
// project-level concerns (title + search + kind filters); graph-local edge/focus
// controls live in the workspace, not the header. View selection lives only in
// the dedicated view-navigation row (`ViewTabs`). The final overlay geometry of
// the workspace controls is Phase 2 — Phase 1 only relocates them into a clearly
// named workspace-control slot.
import type { KeyboardEvent } from "react";
import { useApp, useProjection, useUi } from "../app/AppContext.tsx";
import { documentToGraph } from "../adapter/adapter.ts";
import { searchEntities } from "../state/selectors.ts";
import { localized } from "../types/index.ts";
import type { EdgeMode } from "../state/url.ts";

/**
 * The global header: the project's identity and the two project-level controls
 * (search + kind filters). It deliberately carries **no** graph-local controls
 * (edge visibility, focus) and **no** view selection — those belong to the
 * workspace and the view-navigation row respectively.
 */
export function GlobalHeader() {
  return (
    <div className="cv-header">
      <ProjectTitle />
      <div className="cv-header-controls">
        <Search />
        <KindFilters />
      </div>
    </div>
  );
}

/**
 * The visible project title, derived ONLY from the loaded document's authoritative
 * project name (never the URL, path, hostname or repository metadata). Falls back
 * to the plain product name when the project name is missing or whitespace-only,
 * matching the browser-tab fallback. It is display text, never an editable field.
 */
export function ProjectTitle() {
  const { model } = useApp();
  const name = (model.document.project?.name ?? "").trim();
  const title = name || "CrateVista";
  return (
    <h1 className="cv-project-title" title={title}>
      {title}
    </h1>
  );
}

/**
 * Graph-local controls (edge visibility + focus). Kept out of the global header so
 * project-level and graph-local concerns are visually separate. Phase 1 places
 * this in a temporary in-workspace slot; Phase 2 gives it the final overlay
 * geometry. It keeps the "Graph controls" toolbar role its controls had before.
 */
export function WorkspaceControls() {
  const { store } = useApp();
  const edgeMode = useUi((s) => s.edgeMode);
  return (
    <div className="cv-workspace-controls" role="toolbar" aria-label="Graph controls">
      <label className="cv-field">
        <span className="cv-muted">Edges</span>
        <select
          className="cv-select"
          aria-label="Edge visibility"
          value={edgeMode}
          onChange={(e) => store.getState().setEdgeMode(e.target.value as EdgeMode)}
        >
          <option value="all">All</option>
          <option value="related">Related</option>
          <option value="hidden">Hidden</option>
        </select>
      </label>
      <FocusControls />
    </div>
  );
}

/**
 * Focus controls: a single group of three real buttons over the ONE focus state.
 *
 * - **Hide unrelated** — reduce the graph to the anchor's neighbourhood (legacy
 *   "related only"); normalized URL is a bare `focus=<id>`.
 * - **Dim unrelated** — keep the whole graph, de-emphasise everything unrelated;
 *   normalized URL is `focus=<id>&focusmode=dim`.
 * - **Clear focus** — remove the anchor and any focus mode; back to the full graph.
 *   Never writes `focusmode=all`.
 *
 * The anchor is the selected entity, or the existing focus anchor when nothing is
 * selected. Hide/Dim are disabled with no anchor to focus; Clear is disabled with
 * no active focus. `aria-pressed` exposes the current mode.
 */
function FocusControls() {
  const { store } = useApp();
  const focusMode = useUi((s) => s.focusMode);
  const focusId = useUi((s) => s.focusId);
  const selection = useUi((s) => s.selection);
  const anchorId = selection.kind === "entity" ? selection.id : focusId;
  const active = focusId != null;

  return (
    <div className="cv-focus-controls" role="group" aria-label="Focus">
      <button
        type="button"
        className="cv-control"
        aria-pressed={active && focusMode === "hide"}
        disabled={!anchorId}
        title="Reduce the graph to the selected entity and its immediate neighbours"
        onClick={() => anchorId && store.getState().setFocus(anchorId, "hide")}
      >
        Hide unrelated
      </button>
      <button
        type="button"
        className="cv-control"
        aria-pressed={active && focusMode === "dim"}
        disabled={!anchorId}
        title="Keep the whole graph but dim everything unrelated to the selected entity"
        onClick={() => anchorId && store.getState().setFocus(anchorId, "dim")}
      >
        Dim unrelated
      </button>
      <button
        type="button"
        className="cv-control"
        disabled={!active}
        title="Show the complete graph again"
        onClick={() => store.getState().clearFocus()}
      >
        Clear focus
      </button>
    </div>
  );
}

export function Search() {
  const { store, model } = useApp();
  const query = useUi((s) => s.search);
  const focusMode = useUi((s) => s.focusMode);
  const focusId = useUi((s) => s.focusId);
  const results = query.trim() ? searchEntities(model, query) : [];
  return (
    <div className="cv-search">
      <input
        type="search"
        aria-label="Search entities"
        placeholder="Search entities…"
        value={query}
        onChange={(e) => store.getState().setSearch(e.target.value)}
      />
      {results.length > 0 && (
        <ul className="cv-search-results" role="listbox" aria-label="Search results">
          {results.slice(0, 20).map((id) => {
            const entity = model.entityById.get(id)!;
            return (
              <li key={id}>
                <button
                  type="button"
                  role="option"
                  aria-selected="false"
                  onClick={() => {
                    store.getState().selectEntity(id);
                    // Re-anchor an ALREADY-active focus onto the chosen result
                    // (preserving its hide/dim style); never force focus on when
                    // none is active.
                    if (focusId) store.getState().setFocus(id, focusMode);
                  }}
                >
                  {localized(entity.label)}{" "}
                  <span className="cv-muted">{entity.qualified_name}</span>
                </button>
              </li>
            );
          })}
        </ul>
      )}
    </div>
  );
}

export function KindFilters() {
  const { store } = useApp();
  const projection = useProjection();
  const active = useUi((s) => s.kindFilters);
  const kinds = projection?.kinds ?? [];
  if (kinds.length === 0) return null;
  return (
    <fieldset className="cv-filters">
      <legend className="cv-muted">Kinds</legend>
      {kinds.map((kind) => (
        <label key={kind} className="cv-chip">
          <input
            type="checkbox"
            checked={active.has(kind)}
            onChange={() => store.getState().toggleKind(kind)}
          />
          {kind}
        </label>
      ))}
      {active.size > 0 && (
        <button type="button" onClick={() => store.getState().setKindFilters([])}>
          Clear
        </button>
      )}
    </fieldset>
  );
}

/** DOM id of a view tab (referenced by the graph tabpanel's aria-labelledby). */
export function viewTabId(viewId: string): string {
  return `cv-tab-${viewId.replace(/[^a-zA-Z0-9_-]/g, "-")}`;
}

/** The id of the single tabpanel the tabs control. */
export const GRAPH_PANEL_ID = "cv-graph-panel";

export function ViewTabs() {
  const { store, model } = useApp();
  const activeViewId = useUi((s) => s.activeViewId);

  const activate = (view: (typeof model.views)[number]) => {
    const sel = store.getState().selection;
    const keep =
      sel.kind === "entity" &&
      documentToGraph(model, view, {}).nodes.some((n) => n.id === sel.id);
    store.getState().switchView(view.id, { keepSelection: keep });
  };

  // Roving tabindex + Left/Right/Home/End. Activation follows focus (documented
  // model: automatic activation, as the panel is cheap to reproject).
  const onKeyDown = (event: KeyboardEvent<HTMLButtonElement>, index: number) => {
    const keys = ["ArrowLeft", "ArrowRight", "Home", "End"];
    if (!keys.includes(event.key)) return;
    event.preventDefault();
    const last = model.views.length - 1;
    let next: number;
    if (event.key === "ArrowLeft") next = index === 0 ? last : index - 1;
    else if (event.key === "ArrowRight") next = index === last ? 0 : index + 1;
    else if (event.key === "Home") next = 0;
    else next = last;
    const target = model.views[next];
    activate(target);
    const el = document.getElementById(viewTabId(target.id));
    el?.focus();
  };

  return (
    <div role="tablist" aria-label="Views" className="cv-tabs">
      {model.views.map((view, index) => {
        const selected = view.id === activeViewId;
        return (
          <button
            key={view.id}
            id={viewTabId(view.id)}
            role="tab"
            aria-selected={selected}
            aria-controls={GRAPH_PANEL_ID}
            tabIndex={selected ? 0 : -1}
            className={selected ? "cv-tab cv-tab-active" : "cv-tab"}
            onClick={() => activate(view)}
            onKeyDown={(e) => onKeyDown(e, index)}
          >
            {localized(view.title)}
          </button>
        );
      })}
    </div>
  );
}

export function StageBar() {
  const { store, model } = useApp();
  const activeViewId = useUi((s) => s.activeViewId);
  const activeStage = useUi((s) => s.activeStage);
  const view = activeViewId ? model.viewById.get(activeViewId) : undefined;
  const stages = view?.stages ?? [];
  if (stages.length === 0) return null;
  const ordered = [...stages].sort((a, b) => a.order - b.order);
  return (
    <div className="cv-stages" role="tablist" aria-label="Stages">
      {ordered.map((stage) => {
        const selected = stage.id === activeStage;
        return (
          <button
            key={stage.id}
            role="tab"
            aria-selected={selected}
            onClick={() => store.getState().setStage(selected ? null : stage.id)}
          >
            {localized(stage.title)}
          </button>
        );
      })}
    </div>
  );
}
