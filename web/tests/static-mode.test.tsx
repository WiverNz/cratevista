import { describe, it, expect, beforeEach, afterEach, vi } from "vitest";

vi.mock("@xyflow/react", () => import("./support/xyflow.tsx"));

import { render, screen, fireEvent, waitFor } from "@testing-library/react";
import { App, type LiveReloadLike } from "../src/App.tsx";
import type { LiveReloadHandlers } from "../src/api/liveReload.ts";
import {
  fakeSource,
  fakeLayout,
  okOutcome,
  watchDisabledLiveReload,
  STRUCT,
} from "./support/harness.tsx";

/** A live-reload factory that records how many times it is constructed. */
function countingFactory() {
  const constructions: LiveReloadHandlers[] = [];
  const factory = (handlers: LiveReloadHandlers): LiveReloadLike => {
    constructions.push(handlers);
    return { start: async () => false, dispose: () => {} };
  };
  return { factory, constructions };
}

beforeEach(() => {
  window.history.pushState(null, "", "/");
});

describe("static mode wiring", () => {
  let fetchSpy: ReturnType<typeof vi.fn>;
  let eventSourceSpy: ReturnType<typeof vi.fn>;
  const originalFetch = globalThis.fetch;
  const originalEventSource = globalThis.EventSource;

  beforeEach(() => {
    fetchSpy = vi.fn(async () => {
      throw new Error("no network in static-mode wiring test");
    });
    globalThis.fetch = fetchSpy as unknown as typeof fetch;
    eventSourceSpy = vi.fn(() => {
      throw new Error("static mode must not construct EventSource");
    });
    globalThis.EventSource = eventSourceSpy as unknown as typeof EventSource;
  });
  afterEach(() => {
    globalThis.fetch = originalFetch;
    globalThis.EventSource = originalEventSource;
  });

  it("constructs no LiveReload, no EventSource, and probes no /api/health", async () => {
    const { factory, constructions } = countingFactory();
    render(
      <App
        mode="static"
        source={fakeSource(okOutcome())}
        layout={fakeLayout().engine}
        liveReloadFactory={factory}
      />,
    );
    await screen.findByRole("tablist", { name: "Views" });
    // Give any (forbidden) live-reload effect a chance to run.
    await new Promise((r) => setTimeout(r, 20));

    expect(constructions).toHaveLength(0);
    expect(eventSourceSpy).not.toHaveBeenCalled();
    // No fetch at all here — the fake source never touches the network, and nothing
    // else (no health probe, no events) may either.
    for (const call of fetchSpy.mock.calls) {
      expect(String(call[0])).not.toContain("/api/");
    }
  });

  it("detects the mode before constructing the source (static marker → ./*.json)", async () => {
    // No `mode` and no `source` prop: App must detect the injected marker and
    // construct a StaticArtifactSource that fetches the sibling files — proving the
    // mode is read before the source is built and its first fetch fires.
    const meta = document.createElement("meta");
    meta.setAttribute("name", "cratevista-mode");
    meta.setAttribute("content", "static");
    document.head.appendChild(meta);
    try {
      fetchSpy.mockImplementation(async (url: string) => {
        const body = url.includes("document")
          ? { schema_version: "1.0", project: { id: "p", name: "p", description: "" }, entities: [], relations: [], views: [] }
          : url.includes("generation")
            ? { generated_at: "t", partial: false }
            : { schema_version: "1.0", diagnostics: [] };
        return { ok: true, status: 200, json: async () => body } as unknown as Response;
      });
      render(<App layout={fakeLayout().engine} />);
      await new Promise((r) => setTimeout(r, 30));
      const urls = fetchSpy.mock.calls.map((c) => String(c[0]));
      expect(urls).toContain("./document.json");
      expect(urls.some((u) => u.includes("/api/"))).toBe(false);
    } finally {
      meta.remove();
    }
  });

  it("performs no /api/source request when an entity with a location is selected", async () => {
    render(
      <App mode="static" source={fakeSource(okOutcome())} layout={fakeLayout().engine} />,
    );
    await screen.findByRole("tablist", { name: "Views" });
    fireEvent.click(screen.getByTestId(`node-${STRUCT}`));
    await screen.findByRole("region", { name: "Entity inspector" });

    // The selected struct HAS a SourceLocation, but static mode has no source
    // client, so there is no "Show source" action and no source fetch.
    expect(screen.queryByRole("button", { name: "Show source" })).not.toBeInTheDocument();
    expect(screen.queryByLabelText("Source contents")).not.toBeInTheDocument();
    for (const call of fetchSpy.mock.calls) {
      expect(String(call[0])).not.toContain("/api/source");
    }
  });
});

describe("server mode wiring (regression)", () => {
  it("constructs LiveReload exactly once", async () => {
    const { factory, constructions } = countingFactory();
    render(
      <App
        mode="server"
        source={fakeSource(okOutcome())}
        layout={fakeLayout().engine}
        liveReloadFactory={factory}
      />,
    );
    await screen.findByRole("tablist", { name: "Views" });
    await waitFor(() => expect(constructions).toHaveLength(1));
  });

  it("keeps the opt-in source action when a client is provided", async () => {
    render(
      <App
        mode="server"
        source={fakeSource(okOutcome())}
        layout={fakeLayout().engine}
        sourceClient={{ fetchSource: async () => ({ status: "disabled" }) }}
        liveReload={watchDisabledLiveReload}
      />,
    );
    await screen.findByRole("tablist", { name: "Views" });
    fireEvent.click(screen.getByTestId(`node-${STRUCT}`));
    await screen.findByRole("region", { name: "Entity inspector" });
    expect(screen.getByRole("button", { name: "Show source" })).toBeInTheDocument();
  });
});
