// Pure large-graph reduced-mode selection. Separated from React so it is fully
// unit-testable. When the projected node count exceeds the budget, it picks a
// deterministic focus and a bounded breadth-first neighborhood; the adapter then
// projects only `visibleIds` (so no dangling edges are ever produced). Every
// entity remains reachable via the full projected node list (GraphList).

export interface ReduceInput {
  /** All projected node ids for the active view (any order). */
  nodeIds: readonly string[];
  /** Projected edges (endpoints among `nodeIds`). */
  edges: readonly { id: string; source: string; target: string }[];
  /** Maximum visible nodes before reduced mode engages. */
  budget: number;
  selectedId?: string | null;
  searchResultId?: string | null;
  defaultFocusId?: string | null;
  /** Extra seeds whose neighborhoods are also included (Expand neighborhood). */
  expanded?: readonly string[];
}

export interface ReduceResult {
  reduced: boolean;
  fullCount: number;
  visibleCount: number;
  visibleIds: Set<string>;
  focusId: string | null;
}

/** Chooses the focus deterministically: selection → first search result →
 *  visible default_focus → smallest node id. */
function chooseFocus(input: ReduceInput, present: Set<string>): string | null {
  const candidates = [input.selectedId, input.searchResultId, input.defaultFocusId];
  for (const c of candidates) {
    if (typeof c === "string" && present.has(c)) return c;
  }
  const sorted = [...present].sort();
  return sorted[0] ?? null;
}

export function reduceGraph(input: ReduceInput): ReduceResult {
  const present = new Set(input.nodeIds);
  const fullCount = present.size;

  if (fullCount <= input.budget) {
    return {
      reduced: false,
      fullCount,
      visibleCount: fullCount,
      visibleIds: new Set(present),
      focusId: null,
    };
  }

  // Undirected adjacency over projected edges.
  const adj = new Map<string, Set<string>>();
  const link = (a: string, b: string) => {
    if (!present.has(a) || !present.has(b)) return;
    (adj.get(a) ?? adj.set(a, new Set()).get(a)!).add(b);
    (adj.get(b) ?? adj.set(b, new Set()).get(b)!).add(a);
  };
  for (const e of input.edges) link(e.source, e.target);

  const focus = chooseFocus(input, present);
  const seeds: string[] = [];
  if (focus) seeds.push(focus);
  for (const id of input.expanded ?? []) if (present.has(id)) seeds.push(id);

  const visible = new Set<string>();
  // Deterministic BFS: process a sorted frontier, expand each node's neighbors
  // in id order, stop at the budget.
  let frontier = [...new Set(seeds)].sort();
  for (const s of frontier) {
    if (visible.size >= input.budget) break;
    visible.add(s);
  }
  while (frontier.length > 0 && visible.size < input.budget) {
    const next: string[] = [];
    for (const node of frontier) {
      const neighbors = [...(adj.get(node) ?? [])].sort();
      for (const nb of neighbors) {
        if (visible.size >= input.budget) break;
        if (!visible.has(nb)) {
          visible.add(nb);
          next.push(nb);
        }
      }
      if (visible.size >= input.budget) break;
    }
    frontier = next;
  }

  return {
    reduced: true,
    fullCount,
    visibleCount: visible.size,
    visibleIds: visible,
    focusId: focus,
  };
}
