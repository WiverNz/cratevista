// Watch mode through the real <App>: what survives a reload, and what a failure
// is allowed to take away (PRD 09).
import { describe, it, expect, vi } from "vitest";

// React Flow's ZoomPane throws in jsdom and takes the whole tree down with it.
// The component suite mocks it everywhere; see support/xyflow.tsx.
vi.mock("@xyflow/react", () => import("./support/xyflow.tsx"));

import { render, screen, waitFor } from "@testing-library/react";

import { App } from "../src/App.tsx";
import type { ArtifactSource, LoadOutcome } from "../src/api/load.ts";
import type { EventSourceLike } from "../src/api/liveReload.ts";
import { EVENT_FAILED, EVENT_STARTED, EVENT_SUCCEEDED } from "../src/api/liveReload.ts";
import { fakeLayout, sampleDocument } from "./support/harness.tsx";
import type { ExplorerDocument } from "../src/types/index.ts";

class FakeEventSource implements EventSourceLike {
  readonly listeners = new Map<string, ((event: MessageEvent) => void)[]>();
  closed = 0;
  onerror: ((event: Event) => void) | null = null;
  addEventListener(type: string, listener: (event: MessageEvent) => void): void {
    const existing = this.listeners.get(type) ?? [];
    existing.push(listener);
    this.listeners.set(type, existing);
  }
  close(): void {
    this.closed += 1;
  }
  emit(type: string, data?: string): void {
    for (const listener of [...(this.listeners.get(type) ?? [])]) {
      listener({ data } as MessageEvent);
    }
  }
}

/** A document whose project name identifies the snapshot it came from. */
function documentNamed(name: string): ExplorerDocument {
  const doc = sampleDocument();
  return { ...doc, project: { ...doc.project, name } };
}

function okOutcome(document: ExplorerDocument): LoadOutcome {
  return {
    status: "ok",
    document,
    generation: { generated_at: "t", partial: false } as never,
    generationAvailable: true,
    diagnostics: null,
    diagnosticsAvailable: false,
    partial: false,
  };
}

/** A source whose next outcome the test sets. */
class ScriptedSource implements ArtifactSource {
  next: LoadOutcome = okOutcome(documentNamed("first"));
  loads = 0;
  aborts = 0;
  async load(): Promise<LoadOutcome | { stale: true }> {
    this.loads += 1;
    return this.next;
  }
  abort(): void {
    this.aborts += 1;
  }
}

function mount(source: ArtifactSource, watchEnabled = true) {
  const sources: FakeEventSource[] = [];
  const result = render(
    <App
      source={source}
      layout={fakeLayout().engine}
      initialSearch=""
      liveReload={{
        fetchFn: async () =>
          ({ ok: true, status: 200, json: async () => ({ watch_enabled: watchEnabled }) }) as Response,
        createEventSource: () => {
          const es = new FakeEventSource();
          sources.push(es);
          return es;
        },
      }}
    />,
  );
  return { sources, ...result };
}

async function waitForEventSource(sources: FakeEventSource[]): Promise<FakeEventSource> {
  await waitFor(() => expect(sources.length).toBe(1));
  return sources[0];
}

describe("live reload through the app", () => {
  it("a successful reload swaps the document without unmounting the shell", async () => {
    const source = new ScriptedSource();
    const { sources } = mount(source);
    await screen.findByRole("navigation", { name: "Views" });
    const es = await waitForEventSource(sources);

    source.next = okOutcome(documentNamed("second"));
    es.emit(EVENT_SUCCEEDED);

    await waitFor(() => expect(source.loads).toBeGreaterThanOrEqual(2));
    // The shell is still mounted: no blocking state, no empty state.
    expect(screen.getByRole("navigation", { name: "Views" })).toBeInTheDocument();
    expect(screen.queryByRole("alert")).not.toBeInTheDocument();
  });

  it("generation-started shows a non-blocking indicator and keeps the graph", async () => {
    const source = new ScriptedSource();
    const { sources } = mount(source);
    await screen.findByRole("navigation", { name: "Views" });
    const es = await waitForEventSource(sources);
    const loadsBefore = source.loads;

    es.emit(EVENT_STARTED);

    await screen.findByText("Regenerating…");
    // Progress is `role="status"`, never `alert`: it must not interrupt a reader.
    expect(screen.getByText("Regenerating…")).toHaveAttribute("role", "status");
    expect(screen.getByRole("navigation", { name: "Views" })).toBeInTheDocument();
    // Nothing was fetched merely because a rebuild began.
    expect(source.loads).toBe(loadsBefore);
  });

  it("generation-failed keeps the graph and announces the safe code/message", async () => {
    const source = new ScriptedSource();
    const { sources } = mount(source);
    await screen.findByRole("navigation", { name: "Views" });
    const es = await waitForEventSource(sources);
    const loadsBefore = source.loads;

    es.emit(EVENT_STARTED);
    await screen.findByText("Regenerating…");
    es.emit(
      EVENT_FAILED,
      JSON.stringify({ code: "watch_generation_failed", message: "generation failed" }),
    );

    const alert = await screen.findByRole("alert");
    expect(alert).toHaveTextContent("generation failed");
    expect(alert).toHaveTextContent("watch_generation_failed");
    // The indicator stops, the graph stays, and nothing was fetched.
    expect(screen.queryByText("Regenerating…")).not.toBeInTheDocument();
    expect(screen.getByRole("navigation", { name: "Views" })).toBeInTheDocument();
    expect(source.loads).toBe(loadsBefore);
  });

  it("a banner never renders an absolute path", async () => {
    const source = new ScriptedSource();
    const { sources } = mount(source);
    await screen.findByRole("navigation", { name: "Views" });
    const es = await waitForEventSource(sources);
    es.emit(
      EVENT_FAILED,
      JSON.stringify({ code: "watch_generation_failed", message: "generation failed" }),
    );
    const alert = await screen.findByRole("alert");
    expect(alert.textContent).not.toMatch(/[A-Z]:\\|\/home\/|\/Users\//);
  });
});

describe("the last rendered snapshot survives a failed reload", () => {
  it("an exhausted coherence retry keeps the graph and shows a banner", async () => {
    const source = new ScriptedSource();
    const { sources } = mount(source);
    await screen.findByRole("navigation", { name: "Views" });
    const es = await waitForEventSource(sources);

    source.next = { status: "incoherent-snapshot", attempts: 3 };
    es.emit(EVENT_SUCCEEDED);

    const alert = await screen.findByRole("alert");
    expect(alert).toHaveTextContent("Could not refresh");
    // The graph is still there, and the empty state is NOT.
    expect(screen.getByRole("navigation", { name: "Views" })).toBeInTheDocument();
    expect(screen.queryByText("No entities match this view.")).not.toBeInTheDocument();
  });

  it("a reload document-error keeps the graph rather than showing the fatal state", async () => {
    const source = new ScriptedSource();
    const { sources } = mount(source);
    await screen.findByRole("navigation", { name: "Views" });
    const es = await waitForEventSource(sources);

    source.next = { status: "document-error", message: "HTTP 500" };
    es.emit(EVENT_SUCCEEDED);

    await screen.findByRole("alert");
    expect(screen.getByRole("navigation", { name: "Views" })).toBeInTheDocument();
    // The blocking ErrorState has a Retry button; this must not be it.
    expect(screen.queryByRole("button", { name: /retry/i })).not.toBeInTheDocument();
  });

  it("the next successful reload clears the reload-error banner", async () => {
    const source = new ScriptedSource();
    const { sources } = mount(source);
    await screen.findByRole("navigation", { name: "Views" });
    const es = await waitForEventSource(sources);

    source.next = { status: "incoherent-snapshot", attempts: 3 };
    es.emit(EVENT_SUCCEEDED);
    await screen.findByRole("alert");

    source.next = okOutcome(documentNamed("recovered"));
    es.emit(EVENT_SUCCEEDED);
    await waitFor(() => expect(screen.queryByRole("alert")).not.toBeInTheDocument());
  });

  it("a successful reload clears a prior generation-failed banner", async () => {
    const source = new ScriptedSource();
    const { sources } = mount(source);
    await screen.findByRole("navigation", { name: "Views" });
    const es = await waitForEventSource(sources);

    es.emit(EVENT_FAILED, JSON.stringify({ code: "c", message: "m" }));
    await screen.findByRole("alert");

    source.next = okOutcome(documentNamed("fixed"));
    es.emit(EVENT_SUCCEEDED);
    await waitFor(() => expect(screen.queryByRole("alert")).not.toBeInTheDocument());
  });

  it("an INITIAL incoherent load is still fatal — there is nothing to preserve", async () => {
    const source = new ScriptedSource();
    source.next = { status: "incoherent-snapshot", attempts: 3 };
    mount(source);
    // The blocking state, distinguishable from an ordinary load failure.
    const alert = await screen.findByRole("alert");
    expect(alert).toHaveTextContent(/changed while it was being loaded/i);
    expect(screen.getByRole("button", { name: /retry/i })).toBeInTheDocument();
  });
});

describe("EventSource lifecycle in the app", () => {
  it("watch_enabled=false constructs no EventSource", async () => {
    const source = new ScriptedSource();
    const { sources } = mount(source, false);
    await screen.findByRole("navigation", { name: "Views" });
    // Give the probe a turn to settle before asserting the absence.
    await waitFor(() => expect(source.loads).toBeGreaterThanOrEqual(1));
    expect(sources).toHaveLength(0);
  });

  it("exactly one EventSource per mounted app", async () => {
    const source = new ScriptedSource();
    const { sources } = mount(source);
    await screen.findByRole("navigation", { name: "Views" });
    await waitForEventSource(sources);
    // Re-render-inducing events must not open a second stream.
    sources[0].emit(EVENT_STARTED);
    sources[0].emit(EVENT_SUCCEEDED);
    await waitFor(() => expect(source.loads).toBeGreaterThanOrEqual(2));
    expect(sources).toHaveLength(1);
  });

  it("unmount closes the EventSource and aborts in-flight work", async () => {
    const source = new ScriptedSource();
    const { sources, unmount } = mount(source);
    await screen.findByRole("navigation", { name: "Views" });
    const es = await waitForEventSource(sources);

    unmount();
    expect(es.closed).toBe(1);
    expect(source.aborts).toBeGreaterThanOrEqual(1);

    // A message that was already queued cannot resurrect the unmounted tree.
    const loadsAfter = source.loads;
    es.emit(EVENT_SUCCEEDED);
    expect(source.loads).toBe(loadsAfter);
  });

  it("a stale reload failure never replaces a newer success with a banner", async () => {
    // The source reports `stale` for the older invocation, exactly as the real
    // loader does when a newer load supersedes it. Nothing may be rendered from it.
    const source = new ScriptedSource();
    const { sources } = mount(source);
    await screen.findByRole("navigation", { name: "Views" });
    const es = await waitForEventSource(sources);

    source.next = { stale: true } as never;
    es.emit(EVENT_SUCCEEDED);
    await waitFor(() => expect(source.loads).toBeGreaterThanOrEqual(2));

    expect(screen.queryByRole("alert")).not.toBeInTheDocument();
    expect(screen.getByRole("navigation", { name: "Views" })).toBeInTheDocument();
  });
});

describe("no live reload without the capability", () => {
  it("a health probe that throws leaves the app fully functional", async () => {
    const source = new ScriptedSource();
    const errors = vi.spyOn(console, "error").mockImplementation(() => {});
    render(
      <App
        source={source}
        layout={fakeLayout().engine}
        initialSearch=""
        liveReload={{
          fetchFn: async () => {
            throw new Error("no server");
          },
          createEventSource: () => {
            throw new Error("must not be constructed");
          },
        }}
      />,
    );
    await screen.findByRole("navigation", { name: "Views" });
    expect(errors).not.toHaveBeenCalled();
    errors.mockRestore();
  });
});

describe("project-aware browser tab title", () => {
  it("sets `CV · <project>` from the loaded document (server mode)", async () => {
    document.title = "CrateVista";
    const source = new ScriptedSource();
    source.next = okOutcome(documentNamed("FlightTrace"));
    mount(source);
    await waitFor(() => expect(document.title).toBe("CV · FlightTrace"));
  });

  it("updates the title when a reload swaps in a different project name", async () => {
    document.title = "CrateVista";
    const source = new ScriptedSource();
    source.next = okOutcome(documentNamed("Alpha"));
    const { sources } = mount(source);
    await waitFor(() => expect(document.title).toBe("CV · Alpha"));
    const es = await waitForEventSource(sources);
    source.next = okOutcome(documentNamed("Beta"));
    es.emit(EVENT_SUCCEEDED);
    await waitFor(() => expect(document.title).toBe("CV · Beta"));
  });

  it("behaves identically without the live-reload machinery (static mode path)", async () => {
    document.title = "CrateVista";
    const source = new ScriptedSource();
    source.next = okOutcome(documentNamed("StaticProject"));
    // No `liveReload` prop: the shared load path still titles the tab — the title is
    // set before, and independently of, the server-only live-reload effect.
    render(<App source={source} layout={fakeLayout().engine} initialSearch="" />);
    await waitFor(() => expect(document.title).toBe("CV · StaticProject"));
  });

  it("keeps the `CrateVista` fallback when the initial load fails with no document", async () => {
    document.title = "CrateVista";
    const source = new ScriptedSource();
    source.next = { status: "document-error", message: "boom" } as LoadOutcome;
    render(<App source={source} layout={fakeLayout().engine} initialSearch="" />);
    await screen.findByText(/boom/i);
    expect(document.title).toBe("CrateVista");
  });
});
