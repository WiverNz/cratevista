import { describe, it, expect, beforeEach, vi } from "vitest";

vi.mock("@xyflow/react", () => import("./support/xyflow.tsx"));

import { screen, fireEvent, within, waitFor } from "@testing-library/react";
import { renderApp, STRUCT } from "./support/harness.tsx";

async function ready() {
  return screen.findByRole("tablist", { name: "Views" });
}

beforeEach(() => {
  window.history.pushState(null, "", "/");
});

describe("large-graph reduced mode", () => {
  it("stays normal below the budget", async () => {
    renderApp({ budget: 1500 }); // sample doc has 5 entities
    await ready();
    expect(screen.queryByText(/Reduced view/i)).not.toBeInTheDocument();
    expect(screen.queryByLabelText("All entities")).not.toBeInTheDocument();
  });

  it("enters reduced mode above the budget with counts + list", async () => {
    renderApp({ budget: 2 }); // force reduction (5 > 2)
    await ready();
    const banner = await screen.findByText(/Reduced view/i);
    expect(banner).toHaveTextContent(/of\s*5\s*nodes/i);
    // Complete list of every entity (incl. hidden) is available.
    const list = screen.getByLabelText("All entities");
    expect(within(list).getAllByRole("button").length).toBe(5);
    expect(within(list).getAllByText(/hidden/).length).toBeGreaterThan(0);
  });

  it("selecting a hidden entity from the list recenters on it", async () => {
    renderApp({ budget: 2 });
    await ready();
    const list = screen.getByLabelText("All entities");
    // Pick the struct entry and select it.
    const structBtn = within(list)
      .getAllByRole("button")
      .find((b) => b.textContent?.includes("Thing"))!;
    fireEvent.click(structBtn);
    // It becomes selected (inspector shows it) and visible in the reduced graph.
    expect(await screen.findByRole("region", { name: "Entity inspector" })).toHaveTextContent("Thing");
    await waitFor(() => expect(screen.getByTestId(`node-${STRUCT}`)).toBeInTheDocument());
  });

  it("Render full graph then Return to reduced toggles the full set", async () => {
    renderApp({ budget: 2 });
    await ready();
    fireEvent.click(screen.getByRole("button", { name: "Render full graph" }));
    // All five nodes now render.
    await waitFor(() => expect(screen.getByText(/Full graph/i)).toBeInTheDocument());
    expect(screen.getByTestId(`node-${STRUCT}`)).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "Return to reduced view" }));
    await waitFor(() => expect(screen.getByText(/Reduced view/i)).toBeInTheDocument());
  });
});
