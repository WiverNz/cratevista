// Pure, precomputed node-card projection.
//
// Everything a graph node card can display is derived here, ONCE per document
// model (memoized by model reference). Card rendering never aggregates over the
// graph: it reads a prebuilt `NodeCard`. Selection and zoom change only which
// parts of an already-built card are shown, never the underlying metrics.
//
// Only deterministic, semantically-correct values from the existing document are
// projected — no invented "roles", no values parsed from diagnostic message
// strings, and no metric that is structurally always zero (count metrics that
// evaluate to zero for a node are omitted rather than shown as `0`/`N/A`).

import type { DocumentModel } from "./model.ts";
import type { Entity, DocumentDiagnostic } from "../types/index.ts";
import { localized } from "../types/index.ts";
import { entityStyle } from "../adapter/kindStyle.ts";

/** Progressive-disclosure density level. */
export type CardLevel = "compact" | "normal" | "detailed";

/** Visual category driving the token-based node styling (border/accent/icon). */
export type NodeCategory =
  | "workspace"
  | "package"
  | "target"
  | "module"
  | "type"
  | "trait"
  | "function"
  | "impl"
  | "manual"
  | "unknown";

/** A single key/value metric shown at or above `minLevel`. */
export interface CardMetric {
  key: string;
  label: string;
  value: string;
  minLevel: Exclude<CardLevel, "compact">;
}

/** A compact, node-associated diagnostic marker (occurrence-aware). */
export interface CardDiagnostic {
  severity: "error" | "warning" | "info";
  /** Represented occurrences (sum of occurrence_count), not record count. */
  occurrences: number;
  records: number;
  /** Accessible text, e.g. "2 errors (5 occurrences)". */
  label: string;
}

/** The fully-projected card for one entity. Immutable; shared across renders. */
export interface NodeCard {
  id: string;
  kind: string;
  category: NodeCategory;
  known: boolean;
  /** Short kind/type-of-target badge, e.g. "Package", "Struct", "lib". */
  kindLabel: string;
  /** Display title (CSS truncates); `fullTitle` is the accessible full text. */
  title: string;
  fullTitle: string;
  /** Short owning module/package context for code entities (never the full qn). */
  context?: string;
  /** One bounded supporting line from the entity's own `description` — NEVER
   *  synthesized from the name/qualified-name/id. `undefined` when the entity has
   *  no description, so the card leaves no empty supporting block. */
  description?: string;
  visibility?: string;
  /** Documentation state; `undefined` when the entity carries no doc block. */
  documented?: boolean;
  hasSource: boolean;
  metrics: CardMetric[];
  diagnostic?: CardDiagnostic;
  /** Bounded, deterministic layout box (px). */
  width: number;
  height: number;
}

/** The single visual emphasis a node draws, highest priority first. Selection
 *  always dominates related / search-match / diagnostic emphasis; the diagnostic
 *  badge itself is shown independently of this state. */
export type NodeVisualState =
  | "selected"
  | "search"
  | "diagnostic-error"
  | "diagnostic-warning"
  | "related"
  | "normal";

/** Resolves the dominant node emphasis from its state flags (deterministic order:
 *  selected → search → diagnostic error → diagnostic warning → related → normal). */
export function nodeVisualState(flags: {
  selected: boolean;
  searchMatch: boolean;
  related: boolean;
  diagnosticSeverity?: "error" | "warning" | "info";
}): NodeVisualState {
  if (flags.selected) return "selected";
  if (flags.searchMatch) return "search";
  if (flags.diagnosticSeverity === "error") return "diagnostic-error";
  if (flags.diagnosticSeverity === "warning") return "diagnostic-warning";
  if (flags.related) return "related";
  return "normal";
}

/** Zoom thresholds for density. Selection forces `detailed` regardless. */
export const LEVEL_ZOOM_NORMAL = 0.55;
export const LEVEL_ZOOM_DETAILED = 1.1;

/** The density level for a node given zoom + whether it is selected. */
export function cardLevel(opts: { zoom: number; selected: boolean }): CardLevel {
  if (opts.selected) return "detailed";
  if (opts.zoom >= LEVEL_ZOOM_DETAILED) return "detailed";
  if (opts.zoom >= LEVEL_ZOOM_NORMAL) return "normal";
  return "compact";
}

const CATEGORY_BY_KIND: Record<string, NodeCategory> = {
  workspace: "workspace",
  package: "package",
  target: "target",
  module: "module",
  struct: "type",
  enum: "type",
  union: "type",
  type_alias: "type",
  constant: "type",
  static: "type",
  field: "type",
  variant: "type",
  assoc_type: "type",
  trait: "trait",
  function: "function",
  method: "function",
  macro: "function",
  impl: "impl",
};

/** Visual category for an entity (manual provenance and unknown kinds override). */
export function nodeCategory(kind: string, provenance: string): NodeCategory {
  if (provenance === "manual") return "manual";
  const cat = CATEGORY_BY_KIND[kind];
  if (cat) return cat;
  return entityStyle(kind).known ? "type" : "unknown";
}

/** Item kinds counted as a target/package's public API surface. */
const PUBLIC_ITEM_KINDS = new Set([
  "struct",
  "enum",
  "union",
  "trait",
  "function",
  "type_alias",
  "constant",
  "static",
  "macro",
]);

function attr(entity: Entity, key: string): unknown {
  return entity.attributes?.[key];
}
function stringAttr(entity: Entity, key: string): string | undefined {
  const v = attr(entity, key);
  return typeof v === "string" ? v : undefined;
}
function coveragePercent(entity: Entity): number | undefined {
  const cov = attr(entity, "doc_coverage");
  if (cov && typeof cov === "object" && typeof (cov as Record<string, unknown>).percent === "number") {
    return (cov as { percent: number }).percent;
  }
  return undefined;
}
function coverageParts(entity: Entity): { documented: number; total: number } | undefined {
  const cov = attr(entity, "doc_coverage");
  if (
    cov &&
    typeof cov === "object" &&
    typeof (cov as Record<string, unknown>).documented === "number" &&
    typeof (cov as Record<string, unknown>).total === "number"
  ) {
    return cov as { documented: number; total: number };
  }
  return undefined;
}

/** Exact Cargo target kind from `crate_types` (lib / bin / proc-macro / …). */
function targetKind(entity: Entity): string {
  const types = attr(entity, "crate_types");
  if (Array.isArray(types) && types.length > 0) {
    const t = types.map(String);
    if (t.includes("proc-macro")) return "proc-macro";
    if (t.includes("bin")) return "bin";
    if (t.every((x) => x === "lib" || x === "rlib" || x === "dylib" || x === "cdylib" || x === "staticlib")) {
      return "lib";
    }
    return t[0];
  }
  return "target";
}

function humanKindLabel(entity: Entity): string {
  if (entity.kind === "target") return targetKind(entity);
  const s = entityStyle(entity.kind);
  return s.known ? s.category : entity.kind;
}

/** A short owning-module/package context that does not repeat the full name. */
function contextFor(entity: Entity, model: DocumentModel): string | undefined {
  const qn = entity.qualified_name;
  if (qn.includes("::")) return qn.slice(0, qn.lastIndexOf("::"));
  const parentId = typeof entity.parent === "string" ? entity.parent : undefined;
  const parent = parentId ? model.entityById.get(parentId) : undefined;
  return parent ? localized(parent.label) : undefined;
}

/** Post-order count of public API items in each entity's subtree. O(N) once. */
function computePublicItemCounts(model: DocumentModel): Map<string, number> {
  const memo = new Map<string, number>();
  const visit = (id: string): number => {
    const cached = memo.get(id);
    if (cached !== undefined) return cached;
    memo.set(id, 0); // guard against pathological cycles
    let total = 0;
    for (const childId of model.childrenByParent.get(id) ?? []) {
      const child = model.entityById.get(childId);
      if (!child) continue;
      const self =
        PUBLIC_ITEM_KINDS.has(child.kind) && stringAttr(child, "visibility") === "public" ? 1 : 0;
      total += self + visit(childId);
    }
    memo.set(id, total);
    return total;
  };
  for (const e of model.document.entities) visit(e.id);
  return memo;
}

/** Highest-severity, node-owned diagnostic badge — or `undefined`.
 *
 *  Uses ONLY diagnostics the artifact explicitly associates with this entity
 *  (`diagnosticsByEntity`, built from each diagnostic's `entities` field).
 *  Global, unassociated diagnostics (e.g. aggregated external-reference
 *  summaries) never become node badges — they live only in the Diagnostics
 *  explorer. Ownership is never guessed from message text. */
function diagnosticBadge(diags: readonly DocumentDiagnostic[] | undefined): CardDiagnostic | undefined {
  if (!diags || diags.length === 0) return undefined;
  const rank = { error: 0, warning: 1, info: 2 } as const;
  let best: "error" | "warning" | "info" = "info";
  let seenError = false;
  let seenWarning = false;
  for (const d of diags) {
    if (d.severity === "error") seenError = true;
    else if (d.severity === "warning") seenWarning = true;
  }
  best = seenError ? "error" : seenWarning ? "warning" : "info";
  const owned = diags.filter((d) => d.severity === best);
  let occurrences = 0;
  for (const d of owned) occurrences += occurrenceOf(d);
  const records = owned.length;
  const word = best === "error" ? "error" : best === "warning" ? "warning" : "info";
  const label = `${records} ${word}${records === 1 ? "" : "s"} (${occurrences} occurrence${occurrences === 1 ? "" : "s"})`;
  void rank;
  return { severity: best, occurrences, records, label };
}

function occurrenceOf(d: DocumentDiagnostic): number {
  const n = (d as { occurrence_count?: unknown }).occurrence_count;
  return typeof n === "number" && Number.isInteger(n) && n >= 1 ? n : 1;
}

/**
 * Bounded, deterministic layout box per visual category (Issue 15, Phase 3).
 *
 * These are the single source of truth for card dimensions: ELK receives exactly
 * these, and the rendered card root consumes exactly these. They are sized to hold
 * the fullest (detailed) composition — header, title, one description line, and the
 * metrics/indicator footer — so density levels only hide/show content WITHIN a
 * fixed box; no interaction, zoom, or state ever changes the box. Values are locked
 * within the PRD-approved per-category ranges (workspace/package 240–260 × 120–136;
 * target 228–244 × 108–124; module/manual 216–232 × 100–116; code/default
 * 208–224 × 96–112); leaving a range requires a PRD amendment.
 */
export function cardSize(kind: string, provenance = "discovered"): { width: number; height: number } {
  switch (nodeCategory(kind, provenance)) {
    case "workspace":
    case "package":
      return { width: 252, height: 128 };
    case "target":
      return { width: 236, height: 116 };
    case "module":
    case "manual":
      return { width: 224, height: 108 };
    default:
      return { width: 216, height: 104 };
  }
}

/** Max characters for the one bounded description line (CSS further clamps lines). */
const DESCRIPTION_MAX = 140;

/** The entity's own description, trimmed and length-bounded — or `undefined` when
 *  it has none. Never derived from the name, qualified name or id. */
function boundedDescription(entity: Entity): string | undefined {
  const raw = entity.description ? localized(entity.description).trim() : "";
  if (!raw) return undefined;
  return raw.length > DESCRIPTION_MAX ? `${raw.slice(0, DESCRIPTION_MAX - 1).trimEnd()}…` : raw;
}

function childrenOfKind(model: DocumentModel, id: string, kind: string): number {
  let n = 0;
  for (const childId of model.childrenByParent.get(id) ?? []) {
    if (model.entityById.get(childId)?.kind === kind) n += 1;
  }
  return n;
}

function dependsCount(rels: readonly { kind: string }[] | undefined): number {
  if (!rels) return 0;
  let n = 0;
  for (const r of rels) if (r.kind === "depends_on") n += 1;
  return n;
}

function buildOne(
  entity: Entity,
  model: DocumentModel,
  publicItems: Map<string, number>,
): NodeCard {
  const kind = entity.kind;
  const provenance = entity.provenance;
  const category = nodeCategory(kind, provenance);
  const known = category !== "unknown";
  const fullTitle = localized(entity.label);
  const size = cardSize(kind, provenance);
  const metrics: CardMetric[] = [];

  const pushCount = (key: string, label: string, value: number, minLevel: CardMetric["minLevel"]) => {
    if (value > 0) metrics.push({ key, label, value: String(value), minLevel });
  };

  if (kind === "workspace") {
    const packages = childrenOfKind(model, entity.id, "package");
    pushCount("packages", "packages", packages, "normal");
    // Coverage aggregated across packages (deterministic; omitted if unknown).
    let documented = 0;
    let total = 0;
    for (const childId of model.childrenByParent.get(entity.id) ?? []) {
      const child = model.entityById.get(childId);
      if (child?.kind !== "package") continue;
      const parts = coverageParts(child);
      if (parts) {
        documented += parts.documented;
        total += parts.total;
      }
    }
    if (total > 0) {
      metrics.push({ key: "docs", label: "docs", value: `${Math.round((documented / total) * 100)}%`, minLevel: "detailed" });
    }
  } else if (kind === "package") {
    const version = stringAttr(entity, "version");
    if (version) metrics.push({ key: "version", label: "v", value: version, minLevel: "normal" });
    pushCount("deps", "deps", dependsCount(model.outgoing.get(entity.id)), "normal");
    pushCount("targets", "targets", childrenOfKind(model, entity.id, "target"), "detailed");
    pushCount("dependents", "dependents", dependsCount(model.incoming.get(entity.id)), "detailed");
    const pct = coveragePercent(entity);
    if (pct !== undefined) metrics.push({ key: "docs", label: "docs", value: `${pct}%`, minLevel: "detailed" });
  } else if (kind === "target") {
    pushCount("public", "public items", publicItems.get(entity.id) ?? 0, "detailed");
  } else if (kind === "module") {
    const pct = coveragePercent(entity);
    if (pct !== undefined) metrics.push({ key: "docs", label: "docs", value: `${pct}%`, minLevel: "detailed" });
  }

  const documented =
    entity.docs && typeof entity.docs.documented === "boolean" ? entity.docs.documented : undefined;
  const visibility = stringAttr(entity, "visibility");
  const isCode = category === "type" || category === "trait" || category === "function" || category === "impl";

  return {
    id: entity.id,
    kind,
    category,
    known,
    kindLabel: humanKindLabel(entity),
    title: fullTitle,
    fullTitle,
    context: isCode ? contextFor(entity, model) : undefined,
    description: boundedDescription(entity),
    visibility,
    documented,
    hasSource: !!entity.source,
    metrics,
    diagnostic: diagnosticBadge(model.diagnosticsByEntity.get(entity.id)),
    width: size.width,
    height: size.height,
  };
}

/** Builds cards for every entity. O(N + relations); call via {@link getNodeCards}. */
export function buildNodeCards(model: DocumentModel): Map<string, NodeCard> {
  const publicItems = computePublicItemCounts(model);
  const cards = new Map<string, NodeCard>();
  for (const entity of model.document.entities) {
    cards.set(entity.id, buildOne(entity, model, publicItems));
  }
  return cards;
}

// Memoize by model reference: the same document yields the same card map, so
// selection/zoom re-renders never rebuild metrics.
const cache = new WeakMap<DocumentModel, ReadonlyMap<string, NodeCard>>();

/** Cards for a model, computed once and cached by model identity. */
export function getNodeCards(model: DocumentModel): ReadonlyMap<string, NodeCard> {
  const hit = cache.get(model);
  if (hit) return hit;
  const built = buildNodeCards(model);
  cache.set(model, built);
  return built;
}
