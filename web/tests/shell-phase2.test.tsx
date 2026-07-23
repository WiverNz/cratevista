// Issue 15 Phase 2 (wide/default jsdom): StageBar closure, final graph overlays,
// overlay-safe fit, and the wide inspector column.
import { describe, it, expect, beforeEach, vi } from "vitest";

vi.mock("@xyflow/react", () => import("./support/xyflow.tsx"));

import { screen, within, fireEvent, waitFor } from "@testing-library/react";
import { controlCalls, fitViewCalls } from "./support/xyflow.tsx";
import { renderApp, STRUCT } from "./support/harness.tsx";

async function ready() {
  return screen.findByRole("tablist", { name: "Views" });
}

beforeEach(() => {
  controlCalls.length = 0;
  fitViewCalls.length = 0;
  window.history.pushState(null, "", "/");
});

describe("StageBar structural closure (four regions, no fifth)", () => {
  it("has header/nav/main shell regions and NO top-level stage region", async () => {
    renderApp({});
    await ready();
    expect(screen.getByRole("banner")).toBeInTheDocument();
    expect(screen.getByRole("navigation", { name: "Views" })).toBeInTheDocument();
    expect(screen.getByRole("main")).toBeInTheDocument();
    expect(screen.getByRole("complementary", { name: "Details inspector" })).toBeInTheDocument();
    // The removed fifth top-level row must not exist.
    expect(document.querySelector(".cv-region-stage")).toBeNull();
    const shell = document.querySelector(".cv-shell")!;
    const regionTags = [...shell.children]
      .map((c) => c.tagName)
      .filter((t) => ["HEADER", "NAV", "MAIN"].includes(t));
    expect(regionTags).toEqual(["HEADER", "NAV", "MAIN"]);
  });

  it("renders no stage strip for a view without stages", async () => {
    renderApp({});
    await ready();
    expect(document.querySelector(".cv-workspace-stage-slot")).toBeNull();
    expect(screen.queryByRole("tablist", { name: "Stages" })).not.toBeInTheDocument();
  });

  it("nests StageBar inside the workspace (not a top-level row) when the view has stages", async () => {
    renderApp({});
    await ready();
    fireEvent.click(screen.getByRole("tab", { name: "Staged" }));
    const stages = await screen.findByRole("tablist", { name: "Stages" });
    // Still no top-level stage region, and the bar is a workspace descendant.
    expect(document.querySelector(".cv-region-stage")).toBeNull();
    const workspace = document.getElementById("cv-graph-panel")!;
    expect(workspace.contains(stages)).toBe(true);
    expect(document.querySelector(".cv-workspace-stage-slot")).not.toBeNull();
  });
});

describe("final graph overlays", () => {
  it("places canvas / edge-focus / legend in their overlay corners", async () => {
    renderApp({});
    await ready();
    const tl = document.querySelector(".cv-overlay-panel--tl")! as HTMLElement;
    const tr = document.querySelector(".cv-overlay-panel--tr")! as HTMLElement;
    const bl = document.querySelector(".cv-overlay-panel--bl")! as HTMLElement;
    expect(within(tl).getByRole("button", { name: "Fit" })).toBeInTheDocument();
    expect(within(tr).getByLabelText("Edge visibility")).toBeInTheDocument();
    expect(within(tr).getByRole("button", { name: "Hide unrelated" })).toBeInTheDocument();
    expect(within(bl).getByRole("heading", { name: "Legend" })).toBeInTheDocument();
  });

  it("keeps every graph-local control out of the global header", async () => {
    renderApp({});
    await ready();
    const banner = screen.getByRole("banner");
    expect(within(banner).queryByRole("button", { name: "Fit" })).not.toBeInTheDocument();
    expect(within(banner).queryByLabelText("Edge visibility")).not.toBeInTheDocument();
    expect(within(banner).queryByRole("button", { name: "Hide unrelated" })).not.toBeInTheDocument();
  });

  it("uses exactly three overlay panels sharing one primitive, with no duplicated controls", async () => {
    renderApp({});
    await ready();
    expect(document.querySelectorAll(".cv-overlay-panel")).toHaveLength(3);
    expect(screen.getAllByRole("button", { name: "Fit" })).toHaveLength(1);
    expect(screen.getAllByLabelText("Edge visibility")).toHaveLength(1);
  });
});

describe("overlay-safe fit", () => {
  it("issues a per-side padding object on explicit Fit", async () => {
    renderApp({});
    await ready();
    fireEvent.click(screen.getByRole("button", { name: "Fit" }));
    const last = fitViewCalls.at(-1) as { padding?: Record<string, number> } | undefined;
    expect(last).toBeTruthy();
    expect(last!.padding).toBeTruthy();
    for (const side of ["top", "right", "bottom", "left"] as const) {
      expect(typeof last!.padding![side]).toBe("number");
    }
  });

  it("Fit does not request an ELK layout", async () => {
    const { layout } = renderApp({});
    await ready();
    const before = layout.calls.length;
    fireEvent.click(screen.getByRole("button", { name: "Fit" }));
    expect(layout.calls.length).toBe(before);
  });
});

describe("wide inspector", () => {
  it("is the complementary grid column and opens no dialog on selection", async () => {
    const { layout } = renderApp({});
    await ready();
    const aside = screen.getByRole("complementary", { name: "Details inspector" });
    expect(aside).toHaveClass("cv-inspector--wide");
    await waitFor(() => expect(layout.calls.length).toBeGreaterThan(0)); // initial layout settled
    const before = layout.calls.length;
    fireEvent.click(await screen.findByTestId(`node-${STRUCT}`));
    await screen.findByRole("region", { name: "Entity inspector" });
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
    expect(layout.calls.length).toBe(before); // selection never relayouts
  });
});
