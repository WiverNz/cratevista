import { describe, it, expect, beforeEach, vi } from "vitest";

vi.mock("@xyflow/react", () => import("./support/xyflow.tsx"));

import { screen, fireEvent, waitFor } from "@testing-library/react";
import { controlCalls } from "./support/xyflow.tsx";
import { renderApp, STRUCT, ENUM } from "./support/harness.tsx";

const WS = "workspace";
const PKG = "package:demo";
const MOD = "module:demo::app";

async function ready() {
  return screen.findByRole("tablist", { name: "Views" });
}

beforeEach(() => {
  controlCalls.length = 0;
  window.history.pushState(null, "", "/");
});

describe("graph controls", () => {
  it("Reset clears search/filters/selection/stage, keeps the view, and fits", async () => {
    renderApp({});
    await ready();
    // Establish some state.
    fireEvent.change(screen.getByRole("searchbox", { name: "Search entities" }), {
      target: { value: "Thing" },
    });
    fireEvent.click(screen.getByTestId(`node-${STRUCT}`));
    await screen.findByRole("region", { name: "Entity inspector" });
    fireEvent.click(screen.getByRole("checkbox", { name: "struct" }));
    expect((screen.getByRole("checkbox", { name: "struct" }) as HTMLInputElement).checked).toBe(true);

    fireEvent.click(screen.getByRole("button", { name: "Reset" }));

    expect((screen.getByRole("searchbox", { name: "Search entities" }) as HTMLInputElement).value).toBe("");
    expect(screen.queryByRole("region", { name: "Entity inspector" })).not.toBeInTheDocument();
    expect((screen.getByRole("checkbox", { name: "struct" }) as HTMLInputElement).checked).toBe(false);
    expect(screen.getByRole("tab", { name: "Workspace overview" })).toHaveAttribute("aria-selected", "true");
    expect(controlCalls).toContain("fitView");
  });

  it("Zoom in / out invoke the flow instance", async () => {
    renderApp({});
    await ready();
    fireEvent.click(screen.getByRole("button", { name: "Zoom in" }));
    fireEvent.click(screen.getByRole("button", { name: "Zoom out" }));
    expect(controlCalls).toContain("zoomIn");
    expect(controlCalls).toContain("zoomOut");
  });

  it("edge mode hidden removes all edges; all shows them", async () => {
    renderApp({});
    await ready();
    expect(screen.getByTestId("edge-rel:contains:ws-pkg")).toBeInTheDocument();
    fireEvent.change(screen.getByLabelText("Edge visibility"), { target: { value: "hidden" } });
    expect(screen.queryByTestId("edge-rel:contains:ws-pkg")).not.toBeInTheDocument();
    fireEvent.change(screen.getByLabelText("Edge visibility"), { target: { value: "all" } });
    expect(screen.getByTestId("edge-rel:contains:ws-pkg")).toBeInTheDocument();
  });

  it("edge mode related shows only edges touching the selection", async () => {
    renderApp({});
    await ready();
    fireEvent.click(screen.getByTestId(`node-${PKG}`));
    fireEvent.change(screen.getByLabelText("Edge visibility"), { target: { value: "related" } });
    // pkg touches ws-pkg and pkg-mod; not struct-enum.
    expect(screen.getByTestId("edge-rel:contains:ws-pkg")).toBeInTheDocument();
    expect(screen.getByTestId("edge-rel:contains:pkg-mod")).toBeInTheDocument();
    expect(screen.queryByTestId("edge-rel:has_field_type:struct-enum")).not.toBeInTheDocument();
  });

  it("Related only (focus mode) hides unrelated nodes", async () => {
    renderApp({});
    await ready();
    fireEvent.click(screen.getByTestId(`node-${PKG}`));
    fireEvent.click(screen.getByRole("button", { name: /Related only/ }));
    // pkg + neighbors (ws, mod) visible; struct/enum hidden.
    expect(screen.getByTestId(`node-${PKG}`)).toBeInTheDocument();
    expect(screen.getByTestId(`node-${WS}`)).toBeInTheDocument();
    expect(screen.getByTestId(`node-${MOD}`)).toBeInTheDocument();
    expect(screen.queryByTestId(`node-${STRUCT}`)).not.toBeInTheDocument();
    expect(screen.queryByTestId(`node-${ENUM}`)).not.toBeInTheDocument();
  });

  it("double-click focuses/expands around a node without blocking selection", async () => {
    renderApp({});
    await ready();
    const node = screen.getByTestId(`node-${STRUCT}`);
    fireEvent.click(node); // immediate selection
    await screen.findByRole("region", { name: "Entity inspector" });
    fireEvent.doubleClick(node); // adds focus
    await waitFor(() => {
      // focus mode → only struct + neighbors (mod, enum); ws/pkg hidden.
      expect(screen.queryByTestId(`node-${WS}`)).not.toBeInTheDocument();
      expect(screen.getByTestId(`node-${MOD}`)).toBeInTheDocument();
      expect(screen.getByTestId(`node-${ENUM}`)).toBeInTheDocument();
    });
  });
});
