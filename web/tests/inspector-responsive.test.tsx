// Issue 15 Phase 2: responsive inspector drawer (medium) and full-screen (narrow).
// jsdom has no matchMedia, so we stub it to report a chosen viewport class BEFORE
// render (the hook reads it synchronously on mount).
import { describe, it, expect, beforeEach, afterEach, vi } from "vitest";

vi.mock("@xyflow/react", () => import("./support/xyflow.tsx"));

import { screen, within, fireEvent, waitFor } from "@testing-library/react";
import { renderApp, STRUCT } from "./support/harness.tsx";
import type { SourceClient } from "../src/api/source.ts";

const PKG = "package:demo";

function setViewport(cls: "wide" | "medium" | "narrow") {
  (window as unknown as { matchMedia: (q: string) => MediaQueryList }).matchMedia = (q: string) =>
    ({
      matches: q.includes("1200") ? cls === "wide" : q.includes("768") ? cls !== "narrow" : false,
      media: q,
      onchange: null,
      addEventListener() {},
      removeEventListener() {},
      addListener() {},
      removeListener() {},
      dispatchEvent() {
        return false;
      },
    }) as unknown as MediaQueryList;
}

async function ready() {
  return screen.findByRole("tablist", { name: "Views" });
}

beforeEach(() => {
  window.history.pushState(null, "", "/");
});
afterEach(() => {
  delete (window as unknown as { matchMedia?: unknown }).matchMedia;
});

describe("medium — modal drawer", () => {
  it("selecting an entity opens a modal dialog with an accessible name and Close", async () => {
    setViewport("medium");
    renderApp({});
    await ready();
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
    fireEvent.click(screen.getByTestId(`node-${PKG}`));
    const dialog = await screen.findByRole("dialog", { name: "Details inspector" });
    expect(dialog).toHaveAttribute("aria-modal", "true");
    expect(within(dialog).getByRole("button", { name: "Close details" })).toBeInTheDocument();
    expect(within(dialog).getByRole("region", { name: "Entity inspector" })).toBeInTheDocument();
    // Focus moved into the dialog.
    expect(dialog.contains(document.activeElement)).toBe(true);
  });

  it("makes the background workspace inert while open and restores it on close", async () => {
    setViewport("medium");
    renderApp({});
    await ready();
    fireEvent.click(screen.getByTestId(`node-${PKG}`));
    const dialog = await screen.findByRole("dialog");
    expect(document.getElementById("cv-graph-panel")).toHaveAttribute("inert");
    expect(document.querySelector(".cv-region-header")).toHaveAttribute("inert");
    fireEvent.keyDown(dialog, { key: "Escape" });
    await waitFor(() => expect(screen.queryByRole("dialog")).not.toBeInTheDocument());
    expect(document.getElementById("cv-graph-panel")).not.toHaveAttribute("inert");
  });

  it("Escape closes the drawer but keeps the selection (reopenable)", async () => {
    setViewport("medium");
    renderApp({});
    await ready();
    fireEvent.click(screen.getByTestId(`node-${PKG}`));
    const dialog = await screen.findByRole("dialog");
    fireEvent.keyDown(dialog, { key: "Escape" });
    await waitFor(() => expect(screen.queryByRole("dialog")).not.toBeInTheDocument());
    // Selection survived: the persistent trigger reopens the same entity.
    const trigger = screen.getByRole("button", { name: "Details" });
    fireEvent.click(trigger);
    const reopened = await screen.findByRole("dialog");
    expect(within(reopened).getByRole("region", { name: "Entity inspector" })).toBeInTheDocument();
  });

  it("returns focus to the trigger after Close", async () => {
    setViewport("medium");
    renderApp({});
    await ready();
    fireEvent.click(screen.getByTestId(`node-${PKG}`));
    await screen.findByRole("dialog");
    fireEvent.click(screen.getByRole("button", { name: "Close details" }));
    await waitFor(() => expect(screen.queryByRole("dialog")).not.toBeInTheDocument());
    expect(screen.getByRole("button", { name: "Details" })).toHaveFocus();
  });

  it("opening/closing the drawer changes neither the URL nor the layout", async () => {
    setViewport("medium");
    const { layout } = renderApp({});
    await ready();
    // Let the INITIAL layout settle before baselining, so we measure only the
    // drawer's (non-)effect on layout, not the startup layout landing late.
    await waitFor(() => expect(layout.calls.length).toBeGreaterThan(0));
    const layoutBefore = layout.calls.length;
    fireEvent.click(screen.getByTestId(`node-${PKG}`));
    const dialog = await screen.findByRole("dialog");
    // The selection's URL push is async; let it settle before capturing the
    // baseline, so the comparison measures ONLY the drawer's effect on the URL.
    await waitFor(() => expect(window.location.search).toMatch(/entity=/));
    const urlOpen = window.location.search;
    fireEvent.keyDown(dialog, { key: "Escape" });
    await waitFor(() => expect(screen.queryByRole("dialog")).not.toBeInTheDocument());
    expect(window.location.search).toBe(urlOpen); // drawer close adds no URL state
    fireEvent.click(screen.getByRole("button", { name: "Details" }));
    await screen.findByRole("dialog");
    expect(window.location.search).toBe(urlOpen); // reopen adds no URL state
    expect(layout.calls.length).toBe(layoutBefore); // no relayout from any of it
  });
});

describe("nested source viewer focus (Escape precedence)", () => {
  const okClient: SourceClient = {
    fetchSource: () => Promise.resolve({ status: "ok", text: "pub struct Thing;" }),
  };

  it("Escape closes the source viewer first, then the drawer; selection/URL survive", async () => {
    setViewport("medium");
    const { layout } = renderApp({ sourceClient: okClient });
    await ready();
    await waitFor(() => expect(layout.calls.length).toBeGreaterThan(0));
    const layoutBefore = layout.calls.length;

    // Select an entity WITH a source location → drawer opens.
    fireEvent.click(screen.getByTestId(`node-${STRUCT}`));
    const dialog = await screen.findByRole("dialog", { name: "Details inspector" });
    await waitFor(() => expect(window.location.search).toMatch(/entity=/));
    const url = window.location.search;

    // Open the source viewer inside the drawer.
    fireEvent.click(within(dialog).getByRole("button", { name: "Show source" }));
    const viewer = await within(dialog).findByRole("group", { name: "Source contents viewer" });
    expect(viewer).toBeInTheDocument();

    // First Escape → the SOURCE viewer closes; the drawer stays open.
    fireEvent.keyDown(window, { key: "Escape" });
    await waitFor(() =>
      expect(within(dialog).queryByRole("group", { name: "Source contents viewer" })).not.toBeInTheDocument(),
    );
    expect(screen.getByRole("dialog", { name: "Details inspector" })).toBeInTheDocument();
    // Focus returned to the "Show source" button.
    expect(within(dialog).getByRole("button", { name: "Show source" })).toHaveFocus();

    // Second Escape → the DRAWER closes.
    fireEvent.keyDown(window, { key: "Escape" });
    await waitFor(() => expect(screen.queryByRole("dialog")).not.toBeInTheDocument());

    // Selection + URL survived the whole sequence; no relayout occurred.
    expect(window.location.search).toBe(url);
    expect(layout.calls.length).toBe(layoutBefore);
    fireEvent.click(screen.getByRole("button", { name: "Details" }));
    expect(await screen.findByRole("dialog")).toHaveTextContent("Thing");
  });
});

describe("narrow — full-viewport panel", () => {
  it("selecting an entity opens a full-viewport dialog; Escape closes, selection/URL survive", async () => {
    setViewport("narrow");
    const { layout } = renderApp({});
    await ready();
    await waitFor(() => expect(layout.calls.length).toBeGreaterThan(0));
    const layoutBefore = layout.calls.length;
    fireEvent.click(screen.getByTestId(`node-${PKG}`));
    const dialog = await screen.findByRole("dialog", { name: "Details inspector" });
    expect(dialog).toHaveClass("cv-inspector-dialog--narrow");
    await waitFor(() => expect(window.location.search).toMatch(/entity=/));
    const url = window.location.search;
    fireEvent.keyDown(dialog, { key: "Escape" });
    await waitFor(() => expect(screen.queryByRole("dialog")).not.toBeInTheDocument());
    expect(window.location.search).toBe(url);
    expect(layout.calls.length).toBe(layoutBefore);
    // Reopenable → selection intact.
    fireEvent.click(screen.getByRole("button", { name: "Details" }));
    expect(await screen.findByRole("dialog")).toBeInTheDocument();
  });
});
