// The single pure adapter boundary: (model, view, options) → graph nodes/edges.
// No React Flow imports here (kept pure + unit-testable); the GraphCanvas maps
// these plain objects onto React Flow. Rules:
//  - an edge is emitted only when both endpoints are in the visible projection;
//  - unknown kinds keep their raw kind label + a generic style;
//  - unknown/empty/manual views are handled purely from their filters.
import type { DocumentModel } from "../model/model.ts";
import type { View } from "../types/index.ts";
import { localized } from "../types/index.ts";
import { entityStyle, relationStyle, type KindStyle } from "./kindStyle.ts";
import { isAnimationEligible } from "./relationStyle.ts";
import { authoredRole } from "./roleStyle.ts";

export interface GraphNode {
  id: string;
  kind: string;
  label: string;
  qualifiedName: string;
  style: KindStyle;
  parent?: string;
  stage?: string;
  /** The authored architectural role value (`attributes.category`), trimmed, or
   *  undefined when absent — parsed once here via the shared `authoredRole`, never
   *  re-read from raw attributes in components. */
  category?: string;
}

export interface GraphEdge {
  id: string;
  source: string;
  target: string;
  kind: string;
  label?: string;
  style: KindStyle;
  /** Precomputed once here (never re-parsed in components): whether this relation
   *  opts into active-flow animation via the manual `attributes.flow = "active"`
   *  contract. See `isAnimationEligible`. */
  flowEligible: boolean;
}

export interface Graph {
  nodes: GraphNode[];
  edges: GraphEdge[];
}

export interface AdapterOptions {
  /** Extra entity-kind filter from the UI (null = no extra filter). */
  kindFilter?: ReadonlySet<string> | null;
  /** Restrict to a focus entity + its 1-hop neighborhood. */
  relatedOnly?: boolean;
  focusId?: string | null;
  /** Explicit visible-entity allow-list (large-graph reduced mode). */
  visibleIds?: ReadonlySet<string> | null;
  lang?: string;
}

/** The set of entity ids a view projects, before UI/large-graph narrowing. */
export function viewEntityIds(model: DocumentModel, view: View): Set<string> {
  const ids = new Set<string>();
  const explicit = view.entity_ids ?? null;
  if (explicit && explicit.length > 0) {
    for (const id of explicit) if (model.entityById.has(id)) ids.add(id);
    return ids;
  }
  const kinds = view.entity_kinds ?? [];
  if (kinds.length === 0) {
    for (const e of model.document.entities) ids.add(e.id);
    return ids;
  }
  const kindSet = new Set(kinds);
  for (const e of model.document.entities) {
    if (kindSet.has(e.kind)) ids.add(e.id);
  }
  return ids;
}

export function documentToGraph(
  model: DocumentModel,
  view: View,
  options: AdapterOptions = {},
): Graph {
  const lang = options.lang ?? "en";
  let visible = viewEntityIds(model, view);

  if (options.kindFilter && options.kindFilter.size > 0) {
    visible = new Set(
      [...visible].filter((id) =>
        options.kindFilter!.has(model.entityById.get(id)!.kind),
      ),
    );
  }

  if (options.relatedOnly && options.focusId && visible.has(options.focusId)) {
    const keep = new Set<string>([options.focusId]);
    for (const r of model.outgoing.get(options.focusId) ?? [])
      if (visible.has(r.to)) keep.add(r.to);
    for (const r of model.incoming.get(options.focusId) ?? [])
      if (visible.has(r.from)) keep.add(r.from);
    visible = keep;
  }

  if (options.visibleIds) {
    visible = new Set([...visible].filter((id) => options.visibleIds!.has(id)));
  }

  const relationKinds = new Set(view.relation_kinds ?? []);
  const filterRelations = relationKinds.size > 0;

  // Stage assignment: view stages map to entities via `entity_ids` order is not
  // authoritative; stage membership is not encoded on entities in the MVP, so we
  // only surface stage lanes when a view defines stages (grouping is by the
  // presence of a `stage` attribute on the entity if present).
  const nodes: GraphNode[] = [];
  for (const id of visible) {
    const e = model.entityById.get(id)!;
    const stageAttr = e.attributes?.["stage"];
    nodes.push({
      id: e.id,
      kind: e.kind,
      label: localized(e.label, lang),
      qualifiedName: e.qualified_name,
      style: entityStyle(e.kind),
      parent: typeof e.parent === "string" ? e.parent : undefined,
      stage: typeof stageAttr === "string" ? stageAttr : undefined,
      category: authoredRole(e.attributes),
    });
  }

  const edges: GraphEdge[] = [];
  for (const r of model.document.relations) {
    if (filterRelations && !relationKinds.has(r.kind)) continue;
    // Endpoint containment rule: both ends must be visible.
    if (!visible.has(r.from) || !visible.has(r.to)) continue;
    edges.push({
      id: r.id,
      source: r.from,
      target: r.to,
      kind: r.kind,
      label: r.label ? localized(r.label, lang) : undefined,
      style: relationStyle(r.kind),
      flowEligible: isAnimationEligible(r),
    });
  }

  return { nodes, edges };
}
