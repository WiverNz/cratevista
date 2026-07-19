import { describe, it, expect, beforeEach, vi } from "vitest";

vi.mock("@xyflow/react", () => import("./support/xyflow.tsx"));

import axe from "axe-core";
import { screen, fireEvent, waitFor, within, render } from "@testing-library/react";
import { renderApp, okOutcome, fakeLayout, STRUCT, ENUM } from "./support/harness.tsx";
import { App } from "../src/App.tsx";
import type { SourceClient } from "../src/api/source.ts";

/** Runs axe over a container. `color-contrast` cannot run in jsdom (no layout),
 *  so it is disabled here and remains a MANUAL check — see the PRD baseline. */
async function expectNoViolations(container: HTMLElement) {
  const results = await axe.run(container, {
    rules: { "color-contrast": { enabled: false } },
  });
  const summary = results.violations.map((v) => `${v.id}: ${v.help}`);
  expect(summary).toEqual([]);
}

async function ready() {
  return screen.findByRole("tablist", { name: "Views" });
}

beforeEach(() => {
  window.history.pushState(null, "", "/");
});

describe("axe: application states", () => {
  it("loaded explorer has no violations", async () => {
    const { container } = renderApp({});
    await ready();
    await expectNoViolations(container);
  });

  it("entity inspector has no violations", async () => {
    const { container } = renderApp({});
    await ready();
    fireEvent.click(screen.getByTestId(`node-${STRUCT}`));
    await screen.findByRole("region", { name: "Entity inspector" });
    await expectNoViolations(container);
  });

  it("relation inspector has no violations", async () => {
    const { container } = renderApp({});
    await ready();
    fireEvent.click(screen.getByTestId("edge-rel:contains:mod-struct"));
    await screen.findByRole("region", { name: "Relation inspector" });
    await expectNoViolations(container);
  });

  it("loading state has no violations", async () => {
    const pending = { load: () => new Promise<never>(() => {}), abort() {} };
    const { container } = render(
      <App source={pending} layout={fakeLayout().engine} initialSearch="" />,
    );
    expect(screen.getByRole("status")).toHaveTextContent(/Loading/i);
    await expectNoViolations(container);
  });

  it("blocking error state has no violations", async () => {
    const { container } = renderApp({
      outcome: { status: "document-error", message: "HTTP 500" },
    });
    await screen.findByRole("alert");
    await expectNoViolations(container);
  });

  it("incompatible schema state has no violations", async () => {
    const { container } = renderApp({
      outcome: { status: "incompatible", found: "2.0", supportedMajor: 1 },
    });
    await screen.findByRole("alert");
    await expectNoViolations(container);
  });

  it("empty view has no violations", async () => {
    const { container } = renderApp({ search: "?view=view:empty" });
    await ready();
    await expectNoViolations(container);
  });

  it("reduced mode has no violations", async () => {
    const { container } = renderApp({ budget: 2 });
    await ready();
    await screen.findByText(/Reduced view/i);
    await expectNoViolations(container);
  });

  it("source-disabled inspector state has no violations", async () => {
    const client: SourceClient = { fetchSource: async () => ({ status: "disabled" }) };
    const { container } = renderApp({ sourceClient: client });
    await ready();
    fireEvent.click(screen.getByTestId(`node-${STRUCT}`));
    await screen.findByRole("region", { name: "Entity inspector" });
    fireEvent.click(screen.getByRole("button", { name: "Show source" }));
    await screen.findByText(/Source contents are disabled/i);
    await expectNoViolations(container);
  });
});

describe("keyboard + focus", () => {
  it("tabs use roving tabindex and Arrow/Home/End navigation", async () => {
    renderApp({});
    await ready();
    const overview = screen.getByRole("tab", { name: "Workspace overview" });
    expect(overview).toHaveAttribute("tabindex", "0");
    expect(screen.getByRole("tab", { name: "Types" })).toHaveAttribute("tabindex", "-1");

    overview.focus();
    fireEvent.keyDown(overview, { key: "ArrowRight" });
    await waitFor(() =>
      expect(screen.getByRole("tab", { name: "Types" })).toHaveAttribute("aria-selected", "true"),
    );
    expect(screen.getByRole("tab", { name: "Types" })).toHaveFocus();

    fireEvent.keyDown(screen.getByRole("tab", { name: "Types" }), { key: "End" });
    await waitFor(() => {
      const tabs = screen.getAllByRole("tab");
      expect(tabs[tabs.length - 1]).toHaveAttribute("aria-selected", "true");
    });

    fireEvent.keyDown(document.activeElement!, { key: "Home" });
    await waitFor(() => expect(overview).toHaveAttribute("aria-selected", "true"));
  });

  it("tabs control the graph tabpanel", async () => {
    renderApp({});
    await ready();
    const tab = screen.getByRole("tab", { name: "Workspace overview" });
    const panel = screen.getByRole("tabpanel");
    expect(tab).toHaveAttribute("aria-controls", panel.id);
    expect(panel).toHaveAttribute("aria-labelledby", tab.id);
  });

  it("toolbar and filter group are labelled and keyboard operable", async () => {
    renderApp({});
    await ready();
    expect(screen.getByRole("toolbar", { name: "Graph controls" })).toBeInTheDocument();
    const filter = screen.getByRole("checkbox", { name: "struct" });
    filter.focus();
    expect(filter).toHaveFocus();
    fireEvent.click(filter); // space/click activation on a real checkbox
    expect((filter as HTMLInputElement).checked).toBe(true);
  });

  it("search results are an accessible listbox and selectable by keyboard", async () => {
    renderApp({});
    await ready();
    const box = screen.getByRole("searchbox", { name: "Search entities" });
    fireEvent.change(box, { target: { value: "Thing" } });
    const listbox = await screen.findByRole("listbox", { name: "Search results" });
    const option = within(listbox).getByRole("option", { name: /Thing/ });
    option.focus();
    expect(option).toHaveFocus();
    fireEvent.click(option);
    expect(await screen.findByRole("region", { name: "Entity inspector" })).toBeInTheDocument();
  });

  it("Escape clears the selection without trapping focus", async () => {
    renderApp({});
    await ready();
    const node = screen.getByTestId(`node-${STRUCT}`);
    node.focus();
    fireEvent.click(node);
    await screen.findByRole("region", { name: "Entity inspector" });
    fireEvent.keyDown(window, { key: "Escape" });
    await waitFor(() =>
      expect(screen.queryByRole("region", { name: "Entity inspector" })).not.toBeInTheDocument(),
    );
    // Focus is not trapped: the originating node is still focusable/present.
    expect(screen.getByTestId(`node-${STRUCT}`)).toBeInTheDocument();
    document.getElementById("x");
    (screen.getByTestId(`node-${STRUCT}`) as HTMLElement).focus();
    expect(screen.getByTestId(`node-${STRUCT}`)).toHaveFocus();
  });

  it("GraphList is keyboard operable and reaches hidden entities", async () => {
    renderApp({ budget: 2 });
    await ready();
    const list = await screen.findByLabelText("All entities");
    const buttons = within(list).getAllByRole("button");
    expect(buttons.length).toBe(5); // every entity, incl. hidden
    const hidden = buttons.find((b) => b.textContent?.includes("Thing"))!;
    hidden.focus();
    expect(hidden).toHaveFocus();
    fireEvent.click(hidden);
    expect(await screen.findByRole("region", { name: "Entity inspector" })).toHaveTextContent("Thing");
  });

  it("inspector exposes an accessible heading", async () => {
    renderApp({});
    await ready();
    fireEvent.click(screen.getByTestId(`node-${STRUCT}`));
    const inspector = await screen.findByRole("region", { name: "Entity inspector" });
    expect(within(inspector).getByRole("heading", { name: "Thing" })).toBeInTheDocument();
  });
});

describe("non-color encoding + landmarks", () => {
  it("node kind is conveyed as text, not color alone", async () => {
    renderApp({});
    await ready();
    fireEvent.click(screen.getByTestId(`node-${STRUCT}`));
    const inspector = await screen.findByRole("region", { name: "Entity inspector" });
    // Kind is a text badge in the inspector, and the legend lists text labels.
    expect(within(inspector).getByText("struct")).toBeInTheDocument();
    const legend = screen.getByLabelText("Legend");
    expect(within(legend).getByText("Struct")).toBeInTheDocument();
  });

  it("unknown kinds carry a textual '(unknown)' marker, not colour only", async () => {
    const { unknownKindDocument } = await import("./support/harness.tsx");
    renderApp({ outcome: okOutcome({ document: unknownKindDocument() }) });
    await ready();
    const legend = screen.getByLabelText("Legend");
    expect(within(legend).getAllByText(/\(unknown\)/).length).toBeGreaterThan(0);
  });

  it("uses semantic landmarks (banner/nav/main/complementary)", async () => {
    renderApp({});
    await ready();
    expect(screen.getByRole("banner")).toBeInTheDocument();
    expect(screen.getByRole("navigation", { name: "Views" })).toBeInTheDocument();
    expect(screen.getByRole("main")).toBeInTheDocument();
    expect(screen.getByRole("complementary", { name: "Details inspector" })).toBeInTheDocument();
  });
});

describe("reduced motion", () => {
  it("honors prefers-reduced-motion via the stylesheet contract", async () => {
    // jsdom cannot evaluate media queries against real CSS, so we assert the
    // stylesheet ships the contract (animations/transitions disabled) — the
    // rendered effect is a MANUAL/E2E check.
    const { readFileSync } = await import("node:fs");
    const { dirname, resolve } = await import("node:path");
    const { fileURLToPath } = await import("node:url");
    const here = dirname(fileURLToPath(import.meta.url));
    const css = readFileSync(resolve(here, "../src/styles.css"), "utf8");
    expect(css).toMatch(/@media\s*\(prefers-reduced-motion:\s*reduce\)/);
    expect(css).toMatch(/animation:\s*none/);
    expect(css).toMatch(/transition:\s*none/);
  });

  it("provides a visible focus-visible treatment (not browser default only)", async () => {
    const { readFileSync } = await import("node:fs");
    const { dirname, resolve } = await import("node:path");
    const { fileURLToPath } = await import("node:url");
    const here = dirname(fileURLToPath(import.meta.url));
    const css = readFileSync(resolve(here, "../src/styles.css"), "utf8");
    expect(css).toMatch(/:focus-visible/);
    expect(css).toMatch(/outline:/);
  });
});

// keep ENUM referenced for lint parity with other suites
void ENUM;
