// Pure selectors over (model, UI state). No stored derived copies.
import type { DocumentModel } from "../model/model.ts";
import type { Graph } from "../adapter/adapter.ts";
import { localized } from "../types/index.ts";
import { relationStyleFor, type RelationStyle, type Emphasis } from "../adapter/relationStyle.ts";

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

/** A relation-legend row: a relation kind present in the active view plus its
 *  registry style (the legend samples are drawn from `style`, never redefined). */
export interface RelationLegendEntry {
  kind: string;
  label: string;
  style: RelationStyle;
  known: boolean;
}

const EMPHASIS_RANK: Record<Emphasis, number> = { strong: 0, normal: 1, subordinate: 2 };

/** Relation kinds present in the active graph projection, each carrying its
 *  central-registry style. Ordered strong → normal → subordinate then by label,
 *  so dominant relations (e.g. `depends on`) lead and quiet ones (`contains`)
 *  trail. Contains only kinds actually drawn in the view. */
export function relationLegendForGraph(graph: Graph): RelationLegendEntry[] {
  const seen = new Map<string, RelationLegendEntry>();
  for (const edge of graph.edges) {
    if (seen.has(edge.kind)) continue;
    const style = relationStyleFor(edge.kind);
    seen.set(edge.kind, { kind: edge.kind, label: style.label, style, known: style.known });
  }
  return [...seen.values()].sort(
    (a, b) =>
      EMPHASIS_RANK[a.style.emphasis] - EMPHASIS_RANK[b.style.emphasis] ||
      a.label.localeCompare(b.label),
  );
}
