// Pure, memoizable inspector projection (Issue 15, Phase 5).
//
// The inspector must not sort/group large relation sets inside React render. This
// layer builds a discriminated, deterministically-ordered projection from the
// already-indexed `DocumentModel` (O(degree) via `outgoing`/`incoming`/
// `childrenByParent`, never O(all-relations)). It reuses the single role parse
// (`model.categoryById`) — it never re-reads raw `attributes`. Bounded PREVIEW
// limits are a UI concern; this layer holds the full sorted rows (cheap references)
// and never mutates the document.
import type { DocumentModel } from "./model.ts";
import type { Entity, Relation, DocumentDiagnostic } from "../types/index.ts";
import { localized } from "../types/index.ts";
import { relationStyleFor, type RelationStyle } from "../adapter/relationStyle.ts";
import { roleStyleFor } from "../adapter/roleStyle.ts";

/** One relation shown in a group: its kind/label/style plus the OTHER endpoint. */
export interface RelationRow {
  relationId: string;
  kind: string;
  /** The edge label (authored) or the registry label. */
  label: string;
  style: RelationStyle;
  /** The endpoint that is not the inspected entity. */
  otherId: string;
  otherLabel: string;
}

/** Relations of one kind, in a stable order. Direction is fixed by the group it
 *  belongs to (outgoing vs incoming), never merged. */
export interface RelationGroup {
  kind: string;
  label: string;
  known: boolean;
  /** The registry style for the group's kind (for the direction/kind swatch). */
  style: RelationStyle;
  rows: RelationRow[];
}

export interface EntityInspection {
  readonly kind: "entity";
  readonly entity: Entity;
  /** Role label + known status, from the single-parse `model.categoryById`. */
  readonly roleLabel?: string;
  readonly roleKnown?: boolean;
  readonly children: { id: string; label: string }[];
  readonly outgoing: RelationGroup[];
  readonly incoming: RelationGroup[];
  readonly outgoingTotal: number;
  readonly incomingTotal: number;
  readonly diagnostics: DocumentDiagnostic[];
}

export interface RelationInspection {
  readonly kind: "relation";
  readonly relation: Relation;
  readonly fromId: string;
  readonly fromLabel: string;
  readonly toId: string;
  readonly toLabel: string;
  readonly relLabel: string;
  readonly style: RelationStyle;
  readonly known: boolean;
  readonly diagnostics: DocumentDiagnostic[];
}

export type Inspection = EntityInspection | RelationInspection;

function groupRelations(
  model: DocumentModel,
  relations: readonly Relation[],
  outgoing: boolean,
  lang: string,
): { groups: RelationGroup[]; total: number } {
  const byKind = new Map<string, RelationRow[]>();
  for (const r of relations) {
    const otherId = outgoing ? r.to : r.from;
    const other = model.entityById.get(otherId);
    const style = relationStyleFor(r.kind);
    const row: RelationRow = {
      relationId: r.id,
      kind: r.kind,
      label: r.label ? localized(r.label, lang) : style.label,
      style,
      otherId,
      otherLabel: other ? localized(other.label, lang) : otherId,
    };
    const list = byKind.get(r.kind);
    if (list) list.push(row);
    else byKind.set(r.kind, [row]);
  }
  const groups: RelationGroup[] = [...byKind.entries()].map(([kind, rows]) => {
    // Deterministic within a group: by the other endpoint's label, then id.
    rows.sort((a, b) => a.otherLabel.localeCompare(b.otherLabel) || a.relationId.localeCompare(b.relationId));
    const style = relationStyleFor(kind);
    return { kind, label: style.label, known: style.known, style, rows };
  });
  groups.sort((a, b) => a.label.localeCompare(b.label) || a.kind.localeCompare(b.kind));
  return { groups, total: relations.length };
}

/** Projects the inspection for a selected entity (O(degree)). */
export function entityInspection(model: DocumentModel, entity: Entity, lang: string): EntityInspection {
  const out = groupRelations(model, model.outgoing.get(entity.id) ?? [], true, lang);
  const inc = groupRelations(model, model.incoming.get(entity.id) ?? [], false, lang);
  const children = (model.childrenByParent.get(entity.id) ?? [])
    .map((id) => {
      const e = model.entityById.get(id);
      return { id, label: e ? localized(e.label, lang) : id };
    })
    .sort((a, b) => a.label.localeCompare(b.label) || a.id.localeCompare(b.id));
  const category = model.categoryById.get(entity.id);
  const role = category !== undefined ? roleStyleFor(category) : undefined;
  return {
    kind: "entity",
    entity,
    roleLabel: role?.label,
    roleKnown: role?.known,
    children,
    outgoing: out.groups,
    incoming: inc.groups,
    outgoingTotal: out.total,
    incomingTotal: inc.total,
    diagnostics: [...(model.diagnosticsByEntity.get(entity.id) ?? [])],
  };
}

/** Projects the inspection for a selected relation. */
export function relationInspection(model: DocumentModel, relation: Relation, lang: string): RelationInspection {
  const from = model.entityById.get(relation.from);
  const to = model.entityById.get(relation.to);
  const style = relationStyleFor(relation.kind);
  return {
    kind: "relation",
    relation,
    fromId: relation.from,
    fromLabel: from ? localized(from.label, lang) : relation.from,
    toId: relation.to,
    toLabel: to ? localized(to.label, lang) : relation.to,
    relLabel: relation.label ? localized(relation.label, lang) : style.label,
    style,
    known: style.known,
    diagnostics: [...(model.diagnosticsByRelation.get(relation.id) ?? [])],
  };
}
