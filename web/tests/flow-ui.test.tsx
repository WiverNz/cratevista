import { describe, it, expect, beforeEach, afterEach, vi } from "vitest";

vi.mock("@xyflow/react", () => import("./support/xyflow.tsx"));

import { readFileSync } from "node:fs";
import { render, screen, cleanup, within } from "@testing-library/react";
import { setMockZoom } from "./support/xyflow.tsx";
import { RelationEdge } from "../src/components/Graph.tsx";
import { Legend, type LegendFlow } from "../src/components/Panels.tsx";
import { buildModel } from "../src/model/model.ts";
import { documentToGraph } from "../src/adapter/adapter.ts";
import { renderApp, okOutcome } from "./support/harness.tsx";
import type { ExplorerDocument } from "../src/types/index.ts";

beforeEach(() => {
  setMockZoom(1);
  cleanup();
});

// --- a small manual-flow document -----------------------------------------
const A = "block:a";
const B = "block:b";
const C = "block:c";
function flowDocument(): ExplorerDocument {
  return {
    schema_version: "1.0",
    project: { id: "d", name: "D", description: "" },
    entities: [
      { id: A, kind: "package", label: { default: "A" }, qualified_name: "a", provenance: "manual" },
      { id: B, kind: "package", label: { default: "B" }, qualified_name: "b", provenance: "manual" },
      { id: C, kind: "package", label: { default: "C" }, qualified_name: "c", provenance: "manual" },
    ],
    relations: [
      // Eligible: manual + attributes.flow = "active".
      { id: "rel:flow", kind: "manual", from: A, to: B, provenance: "manual", attributes: { flow: "active" } },
      // Ordinary manual: no attribute → static.
      { id: "rel:plain", kind: "manual", from: B, to: C, provenance: "manual" },
      // Discovered carrying the same attribute → still static (provenance gates).
      { id: "rel:disc", kind: "depends_on", from: A, to: C, provenance: "discovered", attributes: { flow: "active" } },
    ],
    views: [
      { id: "view:flow", title: { default: "Flow" }, entity_kinds: [], relation_kinds: [], stages: [], presentation: {} },
    ],
  } as unknown as ExplorerDocument;
}

// --- adapter derives flowEligible via the central helper -------------------
describe("adapter: flowEligible derivation", () => {
  it("marks only the manual + active relation eligible", () => {
    const model = buildModel(flowDocument());
    const graph = documentToGraph(model, model.viewById.get("view:flow")!);
    const byId = new Map(graph.edges.map((e) => [e.id, e.flowEligible]));
    expect(byId.get("rel:flow")).toBe(true);
    expect(byId.get("rel:plain")).toBe(false);
    expect(byId.get("rel:disc")).toBe(false);
  });
});

// --- RelationEdge flow classes (direct render) ----------------------------
function renderEdge(opts: {
  flowEligible: boolean;
  state?: "normal" | "related" | "selected" | "faded";
  flowMotionAllowed?: boolean;
  zoom?: number;
}) {
  setMockZoom(opts.zoom ?? 1);
  const edge = {
    id: "e:1",
    source: "a",
    target: "b",
    kind: "manual",
    label: "flow",
    style: { category: "manual", color: "", known: true },
    flowEligible: opts.flowEligible,
  };
  const props = {
    id: "e:1",
    sourceX: 0,
    sourceY: 0,
    targetX: 40,
    targetY: 0,
    data: {
      edge,
      label: "flow",
      state: opts.state ?? "normal",
      repeated: false,
      flowMotionAllowed: opts.flowMotionAllowed ?? true,
    },
  };
  render(
    <svg>
      <RelationEdge {...(props as unknown as Parameters<typeof RelationEdge>[0])} />
    </svg>,
  );
  return screen.getByTestId("edgepath-e:1");
}

describe("RelationEdge — flow presentation", () => {
  it("eligible + motion allowed → static flow class AND motion class", () => {
    const p = renderEdge({ flowEligible: true });
    expect(p.dataset.classname).toBe("cv-edge-flow cv-edge-flow--motion");
    // The distinct flow dash is class-driven, so no inline dasharray competes.
    expect(p.dataset.dash).toBe("");
  });
  it("eligible related/selected still animate", () => {
    expect(renderEdge({ flowEligible: true, state: "related" }).dataset.classname).toContain("--motion");
    cleanup();
    expect(renderEdge({ flowEligible: true, state: "selected" }).dataset.classname).toContain("--motion");
  });
  it("eligible FADED edge keeps the static flow class but never animates", () => {
    const p = renderEdge({ flowEligible: true, state: "faded" });
    expect(p.dataset.classname).toBe("cv-edge-flow");
  });
  it("eligible edge under view-wide suppression is static (no motion)", () => {
    expect(renderEdge({ flowEligible: true, flowMotionAllowed: false }).dataset.classname).toBe("cv-edge-flow");
  });
  it("low zoom suppresses motion but keeps the static flow class", () => {
    expect(renderEdge({ flowEligible: true, zoom: 0.2 }).dataset.classname).toBe("cv-edge-flow");
  });
  it("ineligible edge gets no flow class at all", () => {
    const p = renderEdge({ flowEligible: false });
    expect(p.dataset.classname).toBe("");
  });
  it("keeps kind stroke/width/opacity/marker from the registry (unchanged)", () => {
    const p = renderEdge({ flowEligible: true, state: "selected" });
    expect(p.dataset.stroke).toBe("var(--rel-manual)");
    expect(p.dataset.width).toBe("3.5"); // manual base 2 + selected 1.5
    expect(p.dataset.opacity).toBe("1");
    expect(p.dataset.marker).toBe("url(#cv-edge-arrow-manual)");
  });
});

// --- Legend Active-flow sample --------------------------------------------
function renderLegend(flow?: LegendFlow) {
  render(<Legend entries={[]} relations={[]} flow={flow} />);
}

describe("Legend — Active flow sample", () => {
  it("is absent when no eligible relations are present", () => {
    renderLegend({ present: false, motionEnabled: false, suppressedByCount: false });
    expect(screen.queryByRole("group", { name: "Active flow" })).toBeNull();
  });
  it("shows one focusable, direction-describing sample when motion is on", () => {
    renderLegend({ present: true, motionEnabled: true, suppressedByCount: false });
    const group = screen.getByRole("group", { name: "Active flow" });
    const sample = within(group).getByRole("img");
    expect(sample).toHaveAttribute("tabindex", "0");
    expect(sample.getAttribute("aria-label")).toMatch(/flow/i);
    expect(sample.getAttribute("aria-label")).toMatch(/source to target/i);
    expect(sample.getAttribute("aria-label")).toMatch(/animated/i);
    expect(sample.getAttribute("data-motion")).toBe("on");
  });
  it("renders statically with a note when motion is off (reduced or otherwise)", () => {
    renderLegend({ present: true, motionEnabled: false, suppressedByCount: false });
    const sample = within(screen.getByRole("group", { name: "Active flow" })).getByRole("img");
    expect(sample.getAttribute("data-motion")).toBe("off");
    expect(sample.getAttribute("aria-label")).toMatch(/arrow shows direction/i);
    expect(screen.getByText(/motion off/i)).toBeTruthy();
  });
  it("annotates the many-flows suppression above the threshold", () => {
    renderLegend({ present: true, motionEnabled: false, suppressedByCount: true });
    expect(screen.getByText(/motion off: many flows/i)).toBeTruthy();
  });
  it("reuses the shared cv-edge-flow classes (no independent legend animation)", () => {
    renderLegend({ present: true, motionEnabled: true, suppressedByCount: false });
    const line = screen.getByRole("group", { name: "Active flow" }).querySelector("line")!;
    expect(line.classList.contains("cv-edge-flow")).toBe(true);
    expect(line.classList.contains("cv-edge-flow--motion")).toBe(true);
    cleanup();
    renderLegend({ present: true, motionEnabled: false, suppressedByCount: false });
    const still = screen.getByRole("group", { name: "Active flow" }).querySelector("line")!;
    expect(still.classList.contains("cv-edge-flow")).toBe(true);
    expect(still.classList.contains("cv-edge-flow--motion")).toBe(false);
  });
});

// --- reduced-motion end to end (folded in GraphInner) ----------------------
describe("reduced motion (app-level fold)", () => {
  const original = window.matchMedia;
  function mockReduce(reduce: boolean) {
    window.matchMedia = vi.fn().mockImplementation((q: string) => ({
      matches: reduce && q.includes("reduce"),
      media: q,
      addEventListener: () => {},
      removeEventListener: () => {},
      addListener: () => {},
      removeListener: () => {},
      onchange: null,
      dispatchEvent: () => false,
    })) as unknown as typeof window.matchMedia;
  }
  afterEach(() => {
    window.matchMedia = original;
  });

  it("legend animates when motion is not reduced", async () => {
    mockReduce(false);
    renderApp({ outcome: okOutcome({ document: flowDocument() }), search: "?view=view:flow" });
    const sample = within(await screen.findByRole("group", { name: "Active flow" })).getByRole("img");
    expect(sample.getAttribute("data-motion")).toBe("on");
  });

  it("under prefers-reduced-motion the legend flow sample is static", async () => {
    mockReduce(true);
    renderApp({ outcome: okOutcome({ document: flowDocument() }), search: "?view=view:flow" });
    const sample = within(await screen.findByRole("group", { name: "Active flow" })).getByRole("img");
    expect(sample.getAttribute("data-motion")).toBe("off");
  });
});

// --- CSS contract: explicit reduced-motion rule + direction ----------------
describe("flow CSS contract", () => {
  const css = readFileSync("src/styles.css", "utf8");
  it("explicitly disables the flow animation under prefers-reduced-motion (not only the global kill-switch)", () => {
    const block = css.slice(css.indexOf(".cv-edge-label"));
    expect(block).toMatch(/@media \(prefers-reduced-motion: reduce\)\s*\{\s*\.cv-edge-flow--motion\s*\{\s*animation:\s*none/);
  });
  it("dashes travel toward the target (negative dash offset)", () => {
    expect(css).toMatch(/@keyframes cv-edge-flow-dash[\s\S]*stroke-dashoffset:\s*calc\(-1/);
  });
  it("centralizes the timing token and consumes a scaled dash var (no fixed literal)", () => {
    expect(css).toMatch(/--edge-flow-duration:/);
    // The dash is CONSUMED via a var (set per element, scaled) — not defined as a
    // fixed literal in the stylesheet.
    expect(css).toMatch(/stroke-dasharray:\s*var\(--edge-flow-dash\)/);
    expect(css).not.toMatch(/--edge-flow-dash:\s*\d/);
  });
  it("keeps a static flow dash cue (the redundant, motion-independent marker)", () => {
    expect(css).toMatch(/\.cv-edge-flow\s*\{[^}]*stroke-dasharray:\s*var\(--edge-flow-dash\)/);
  });
});
