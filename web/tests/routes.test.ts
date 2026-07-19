import { describe, it, expect } from "vitest";
import { sectionsToPoints, type RouteSection } from "../src/layout/routes.ts";

const p = (x: number, y: number) => ({ x, y });

describe("sectionsToPoints", () => {
  it("flattens a single section (start, bends, end) in order", () => {
    const sections: RouteSection[] = [
      { startPoint: p(0, 0), bendPoints: [p(10, 0), p(10, 20)], endPoint: p(30, 20) },
    ];
    expect(sectionsToPoints(sections)).toEqual([p(0, 0), p(10, 0), p(10, 20), p(30, 20)]);
  });

  it("returns [] for no sections (route absent)", () => {
    expect(sectionsToPoints(undefined)).toEqual([]);
    expect(sectionsToPoints([])).toEqual([]);
  });

  it("joins a connected multi-section chain, de-duplicating the joint", () => {
    const sections: RouteSection[] = [
      { startPoint: p(0, 0), endPoint: p(10, 0) },
      { startPoint: p(10, 0), bendPoints: [p(10, 10)], endPoint: p(20, 10) },
    ];
    expect(sectionsToPoints(sections)).toEqual([p(0, 0), p(10, 0), p(10, 10), p(20, 10)]);
  });

  it("rejects a disconnected multi-section route (→ [] → fallback)", () => {
    const sections: RouteSection[] = [
      { startPoint: p(0, 0), endPoint: p(10, 0) },
      { startPoint: p(50, 50), endPoint: p(60, 60) }, // does not continue the first
    ];
    expect(sectionsToPoints(sections)).toEqual([]);
  });

  it("rejects non-finite coordinates (→ [])", () => {
    expect(sectionsToPoints([{ startPoint: p(0, 0), endPoint: p(NaN, 10) }])).toEqual([]);
    expect(sectionsToPoints([{ startPoint: p(0, 0), endPoint: p(Infinity, 10) }])).toEqual([]);
    expect(
      sectionsToPoints([{ startPoint: p(0, 0), bendPoints: [p(NaN, 1)], endPoint: p(10, 10) }]),
    ).toEqual([]);
  });

  it("rejects a zero-length route (start == end, no extent)", () => {
    expect(sectionsToPoints([{ startPoint: p(5, 5), endPoint: p(5, 5) }])).toEqual([]);
  });

  it("is deterministic across repeated calls", () => {
    const sections: RouteSection[] = [
      { startPoint: p(0, 0), bendPoints: [p(10, 0)], endPoint: p(10, 20) },
    ];
    expect(sectionsToPoints(sections)).toEqual(sectionsToPoints(sections));
  });
});
