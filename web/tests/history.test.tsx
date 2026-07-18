import { describe, it, expect, beforeEach, vi } from "vitest";

vi.mock("@xyflow/react", () => import("./support/xyflow.tsx"));

import { screen, fireEvent, waitFor } from "@testing-library/react";
import { renderApp, STRUCT } from "./support/harness.tsx";

async function ready() {
  return screen.findByRole("tablist", { name: "Views" });
}

let pushes: string[];
let replaces: string[];

beforeEach(() => {
  window.history.pushState(null, "", "/");
  pushes = [];
  replaces = [];
  vi.spyOn(window.history, "pushState").mockImplementation(((
    _s: unknown,
    _t: string,
    url?: string | URL | null,
  ) => {
    pushes.push(String(url ?? ""));
  }) as typeof window.history.pushState);
  vi.spyOn(window.history, "replaceState").mockImplementation(((
    _s: unknown,
    _t: string,
    url?: string | URL | null,
  ) => {
    replaces.push(String(url ?? ""));
  }) as typeof window.history.replaceState);
});

describe("history semantics", () => {
  it("initialization normalizes with replaceState (no history entry)", async () => {
    // A stale view + unknown kind must be normalized away, via replaceState.
    renderApp({ search: "?view=view:ghost&kinds=ghostkind" });
    await ready();
    expect(replaces.length).toBeGreaterThanOrEqual(1);
    expect(replaces[0]).toContain("view%3Aworkspace-overview");
    expect(replaces[0]).not.toContain("ghostkind");
    expect(pushes).toEqual([]); // initialization must not push
  });

  it("a meaningful navigation step uses pushState", async () => {
    renderApp({});
    await ready();
    pushes.length = 0;
    fireEvent.click(screen.getByRole("tab", { name: "Types" }));
    expect(pushes.length).toBe(1);
    expect(pushes[0]).toContain("view%3Atypes");
  });

  it("selection uses pushState and serializes the entity", async () => {
    renderApp({});
    await ready();
    pushes.length = 0;
    fireEvent.click(screen.getByTestId(`node-${STRUCT}`));
    await waitFor(() => expect(pushes.length).toBe(1));
    expect(decodeURIComponent(pushes[0])).toContain(STRUCT);
  });

  it("search typing uses replaceState (no history spam)", async () => {
    renderApp({});
    await ready();
    pushes.length = 0;
    replaces.length = 0;
    const box = screen.getByRole("searchbox", { name: "Search entities" });
    fireEvent.change(box, { target: { value: "T" } });
    fireEvent.change(box, { target: { value: "Th" } });
    fireEvent.change(box, { target: { value: "Thi" } });
    expect(pushes).toEqual([]); // typing never pushes
    expect(replaces.length).toBe(3);
    expect(replaces[2]).toContain("q=Thi");
  });

  it("restores full durable state on popstate and does not push a duplicate", async () => {
    renderApp({ search: "?view=view:types" });
    await ready();
    pushes.length = 0;
    replaces.length = 0;
    // Real history/location change, then a popstate event.
    vi.mocked(window.history.pushState).mockRestore();
    window.history.pushState(null, "", `?view=view:workspace-overview&entity=${STRUCT}&edges=related`);
    vi.spyOn(window.history, "pushState").mockImplementation(((
      _s: unknown,
      _t: string,
      url?: string | URL | null,
    ) => {
      pushes.push(String(url ?? ""));
    }) as typeof window.history.pushState);
    pushes.length = 0;

    fireEvent.popState(window);

    await waitFor(() =>
      expect(screen.getByRole("tab", { name: "Workspace overview" })).toHaveAttribute(
        "aria-selected",
        "true",
      ),
    );
    // Durable state restored: selection + edge mode.
    expect(await screen.findByRole("region", { name: "Entity inspector" })).toHaveTextContent("Thing");
    expect((screen.getByLabelText("Edge visibility") as HTMLSelectElement).value).toBe("related");
    // Restoration must not create a duplicate history entry.
    expect(pushes).toEqual([]);
  });

  it("popstate with a stale view degrades to workspace-overview safely", async () => {
    renderApp({ search: "?view=view:types" });
    await ready();
    vi.mocked(window.history.pushState).mockRestore();
    window.history.pushState(null, "", "?view=view:ghost&entity=item:ghost");
    fireEvent.popState(window);
    await waitFor(() =>
      expect(screen.getByRole("tab", { name: "Workspace overview" })).toHaveAttribute(
        "aria-selected",
        "true",
      ),
    );
    // Stale entity removed → no inspector selection.
    expect(screen.queryByRole("region", { name: "Entity inspector" })).not.toBeInTheDocument();
  });
});
