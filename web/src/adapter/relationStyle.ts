// The single, centralized, typed relation-to-style registry.
//
// Every React Flow view and the legend derive their edge appearance from this
// module and nothing else — there are no relation-style constants duplicated in
// components. Meaning is encoded through *stroke colour token + line pattern +
// width + directional marker* together, never colour alone, so `contains` and
// `depends_on` stay distinguishable without reading a single edge label and to
// viewers who cannot perceive the colour difference.
//
// Colour is expressed as a CSS custom-property *token* (`--rel-*`) resolved by
// the stylesheet, so the same registry renders correctly in dark and light
// themes. The concrete token values live in `styles.css`.

/** Line pattern for an edge. */
export type EdgePattern = "solid" | "dashed" | "dotted";

/** Directional end marker. `"none"` means the relation is non-directional
 *  (e.g. quiet containment) and draws no arrowhead. */
export type EdgeMarker = "arrow" | "arrow-closed" | "none";

/** Interaction state an edge is drawn in. */
export type EdgeState = "normal" | "related" | "selected" | "faded";

/** How loudly a relation competes for attention in the visual hierarchy. */
export type Emphasis = "subordinate" | "normal" | "strong";

/** Concrete width + opacity for one interaction state. */
export interface StateVisual {
  /** Stroke width in px. */
  readonly width: number;
  /** Opacity 0..1. */
  readonly opacity: number;
}

/** A fully-resolved relation style. Immutable; shared by edges and the legend. */
export interface RelationStyle {
  /** Canonical relation kind (or the raw kind for the unknown fallback). */
  readonly kind: string;
  /** Human-readable label used in the legend and accessible names. */
  readonly label: string;
  /** CSS custom-property name for the stroke colour (dark + light aware). */
  readonly strokeToken: string;
  /** Base stroke width (the `normal` state width). */
  readonly width: number;
  /** Base opacity (the `normal` state opacity). */
  readonly opacity: number;
  readonly pattern: EdgePattern;
  readonly marker: EdgeMarker;
  /** CSS custom-property tokens for the label halo. */
  readonly labelFg: string;
  readonly labelBg: string;
  readonly emphasis: Emphasis;
  /** Whether this kind was recognized (vs the neutral unknown fallback). */
  readonly known: boolean;
  /** Explicit visuals for every interaction state. */
  readonly states: Readonly<Record<EdgeState, StateVisual>>;
}

const LABEL_FG_TOKEN = "--rel-label-fg";
const LABEL_BG_TOKEN = "--rel-label-bg";

/** Below this zoom, non-forced edge labels are hidden entirely. */
export const LABEL_ZOOM_MIN = 0.4;
/** A repeated label needs at least this zoom to appear unless it is forced
 *  (hovered / selected / related). */
export const LABEL_ZOOM_REPEATED = 0.85;

function round(n: number): number {
  return Math.round(n * 100) / 100;
}
function clamp01(n: number): number {
  return Math.max(0, Math.min(1, n));
}

/** Derives the four interaction states from a base width + opacity so that,
 *  for every relation: `selected` is the thickest and fully opaque, `related`
 *  is stronger than `normal`, and `faded` drops to a substantially lower
 *  opacity. */
function statesFor(width: number, opacity: number): Record<EdgeState, StateVisual> {
  return {
    normal: { width, opacity },
    related: { width: round(width + 0.5), opacity: clamp01(opacity + 0.2) },
    selected: { width: round(width + 1.5), opacity: 1 },
    faded: { width, opacity: Math.min(opacity, 0.1) },
  };
}

interface Spec {
  readonly label: string;
  readonly pattern: EdgePattern;
  readonly marker: EdgeMarker;
  readonly width: number;
  readonly opacity: number;
  readonly emphasis: Emphasis;
}

/** The authoritative relation → spec table. Adding a kind here is the only
 *  place a new relation style is introduced. */
const SPECS: Record<string, Spec> = {
  // Quiet, thin, non-directional: structural containment must never compete
  // with dependency edges.
  contains: { label: "contains", pattern: "dotted", marker: "none", width: 1, opacity: 0.4, emphasis: "subordinate" },
  // Loud, thick, solid, clear arrow: the primary architectural signal.
  depends_on: { label: "depends on", pattern: "solid", marker: "arrow-closed", width: 2.5, opacity: 0.95, emphasis: "strong" },
  implements: { label: "implements", pattern: "dashed", marker: "arrow-closed", width: 1.75, opacity: 0.85, emphasis: "normal" },
  implemented_by: { label: "implemented by", pattern: "dashed", marker: "arrow", width: 1.75, opacity: 0.85, emphasis: "normal" },
  implemented_for: { label: "implemented for", pattern: "dashed", marker: "arrow", width: 1.75, opacity: 0.85, emphasis: "normal" },
  calls: { label: "calls", pattern: "solid", marker: "arrow", width: 1.75, opacity: 0.9, emphasis: "normal" },
  uses: { label: "uses", pattern: "dashed", marker: "arrow", width: 1.5, opacity: 0.8, emphasis: "normal" },
  has_field_type: { label: "has field type", pattern: "solid", marker: "arrow", width: 1.5, opacity: 0.8, emphasis: "normal" },
  accepts_type: { label: "accepts type", pattern: "dashed", marker: "arrow", width: 1.5, opacity: 0.8, emphasis: "normal" },
  returns_type: { label: "returns type", pattern: "solid", marker: "arrow", width: 1.5, opacity: 0.8, emphasis: "normal" },
  error_type: { label: "error type", pattern: "dashed", marker: "arrow", width: 1.5, opacity: 0.8, emphasis: "normal" },
  re_exports: { label: "re-exports", pattern: "dashed", marker: "arrow", width: 1.5, opacity: 0.8, emphasis: "normal" },
  imports: { label: "imports", pattern: "dotted", marker: "arrow", width: 1.25, opacity: 0.7, emphasis: "normal" },
  references_type: { label: "references type", pattern: "dotted", marker: "arrow", width: 1.25, opacity: 0.7, emphasis: "normal" },
  // Manually-authored flow relations: distinct, deliberately visible.
  manual: { label: "manual flow", pattern: "dashed", marker: "arrow", width: 2, opacity: 0.9, emphasis: "strong" },
};

/** The neutral, safe fallback for any relation kind not in {@link SPECS}. */
const UNKNOWN_SPEC: Spec = {
  label: "unknown",
  pattern: "solid",
  marker: "arrow",
  width: 1.5,
  opacity: 0.6,
  emphasis: "normal",
};

function tokenFor(kind: string, known: boolean): string {
  return `--rel-${(known ? kind : "unknown").replace(/_/g, "-")}`;
}

function build(kind: string, spec: Spec, known: boolean): RelationStyle {
  return {
    kind,
    label: known ? spec.label : kind,
    strokeToken: tokenFor(kind, known),
    width: spec.width,
    opacity: spec.opacity,
    pattern: spec.pattern,
    marker: spec.marker,
    labelFg: LABEL_FG_TOKEN,
    labelBg: LABEL_BG_TOKEN,
    emphasis: spec.emphasis,
    known,
    states: statesFor(spec.width, spec.opacity),
  };
}

const REGISTRY: Record<string, RelationStyle> = Object.fromEntries(
  Object.entries(SPECS).map(([kind, spec]) => [kind, build(kind, spec, true)]),
);

/** The canonical neutral style, used for defs/legend of unknown relations. */
export const UNKNOWN_RELATION_STYLE: RelationStyle = build("unknown", UNKNOWN_SPEC, false);

/**
 * Resolves a relation kind to its style.
 *
 * Manual-flow relations carry the schema's `manual` kind and map to the
 * dedicated manual-flow style. Any unrecognized kind gets the neutral
 * {@link UNKNOWN_RELATION_STYLE} shape (never a crash), carrying its raw kind as
 * the label so it still reads as `(unknown)`.
 */
export function relationStyleFor(kind: string): RelationStyle {
  const hit = REGISTRY[kind];
  if (hit) return hit;
  return build(kind, UNKNOWN_SPEC, false);
}

/** All recognized relation styles, plus the neutral fallback — for rendering
 *  the shared marker `<defs>` once per canvas. */
export function allRelationStyles(): RelationStyle[] {
  return [...Object.values(REGISTRY), UNKNOWN_RELATION_STYLE];
}

/** The `stroke-dasharray` for a pattern at a given width (undefined = solid). */
export function dashArrayFor(pattern: EdgePattern, width: number): string | undefined {
  if (pattern === "solid") return undefined;
  if (pattern === "dashed") return `${round(width * 3)} ${round(width * 2)}`;
  return `${round(width)} ${round(width * 2)}`; // dotted
}

/** A concrete edge appearance resolved for one interaction state. */
export interface EdgeVisual {
  /** `var(--rel-*)` stroke reference. */
  readonly stroke: string;
  readonly strokeWidth: number;
  readonly strokeDasharray?: string;
  readonly opacity: number;
  readonly marker: EdgeMarker;
  readonly labelFg: string;
  readonly labelBg: string;
}

/** Resolves a style to concrete SVG stroke properties for an interaction state. */
export function edgeVisual(style: RelationStyle, state: EdgeState): EdgeVisual {
  const st = style.states[state];
  return {
    stroke: `var(${style.strokeToken})`,
    strokeWidth: st.width,
    strokeDasharray: dashArrayFor(style.pattern, st.width),
    opacity: st.opacity,
    marker: style.marker,
    labelFg: `var(${style.labelFg})`,
    labelBg: `var(${style.labelBg})`,
  };
}

/** Stable DOM id of the shared arrow marker for a style (or null when none). */
export function markerId(style: RelationStyle): string | null {
  if (style.marker === "none") return null;
  return `cv-edge-arrow-${(style.known ? style.kind : "unknown").replace(/_/g, "-")}`;
}

/**
 * Whether an edge label should render, given zoom, interaction state, hover and
 * whether the same label text repeats across many edges.
 *
 * Forced states (hovered, selected, related to the selection) always show.
 * Otherwise labels vanish below {@link LABEL_ZOOM_MIN}, and *repeated* labels —
 * the ones that form a dense visual wall — stay hidden until a useful zoom
 * ({@link LABEL_ZOOM_REPEATED}).
 */
export function shouldShowEdgeLabel(opts: {
  zoom: number;
  state: EdgeState;
  hovered: boolean;
  repeated: boolean;
}): boolean {
  if (opts.hovered || opts.state === "selected" || opts.state === "related") return true;
  if (opts.zoom < LABEL_ZOOM_MIN) return false;
  if (opts.repeated) return opts.zoom >= LABEL_ZOOM_REPEATED;
  return true;
}

/** z-order for an edge so strong/selected relations paint above quiet/faded
 *  ones (containment sits underneath dependency edges). */
export function edgeZIndex(style: RelationStyle, state: EdgeState): number {
  const base = style.emphasis === "strong" ? 3 : style.emphasis === "subordinate" ? 0 : 1;
  const stateBump =
    state === "selected" ? 40 : state === "related" ? 20 : state === "faded" ? -5 : 0;
  return base + stateBump;
}
