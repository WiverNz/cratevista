import { describe, it, expect, beforeEach, vi } from "vitest";

vi.mock("@xyflow/react", () => import("./support/xyflow.tsx"));

import { render, screen, fireEvent, waitFor, cleanup, within } from "@testing-library/react";
import { NodeCardView } from "../src/components/NodeCard.tsx";
import type { NodeCard } from "../src/model/nodeCards.ts";
import { renderApp } from "./support/harness.tsx";

const WS = "workspace";
const PKG = "package:demo";
const MOD = "module:demo::app";
const STRUCT = "item:struct:demo::app::Thing";
const ENUM = "item:enum:demo::app::Color";

async function ready() {
  return screen.findByRole("tablist", { name: "Views" });
}

beforeEach(() => {
  cleanup();
  window.history.pushState(null, "", "/");
});

// =====================================================================
// NodeCardView dim rendering + state priority (direct)
// =====================================================================
function card(overrides: Partial<NodeCard> = {}): NodeCard {
  return {
    id: "n",
    kind: "package",
    category: "package",
    known: true,
    kindLabel: "Package",
    title: "demo",
    fullTitle: "demo",
    hasSource: false,
    metrics: [],
    width: 216,
    height: 92,
    ...overrides,
  };
}
function box(): HTMLElement {
  return document.querySelector(".cv-node") as HTMLElement;
}

describe("NodeCardView — dim treatment + state priority", () => {
  it("an unrelated ordinary card gets the dimmed class + data flag", () => {
    render(<NodeCardView card={card()} zoom={1} selected={false} related={false} searchMatch={false} dimmed />);
    expect(box().className).toContain("cv-node--dimmed");
    expect(box().dataset.dimmed).toBe("true");
  });
  it("selected dominates dim (no dim class, selected state kept)", () => {
    render(<NodeCardView card={card()} zoom={1} selected={true} related={false} searchMatch={false} dimmed />);
    expect(box().className).not.toContain("cv-node--dimmed");
    expect(box().dataset.state).toBe("selected");
  });
  it("search dominates dim", () => {
    render(<NodeCardView card={card()} zoom={1} selected={false} related={false} searchMatch={true} dimmed />);
    expect(box().className).not.toContain("cv-node--dimmed");
    expect(box().dataset.state).toBe("search");
  });
  it("diagnostic error/warning dominate dim", () => {
    for (const severity of ["error", "warning"] as const) {
      render(<NodeCardView card={card({ diagnostic: { severity, occurrences: 1, records: 1, label: `1 ${severity}` } })} zoom={1} selected={false} related={false} searchMatch={false} dimmed />);
      expect(box().className).not.toContain("cv-node--dimmed");
      expect(box().dataset.state).toBe(`diagnostic-${severity}`);
      cleanup();
    }
  });
  it("dim never hides the card and never changes dimensions", () => {
    render(<NodeCardView card={card()} zoom={1} selected={false} related={false} searchMatch={false} dimmed />);
    const el = box();
    expect(el.getAttribute("aria-hidden")).toBeNull();
    expect(el).not.toHaveStyle({ display: "none" });
    expect(el.style.width).toBe("216px");
    expect(el.style.height).toBe("92px");
    // Title + kind badge remain rendered (essential content, not hover-only).
    expect(within(el).getByText("Package")).toBeInTheDocument();
    expect(within(el).getByText("demo")).toBeInTheDocument();
  });
  it("dimmed default is false (ordinary cards unaffected)", () => {
    render(<NodeCardView card={card()} zoom={1} selected={false} related={false} searchMatch={false} />);
    expect(box().className).not.toContain("cv-node--dimmed");
    expect(box().dataset.dimmed).toBeUndefined();
  });
});

// =====================================================================
// Focus controls (full app)
// =====================================================================
describe("Focus controls", () => {
  it("exposes Hide/Dim/Clear with disabled + pressed state", async () => {
    renderApp({});
    await ready();
    const hide = screen.getByRole("button", { name: "Hide unrelated" });
    const dim = screen.getByRole("button", { name: "Dim unrelated" });
    const clear = screen.getByRole("button", { name: "Clear focus" });
    // No anchor yet: Hide/Dim disabled, Clear disabled.
    expect(hide).toBeDisabled();
    expect(dim).toBeDisabled();
    expect(clear).toBeDisabled();

    // Enabled/pressed state is driven by a store update → async re-render; await it
    // rather than reading synchronously (which races under parallel-suite load).
    fireEvent.click(screen.getByTestId(`node-${PKG}`));
    await waitFor(() => expect(hide).toBeEnabled());
    expect(dim).toBeEnabled();

    fireEvent.click(dim);
    await waitFor(() => expect(dim).toHaveAttribute("aria-pressed", "true"));
    expect(hide).toHaveAttribute("aria-pressed", "false");
    expect(clear).toBeEnabled();

    fireEvent.click(hide);
    await waitFor(() => expect(hide).toHaveAttribute("aria-pressed", "true"));
    expect(dim).toHaveAttribute("aria-pressed", "false");
  });

  it("hide vs dim is distinguishable by accessible name (not colour)", async () => {
    renderApp({});
    await ready();
    expect(screen.getByRole("button", { name: "Hide unrelated" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Dim unrelated" })).toBeInTheDocument();
  });
});

// =====================================================================
// Edge-mode independence (full app)
// =====================================================================
describe("dim × edge-mode are independent axes", () => {
  it("dim + edges=hidden: full nodes, no edges", async () => {
    renderApp({});
    await ready();
    fireEvent.click(screen.getByTestId(`node-${PKG}`));
    fireEvent.click(screen.getByRole("button", { name: "Dim unrelated" }));
    fireEvent.change(screen.getByLabelText("Edge visibility"), { target: { value: "hidden" } });
    await waitFor(() => expect(screen.queryByTestId("edge-rel:contains:ws-pkg")).not.toBeInTheDocument());
    // Every node still present (dim never removes nodes).
    for (const id of [PKG, WS, MOD, STRUCT, ENUM]) expect(screen.getByTestId(`node-${id}`)).toBeInTheDocument();
  });
  it("entering dim never rewrites the edges mode", async () => {
    renderApp({});
    await ready();
    fireEvent.change(screen.getByLabelText("Edge visibility"), { target: { value: "hidden" } });
    // `findByTestId`: the graph node may render a commit after the Views tablist
    // that `ready()` awaits (esp. under parallel-suite load).
    fireEvent.click(await screen.findByTestId(`node-${PKG}`));
    fireEvent.click(screen.getByRole("button", { name: "Dim unrelated" }));
    // The edges axis is untouched by focus mode.
    await waitFor(() =>
      expect((screen.getByLabelText("Edge visibility") as HTMLSelectElement).value).toBe("hidden"),
    );
  });

  it("dim + edges=related: full nodes, only anchor edges", async () => {
    renderApp({});
    await ready();
    fireEvent.click(screen.getByTestId(`node-${PKG}`));
    fireEvent.click(screen.getByRole("button", { name: "Dim unrelated" }));
    fireEvent.change(screen.getByLabelText("Edge visibility"), { target: { value: "related" } });
    expect(await screen.findByTestId("edge-rel:contains:ws-pkg")).toBeInTheDocument();
    // Edge-mode filtering re-renders asynchronously (React Flow); the graph-local
    // edge control now lives in the workspace overlay, so its effect flushes with
    // the graph. Wait for the unrelated edge to leave rather than racing it.
    await waitFor(() =>
      expect(screen.queryByTestId("edge-rel:has_field_type:struct-enum")).not.toBeInTheDocument(),
    );
    // Unrelated nodes still on screen.
    expect(screen.getByTestId(`node-${STRUCT}`)).toBeInTheDocument();
  });
});

// =====================================================================
// Relayout contract (layout-client instrumentation)
// =====================================================================
async function settleLayout() {
  // Allow the layout effect to flush.
  await waitFor(() => {});
}

describe("relayout contract (instrumented layout requests)", () => {
  it("dim toggle + dim anchor change do NOT request layout; hide does", async () => {
    const { layout } = renderApp({});
    await ready();
    await settleLayout();
    const base = layout.calls.length;

    // Enter dim over the full projection → no new layout.
    fireEvent.click(screen.getByTestId(`node-${PKG}`));
    fireEvent.click(screen.getByRole("button", { name: "Dim unrelated" }));
    await settleLayout();
    expect(layout.calls.length).toBe(base);

    // Move the dim anchor to another node → still full projection → no layout.
    fireEvent.click(screen.getByTestId(`node-${STRUCT}`));
    await settleLayout();
    expect(layout.calls.length).toBe(base);

    // Entering hide reduces the projection → a new layout is legitimately requested.
    fireEvent.click(screen.getByRole("button", { name: "Hide unrelated" }));
    await settleLayout();
    expect(layout.calls.length).toBeGreaterThan(base);
  });

  it("plain selection over the full projection does not request layout", async () => {
    const { layout } = renderApp({});
    await ready();
    await settleLayout();
    const base = layout.calls.length;
    fireEvent.click(screen.getByTestId(`node-${PKG}`));
    fireEvent.click(screen.getByTestId(`node-${MOD}`));
    await settleLayout();
    expect(layout.calls.length).toBe(base);
  });
});

// =====================================================================
// History / refresh
// =====================================================================
describe("dim focus history + refresh", () => {
  it("refresh (initial URL) restores dim focus", async () => {
    renderApp({ search: `?view=view:workspace-overview&focus=${PKG}&focusmode=dim` });
    await ready();
    // All nodes present (dim, not reduced) and Dim control is pressed.
    for (const id of [PKG, WS, MOD, STRUCT, ENUM]) expect(screen.getByTestId(`node-${id}`)).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Dim unrelated" })).toHaveAttribute("aria-pressed", "true");
  });

  it("dim keeps every node keyboard-reachable and never aria-hidden", async () => {
    renderApp({});
    await ready();
    fireEvent.click(screen.getByTestId(`node-${PKG}`));
    fireEvent.click(screen.getByRole("button", { name: "Dim unrelated" }));
    // Unrelated nodes stay as real interactive elements (mock renders <button>),
    // reachable by keyboard, with no aria-hidden anywhere.
    for (const id of [STRUCT, ENUM]) {
      const el = screen.getByTestId(`node-${id}`);
      expect(el.tagName).toBe("BUTTON");
      expect(el.getAttribute("aria-hidden")).toBeNull();
      expect(el.closest("[aria-hidden='true']")).toBeNull();
    }
  });

  it("Back/Forward restores dim after clearing", async () => {
    renderApp({});
    await ready();
    fireEvent.click(screen.getByTestId(`node-${PKG}`));
    fireEvent.click(screen.getByRole("button", { name: "Dim unrelated" }));
    await waitFor(() => expect(window.location.search).toContain("focusmode=dim"));
    fireEvent.click(screen.getByRole("button", { name: "Clear focus" }));
    await waitFor(() => expect(window.location.search).not.toContain("focusmode"));
    // Back restores the dim URL; the store re-derives dim from it.
    window.history.back();
    await waitFor(() => expect(window.location.search).toContain("focusmode=dim"));
  });
});
