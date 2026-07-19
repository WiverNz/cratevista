// App-wide context (loaded model + store + artifact availability) and the
// projection / layout / URL-sync hooks that wire the pure core to React.
import { createContext, useContext, useEffect, useMemo, useRef, useState } from "react";
import type { StoreApi } from "zustand";
import { useStore } from "zustand";

import type { DocumentModel } from "../model/model.ts";
import {
  documentToGraph,
  type Graph,
} from "../adapter/adapter.ts";
import {
  legendForGraph,
  relationLegendForGraph,
  kindsInGraph,
  searchEntities,
  type LegendEntry,
  type RelationLegendEntry,
} from "../state/selectors.ts";
import { reduceGraph } from "../adapter/reduce.ts";
import { cardSize } from "../model/nodeCards.ts";
import { count, measure, record } from "./perf.ts";
import type { GraphNode } from "../adapter/adapter.ts";
import {
  parseUrlState,
  serializeUrlState,
  type UrlState,
} from "../state/url.ts";
import {
  toUrlState,
  type UiStore,
} from "../state/store.ts";
import { chooseView, differsOnlyBySearch, normalizeUrlState } from "../state/normalize.ts";
import { layoutCacheKey } from "../layout/cache.ts";
import type { LayoutEngine } from "../layout/client.ts";
import type { SourceClient } from "../api/source.ts";
import type { Point, PositionedNode } from "../layout/types.ts";
import type { View } from "../types/index.ts";
import type { GenerationReport, DiagnosticsReport } from "../types/runtime.ts";

/**
 * Default large-graph visible-node budget: above this many **projected** nodes,
 * the canvas renders a bounded neighbourhood instead of the whole graph.
 *
 * Benchmarked and retained, not provisional — see
 * `docs/benchmarks/prd-07-large-graph.md`. Rendering 1,212 nodes in full stays
 * usable (~1.0 s to first usable graph, 206 ms click-to-inspector), while 3,232
 * costs 3.7 s and 3.4 s per selection; reduced mode holds first-usable near
 * 0.5 s regardless of document size.
 *
 * This is a **frontend engineering default**, configurable via `<App budget>`.
 * It is not a schema or server limit, and not a guarantee that every machine
 * behaves identically — the measurements come from one fast desktop.
 */
export const DEFAULT_LARGE_GRAPH_BUDGET = 1500;

export interface AppData {
  store: StoreApi<UiStore>;
  model: DocumentModel;
  generation: GenerationReport | null;
  generationAvailable: boolean;
  diagnostics: DiagnosticsReport | null;
  diagnosticsAvailable: boolean;
  /** Large-graph reduced-mode budget. */
  budget: number;
  /** Opt-in source-content client (`GET /api/source`) in server mode, or `null` in
   *  static mode, where there is no `/api/source` capability at all. */
  sourceClient: SourceClient | null;
}

const AppContext = createContext<AppData | null>(null);

export function AppProvider(props: { value: AppData; children: React.ReactNode }) {
  return <AppContext.Provider value={props.value}>{props.children}</AppContext.Provider>;
}

export function useApp(): AppData {
  const ctx = useContext(AppContext);
  if (!ctx) throw new Error("useApp must be used within AppProvider");
  return ctx;
}

export function useUi<T>(selector: (s: UiStore) => T): T {
  const { store } = useApp();
  return useStore(store, selector);
}

export interface Projection {
  view: View;
  graph: Graph;
  legend: LegendEntry[];
  /** Relation kinds present in this view, with their central-registry styles. */
  relationLegend: RelationLegendEntry[];
  kinds: string[];
  cacheKey: string;
  /** Reduced (large-graph) mode is active. */
  reduced: boolean;
  fullCount: number;
  visibleCount: number;
  /** All projected nodes before reduction (for the GraphList alternative). */
  allNodes: GraphNode[];
}

/** Pure projection derived from (model, UI state). In normal mode the cache key
 *  is unchanged by selection (so selection never relayouts); in reduced mode the
 *  selection/search focus changes the visible set (intended recenter). */
export function useProjection(): Projection | null {
  const { model, budget } = useApp();
  const activeViewId = useUi((s) => s.activeViewId);
  const kindFilters = useUi((s) => s.kindFilters);
  const focusMode = useUi((s) => s.focusMode);
  const focusId = useUi((s) => s.focusId);
  const edgeMode = useUi((s) => s.edgeMode);
  const activeStage = useUi((s) => s.activeStage);
  const expanded = useUi((s) => s.expandedNeighborhoods);
  const language = useUi((s) => s.language);
  const selection = useUi((s) => s.selection);
  const search = useUi((s) => s.search);
  const renderFull = useUi((s) => s.renderFullGraph);

  return useMemo(() => {
    const view = activeViewId ? model.viewById.get(activeViewId) : undefined;
    if (!view) return null;
    const base = {
      kindFilter: kindFilters.size > 0 ? kindFilters : null,
      relatedOnly: focusMode,
      focusId,
      lang: language,
    };
    const fullGraph = measure("cv.adapter.project", () => documentToGraph(model, view, base), {
      nodes: model.document.entities.length,
    });

    let graph = fullGraph;
    let reduced = false;
    let visibleCount = fullGraph.nodes.length;
    if (!renderFull && fullGraph.nodes.length > budget) {
      const selectedId = selection.kind === "entity" ? selection.id : null;
      const searchResultId = search.trim() ? (searchEntities(model, search)[0] ?? null) : null;
      const rr = measure(
        "cv.reduce",
        () =>
          reduceGraph({
            nodeIds: fullGraph.nodes.map((n) => n.id),
            edges: fullGraph.edges,
            budget,
            selectedId,
            searchResultId,
            defaultFocusId: (view.default_focus as string | null | undefined) ?? null,
            expanded: [...expanded],
          }),
        { fullNodes: fullGraph.nodes.length, budget },
      );
      reduced = rr.reduced;
      visibleCount = rr.visibleCount;
      graph = measure("cv.adapter.project.reduced", () =>
        documentToGraph(model, view, { ...base, visibleIds: rr.visibleIds }),
      );
    }
    count("fullNodes", fullGraph.nodes.length);
    count("visibleNodes", graph.nodes.length);
    count("visibleEdges", graph.edges.length);
    count("reduced", reduced ? 1 : 0);

    const cacheKey = layoutCacheKey({
      identity: model.identity,
      viewId: view.id,
      kinds: [...kindFilters],
      focusMode,
      focusId: focusId ?? null,
      relatedOnly: focusMode,
      edgeMode,
      expanded: [...expanded],
      stage: activeStage,
      nodeIds: graph.nodes.map((n) => n.id),
      edgeIds: graph.edges.map((e) => e.id),
    });
    return {
      view,
      graph,
      legend: legendForGraph(graph),
      relationLegend: relationLegendForGraph(graph),
      kinds: kindsInGraph(graph),
      cacheKey,
      reduced,
      fullCount: fullGraph.nodes.length,
      visibleCount,
      allNodes: fullGraph.nodes,
    };
  }, [
    model,
    budget,
    activeViewId,
    kindFilters,
    focusMode,
    focusId,
    edgeMode,
    activeStage,
    expanded,
    language,
    selection,
    search,
    renderFull,
  ]);
}

/** Empty route map shared for every non-current layout, so a stale layout can
 *  never hand out routes that belong to a superseded projection. */
const EMPTY_ROUTES: ReadonlyMap<string, Point[]> = new Map();

export type LayoutState = {
  status: "idle" | "loading" | "ok" | "error";
  positions: Map<string, PositionedNode>;
  /** ELK route polylines keyed by relation id, for the *current* layout only.
   *  Empty while a layout is pending/errored so routes are never paired with a
   *  newer projection's node positions. */
  routes: ReadonlyMap<string, Point[]>;
  error?: string;
  retry: () => void;
};

/** Drives the layout engine from the projection cache key. Selection/hover do
 *  not change the key, so they never trigger a relayout. */
export function useLayout(engine: LayoutEngine, projection: Projection | null): LayoutState {
  const [nonce, setNonce] = useState(0);
  // Only the resolved result is stored; "loading" is derived (resolved key !=
  // current key). This keeps the effect free of synchronous setState.
  const [resolved, setResolved] = useState<{
    key: string | null;
    positions: Map<string, PositionedNode>;
    routes: Map<string, Point[]>;
    error?: string;
  }>({ key: null, positions: new Map(), routes: new Map() });
  const key = projection?.cacheKey ?? null;

  useEffect(() => {
    if (!projection) return;
    let cancelled = false;
    const nodes = projection.graph.nodes.map((n) => {
      const size = cardSize(n.kind);
      return { id: n.id, width: size.width, height: size.height, parent: n.parent };
    });
    const edges = projection.graph.edges.map((e) => ({
      id: e.id,
      source: e.source,
      target: e.target,
    }));
    const stages =
      projection.view.stages && projection.view.stages.length > 0
        ? projection.view.stages.map((s) => ({ id: s.id, order: s.order }))
        : undefined;
    const nodeStage = stages
      ? Object.fromEntries(
          projection.graph.nodes.filter((n) => n.stage).map((n) => [n.id, n.stage!]),
        )
      : undefined;
    const requestKey = projection.cacheKey;
    // Queue-to-result: spans posting the request and receiving positions back,
    // so it includes the worker's own ELK time plus the round-trip.
    const queuedAt = performance.now();
    engine.layout({ key: requestKey, request: { nodes, edges, stages, nodeStage } }).then(
      (outcome) => {
        if (cancelled) return;
        record("cv.layout.worker", performance.now() - queuedAt, {
          nodes: nodes.length,
          edges: edges.length,
        });
        if (outcome.status === "ok") {
          setResolved({
            key: requestKey,
            positions: new Map(outcome.result.nodes.map((p) => [p.id, p])),
            // Routes come from the same result as the positions, so the two can
            // never be mismatched: they are stored together, keyed by relation id.
            routes: new Map(outcome.result.edges.map((e) => [e.id, e.points])),
          });
        } else if (outcome.status === "error") {
          setResolved((r) => ({
            key: requestKey,
            positions: r.positions,
            routes: r.routes,
            error: outcome.error,
          }));
        }
        // "stale" is ignored.
      },
    );
    return () => {
      cancelled = true;
    };
    // Intentionally keyed only by (engine, cacheKey, nonce): selection/inspector/
    // hover changes do not alter the key and must not relayout; `nonce` lets the
    // UI retry after a layout error.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [engine, key, nonce]);

  const current = resolved.key === key;
  const status: LayoutState["status"] = !projection
    ? "idle"
    : current && resolved.error
      ? "error"
      : current
        ? "ok"
        : "loading";
  return {
    status,
    positions: resolved.positions,
    // Only expose routes for the current layout; a pending/errored (stale) layout
    // hands out no routes, so the renderer falls back rather than pairing old
    // routes with new positions.
    routes: status === "ok" ? resolved.routes : EMPTY_ROUTES,
    error: current ? resolved.error : undefined,
    retry: () => setNonce((n) => n + 1),
  };
}

/** Syncs durable URL state with history, and restores (normalized) on popstate.
 *
 *  - meaningful navigation steps (view/selection/filters/…) → `pushState`;
 *  - high-frequency search typing → `replaceState`;
 *  - popstate restoration never pushes a duplicate entry.
 */
export function useUrlSync(store: StoreApi<UiStore>, model: DocumentModel): void {
  const applying = useRef(false);

  useEffect(() => {
    const unsub = store.subscribe((s, prev) => {
      if (applying.current) return; // restoring from popstate — never push
      const nextState = toUrlState(s);
      const next = serializeUrlState(nextState);
      if (next === window.location.search) return;
      const target = next || window.location.pathname;
      if (differsOnlyBySearch(nextState, toUrlState(prev))) {
        window.history.replaceState(null, "", target);
      } else {
        window.history.pushState(null, "", target);
      }
    });
    return unsub;
  }, [store]);

  useEffect(() => {
    const onPop = () => {
      applying.current = true;
      try {
        const url = normalizeUrlState(parseUrlState(window.location.search), model);
        store.getState().initialize({ activeViewId: url.view ?? "", url });
      } finally {
        applying.current = false;
      }
    };
    window.addEventListener("popstate", onPop);
    return () => window.removeEventListener("popstate", onPop);
  }, [store, model]);
}

/** Initial view: valid URL view → workspace-overview → first document view. */
export function chooseInitialView(model: DocumentModel, url: UrlState): string {
  return chooseView(url.view, model);
}

/** Applies View.default_focus only when the URL selects nothing, the entity
 *  exists, and it is visible in the chosen view's projection. */
export function applyDefaultFocus(
  store: StoreApi<UiStore>,
  model: DocumentModel,
  url: UrlState,
): void {
  if (url.entity || url.relation) return;
  const viewId = store.getState().activeViewId;
  const view = viewId ? model.viewById.get(viewId) : undefined;
  if (!view) return;
  const focus = view.default_focus;
  if (typeof focus !== "string" || !model.entityById.has(focus)) return;
  const graph = documentToGraph(model, view, {});
  if (!graph.nodes.some((n) => n.id === focus)) return;
  store.getState().selectEntity(focus);
}
