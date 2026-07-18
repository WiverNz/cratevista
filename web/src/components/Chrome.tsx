// Toolbar, view tabs, search, kind filters, stage bar.
import type { KeyboardEvent } from "react";
import { useApp, useProjection, useUi } from "../app/AppContext.tsx";
import { documentToGraph } from "../adapter/adapter.ts";
import { searchEntities } from "../state/selectors.ts";
import { localized } from "../types/index.ts";
import type { EdgeMode } from "../state/url.ts";

export function Toolbar() {
  const { store } = useApp();
  const edgeMode = useUi((s) => s.edgeMode);
  const focusMode = useUi((s) => s.focusMode);
  const selection = useUi((s) => s.selection);
  return (
    <div className="cv-toolbar" role="toolbar" aria-label="Graph controls">
      <Search />
      <KindFilters />
      <label className="cv-control">
        <span className="cv-muted">Edges</span>
        <select
          aria-label="Edge visibility"
          value={edgeMode}
          onChange={(e) => store.getState().setEdgeMode(e.target.value as EdgeMode)}
        >
          <option value="all">All</option>
          <option value="related">Related</option>
          <option value="hidden">Hidden</option>
        </select>
      </label>
      <button
        type="button"
        className="cv-control"
        aria-pressed={focusMode}
        onClick={() => {
          const id = selection.kind === "entity" ? selection.id : null;
          store.getState().setFocus(id, !focusMode);
        }}
      >
        {focusMode ? "Related only: on" : "Related only: off"}
      </button>
    </div>
  );
}

export function Search() {
  const { store, model } = useApp();
  const query = useUi((s) => s.search);
  const focusMode = useUi((s) => s.focusMode);
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
                    store.getState().setFocus(id, focusMode);
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
