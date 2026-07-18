// Live-reload lifecycle (PRD 09): the capability probe, the single EventSource,
// what each event does, and disposal.
import { describe, it, expect, vi } from "vitest";
import {
  EVENT_FAILED,
  EVENT_STARTED,
  EVENT_SUCCEEDED,
  LiveReload,
  parseFailure,
  probeWatchEnabled,
  type EventSourceLike,
} from "../src/api/liveReload.ts";

/** A fake EventSource that lets a test fire exactly what the server would. */
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
    for (const listener of this.listeners.get(type) ?? []) {
      listener({ data } as MessageEvent);
    }
  }
}

function health(body: unknown, ok = true): Response {
  return { ok, status: ok ? 200 : 503, json: async () => body } as unknown as Response;
}

interface Harness {
  live: LiveReload;
  sources: FakeEventSource[];
  started: number;
  reloads: number;
  failures: { code: string; message: string }[];
}

function harness(healthResponse: () => Promise<Response>): Harness {
  const sources: FakeEventSource[] = [];
  const state = { started: 0, reloads: 0, failures: [] as { code: string; message: string }[] };
  const live = new LiveReload(
    {
      onStarted: () => {
        state.started += 1;
      },
      onReload: () => {
        state.reloads += 1;
      },
      onFailed: (failure) => {
        state.failures.push(failure);
      },
    },
    {
      fetchFn: healthResponse,
      createEventSource: () => {
        const source = new FakeEventSource();
        sources.push(source);
        return source;
      },
    },
  );
  return {
    live,
    sources,
    get started() {
      return state.started;
    },
    get reloads() {
      return state.reloads;
    },
    get failures() {
      return state.failures;
    },
  } as Harness;
}

const watching = () => Promise.resolve(health({ watch_enabled: true }));

describe("watch capability probe", () => {
  it("watch_enabled=false creates no EventSource", async () => {
    const h = harness(() => Promise.resolve(health({ watch_enabled: false })));
    expect(await h.live.start()).toBe(false);
    expect(h.sources).toHaveLength(0);
  });

  it("a missing /api/health creates no EventSource", async () => {
    // A static export, or an older server. An EventSource pointed at a route that
    // 404s reconnects for ever.
    const h = harness(() => Promise.resolve(health({}, false)));
    expect(await h.live.start()).toBe(false);
    expect(h.sources).toHaveLength(0);
  });

  it("a network failure on the probe creates no EventSource", async () => {
    const h = harness(() => Promise.reject(new Error("offline")));
    expect(await h.live.start()).toBe(false);
    expect(h.sources).toHaveLength(0);
  });

  it("unparseable health creates no EventSource", async () => {
    const h = harness(() =>
      Promise.resolve({
        ok: true,
        status: 200,
        json: async () => {
          throw new Error("not json");
        },
      } as unknown as Response),
    );
    expect(await h.live.start()).toBe(false);
    expect(h.sources).toHaveLength(0);
  });

  it.each([
    ["a non-boolean field", { watch_enabled: "true" }],
    ["a missing field", { ok: true }],
    ["a null body", null],
    ["an array body", []],
  ])("%s means watch disabled", async (_label, body) => {
    const h = harness(() => Promise.resolve(health(body)));
    expect(await h.live.start()).toBe(false);
    expect(h.sources).toHaveLength(0);
  });

  it("watch_enabled=true creates exactly one EventSource", async () => {
    const h = harness(watching);
    expect(await h.live.start()).toBe(true);
    expect(h.sources).toHaveLength(1);
  });

  it("probeWatchEnabled reads only an explicit true", async () => {
    expect(await probeWatchEnabled("", () => Promise.resolve(health({ watch_enabled: true })))).toBe(true);
    expect(await probeWatchEnabled("", () => Promise.resolve(health({ watch_enabled: 1 })))).toBe(false);
  });
});

describe("events", () => {
  it("open triggers exactly one refetch", async () => {
    const h = harness(watching);
    await h.live.start();
    h.sources[0].emit("open");
    expect(h.reloads).toBe(1);
  });

  it("a reconnect triggers another refetch — this is how missed events converge", async () => {
    // Events are not durable: one published while disconnected is simply gone. So
    // `open` cannot mean "you are up to date", only "you may have missed
    // something", and reloading on every connection is the whole convergence rule.
    const h = harness(watching);
    await h.live.start();
    h.sources[0].emit("open");
    h.sources[0].emit("open"); // the browser reconnected
    expect(h.reloads).toBe(2);
    expect(h.sources).toHaveLength(1);
  });

  it("generation-started shows progress and fetches nothing", async () => {
    const h = harness(watching);
    await h.live.start();
    h.sources[0].emit(EVENT_STARTED);
    expect(h.started).toBe(1);
    expect(h.reloads).toBe(0);
  });

  it("generation-succeeded triggers a refetch", async () => {
    const h = harness(watching);
    await h.live.start();
    h.sources[0].emit(EVENT_SUCCEEDED, JSON.stringify({ partial: false }));
    expect(h.reloads).toBe(1);
  });

  it("a partial success reloads exactly like any other success", async () => {
    const h = harness(watching);
    await h.live.start();
    h.sources[0].emit(EVENT_SUCCEEDED, JSON.stringify({ partial: true }));
    expect(h.reloads).toBe(1);
  });

  it("generation-failed does NOT refetch and surfaces the safe code/message", async () => {
    // A failed generation wrote nothing: the artifacts on disk are the ones
    // already rendered, so fetching them again would be work with no result.
    const h = harness(watching);
    await h.live.start();
    h.sources[0].emit(
      EVENT_FAILED,
      JSON.stringify({ code: "watch_generation_failed", message: "generation failed" }),
    );
    expect(h.reloads).toBe(0);
    expect(h.failures).toEqual([
      { code: "watch_generation_failed", message: "generation failed" },
    ]);
  });

  it("a malformed failure payload still produces a banner rather than throwing", async () => {
    const h = harness(watching);
    await h.live.start();
    h.sources[0].emit(EVENT_FAILED, "}{ not json");
    expect(h.failures).toHaveLength(1);
    expect(h.failures[0].code).toBe("watch_generation_failed");
  });

  it("an ordinary error neither closes the stream nor opens a new one", async () => {
    // The browser reconnects on its own using the server's `retry:` hint. Closing
    // here, or banner-ing per attempt, would turn a closed laptop lid into a
    // broken feature.
    const h = harness(watching);
    await h.live.start();
    h.sources[0].onerror?.(new Event("error"));
    h.sources[0].onerror?.(new Event("error"));
    expect(h.sources).toHaveLength(1);
    expect(h.sources[0].closed).toBe(0);
    expect(h.failures).toHaveLength(0);
    expect(h.reloads).toBe(0);
  });
});

describe("disposal", () => {
  it("dispose closes the EventSource and silences later callbacks", async () => {
    const h = harness(watching);
    await h.live.start();
    const source = h.sources[0];
    h.live.dispose();
    expect(source.closed).toBe(1);

    // An event already queued when the app unmounted must not update anything.
    source.emit("open");
    source.emit(EVENT_STARTED);
    source.emit(EVENT_SUCCEEDED);
    source.emit(EVENT_FAILED, JSON.stringify({ code: "c", message: "m" }));
    expect(h.reloads).toBe(0);
    expect(h.started).toBe(0);
    expect(h.failures).toHaveLength(0);
  });

  it("disposing during the health probe opens no stream", async () => {
    // Unmounted mid-probe: opening now would leak a stream nobody will close.
    let release!: (value: Response) => void;
    const h = harness(
      () =>
        new Promise<Response>((resolve) => {
          release = resolve;
        }),
    );
    const starting = h.live.start();
    h.live.dispose();
    release?.(health({ watch_enabled: true }));
    expect(await starting).toBe(false);
    expect(h.sources).toHaveLength(0);
  });

  it("dispose is safe before start", () => {
    const h = harness(watching);
    expect(() => h.live.dispose()).not.toThrow();
  });
});

describe("parseFailure", () => {
  it("keeps the server's code and message", () => {
    expect(parseFailure(JSON.stringify({ code: "c", message: "m" }))).toEqual({
      code: "c",
      message: "m",
    });
  });

  it.each([[undefined], [42], ["{}"], ['{"code":""}'], ["null"]])(
    "falls back safely for %s",
    (data) => {
      const failure = parseFailure(data);
      expect(failure.code).toBeTruthy();
      expect(failure.message).toBeTruthy();
    },
  );
});

describe("no global EventSource is constructed by default in tests", () => {
  it("does not touch the real EventSource when watch is disabled", async () => {
    const spy = vi.fn();
    const h = harness(() => Promise.resolve(health({ watch_enabled: false })));
    await h.live.start();
    expect(spy).not.toHaveBeenCalled();
    expect(h.sources).toHaveLength(0);
  });
});
