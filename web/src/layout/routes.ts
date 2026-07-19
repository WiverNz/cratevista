// Pure flattening of ELK edge routing sections into a single ordered polyline.
//
// The ELK worker produces one `RoutedEdge` per document relation. ELK usually
// returns a single routing section per edge (one source, one target), but the
// contract permits several; this module joins only sections that form a valid,
// deterministic end→start chain and otherwise reports "no usable route" (an empty
// list) so the renderer falls back to a computed path. It never guesses geometry.
//
// This layer is deliberately free of React Flow and of any relation styling: it
// deals only in coordinates.
import type { Point } from "./types.ts";

/** A single ELK routing section, as read back from a layout result. */
export interface RouteSection {
  startPoint: Point;
  endPoint: Point;
  bendPoints?: Point[];
}

/** Endpoints within this distance are treated as the same joint (ELK emits exact
 *  shared coordinates for a connected chain; the tolerance only absorbs float
 *  noise). */
const JOIN_EPSILON = 0.5;

function finite(p: Point | undefined): p is Point {
  return !!p && Number.isFinite(p.x) && Number.isFinite(p.y);
}

function near(a: Point, b: Point): boolean {
  return Math.abs(a.x - b.x) <= JOIN_EPSILON && Math.abs(a.y - b.y) <= JOIN_EPSILON;
}

function polylineLength(points: Point[]): number {
  let total = 0;
  for (let i = 1; i < points.length; i++) {
    total += Math.hypot(points[i].x - points[i - 1].x, points[i].y - points[i - 1].y);
  }
  return total;
}

/**
 * Flattens ELK sections into one ordered point list, or returns `[]` when the
 * route is unusable.
 *
 * Returns `[]` (→ the caller uses its computed fallback) when there are no
 * sections, any coordinate is non-finite, the sections do not connect
 * end→start in array order, or the whole route collapses to a single point
 * (a zero-length route is not a real path here — genuine self-loops are drawn by
 * the geometry layer from the node's own handles, never from a route).
 */
export function sectionsToPoints(sections: RouteSection[] | undefined): Point[] {
  if (!sections || sections.length === 0) return [];
  const points: Point[] = [];
  for (let i = 0; i < sections.length; i++) {
    const s = sections[i];
    const bends = s.bendPoints ?? [];
    if (!finite(s.startPoint) || !finite(s.endPoint) || !bends.every(finite)) return [];
    const seq: Point[] = [s.startPoint, ...bends, s.endPoint];
    if (i === 0) {
      points.push(...seq);
    } else {
      // A multi-section route is valid only when each section begins where the
      // previous ended; anything else is a fork/hyperedge we will not guess at.
      if (!near(points[points.length - 1], seq[0])) return [];
      points.push(...seq.slice(1)); // drop the duplicated joint
    }
  }
  if (points.length < 2 || polylineLength(points) <= JOIN_EPSILON) return [];
  return points;
}
