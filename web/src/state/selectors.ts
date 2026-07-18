// Pure selectors over (model, UI state). No stored derived copies.
import type { DocumentModel } from "../model/model.ts";
import type { Graph } from "../adapter/adapter.ts";
import { localized } from "../types/index.ts";

/** Entity ids whose label / qualified name / tags match the query. */
export function searchEntities(model: DocumentModel, query: string): string[] {
  const q = query.trim().toLowerCase();
  if (q === "") return [];
  const out: string[] = [];
  for (const e of model.document.entities) {
    const label = localized(e.label).toLowerCase();
    const qn = e.qualified_name.toLowerCase();
    const tags = (e.tags ?? []).join(" ").toLowerCase();
    if (label.includes(q) || qn.includes(q) || tags.includes(q)) out.push(e.id);
  }
  return out;
}

export interface LegendEntry {
  category: string;
  color: string;
  known: boolean;
}

/** Legend categories present in the active graph projection (nodes only). */
export function legendForGraph(graph: Graph): LegendEntry[] {
  const seen = new Map<string, LegendEntry>();
  for (const node of graph.nodes) {
    if (!seen.has(node.style.category)) {
      seen.set(node.style.category, {
        category: node.style.category,
        color: node.style.color,
        known: node.style.known,
      });
    }
  }
  return [...seen.values()].sort((a, b) => a.category.localeCompare(b.category));
}

/** The distinct entity kinds available for filtering in a view's projection. */
export function kindsInGraph(graph: Graph): string[] {
  return [...new Set(graph.nodes.map((n) => n.kind))].sort();
}
