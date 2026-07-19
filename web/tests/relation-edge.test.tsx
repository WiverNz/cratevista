import { describe, it, expect, beforeEach, vi } from "vitest";

vi.mock("@xyflow/react", () => import("./support/xyflow.tsx"));

import { render, screen, cleanup } from "@testing-library/react";
import { setMockZoom } from "./support/xyflow.tsx";
import { RelationEdge } from "../src/components/Graph.tsx";
import type { GraphEdge } from "../src/adapter/adapter.ts";
import type { EdgeState } from "../src/adapter/relationStyle.ts";

function graphEdge(kind: string): GraphEdge {
  return {
    id: `e:${kind}`,
    source: "a",
    target: "b",
    kind,
    label: kind,
    style: { category: kind, color: "", known: true },
  };
}

function renderEdge(opts: {
  kind: string;
  state?: EdgeState;
  label?: string;
  repeated?: boolean;
}) {
  const edge = graphEdge(opts.kind);
  const props = {
    id: edge.id,
    sourceX: 0,
    sourceY: 0,
    targetX: 10,
    targetY: 0,
    data: {
      edge,
      label: opts.label ?? edge.label,
      state: opts.state ?? "normal",
      repeated: opts.repeated ?? false,
    },
  };
  // The component only reads the fields provided above.
  return render(
    <svg>
      <RelationEdge {...(props as unknown as Parameters<typeof RelationEdge>[0])} />
    </svg>,
  );
}

function path(kind: string): HTMLElement {
  return screen.getByTestId(`edgepath-e:${kind}`);
}

beforeEach(() => {
  setMockZoom(1);
  cleanup();
});

describe("RelationEdge visual encoding", () => {
  it("draws depends_on thick, solid, with a directional arrow marker", () => {
    renderEdge({ kind: "depends_on" });
    const p = path("depends_on");
    expect(p.dataset.stroke).toBe("var(--rel-depends-on)");
    expect(p.dataset.width).toBe("2.5");
    expect(p.dataset.dash).toBe(""); // solid → no dasharray
    expect(p.dataset.marker).toBe("url(#cv-edge-arrow-depends-on)");
  });

  it("draws contains quiet, dotted and WITHOUT an arrow marker", () => {
    renderEdge({ kind: "contains" });
    const p = path("contains");
    expect(p.dataset.stroke).toBe("var(--rel-contains)");
    expect(p.dataset.width).toBe("1");
    expect(p.dataset.dash).not.toBe(""); // dotted → has a dasharray
    expect(p.dataset.marker).toBe(""); // no directional marker
  });

  it("emphasizes a selected edge and fades an unrelated one", () => {
    renderEdge({ kind: "depends_on", state: "selected" });
    expect(path("depends_on").dataset.width).toBe("4"); // 2.5 + 1.5
    expect(path("depends_on").dataset.opacity).toBe("1");
    cleanup();
    renderEdge({ kind: "depends_on", state: "faded" });
    expect(path("depends_on").dataset.opacity).toBe("0.1");
  });
});

describe("RelationEdge zoom-dependent labels", () => {
  it("shows a repeated label at a useful zoom but hides it when zoomed out", () => {
    setMockZoom(1);
    renderEdge({ kind: "depends_on", label: "depends on", repeated: true });
    expect(screen.getByText("depends on")).toBeInTheDocument();

    cleanup();
    setMockZoom(0.2);
    renderEdge({ kind: "depends_on", label: "depends on", repeated: true });
    expect(screen.queryByText("depends on")).not.toBeInTheDocument();
  });

  it("keeps a selected edge's label visible even when zoomed out", () => {
    setMockZoom(0.2);
    renderEdge({ kind: "depends_on", label: "depends on", state: "selected", repeated: true });
    expect(screen.getByText("depends on")).toBeInTheDocument();
  });
});
