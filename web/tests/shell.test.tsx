// Issue 15 Phase 1: four-region shell structure + native-control semantics.
// These pin WHERE each capability lives (global header vs view-nav vs workspace
// vs inspector) and that the reorganization kept native semantics — not how it
// looks. Styling/token contracts live in tokens.test.ts.
import { describe, it, expect, beforeEach, vi } from "vitest";

vi.mock("@xyflow/react", () => import("./support/xyflow.tsx"));

import { screen, within } from "@testing-library/react";
import { renderApp, okOutcome, sampleDocument } from "./support/harness.tsx";

async function ready() {
  return screen.findByRole("tablist", { name: "Views" });
}

beforeEach(() => {
  window.history.pushState(null, "", "/");
});

describe("global header (region A)", () => {
  it("shows a visible level-1 project title from the document project name", async () => {
    renderApp({});
    await ready();
    const banner = screen.getByRole("banner");
    expect(within(banner).getByRole("heading", { level: 1, name: "Demo" })).toBeInTheDocument();
  });

  it("falls back to the product name when the project name is blank", async () => {
    const doc = sampleDocument();
    doc.project = { ...doc.project, name: "   " };
    renderApp({ outcome: okOutcome({ document: doc }) });
    await ready();
    const banner = screen.getByRole("banner");
    expect(within(banner).getByRole("heading", { level: 1, name: "CrateVista" })).toBeInTheDocument();
  });

  it("contains the search box and the kind-filter group", async () => {
    renderApp({});
    await ready();
    const banner = screen.getByRole("banner");
    expect(within(banner).getByRole("searchbox", { name: "Search entities" })).toBeInTheDocument();
    expect(within(banner).getByRole("group", { name: "Kinds" })).toBeInTheDocument();
  });

  it("does NOT contain view selection", async () => {
    renderApp({});
    await ready();
    const banner = screen.getByRole("banner");
    expect(within(banner).queryByRole("tablist", { name: "Views" })).not.toBeInTheDocument();
    expect(within(banner).queryByRole("tab")).not.toBeInTheDocument();
  });

  it("does NOT contain graph-local edge/focus controls", async () => {
    renderApp({});
    await ready();
    const banner = screen.getByRole("banner");
    expect(within(banner).queryByLabelText("Edge visibility")).not.toBeInTheDocument();
    expect(within(banner).queryByRole("button", { name: "Hide unrelated" })).not.toBeInTheDocument();
    expect(within(banner).queryByRole("button", { name: "Fit" })).not.toBeInTheDocument();
  });
});

describe("view navigation (region B)", () => {
  it("renders the Views tablist with the active tab marked", async () => {
    renderApp({});
    await ready();
    const nav = screen.getByRole("navigation", { name: "Views" });
    const tablist = within(nav).getByRole("tablist", { name: "Views" });
    expect(within(tablist).getByRole("tab", { name: "Workspace overview" })).toHaveAttribute(
      "aria-selected",
      "true",
    );
  });
});

describe("graph workspace (region C)", () => {
  it("hosts the graph-local edge/focus controls in the 'Graph controls' toolbar", async () => {
    renderApp({});
    await ready();
    const toolbar = screen.getByRole("toolbar", { name: "Graph controls" });
    // The toolbar is not inside the global header.
    expect(within(screen.getByRole("banner")).queryByRole("toolbar")).not.toBeInTheDocument();
    expect(within(toolbar).getByLabelText("Edge visibility")).toBeInTheDocument();
    expect(within(toolbar).getByRole("button", { name: "Hide unrelated" })).toBeInTheDocument();
    expect(within(toolbar).getByRole("button", { name: "Dim unrelated" })).toBeInTheDocument();
    expect(within(toolbar).getByRole("button", { name: "Clear focus" })).toBeInTheDocument();
  });

  it("keeps fit/zoom/reset controls in the workspace", async () => {
    renderApp({});
    await ready();
    for (const name of ["Fit", "Zoom in", "Zoom out", "Reset"]) {
      expect(screen.getByRole("button", { name })).toBeInTheDocument();
    }
  });
});

describe("inspector (region D)", () => {
  it("is a dedicated complementary landmark", async () => {
    renderApp({});
    await ready();
    expect(screen.getByRole("complementary", { name: "Details inspector" })).toBeInTheDocument();
  });
});

describe("native semantics preserved", () => {
  it("edge visibility is a real <select>, focus controls are real <button>s", async () => {
    renderApp({});
    await ready();
    expect((screen.getByLabelText("Edge visibility") as HTMLElement).tagName).toBe("SELECT");
    expect((screen.getByRole("button", { name: "Hide unrelated" }) as HTMLElement).tagName).toBe(
      "BUTTON",
    );
  });

  it("the kind filter keeps fieldset/legend association", async () => {
    renderApp({ search: "?view=view:types" });
    await ready();
    const group = screen.getByRole("group", { name: "Kinds" });
    expect(group.tagName).toBe("FIELDSET");
    expect(within(group).getByRole("checkbox", { name: "struct" })).toBeInTheDocument();
  });
});

describe("no duplicated controls", () => {
  it("has exactly one search box, one edge-visibility select, one Views tablist", async () => {
    renderApp({});
    await ready();
    expect(screen.getAllByRole("searchbox", { name: "Search entities" })).toHaveLength(1);
    expect(screen.getAllByLabelText("Edge visibility")).toHaveLength(1);
    expect(screen.getAllByRole("tablist", { name: "Views" })).toHaveLength(1);
  });
});

describe("semantic landmarks", () => {
  it("exposes banner / navigation / main / complementary", async () => {
    renderApp({});
    await ready();
    expect(screen.getByRole("banner")).toBeInTheDocument();
    expect(screen.getByRole("navigation", { name: "Views" })).toBeInTheDocument();
    expect(screen.getByRole("main")).toBeInTheDocument();
    expect(screen.getByRole("complementary", { name: "Details inspector" })).toBeInTheDocument();
  });
});
