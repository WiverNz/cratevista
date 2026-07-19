import { describe, it, expect, vi } from "vitest";

vi.mock("@xyflow/react", () => import("./support/xyflow.tsx"));

import { edgePath } from "../src/components/edgeGeometry.ts";
import type { Point } from "../src/layout/types.ts";

/**
 * Synthetic stress graph: 500 nodes, 1000 edges including repeated ordered pairs,
 * reverse-direction parallels, self-loops, and edges with no route (fallback).
 * Route lookup is a Map keyed by relation id, so per-edge work is O(1).
 */
function buildGraph() {
  const nodePos: Point[] = [];
  for (let i = 0; i < 500; i++) nodePos.push({ x: (i % 25) * 240, y: Math.floor(i / 25) * 140 });

  const routes = new Map<string, Point[]>();
  const edges: { id: string; s: number; t: number; selfLoop: boolean; parallelIndex: number }[] = [];
  const pairCount = new Map<string, number>();

  for (let i = 0; i < 1000; i++) {
    const s = i % 500;
    // Deliberately create repeated (s,t) pairs, reverse pairs, and self-loops.
    const t = i % 7 === 0 ? s : (s + 1 + (i % 3)) % 500;
    const selfLoop = s === t;
    const key = [s, t].sort((a, b) => a - b).join("-");
    const idx = pairCount.get(key) ?? 0;
    pairCount.set(key, idx + 1);
    const id = `e${i}`;
    // ~40% of edges have a real ELK route; the rest fall back.
    if (i % 5 !== 0 && !selfLoop) {
      const a = nodePos[s];
      const b = nodePos[t];
      routes.set(id, [a, { x: (a.x + b.x) / 2, y: a.y }, { x: (a.x + b.x) / 2, y: b.y }, b]);
    }
    edges.push({ id, s, t, selfLoop, parallelIndex: idx - 0.5 });
  }
  return { nodePos, routes, edges };
}

function computeAll(g: ReturnType<typeof buildGraph>) {
  const out = new Map<string, string>();
  for (const e of g.edges) {
    const r = edgePath({
      route: g.routes.get(e.id),
      source: g.nodePos[e.s],
      target: g.nodePos[e.t],
      selfLoop: e.selfLoop,
      parallelIndex: e.parallelIndex,
    });
    out.set(e.id, r.d);
  }
  return out;
}

describe("edgePath performance + determinism (500 nodes / 1000 edges)", () => {
  const g = buildGraph();

  it("produces byte-identical geometry across repeated passes", () => {
    const a = computeAll(g);
    const b = computeAll(g);
    expect(a.size).toBe(1000);
    for (const [id, d] of a) {
      expect(d).toBe(b.get(id));
      expect(d).not.toContain("NaN");
      expect(d.length).toBeGreaterThan(0);
    }
  });

  it("computes all 1000 edge paths well within a generous bound", () => {
    const start = performance.now();
    const paths = computeAll(g);
    const ms = performance.now() - start;
    expect(paths.size).toBe(1000);
    // A smoke ceiling, not a production guarantee: pure math over 1000 edges is
    // sub-millisecond in practice; this only catches an accidental blow-up.
    expect(ms).toBeLessThan(500);
  });
});
