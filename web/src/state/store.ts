// Zustand UI-only store. Never mutates ExplorerDocument; holds no duplicated
// projected nodes/edges (projection is derived by pure selectors/adapters).
import { createStore } from "zustand/vanilla";
import { useStore } from "zustand";
import type { EdgeMode, UrlState } from "./url.ts";

/** Selection is a discriminated union — entity and relation can't both be set. */
export type Selection =
  | { kind: "none" }
  | { kind: "entity"; id: string }
  | { kind: "relation"; id: string };

export interface UiState {
  activeViewId: string | null;
  selection: Selection;
  search: string;
  kindFilters: ReadonlySet<string>;
  focusMode: boolean;
  focusId: string | null;
  edgeMode: EdgeMode;
  activeStage: string | null;
  reducedMode: boolean;
  /** User chose "Render full graph", bypassing the large-graph budget. */
  renderFullGraph: boolean;
  expandedNeighborhoods: ReadonlySet<string>;
  theme: "dark" | "light";
  language: string;
}

export interface UiActions {
  /** Initialize from a chosen initial view + URL-derived state. */
  initialize(init: {
    activeViewId: string;
    url?: UrlState;
    focusId?: string | null;
  }): void;
  /** Switch view; always clears the active stage; clears selection unless the
   *  caller (which has the model) says the selection is still valid. */
  switchView(viewId: string, opts?: { keepSelection?: boolean }): void;
  selectEntity(id: string): void;
  selectRelation(id: string): void;
  clearSelection(): void;
  setSearch(q: string): void;
  toggleKind(kind: string): void;
  setKindFilters(kinds: Iterable<string>): void;
  setFocus(id: string | null, focusMode: boolean): void;
  setEdgeMode(mode: EdgeMode): void;
  setStage(stage: string | null): void;
  setReducedMode(reduced: boolean): void;
  setRenderFull(full: boolean): void;
  expandNeighborhood(id: string): void;
  setLanguage(lang: string): void;
  setTheme(theme: "dark" | "light"): void;
  /** Toolbar "Reset": clears search/filters/selection/focus/edge/stage/reduced,
   *  but keeps the active view (and theme/language). Fit is a view concern done
   *  by the caller. */
  resetView(): void;
  reset(): void;
}

export type UiStore = UiState & UiActions;

const initialState: UiState = {
  activeViewId: null,
  selection: { kind: "none" },
  search: "",
  kindFilters: new Set(),
  focusMode: false,
  focusId: null,
  edgeMode: "all",
  activeStage: null,
  reducedMode: false,
  renderFullGraph: false,
  expandedNeighborhoods: new Set(),
  theme: "dark",
  language: "en",
};

export function createUiStore() {
  return createStore<UiStore>((set) => ({
    ...initialState,

    initialize({ activeViewId, url, focusId }) {
      set({
        activeViewId: url?.view ?? activeViewId,
        selection: url?.relation
          ? { kind: "relation", id: url.relation }
          : url?.entity
            ? { kind: "entity", id: url.entity }
            : { kind: "none" },
        search: url?.q ?? "",
        kindFilters: new Set(url?.kinds ?? []),
        edgeMode: url?.edges ?? "all",
        activeStage: url?.stage ?? null,
        focusId: url?.focus ?? focusId ?? null,
        focusMode: Boolean(url?.focus),
      });
    },

    switchView(viewId, opts) {
      set((s) => ({
        activeViewId: viewId,
        activeStage: null, // stages are per-view; clear on switch
        selection: opts?.keepSelection ? s.selection : { kind: "none" },
        focusMode: false,
        focusId: null,
      }));
    },

    selectEntity(id) {
      set({ selection: { kind: "entity", id } });
    },
    selectRelation(id) {
      set({ selection: { kind: "relation", id } });
    },
    clearSelection() {
      set({ selection: { kind: "none" } });
    },
    setSearch(q) {
      set({ search: q });
    },
    toggleKind(kind) {
      set((s) => {
        const next = new Set(s.kindFilters);
        if (next.has(kind)) next.delete(kind);
        else next.add(kind);
        return { kindFilters: next };
      });
    },
    setKindFilters(kinds) {
      set({ kindFilters: new Set(kinds) });
    },
    setFocus(id, focusMode) {
      set({ focusId: id, focusMode });
    },
    setEdgeMode(mode) {
      set({ edgeMode: mode });
    },
    setStage(stage) {
      set({ activeStage: stage });
    },
    setReducedMode(reduced) {
      set({ reducedMode: reduced });
    },
    setRenderFull(full) {
      set({ renderFullGraph: full });
    },
    expandNeighborhood(id) {
      set((s) => {
        const next = new Set(s.expandedNeighborhoods);
        next.add(id);
        return { expandedNeighborhoods: next };
      });
    },
    setLanguage(lang) {
      set({ language: lang });
    },
    setTheme(theme) {
      set({ theme });
    },
    resetView() {
      set({
        selection: { kind: "none" },
        search: "",
        kindFilters: new Set(),
        focusMode: false,
        focusId: null,
        edgeMode: "all",
        activeStage: null,
        reducedMode: false,
        renderFullGraph: false,
        expandedNeighborhoods: new Set(),
      });
    },
    reset() {
      set({ ...initialState, kindFilters: new Set(), expandedNeighborhoods: new Set() });
    },
  }));
}

/** Derives the durable URL state from the store (never hover/viewport/expansion). */
export function toUrlState(s: UiState): UrlState {
  const url: UrlState = {};
  if (s.activeViewId) url.view = s.activeViewId;
  if (s.selection.kind === "entity") url.entity = s.selection.id;
  else if (s.selection.kind === "relation") url.relation = s.selection.id;
  if (s.search) url.q = s.search;
  if (s.kindFilters.size > 0) url.kinds = [...s.kindFilters];
  if (s.focusId) url.focus = s.focusId;
  if (s.edgeMode !== "all") url.edges = s.edgeMode;
  if (s.activeStage) url.stage = s.activeStage;
  return url;
}

/** React hook binding a component to a store slice. */
export function useUiStore<T>(
  store: ReturnType<typeof createUiStore>,
  selector: (s: UiStore) => T,
): T {
  return useStore(store, selector);
}
