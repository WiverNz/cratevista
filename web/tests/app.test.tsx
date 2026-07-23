import { describe, it, expect, beforeEach, vi } from "vitest";

// Mock React Flow with a jsdom-friendly stub (see support/xyflow.tsx).
vi.mock("@xyflow/react", () => import("./support/xyflow.tsx"));

import { screen, fireEvent, within, waitFor, render } from "@testing-library/react";
import { controlCalls } from "./support/xyflow.tsx";
import { App } from "../src/App.tsx";
import { fakeLayout, watchDisabledLiveReload } from "./support/harness.tsx";
import {
  renderApp,
  okOutcome,
  sampleDocument,
  unknownKindDocument,
  STRUCT,
  ENUM,
} from "./support/harness.tsx";

async function ready() {
  return screen.findByRole("tablist", { name: "Views" });
}

beforeEach(() => {
  controlCalls.length = 0;
  window.history.pushState(null, "", "/");
});

describe("loading + blocking states", () => {
  it("shows a loading state while the load is pending", () => {
    const source = { calls: 0, load: () => new Promise<never>(() => {}), abort() {} };
    render(<App source={source} layout={fakeLayout().engine} initialSearch="" liveReload={watchDisabledLiveReload} />);
    expect(screen.getByRole("status")).toHaveTextContent(/Loading/i);
  });

  it("shows a blocking error with retry on document failure", async () => {
    const { source } = renderApp({ outcome: { status: "document-error", message: "HTTP 500" } });
    const alert = await screen.findByRole("alert");
    expect(alert).toHaveTextContent(/Could not load/i);
    fireEvent.click(screen.getByRole("button", { name: "Retry" }));
    await waitFor(() => expect(source.calls).toBeGreaterThan(1));
  });

  it("shows an incompatibility screen on unsupported major", async () => {
    renderApp({ outcome: { status: "incompatible", found: "2.0", supportedMajor: 1 } });
    expect(await screen.findByRole("alert")).toHaveTextContent(/Unsupported document schema version: 2.0/i);
  });
});

describe("degraded panels", () => {
  it("warns when generation is unavailable", async () => {
    renderApp({ outcome: okOutcome({ generationAvailable: false, generation: null }) });
    await ready();
    expect(screen.getByText(/Generation status unavailable/i)).toBeInTheDocument();
  });

  it("shows diagnostics-unavailable state", async () => {
    renderApp({ outcome: okOutcome({ diagnosticsAvailable: false, diagnostics: null }) });
    await ready();
    expect(screen.getByText(/Diagnostics unavailable/i)).toBeInTheDocument();
  });

  it("shows a persistent partial banner", async () => {
    renderApp({ outcome: okOutcome({ partial: true, generation: { generated_at: "t", partial: true } }) });
    await ready();
    expect(screen.getByText(/Partial generation/i)).toBeInTheDocument();
  });
});

describe("initial view + URL", () => {
  it("selects workspace-overview by default", async () => {
    renderApp({});
    await ready();
    const tab = screen.getByRole("tab", { name: "Workspace overview" });
    expect(tab).toHaveAttribute("aria-selected", "true");
  });

  it("honors a valid URL view", async () => {
    renderApp({ search: "?view=view:types" });
    await ready();
    expect(screen.getByRole("tab", { name: "Types" })).toHaveAttribute("aria-selected", "true");
  });

  it("applies default_focus when the URL selects nothing", async () => {
    renderApp({ search: "?view=view:focus" });
    await ready();
    expect(await screen.findByRole("region", { name: "Entity inspector" })).toHaveTextContent("Thing");
  });

  it("switches view on tab click", async () => {
    renderApp({});
    await ready();
    fireEvent.click(screen.getByRole("tab", { name: "Types" }));
    await waitFor(() =>
      expect(screen.getByRole("tab", { name: "Types" })).toHaveAttribute("aria-selected", "true"),
    );
  });

  it("restores view on popstate", async () => {
    renderApp({ search: "?view=view:types" });
    await ready();
    window.history.pushState(null, "", "?view=view:workspace-overview");
    fireEvent.popState(window);
    await waitFor(() =>
      expect(screen.getByRole("tab", { name: "Workspace overview" })).toHaveAttribute("aria-selected", "true"),
    );
  });
});

describe("search, filters, legend", () => {
  it("searches by label/qualified name and selects a result", async () => {
    renderApp({});
    await ready();
    fireEvent.change(screen.getByRole("searchbox", { name: "Search entities" }), {
      target: { value: "Thing" },
    });
    const option = await screen.findByRole("option", { name: /Thing/ });
    fireEvent.click(option);
    expect(await screen.findByRole("region", { name: "Entity inspector" })).toHaveTextContent("Thing");
  });

  it("filters by kind", async () => {
    renderApp({ search: "?view=view:types" });
    await ready();
    // Types view has struct + enum → both node buttons present.
    expect(screen.getByTestId(`node-${STRUCT}`)).toBeInTheDocument();
    expect(screen.getByTestId(`node-${ENUM}`)).toBeInTheDocument();
    fireEvent.click(screen.getByRole("checkbox", { name: "struct" }));
    // Now only struct kind is kept. The filter re-render flows through a state
    // update and a React Flow node refresh, which is not guaranteed to have
    // flushed by the time `fireEvent` returns on a slow machine — so wait for the
    // enum node to leave rather than asserting synchronously (a CI-only flake).
    await waitFor(() => expect(screen.queryByTestId(`node-${ENUM}`)).not.toBeInTheDocument());
    expect(screen.getByTestId(`node-${STRUCT}`)).toBeInTheDocument();
  });

  it("legend reflects present categories", async () => {
    renderApp({ search: "?view=view:types" });
    await ready();
    const legend = screen.getByLabelText("Legend");
    expect(within(legend).getByText("Struct")).toBeInTheDocument();
    expect(within(legend).getByText("Enum")).toBeInTheDocument();
  });

  it("renders unknown kinds with a generic legend entry", async () => {
    renderApp({ outcome: okOutcome({ document: unknownKindDocument() }) });
    await ready();
    const legend = screen.getByLabelText("Legend");
    expect(within(legend).getByText(/widget/)).toBeInTheDocument();
    // Both an unknown entity kind and an unknown relation kind carry the marker.
    expect(within(legend).getAllByText(/\(unknown\)/).length).toBeGreaterThan(0);
    expect(screen.getByTestId("node-item:widget")).toBeInTheDocument();
  });
});

describe("selection + inspector", () => {
  it("selecting a node populates the entity inspector", async () => {
    renderApp({});
    await ready();
    fireEvent.click(screen.getByTestId(`node-${STRUCT}`));
    const inspector = await screen.findByRole("region", { name: "Entity inspector" });
    expect(inspector).toHaveTextContent("Thing");
    expect(within(inspector).getByText("struct")).toBeInTheDocument();
    expect(within(inspector).getByText("demo::app::Thing")).toBeInTheDocument();
    // grouped relations + source + diagnostics present (the repo-relative path
    // appears both in the fields list and in the source section).
    expect(within(inspector).getAllByText("src/app.rs").length).toBeGreaterThan(0);
    expect(within(inspector).getByText(/unresolved_type/)).toBeInTheDocument();
  });

  it("selecting an edge populates the relation inspector", async () => {
    renderApp({});
    await ready();
    fireEvent.click(screen.getByTestId("edge-rel:contains:mod-struct"));
    const inspector = await screen.findByRole("region", { name: "Relation inspector" });
    expect(within(inspector).getByText("contains")).toBeInTheDocument();
  });

  it("Escape clears the selection", async () => {
    renderApp({});
    await ready();
    fireEvent.click(screen.getByTestId(`node-${STRUCT}`));
    await screen.findByRole("region", { name: "Entity inspector" });
    fireEvent.keyDown(window, { key: "Escape" });
    await waitFor(() =>
      expect(screen.queryByRole("region", { name: "Entity inspector" })).not.toBeInTheDocument(),
    );
  });
});

describe("layout integration", () => {
  it("requests layout once initially and not again on selection", async () => {
    const { layout } = renderApp({});
    await ready();
    const initial = layout.calls.length;
    expect(initial).toBeGreaterThanOrEqual(1);
    fireEvent.click(screen.getByTestId(`node-${STRUCT}`));
    await screen.findByRole("region", { name: "Entity inspector" });
    expect(layout.calls.length).toBe(initial); // selection must not relayout
  });

  it("shows a recoverable layout error with retry", async () => {
    renderApp({ layoutMode: "error" });
    await ready();
    const alert = await screen.findByRole("alert");
    expect(alert).toHaveTextContent(/Layout failed/i);
    expect(within(alert).getByRole("button", { name: "Retry layout" })).toBeInTheDocument();
  });

  it("ignores a stale layout result without crashing", async () => {
    renderApp({ layoutMode: "stale" });
    await ready();
    // Nodes still render; no crash.
    expect(screen.getByTestId(`node-${STRUCT}`)).toBeInTheDocument();
  });

  it("fit control invokes the flow instance", async () => {
    renderApp({});
    await ready();
    fireEvent.click(screen.getByRole("button", { name: "Fit" }));
    expect(controlCalls).toContain("fitView");
  });
});

describe("empty + stage seam", () => {
  it("shows an empty state for a view with no entities", async () => {
    renderApp({ search: "?view=view:empty" });
    await ready();
    expect(screen.getByText(/no entities to show/i)).toBeInTheDocument();
  });

  it("renders stage controls only when the view has stages", async () => {
    renderApp({});
    await ready();
    expect(screen.queryByRole("tablist", { name: "Stages" })).not.toBeInTheDocument();
    fireEvent.click(screen.getByRole("tab", { name: "Staged" }));
    const stages = await screen.findByRole("tablist", { name: "Stages" });
    const stageA = within(stages).getByRole("tab", { name: "Stage A" });
    fireEvent.click(stageA);
    await waitFor(() => expect(stageA).toHaveAttribute("aria-selected", "true"));
  });
});

describe("document integrity", () => {
  it("does not mutate the source document", async () => {
    const doc = sampleDocument();
    const before = JSON.stringify(doc);
    renderApp({ outcome: okOutcome({ document: doc }) });
    await ready();
    fireEvent.click(screen.getByRole("tab", { name: "Types" }));
    expect(JSON.stringify(doc)).toBe(before);
  });
});
