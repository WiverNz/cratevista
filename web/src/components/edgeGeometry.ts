// Pure, style-agnostic edge geometry.
//
// This is the single seam that turns "where an edge goes" into an SVG path and a
// label anchor. It knows nothing about relation kind, colour, width, marker or
// interaction state — only coordinates. Geometry (this module) and relation style
// (`adapter/relationStyle.ts`) stay separate concerns.
//
// Precedence:
//   1. a valid ELK route  → follow its orthogonal polyline, rounding corners;
//   2. a self-loop        → a deterministic loop drawn from the node's handles;
//   3. otherwise          → React Flow's `getSmoothStepPath` (never a straight
//                           diagonal), so a missing/malformed route still reads
//                           as an orthogonal connector.
import { getSmoothStepPath, Position } from "@xyflow/react";
import type { Point } from "../layout/types.ts";

/** Bounded corner radius for orthogonal joints (px). Clamped per-corner so it can
 *  never exceed half of the shorter adjacent segment. */
export const EDGE_CORNER_RADIUS = 6;

/** Perpendicular spacing between edges that share the same node pair (px). */
export const PARALLEL_EDGE_SPACING = 14;

/** Base extent of a self-loop beyond the node's handles (px). */
export const SELF_LOOP_SIZE = 30;

/** Coordinates below this total length are treated as no usable route. */
const MIN_ROUTE_LENGTH = 0.5;

export interface EdgePathInput {
  /** ELK route polyline for this edge, when the current layout produced one. */
  route?: readonly Point[];
  /** Resolved source-handle anchor (the edge's `from` end). */
  source: Point;
  /** Resolved target-handle anchor (the edge's `to` end). */
  target: Point;
  /** Corner radius override; defaults to {@link EDGE_CORNER_RADIUS}. */
  cornerRadius?: number;
  /** `from === to`: draw a self-loop instead of a connector. */
  selfLoop?: boolean;
  /** Signed rank among edges sharing the same node pair (0 = lone/centre, i.e.
   *  already separated). A non-zero rank fans this edge apart from its overlapping
   *  siblings — perpendicular interior shift for a routed path, perpendicular
   *  anchor offset for a fallback connector. See {@link assignParallelRanks}. */
  parallelIndex?: number;
}

export interface EdgePathResult {
  /** SVG path `d`. */
  d: string;
  /** Label anchor. */
  labelX: number;
  labelY: number;
  /** True when an ELK route was consumed; false for self-loop or fallback. Used
   *  by tests/debug — never for styling. */
  routed: boolean;
}

function round(n: number): number {
  return Math.round(n * 100) / 100;
}
function fmt(p: Point): string {
  return `${round(p.x)},${round(p.y)}`;
}
function dist(a: Point, b: Point): number {
  return Math.hypot(b.x - a.x, b.y - a.y);
}
function finite(p: Point | undefined): p is Point {
  return !!p && Number.isFinite(p.x) && Number.isFinite(p.y);
}
/** A point `d` px from `from` toward `to`. */
function toward(from: Point, to: Point, d: number): Point {
  const l = dist(from, to) || 1;
  const t = d / l;
  return { x: from.x + (to.x - from.x) * t, y: from.y + (to.y - from.y) * t };
}

/** Builds an SVG path through `pts`, rounding each interior corner with a radius
 *  bounded by half of each shorter adjacent segment. Deterministic (2dp). */
function roundedPolyline(pts: Point[], radius: number): string {
  if (pts.length < 2) return "";
  if (pts.length === 2) return `M${fmt(pts[0])} L${fmt(pts[1])}`;
  let d = `M${fmt(pts[0])}`;
  for (let i = 1; i < pts.length - 1; i++) {
    const prev = pts[i - 1];
    const cur = pts[i];
    const next = pts[i + 1];
    const r = Math.min(radius, dist(prev, cur) / 2, dist(cur, next) / 2);
    if (r <= 0) {
      d += ` L${fmt(cur)}`;
      continue;
    }
    const enter = toward(cur, prev, r);
    const exit = toward(cur, next, r);
    d += ` L${fmt(enter)} Q${fmt(cur)} ${fmt(exit)}`;
  }
  d += ` L${fmt(pts[pts.length - 1])}`;
  return d;
}

/** The point at half the total arc length of the polyline. */
function arcMidpoint(pts: Point[]): Point {
  const total = pts.slice(1).reduce((sum, p, i) => sum + dist(pts[i], p), 0);
  const half = total / 2;
  let acc = 0;
  for (let i = 1; i < pts.length; i++) {
    const seg = dist(pts[i - 1], pts[i]);
    if (acc + seg >= half) {
      const t = seg === 0 ? 0 : (half - acc) / seg;
      return {
        x: pts[i - 1].x + (pts[i].x - pts[i - 1].x) * t,
        y: pts[i - 1].y + (pts[i].y - pts[i - 1].y) * t,
      };
    }
    acc += seg;
  }
  return pts[pts.length - 1];
}

function routeUsable(route: readonly Point[] | undefined): route is Point[] {
  if (!route || route.length < 2 || !route.every(finite)) return false;
  let total = 0;
  for (let i = 1; i < route.length; i++) total += dist(route[i - 1], route[i]);
  return total > MIN_ROUTE_LENGTH;
}

/** A deterministic, non-degenerate loop above the node, from the source handle up
 *  and over to the target handle so the arrow still enters the target. */
function selfLoopPath(source: Point, target: Point, radius: number, index: number): EdgePathResult {
  const extent = SELF_LOOP_SIZE + Math.abs(index) * PARALLEL_EDGE_SPACING;
  const top = Math.min(source.y, target.y) - extent;
  const rightX = source.x + extent;
  const leftX = target.x - extent;
  const pts: Point[] = [
    source,
    { x: rightX, y: source.y },
    { x: rightX, y: top },
    { x: leftX, y: top },
    { x: leftX, y: target.y },
    target,
  ];
  return {
    d: roundedPolyline(pts, radius),
    labelX: round((source.x + target.x) / 2),
    labelY: round(top),
    routed: false,
  };
}

/** Shifts a routed polyline's interior perpendicular to its straight
 *  source→target axis by `offset`, keeping the anchored endpoints fixed. A
 *  two-point (bend-free) route gains a single bumped midpoint so overlapping
 *  straight routes still separate. */
function offsetRoutedInterior(pts: Point[], offset: number): Point[] {
  const a = pts[0];
  const b = pts[pts.length - 1];
  const dx = b.x - a.x;
  const dy = b.y - a.y;
  const l = Math.hypot(dx, dy) || 1;
  const nx = -dy / l;
  const ny = dx / l;
  if (pts.length <= 2) {
    return [a, { x: (a.x + b.x) / 2 + nx * offset, y: (a.y + b.y) / 2 + ny * offset }, b];
  }
  return pts.map((p, i) =>
    i === 0 || i === pts.length - 1 ? p : { x: p.x + nx * offset, y: p.y + ny * offset },
  );
}

/** A rounded, direction-normalized signature of a route, so that two edges whose
 *  polylines are geometrically identical — including exact reverse-direction
 *  mirrors — collapse to the same key. Rounding to whole pixels absorbs float
 *  noise so "effectively overlapping" routes also collapse. */
function routeSignature(points: readonly Point[]): string {
  const forward = points.map((p) => `${Math.round(p.x)},${Math.round(p.y)}`);
  const reverse = [...forward].reverse();
  const f = forward.join(";");
  const r = reverse.join(";");
  return f <= r ? f : r;
}

/**
 * Collision-safe key for an **unordered** endpoint pair.
 *
 * A plain delimiter join (e.g. `` `${a} ${b}` ``) is unsafe because entity ids may
 * legitimately contain the delimiter — `["a b", "c"]` and `["a", "b c"]` would
 * collapse to the same `"a b c"`. Encoding a fixed two-element array as JSON keeps
 * them distinct (`["a b","c"]` vs `["a","b c"]`) for any ids, including ones with
 * spaces, separators, quotes or Unicode. Exposed for direct testing.
 */
export function endpointPairKey(a: string, b: string): string {
  return a <= b ? JSON.stringify([a, b]) : JSON.stringify([b, a]);
}

/** One edge, as seen by the parallel-grouping pass. */
export interface ParallelEdgeInput {
  id: string;
  source: string;
  target: string;
  /** The edge's ELK route in the current layout, if any. */
  route?: readonly Point[];
}

/**
 * Assigns each edge a signed perpendicular rank so that edges which would
 * otherwise draw on top of one another fan apart deterministically.
 *
 * Grouping is keyed by the **unordered** endpoint pair: two relations between the
 * same two nodes share a visual corridor regardless of direction, so both
 * same-direction parallels and reverse-direction relations must be considered
 * together (an ordered key would leave a lone A→B and a lone B→A each a singleton
 * and let them overlap). Direction itself is never lost — it is carried by the
 * source/target anchors and the arrow marker, not by the grouping.
 *
 * Within a pair, edges are bucketed by what they will actually draw:
 *   - routed edges by their {@link routeSignature} (identical/mirror/effectively
 *     overlapping routes land in one bucket; routes ELK already separated land in
 *     their own singleton buckets and stay at rank 0 — untouched);
 *   - route-less edges share one "fallback" bucket (they all draw a smooth-step
 *     connector through the same corridor).
 *
 * Only buckets with two or more edges are ranked; ranks are centred on zero and
 * ordered by relation id, so the result is stable regardless of input order and
 * never depends on random or time-based values. Self-loops are excluded (they are
 * drawn from a single node's handles).
 */
export function assignParallelRanks(edges: readonly ParallelEdgeInput[]): Map<string, number> {
  const pairs = new Map<string, ParallelEdgeInput[]>();
  for (const e of edges) {
    if (e.source === e.target) continue;
    const key = endpointPairKey(e.source, e.target);
    const group = pairs.get(key);
    if (group) group.push(e);
    else pairs.set(key, [e]);
  }

  const ranks = new Map<string, number>();
  for (const group of pairs.values()) {
    const buckets = new Map<string, ParallelEdgeInput[]>();
    for (const e of group) {
      const sig = routeUsable(e.route) ? `r:${routeSignature(e.route)}` : "f";
      const bucket = buckets.get(sig);
      if (bucket) bucket.push(e);
      else buckets.set(sig, [e]);
    }
    for (const bucket of buckets.values()) {
      if (bucket.length < 2) continue; // separated already → rank 0 (omitted)
      bucket.sort((a, b) => a.id.localeCompare(b.id));
      const mid = (bucket.length - 1) / 2;
      bucket.forEach((e, i) => ranks.set(e.id, i - mid));
    }
  }
  return ranks;
}

/**
 * Resolves one edge's path + label anchor. Pure and deterministic: identical
 * input always yields an identical result.
 */
export function edgePath(input: EdgePathInput): EdgePathResult {
  const radius = input.cornerRadius ?? EDGE_CORNER_RADIUS;
  const index = input.parallelIndex ?? 0;

  if (input.selfLoop) {
    return selfLoopPath(input.source, input.target, radius, index);
  }

  if (routeUsable(input.route)) {
    // Follow ELK's orthogonal bends, but anchor the endpoints to the actual
    // handles so the edge always meets its nodes even if ELK's port coordinates
    // differ slightly from React Flow's handle centres.
    const middle = input.route.slice(1, input.route.length - 1);
    let pts = [input.source, ...middle, input.target];
    // When several routed edges resolve to the same polyline, ELK did not
    // separate them; a non-zero rank nudges this one's interior perpendicular so
    // the overlapping copies fan apart while their endpoints stay anchored.
    if (index !== 0) pts = offsetRoutedInterior(pts, index * PARALLEL_EDGE_SPACING);
    const mid = arcMidpoint(pts);
    return { d: roundedPolyline(pts, radius), labelX: round(mid.x), labelY: round(mid.y), routed: true };
  }

  // Fallback: a smooth orthogonal connector. Fan parallels apart by nudging the
  // anchors perpendicular to the source→target direction.
  const offset = index * PARALLEL_EDGE_SPACING;
  const dx = input.target.x - input.source.x;
  const dy = input.target.y - input.source.y;
  const len = Math.hypot(dx, dy) || 1;
  const nx = -dy / len;
  const ny = dx / len;
  const sx = input.source.x + nx * offset;
  const sy = input.source.y + ny * offset;
  const tx = input.target.x + nx * offset;
  const ty = input.target.y + ny * offset;
  const [d, labelX, labelY] = getSmoothStepPath({
    sourceX: sx,
    sourceY: sy,
    sourcePosition: Position.Right,
    targetX: tx,
    targetY: ty,
    targetPosition: Position.Left,
    borderRadius: radius,
  });
  return { d, labelX, labelY, routed: false };
}
