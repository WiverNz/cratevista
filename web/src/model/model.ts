// Immutable frontend document model + read-only indexes. Built once per load;
// never mutates the ExplorerDocument.
import type {
  Entity,
  ExplorerDocument,
  Relation,
  View,
  DocumentDiagnostic,
  DiagnosticsReport,
} from "../types/index.ts";
import { authoredRole } from "../adapter/roleStyle.ts";

export interface DocumentModel {
  readonly document: ExplorerDocument;
  readonly entityById: ReadonlyMap<string, Entity>;
  readonly relationById: ReadonlyMap<string, Relation>;
  readonly viewById: ReadonlyMap<string, View>;
  readonly views: readonly View[];
  /** Outgoing relations keyed by `from` entity id. */
  readonly outgoing: ReadonlyMap<string, readonly Relation[]>;
  /** Incoming relations keyed by `to` entity id. */
  readonly incoming: ReadonlyMap<string, readonly Relation[]>;
  /** Child entity ids keyed by parent id. */
  readonly childrenByParent: ReadonlyMap<string, readonly string[]>;
  /** Entity ids grouped by kind. */
  readonly entitiesByKind: ReadonlyMap<string, readonly string[]>;
  /** Diagnostics referencing a given entity id. */
  readonly diagnosticsByEntity: ReadonlyMap<string, readonly DocumentDiagnostic[]>;
  /** Diagnostics referencing a given relation id. */
  readonly diagnosticsByRelation: ReadonlyMap<string, readonly DocumentDiagnostic[]>;
  /** The trimmed authored architectural-role value (`attributes.category`) per
   *  entity id, parsed ONCE here at the raw-document boundary via the shared
   *  `authoredRole`. Absent when no usable value is authored. The adapter and the
   *  card model both consume this — neither re-reads raw `attributes` for the role. */
  readonly categoryById: ReadonlyMap<string, string>;
  /** A stable content identity for layout caching. */
  readonly identity: string;
}

function pushInto<T>(map: Map<string, T[]>, key: string, value: T): void {
  const list = map.get(key);
  if (list) list.push(value);
  else map.set(key, [value]);
}

export function buildModel(
  document: ExplorerDocument,
  diagnostics: DiagnosticsReport | null = null,
): DocumentModel {
  const entityById = new Map<string, Entity>();
  const entitiesByKind = new Map<string, string[]>();
  const categoryById = new Map<string, string>();
  for (const entity of document.entities) {
    entityById.set(entity.id, entity);
    pushInto(entitiesByKind, entity.kind, entity.id);
    // The single role parse: raw `attributes.category` → trimmed value (or nothing).
    const category = authoredRole(entity.attributes);
    if (category !== undefined) categoryById.set(entity.id, category);
  }

  const relationById = new Map<string, Relation>();
  const outgoing = new Map<string, Relation[]>();
  const incoming = new Map<string, Relation[]>();
  for (const relation of document.relations) {
    relationById.set(relation.id, relation);
    pushInto(outgoing, relation.from, relation);
    pushInto(incoming, relation.to, relation);
  }

  const childrenByParent = new Map<string, string[]>();
  for (const entity of document.entities) {
    if (typeof entity.parent === "string") {
      pushInto(childrenByParent, entity.parent, entity.id);
    }
  }

  const viewById = new Map<string, View>();
  for (const view of document.views) viewById.set(view.id, view);

  const diagnosticsByEntity = new Map<string, DocumentDiagnostic[]>();
  const diagnosticsByRelation = new Map<string, DocumentDiagnostic[]>();
  for (const d of diagnostics?.diagnostics ?? []) {
    for (const id of d.entities ?? []) pushInto(diagnosticsByEntity, id, d);
    for (const id of d.relations ?? []) pushInto(diagnosticsByRelation, id, d);
  }

  const identity = `${document.schema_version}:${document.entities.length}:${document.relations.length}:${document.views.length}:${document.project.id}`;

  return {
    document,
    entityById,
    relationById,
    viewById,
    views: document.views,
    outgoing,
    incoming,
    childrenByParent,
    entitiesByKind,
    diagnosticsByEntity,
    diagnosticsByRelation,
    categoryById,
    identity,
  };
}
