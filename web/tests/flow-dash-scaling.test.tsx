import { describe, it, expect, beforeEach, vi } from "vitest";
import { cleanup, render, screen, within } from "@testing-library/react";

vi.mock("@xyflow/react", () => import("./support/xyflow.tsx"));

import { setMockZoom } from "./support/xyflow.tsx";
import { flowDash, FLOW_DASH_MULT, FLOW_GAP_MULT } from "../src/adapter/relationStyle.ts";
import { RelationEdge } from "../src/components/Graph.tsx";
import { Legend } from "../src/components/Panels.tsx";

beforeEach(() => {
  setMockZoom(1);
  cleanup();
});

describe("flowDash — width-proportional geometry (Phase-2 gap closure)", () => {
  it("scales dash and gap linearly with the effective stroke width", () => {
    const a = flowDash(2);
    const b = flowDash(4);
    expect(b.dash).toBeCloseTo(a.dash * 2, 5);
    expect(b.gap).toBeCloseTo(a.gap * 2, 5);
    // cycle is dash+gap (seamless travel distance).
    expect(a.cycle).toBeCloseTo(a.dash + a.gap, 5);
  });
  it("uses the centralized multipliers, not a fixed literal", () => {
    const fd = flowDash(3);
    expect(fd.dash).toBeCloseTo(3 * FLOW_DASH_MULT, 5);
    expect(fd.gap).toBeCloseTo(3 * FLOW_GAP_MULT, 5);
    // A wider edge must NOT resolve to the old fixed "9 7".
    expect(fd.dashArray).not.toBe("9 7");
  });
  it("still yields the familiar 9 7 at the manual base width (2), but derived", () => {
    expect(flowDash(2).dashArray).toBe("9 7");
  });
});

// --- graph edge scales per state -------------------------------------------
function renderFlowEdge(state: "normal" | "related" | "selected" | "faded", motionAllowed = true) {
  const edge = {
    id: "e:1",
    source: "a",
    target: "b",
    kind: "manual",
    label: "flow",
    style: { category: "manual", color: "", known: true },
    flowEligible: true,
  };
  const props = {
    id: "e:1",
    sourceX: 0,
    sourceY: 0,
    targetX: 40,
    targetY: 0,
    data: { edge, label: "flow", state, repeated: false, flowMotionAllowed: motionAllowed },
  };
  render(
    <svg>
      <RelationEdge {...(props as unknown as Parameters<typeof RelationEdge>[0])} />
    </svg>,
  );
  return screen.getByTestId("edgepath-e:1");
}

describe("RelationEdge — flow dash scales with the state's effective width", () => {
  // manual base width 2 → related 2.5 → selected 3.5 → faded 2 (quiet).
  it("normal uses width 2 → 9 7", () => {
    expect(renderFlowEdge("normal").dataset.flowdash).toBe(flowDash(2).dashArray);
  });
  it("related uses the wider related width", () => {
    expect(renderFlowEdge("related").dataset.flowdash).toBe(flowDash(2.5).dashArray);
  });
  it("selected uses the widest selected width (dash grows proportionally)", () => {
    const sel = renderFlowEdge("selected").dataset.flowdash!;
    expect(sel).toBe(flowDash(3.5).dashArray);
    // Strictly larger than the normal-state dash.
    expect(parseFloat(sel.split(" ")[0])).toBeGreaterThan(flowDash(2).dash);
  });
  it("faded keeps a quiet static flow dash (base width, no motion class)", () => {
    const p = renderFlowEdge("faded");
    expect(p.dataset.flowdash).toBe(flowDash(2).dashArray);
    expect(p.dataset.classname).toBe("cv-edge-flow"); // static only
  });
  it("sets a matching seamless cycle (dash+gap) per state", () => {
    const p = renderFlowEdge("selected");
    expect(p.dataset.flowcycle).toBe(String(flowDash(3.5).cycle));
  });
  it("reduced-motion / suppressed edge keeps the SAME scaled static dash", () => {
    const suppressed = renderFlowEdge("selected", /*motionAllowed*/ false);
    expect(suppressed.dataset.classname).toBe("cv-edge-flow"); // no --motion
    expect(suppressed.dataset.flowdash).toBe(flowDash(3.5).dashArray); // still scaled
  });
});

// --- legend uses the SAME helper -------------------------------------------
describe("Legend flow sample shares the flowDash contract", () => {
  it("legend line carries the same scaled dash the helper produces at width 2", () => {
    render(<Legend entries={[]} relations={[]} flow={{ present: true, motionEnabled: true, suppressedByCount: false }} />);
    const line = within(screen.getByRole("group", { name: "Active flow" })).getByRole("img").querySelector("line")!;
    const style = line.getAttribute("style") ?? "";
    const fd = flowDash(2);
    expect(style).toContain(`--edge-flow-dash: ${fd.dashArray}`);
    expect(style).toContain(`--edge-flow-dash-cycle: ${fd.cycle}`);
  });
});
