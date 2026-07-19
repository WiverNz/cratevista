import { describe, it, expect } from "vitest";
import {
  flowAnimationPolicy,
  isAnimationEligible,
  type FlowFlag,
} from "../src/adapter/relationStyle.ts";

/**
 * Eligibility + view policy are O(relations) and run once per projection — never
 * per edge render or per animation frame (motion is entirely CSS). This exercises
 * the counting path at scale with 0 / 60 / 61 eligible relations.
 */
describe("flow policy performance (500 nodes / 1000 relations)", () => {
  function build(eligible: number): FlowFlag[] {
    const edges: FlowFlag[] = [];
    for (let i = 0; i < 1000; i++) {
      // Derive eligibility through the real parser to include its cost.
      const rel =
        i < eligible
          ? { provenance: "manual", attributes: { flow: "active" } }
          : { provenance: i % 2 ? "discovered" : "manual", attributes: {} };
      edges.push({ flowEligible: isAnimationEligible(rel) });
    }
    return edges;
  }

  it("computes the view policy once, deterministically, within a generous bound", () => {
    for (const [eligible, expectMotion] of [
      [0, false],
      [60, true],
      [61, false],
    ] as const) {
      const edges = build(eligible);
      const start = performance.now();
      const a = flowAnimationPolicy(edges);
      const ms = performance.now() - start;
      const b = flowAnimationPolicy(edges);
      expect(a).toEqual(b); // deterministic
      expect(a.eligibleCount).toBe(eligible);
      expect(a.motionAllowed).toBe(expectMotion);
      // Smoke ceiling only, not a production guarantee: one O(n) pass over 1000.
      expect(ms).toBeLessThan(50);
    }
  });
});
