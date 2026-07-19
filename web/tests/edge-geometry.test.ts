import { describe, it, expect, vi } from "vitest";

// Use the same lightweight React Flow stub as the component tests so the
// smooth-step fallback is deterministic and recognizably stepped.
vi.mock("@xyflow/react", () => import("./support/xyflow.tsx"));

import {
  edgePath,
  assignParallelRanks,
  EDGE_CORNER_RADIUS,
  type ParallelEdgeInput,
} from "../src/components/edgeGeometry.ts";
import type { Point } from "../src/layout/types.ts";

const p = (x: number, y: number): Point => ({ x, y });
const source = p(0, 0);
const target = p(100, 40);

/** Count quadratic-curve corner commands in a path (rounded orthogonal joints). */
function corners(d: string): number {
  return (d.match(/Q/g) ?? []).length;
}

describe("edgePath — valid ELK route", () => {
  const route = [p(0, 0), p(50, 0), p(50, 40), p(100, 40)];

  it("consumes the route, is marked routed, and rounds its corners", () => {
    const r = edgePath({ route, source, target });
    expect(r.routed).toBe(true);
    expect(r.d.startsWith("M0,0")).toBe(true);
    // Two interior bends → two rounded corners.
    expect(corners(r.d)).toBe(2);
  });

  it("anchors endpoints to the real handles (source→target direction)", () => {
    const r = edgePath({ route, source: p(1, 2), target: p(99, 41) });
    expect(r.d.startsWith("M1,2")).toBe(true);
    expect(r.d.trimEnd().endsWith("99,41")).toBe(true);
  });

  it("places the label at the routed arc-length midpoint, not the straight midpoint", () => {
    // Asymmetric L route: segments 80 + 40 + 20 = 140; half-length (70) lands on
    // the first horizontal segment at (70, 0) — far from the straight midpoint
    // (50, 20) of source→target.
    const asym = [p(0, 0), p(80, 0), p(80, 40), p(100, 40)];
    const r = edgePath({ route: asym, source, target });
    expect(r.labelX).toBeCloseTo(70, 5);
    expect(r.labelY).toBeCloseTo(0, 5);
    const straightMid = { x: (source.x + target.x) / 2, y: (source.y + target.y) / 2 };
    expect({ x: r.labelX, y: r.labelY }).not.toEqual(straightMid);
  });

  it("clamps the corner radius to half the shorter adjacent segment", () => {
    const tight = [p(0, 0), p(4, 0), p(4, 4), p(8, 4)]; // 4px segments, radius 6
    const r = edgePath({ route: tight, source: p(0, 0), target: p(8, 4), cornerRadius: 6 });
    expect(r.routed).toBe(true);
    // Still a valid path with rounded corners, no radius larger than the segment.
    expect(corners(r.d)).toBe(2);
    expect(r.d).not.toContain("NaN");
  });

  it("is deterministic across repeated calls", () => {
    expect(edgePath({ route, source, target })).toEqual(edgePath({ route, source, target }));
  });
});

describe("edgePath — fallback", () => {
  it("uses the smooth-step fallback (not straight) when there is no route", () => {
    const r = edgePath({ source, target });
    expect(r.routed).toBe(false);
    // Stub smooth-step is stepped: it has an intermediate elbow, so more than the
    // two points a straight line would have.
    const verts = (r.d.match(/[ML]/g) ?? []).length;
    expect(verts).toBeGreaterThan(2);
  });

  it("falls back when the route has fewer than two points", () => {
    expect(edgePath({ route: [p(3, 3)], source, target }).routed).toBe(false);
  });

  it("falls back when the route contains non-finite coordinates", () => {
    expect(edgePath({ route: [p(0, 0), p(NaN, 5)], source, target }).routed).toBe(false);
  });

  it("falls back when the route has zero total length", () => {
    expect(edgePath({ route: [p(5, 5), p(5, 5)], source, target }).routed).toBe(false);
  });

  it("never emits NaN", () => {
    expect(edgePath({ source, target }).d).not.toContain("NaN");
  });
});

describe("edgePath — self-loop", () => {
  it("produces a non-degenerate loop outside the node, never NaN", () => {
    const r = edgePath({ selfLoop: true, source: p(100, 50), target: p(60, 50) });
    expect(r.routed).toBe(false);
    expect(r.d).not.toContain("NaN");
    expect(r.d.length).toBeGreaterThan(10);
    // Loop rises above the node (label anchor sits above both handle ys).
    expect(r.labelY).toBeLessThan(50);
  });

  it("is deterministic", () => {
    const a = edgePath({ selfLoop: true, source: p(100, 50), target: p(60, 50) });
    const b = edgePath({ selfLoop: true, source: p(100, 50), target: p(60, 50) });
    expect(a).toEqual(b);
  });
});

describe("edgePath — parallel separation (fallback)", () => {
  it("separates parallels by signed index and stays stable", () => {
    const center = edgePath({ source, target, parallelIndex: 0 });
    const up = edgePath({ source, target, parallelIndex: -1 });
    const down = edgePath({ source, target, parallelIndex: 1 });
    expect(up.d).not.toBe(center.d);
    expect(down.d).not.toBe(center.d);
    expect(up.d).not.toBe(down.d);
    // Deterministic
    expect(edgePath({ source, target, parallelIndex: 1 }).d).toBe(down.d);
  });

  it("keeps reverse-direction parallels distinct", () => {
    // Same node pair, opposite direction, different ranks → different geometry.
    const forward = edgePath({ source, target, parallelIndex: -0.5 });
    const backward = edgePath({ source: target, target: source, parallelIndex: 0.5 });
    expect(forward.d).not.toBe(backward.d);
  });
});

describe("assignParallelRanks — overlap grouping", () => {
  const routeAB = [p(0, 0), p(50, 0), p(50, 40), p(100, 40)];

  it("ranks two geometrically identical routed paths apart", () => {
    const edges: ParallelEdgeInput[] = [
      { id: "e1", source: "a", target: "b", route: routeAB },
      { id: "e2", source: "a", target: "b", route: routeAB },
    ];
    const r = assignParallelRanks(edges);
    expect(r.get("e1")).toBe(-0.5);
    expect(r.get("e2")).toBe(0.5);
  });

  it("ranks three identical routed paths across the fan", () => {
    const edges: ParallelEdgeInput[] = ["e3", "e1", "e2"].map((id) => ({
      id,
      source: "a",
      target: "b",
      route: routeAB,
    }));
    const r = assignParallelRanks(edges);
    // Sorted by id: e1=-1, e2=0, e3=1.
    expect([r.get("e1"), r.get("e2"), r.get("e3")]).toEqual([-1, 0, 1]);
  });

  it("treats reverse-direction identical routes as overlapping", () => {
    const edges: ParallelEdgeInput[] = [
      { id: "fwd", source: "a", target: "b", route: routeAB },
      { id: "rev", source: "b", target: "a", route: [...routeAB].reverse() },
    ];
    const r = assignParallelRanks(edges);
    expect(r.get("fwd")).toBeDefined();
    expect(r.get("rev")).toBeDefined();
    expect(r.get("fwd")).not.toBe(r.get("rev"));
  });

  it("leaves already-separated ELK routes at rank 0 (untouched)", () => {
    const edges: ParallelEdgeInput[] = [
      { id: "e1", source: "a", target: "b", route: [p(0, 0), p(50, -20), p(100, 40)] },
      { id: "e2", source: "a", target: "b", route: [p(0, 0), p(50, 60), p(100, 40)] },
    ];
    const r = assignParallelRanks(edges);
    // Distinct signatures → singleton buckets → not ranked (offset 0).
    expect(r.get("e1")).toBeUndefined();
    expect(r.get("e2")).toBeUndefined();
  });

  it("ranks route-less (fallback) siblings of the same pair", () => {
    const edges: ParallelEdgeInput[] = [
      { id: "e1", source: "a", target: "b" },
      { id: "e2", source: "a", target: "b" },
    ];
    const r = assignParallelRanks(edges);
    expect(r.get("e1")).toBe(-0.5);
    expect(r.get("e2")).toBe(0.5);
  });

  it("is independent of input iteration order", () => {
    const mk = (id: string): ParallelEdgeInput => ({ id, source: "a", target: "b", route: routeAB });
    const a = assignParallelRanks([mk("e1"), mk("e2"), mk("e3")]);
    const b = assignParallelRanks([mk("e3"), mk("e1"), mk("e2")]);
    expect([...a.entries()].sort()).toEqual([...b.entries()].sort());
  });

  it("excludes self-loops", () => {
    const r = assignParallelRanks([{ id: "loop", source: "a", target: "a", route: routeAB }]);
    expect(r.get("loop")).toBeUndefined();
  });
});

describe("edgePath — routed parallel overlap separation", () => {
  const route = [p(0, 0), p(50, 0), p(50, 40), p(100, 40)];
  const src = p(0, 0);
  const tgt = p(100, 40);

  it("offsets an overlapping routed path (distinct from the un-offset one), still routed", () => {
    const base = edgePath({ route, source: src, target: tgt, parallelIndex: 0 });
    const shifted = edgePath({ route, source: src, target: tgt, parallelIndex: 1 });
    expect(shifted.routed).toBe(true);
    expect(shifted.d).not.toBe(base.d);
  });

  it("separates two identical routed paths given opposite ranks", () => {
    const up = edgePath({ route, source: src, target: tgt, parallelIndex: -0.5 });
    const down = edgePath({ route, source: src, target: tgt, parallelIndex: 0.5 });
    expect(up.d).not.toBe(down.d);
  });

  it("keeps endpoints anchored while shifting the interior", () => {
    const shifted = edgePath({ route, source: src, target: tgt, parallelIndex: 1 });
    expect(shifted.d.startsWith("M0,0")).toBe(true);
    expect(shifted.d.trimEnd().endsWith("100,40")).toBe(true);
  });

  it("computes the label anchor from the displayed (offset) path", () => {
    const base = edgePath({ route, source: src, target: tgt, parallelIndex: 0 });
    const shifted = edgePath({ route, source: src, target: tgt, parallelIndex: 2 });
    // Offsetting the interior moves the arc-length midpoint off the un-offset anchor.
    expect({ x: shifted.labelX, y: shifted.labelY }).not.toEqual({ x: base.labelX, y: base.labelY });
  });

  it("bows a bend-free (2-point) overlapping route apart", () => {
    const straight = [p(0, 0), p(100, 0)];
    const base = edgePath({ route: straight, source: p(0, 0), target: p(100, 0), parallelIndex: 0 });
    const shifted = edgePath({ route: straight, source: p(0, 0), target: p(100, 0), parallelIndex: 1 });
    expect(shifted.d).not.toBe(base.d);
    expect(shifted.d).not.toContain("NaN");
  });

  it("is deterministic for a given rank", () => {
    expect(edgePath({ route, source: src, target: tgt, parallelIndex: 1 })).toEqual(
      edgePath({ route, source: src, target: tgt, parallelIndex: 1 }),
    );
  });
});

describe("edgePath — defaults", () => {
  it("uses the module corner-radius default when none is given", () => {
    // A generous route with long segments so the default radius is not clamped.
    const route = [p(0, 0), p(100, 0), p(100, 100), p(200, 100)];
    const withDefault = edgePath({ route, source: p(0, 0), target: p(200, 100) });
    const explicit = edgePath({
      route,
      source: p(0, 0),
      target: p(200, 100),
      cornerRadius: EDGE_CORNER_RADIUS,
    });
    expect(withDefault.d).toBe(explicit.d);
  });
});
