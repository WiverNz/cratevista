// Issue 15 Phase 2: pure overlay-safe fit-padding model.
import { describe, it, expect } from "vitest";
import {
  overlayFitPadding,
  toInsetRect,
  OVERLAY_FIT_BASE,
  type InsetRect,
} from "../src/components/overlayFit.ts";

const container: InsetRect = { left: 0, top: 0, right: 1000, bottom: 800 };

describe("overlayFitPadding", () => {
  it("is the base gap on every side with no overlays", () => {
    expect(overlayFitPadding(container, {})).toEqual({
      top: OVERLAY_FIT_BASE,
      right: OVERLAY_FIT_BASE,
      bottom: OVERLAY_FIT_BASE,
      left: OVERLAY_FIT_BASE,
    });
  });

  it("an upper-right overlay pushes ONLY the top and right insets (asymmetric)", () => {
    const p = overlayFitPadding(container, {
      topRight: { left: 900, top: 0, right: 1000, bottom: 40 },
    });
    expect(p.right).toBeGreaterThan(OVERLAY_FIT_BASE);
    expect(p.top).toBeGreaterThan(OVERLAY_FIT_BASE);
    expect(p.left).toBe(OVERLAY_FIT_BASE);
    expect(p.bottom).toBe(OVERLAY_FIT_BASE);
    // Right reach dominates left; top dominates bottom — a selected node is kept
    // clear of the upper-right controls.
    expect(p.right).toBeGreaterThan(p.left);
    expect(p.top).toBeGreaterThan(p.bottom);
  });

  it("a lower-left overlay pushes ONLY the bottom and left insets", () => {
    const p = overlayFitPadding(container, {
      bottomLeft: { left: 0, top: 700, right: 120, bottom: 800 },
    });
    expect(p.left).toBeGreaterThan(OVERLAY_FIT_BASE);
    expect(p.bottom).toBeGreaterThan(OVERLAY_FIT_BASE);
    expect(p.right).toBe(OVERLAY_FIT_BASE);
    expect(p.top).toBe(OVERLAY_FIT_BASE);
  });

  it("all four overlays yield insets on all four sides", () => {
    const p = overlayFitPadding(container, {
      topLeft: { left: 0, top: 0, right: 90, bottom: 40 },
      topRight: { left: 910, top: 0, right: 1000, bottom: 40 },
      bottomLeft: { left: 0, top: 760, right: 120, bottom: 800 },
    });
    expect(p.top).toBeGreaterThan(OVERLAY_FIT_BASE);
    expect(p.right).toBeGreaterThan(OVERLAY_FIT_BASE);
    expect(p.bottom).toBeGreaterThan(OVERLAY_FIT_BASE);
    expect(p.left).toBeGreaterThan(OVERLAY_FIT_BASE);
  });

  it("a wide inspector rendered as a separate column (not over the graph) adds no right inset", () => {
    const p = overlayFitPadding(container, {
      // Its left edge is at the container's right edge — it does not overlap.
      inspector: { left: 1000, top: 0, right: 1360, bottom: 800 },
    });
    expect(p.right).toBe(OVERLAY_FIT_BASE);
  });

  it("a medium drawer overlapping the graph's right side DOES add a right inset", () => {
    const p = overlayFitPadding(container, {
      inspector: { left: 640, top: 0, right: 1000, bottom: 800 },
    });
    expect(p.right).toBeGreaterThan(OVERLAY_FIT_BASE);
  });

  it("toInsetRect narrows a DOMRect and passes null through", () => {
    expect(toInsetRect(null)).toBeNull();
    expect(toInsetRect({ left: 1, top: 2, right: 3, bottom: 4 } as DOMRect)).toEqual({
      left: 1,
      top: 2,
      right: 3,
      bottom: 4,
    });
  });
});
