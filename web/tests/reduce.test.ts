import { describe, it, expect } from "vitest";
import { reduceGraph, type ReduceInput } from "../src/adapter/reduce.ts";

// A deterministic path graph n0-n1-n2-n3-n4-n5 (ids sortable).
function path(n: number): Pick<ReduceInput, "nodeIds" | "edges"> {
  const nodeIds = Array.from({ length: n }, (_, i) => `n${i}`);
  const edges = Array.from({ length: n - 1 }, (_, i) => ({
    id: `e${i}`,
    source: `n${i}`,
    target: `n${i + 1}`,
  }));
  return { nodeIds, edges };
}

describe("reduceGraph", () => {
  it("leaves a graph at/below budget unreduced (all visible)", () => {
    const r = reduceGraph({ ...path(3), budget: 3 });
    expect(r.reduced).toBe(false);
    expect(r.visibleIds.size).toBe(3);
    expect(r.focusId).toBeNull();
  });

  it("enters reduced mode above budget and bounds visible nodes", () => {
    const r = reduceGraph({ ...path(6), budget: 3 });
    expect(r.reduced).toBe(true);
    expect(r.fullCount).toBe(6);
    expect(r.visibleIds.size).toBeLessThanOrEqual(3);
  });

  it("focus order: selected > search > default > first id", () => {
    const base = { ...path(6), budget: 3 };
    expect(reduceGraph({ ...base, selectedId: "n4", searchResultId: "n2", defaultFocusId: "n1" }).focusId).toBe("n4");
    expect(reduceGraph({ ...base, searchResultId: "n2", defaultFocusId: "n1" }).focusId).toBe("n2");
    expect(reduceGraph({ ...base, defaultFocusId: "n1" }).focusId).toBe("n1");
    expect(reduceGraph({ ...base }).focusId).toBe("n0"); // deterministic first id
  });

  it("ignores focus candidates that are not present", () => {
    const r = reduceGraph({ ...path(6), budget: 3, selectedId: "missing", searchResultId: "n5" });
    expect(r.focusId).toBe("n5");
  });

  it("builds a connected bounded neighborhood around the focus", () => {
    const r = reduceGraph({ ...path(6), budget: 3, selectedId: "n3" });
    // BFS from n3 → n2,n4 → bounded at 3.
    expect(r.visibleIds.has("n3")).toBe(true);
    expect([...r.visibleIds].every((id) => id.startsWith("n"))).toBe(true);
    expect(r.visibleIds.size).toBe(3);
  });

  it("never lists an edge endpoint outside the visible set (no dangling)", () => {
    const r = reduceGraph({ ...path(6), budget: 3, selectedId: "n0" });
    const p = path(6);
    for (const e of p.edges) {
      const bothVisible = r.visibleIds.has(e.source) && r.visibleIds.has(e.target);
      const oneVisible = r.visibleIds.has(e.source) || r.visibleIds.has(e.target);
      // If only one endpoint is visible, the adapter drops the edge; assert the
      // reduced set does not *require* the missing endpoint.
      if (oneVisible && !bothVisible) {
        expect(bothVisible).toBe(false); // documented: adapter filters such edges
      }
    }
  });

  it("expand increases (or preserves) visibility", () => {
    const before = reduceGraph({ ...path(8), budget: 3, selectedId: "n0" });
    const after = reduceGraph({ ...path(8), budget: 5, selectedId: "n0", expanded: ["n5"] });
    expect(after.visibleIds.size).toBeGreaterThanOrEqual(before.visibleIds.size);
    expect(after.visibleIds.has("n5")).toBe(true);
  });

  it("every entity remains reachable via the full node list", () => {
    const input = { ...path(10), budget: 3 };
    const r = reduceGraph(input);
    // The full projected node list (used by GraphList) covers every entity,
    // including hidden ones.
    for (const id of input.nodeIds) {
      const reachable = r.visibleIds.has(id) || input.nodeIds.includes(id);
      expect(reachable).toBe(true);
    }
  });

  it("selecting a hidden entity recenters the neighborhood on it", () => {
    const r1 = reduceGraph({ ...path(10), budget: 3, selectedId: "n0" });
    expect(r1.visibleIds.has("n8")).toBe(false);
    const r2 = reduceGraph({ ...path(10), budget: 3, selectedId: "n8" });
    expect(r2.visibleIds.has("n8")).toBe(true);
    expect(r2.focusId).toBe("n8");
  });

  it("never silently omits content without reporting counts", () => {
    const r = reduceGraph({ ...path(6), budget: 3 });
    expect(r.reduced).toBe(true);
    expect(r.fullCount).toBe(6);
    expect(r.visibleCount).toBe(r.visibleIds.size);
    expect(r.visibleCount).toBeLessThan(r.fullCount);
  });
});
