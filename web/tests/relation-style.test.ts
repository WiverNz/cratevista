import { describe, it, expect } from "vitest";

import {
  relationStyleFor,
  edgeVisual,
  markerId,
  dashArrayFor,
  shouldShowEdgeLabel,
  edgeZIndex,
  allRelationStyles,
  UNKNOWN_RELATION_STYLE,
  LABEL_ZOOM_MIN,
  LABEL_ZOOM_REPEATED,
  type EdgeState,
} from "../src/adapter/relationStyle.ts";
import { relationLegendForGraph } from "../src/state/selectors.ts";
import type { Graph, GraphEdge } from "../src/adapter/adapter.ts";

/** Minimal edge for legend tests. */
function edge(kind: string): GraphEdge {
  return {
    id: `e:${kind}`,
    source: "a",
    target: "b",
    kind,
    style: { category: kind, color: "", known: true },
  };
}
function graphWith(kinds: string[]): Graph {
  return { nodes: [], edges: kinds.map(edge) };
}

describe("relation-style registry: known mappings", () => {
  // Every relation the registry must support, with its discriminating fields.
  const cases: Array<{
    kind: string;
    label: string;
    pattern: string;
    marker: string;
    token: string;
  }> = [
    { kind: "contains", label: "contains", pattern: "dotted", marker: "none", token: "--rel-contains" },
    { kind: "depends_on", label: "depends on", pattern: "solid", marker: "arrow-closed", token: "--rel-depends-on" },
    { kind: "implements", label: "implements", pattern: "dashed", marker: "arrow-closed", token: "--rel-implements" },
    { kind: "implemented_by", label: "implemented by", pattern: "dashed", marker: "arrow", token: "--rel-implemented-by" },
    { kind: "implemented_for", label: "implemented for", pattern: "dashed", marker: "arrow", token: "--rel-implemented-for" },
    { kind: "calls", label: "calls", pattern: "solid", marker: "arrow", token: "--rel-calls" },
    { kind: "uses", label: "uses", pattern: "dashed", marker: "arrow", token: "--rel-uses" },
    { kind: "manual", label: "manual flow", pattern: "dashed", marker: "arrow", token: "--rel-manual" },
    { kind: "has_field_type", label: "has field type", pattern: "solid", marker: "arrow", token: "--rel-has-field-type" },
    { kind: "accepts_type", label: "accepts type", pattern: "dashed", marker: "arrow", token: "--rel-accepts-type" },
    { kind: "returns_type", label: "returns type", pattern: "solid", marker: "arrow", token: "--rel-returns-type" },
    { kind: "re_exports", label: "re-exports", pattern: "dashed", marker: "arrow", token: "--rel-re-exports" },
  ];

  for (const c of cases) {
    it(`maps ${c.kind}`, () => {
      const s = relationStyleFor(c.kind);
      expect(s.known).toBe(true);
      expect(s.label).toBe(c.label);
      expect(s.pattern).toBe(c.pattern);
      expect(s.marker).toBe(c.marker);
      expect(s.strokeToken).toBe(c.token);
      // Every known style resolves a stroke via a CSS token (dark/light aware).
      expect(edgeVisual(s, "normal").stroke).toBe(`var(${c.token})`);
    });
  }
});

describe("relation-style registry: unknown fallback", () => {
  it("returns a neutral, safe style for an unrecognized kind", () => {
    const s = relationStyleFor("totally_made_up_kind");
    expect(s.known).toBe(false);
    expect(s.label).toBe("totally_made_up_kind"); // raw kind is preserved
    expect(s.strokeToken).toBe("--rel-unknown");
    // Still fully renderable (never crashes / never undefined).
    expect(s.width).toBeGreaterThan(0);
    expect(edgeVisual(s, "normal").stroke).toBe("var(--rel-unknown)");
    expect(markerId(s)).toBe("cv-edge-arrow-unknown");
  });

  it("exposes a canonical unknown style in the defs list", () => {
    const ids = allRelationStyles().map((s) => markerId(s));
    expect(ids).toContain("cv-edge-arrow-unknown");
    expect(UNKNOWN_RELATION_STYLE.known).toBe(false);
  });
});

describe("contains vs depends_on distinction (not colour alone)", () => {
  const contains = relationStyleFor("contains");
  const depends = relationStyleFor("depends_on");

  it("differ in colour token, width and pattern", () => {
    expect(contains.strokeToken).not.toBe(depends.strokeToken);
    expect(contains.width).not.toBe(depends.width);
    expect(contains.pattern).not.toBe(depends.pattern);
  });

  it("contains is quiet/subordinate; depends_on is strong", () => {
    expect(contains.emphasis).toBe("subordinate");
    expect(depends.emphasis).toBe("strong");
    // depends_on is visually heavier than contains.
    expect(depends.width).toBeGreaterThan(contains.width);
    // z-order keeps depends_on above containment.
    expect(edgeZIndex(depends, "normal")).toBeGreaterThan(edgeZIndex(contains, "normal"));
  });

  it("only depends_on carries a directional arrow", () => {
    expect(depends.marker).not.toBe("none");
    expect(markerId(depends)).toBe("cv-edge-arrow-depends-on");
    expect(contains.marker).toBe("none");
    expect(markerId(contains)).toBeNull();
  });
});

describe("depends_on direction marker", () => {
  it("resolves to an arrowhead in every non-faded state", () => {
    const depends = relationStyleFor("depends_on");
    for (const state of ["normal", "related", "selected"] as EdgeState[]) {
      expect(edgeVisual(depends, state).marker).toBe("arrow-closed");
    }
  });
});

describe("selected / related / faded states", () => {
  const depends = relationStyleFor("depends_on");
  it("selected is the thickest and fully opaque", () => {
    const normal = edgeVisual(depends, "normal");
    const selected = edgeVisual(depends, "selected");
    expect(selected.strokeWidth).toBeGreaterThan(normal.strokeWidth);
    expect(selected.opacity).toBe(1);
  });
  it("related is stronger than normal", () => {
    const normal = edgeVisual(depends, "normal");
    const related = edgeVisual(depends, "related");
    expect(related.strokeWidth).toBeGreaterThan(normal.strokeWidth);
    expect(related.opacity).toBeGreaterThanOrEqual(normal.opacity);
  });
  it("faded drops to a substantially lower opacity", () => {
    const normal = edgeVisual(depends, "normal");
    const faded = edgeVisual(depends, "faded");
    expect(faded.opacity).toBeLessThanOrEqual(0.1);
    expect(faded.opacity).toBeLessThan(normal.opacity);
  });
});

describe("dash patterns", () => {
  it("solid has no dasharray; dashed and dotted differ", () => {
    expect(dashArrayFor("solid", 2)).toBeUndefined();
    const dashed = dashArrayFor("dashed", 2);
    const dotted = dashArrayFor("dotted", 2);
    expect(dashed).toBeTruthy();
    expect(dotted).toBeTruthy();
    expect(dashed).not.toBe(dotted);
  });
});

describe("zoom-dependent label visibility", () => {
  it("hides non-forced labels below the minimum zoom", () => {
    expect(
      shouldShowEdgeLabel({ zoom: LABEL_ZOOM_MIN - 0.05, state: "normal", hovered: false, repeated: false }),
    ).toBe(false);
  });
  it("hides repeated labels until a useful zoom, then shows them", () => {
    expect(
      shouldShowEdgeLabel({ zoom: 0.5, state: "normal", hovered: false, repeated: true }),
    ).toBe(false);
    expect(
      shouldShowEdgeLabel({ zoom: LABEL_ZOOM_REPEATED, state: "normal", hovered: false, repeated: true }),
    ).toBe(true);
  });
  it("shows a non-repeated label at a moderate zoom", () => {
    expect(
      shouldShowEdgeLabel({ zoom: 0.5, state: "normal", hovered: false, repeated: false }),
    ).toBe(true);
  });
  it("always shows forced labels (selected / related / hovered) regardless of zoom", () => {
    const low = 0.1;
    expect(shouldShowEdgeLabel({ zoom: low, state: "selected", hovered: false, repeated: true })).toBe(true);
    expect(shouldShowEdgeLabel({ zoom: low, state: "related", hovered: false, repeated: true })).toBe(true);
    expect(shouldShowEdgeLabel({ zoom: low, state: "normal", hovered: true, repeated: true })).toBe(true);
  });
});

describe("relationLegendForGraph", () => {
  it("contains only relation kinds present in the graph", () => {
    const legend = relationLegendForGraph(graphWith(["contains", "depends_on", "depends_on"]));
    expect(legend.map((e) => e.kind).sort()).toEqual(["contains", "depends_on"]);
  });

  it("orders strong relations before subordinate ones (depends_on before contains)", () => {
    const legend = relationLegendForGraph(graphWith(["contains", "depends_on"]));
    expect(legend.map((e) => e.kind)).toEqual(["depends_on", "contains"]);
  });

  it("carries the registry style verbatim (no redefinition)", () => {
    const legend = relationLegendForGraph(graphWith(["depends_on"]));
    expect(legend[0].style).toEqual(relationStyleFor("depends_on"));
  });

  it("marks unknown relation kinds as unknown", () => {
    const legend = relationLegendForGraph(graphWith(["weird_kind"]));
    expect(legend[0].known).toBe(false);
    expect(legend[0].style.strokeToken).toBe("--rel-unknown");
  });
});
