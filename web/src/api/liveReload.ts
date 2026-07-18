// Live reload: turns the server's SSE stream into calls on the existing loader.
//
// This module deliberately owns no artifact fetching of its own. It decides
// *when* to reload; `ArtifactSource` decides *what* a reload is, and its token +
// AbortController decide which result wins. A second fetch implementation here
// would be a second set of race rules to get wrong.
//
// # Capability, not assumption
//
// `/api/events` exists only when the server is watching. `serve`, a degraded
// `open --watch`, a static export and any older server all lack it, and an
// `EventSource` pointed at a route that answers 404 reconnects for ever. So the
// stream is only ever opened after `/api/health` says `watch_enabled === true`,
// and every other answer — missing route, non-2xx, unparseable body, a
// non-boolean field — means "no".
//
// # Convergence
//
// Events are not durable. An event published while the browser was disconnected is
// gone, so an SSE `open` cannot mean "you are up to date"; it means "you may have
// missed something". Every successful connection therefore reloads once, and that
// single rule is what makes reconnect converge without any replay protocol.

/** The event names the server publishes. Nothing else is handled. */
export const EVENT_STARTED = "generation-started";
export const EVENT_SUCCEEDED = "generation-succeeded";
export const EVENT_FAILED = "generation-failed";

/** The `generation-failed` payload: a stable code and a browser-safe message.
 *  Core builds both; neither ever contains a path or a command line. */
export interface GenerationFailure {
  code: string;
  message: string;
}

/** What the UI is told. Every callback is a no-op once `dispose()` has run. */
export interface LiveReloadHandlers {
  /** A regeneration began. Non-blocking: the current graph stays. */
  onStarted: () => void;
  /** Reload now. Resolves when the reload has settled, so the indicator can be
   *  cleared against the real thing rather than a timer. */
  onReload: () => void | Promise<void>;
  /** A regeneration failed. The current graph stays; nothing is fetched. */
  onFailed: (failure: GenerationFailure) => void;
}

export interface LiveReloadOptions {
  base?: string;
  fetchFn?: (url: string, init?: { signal?: AbortSignal }) => Promise<Response>;
  /** Injected so tests need no real EventSource. */
  createEventSource?: (url: string) => EventSourceLike;
}

/** The part of `EventSource` this module uses. */
export interface EventSourceLike {
  addEventListener(type: string, listener: (event: MessageEvent) => void): void;
  close(): void;
  onerror?: ((event: Event) => void) | null;
}

/** Reads `watch_enabled` from `/api/health`, defensively.
 *
 *  Anything that is not an explicit `true` is `false`: a 404 (no watch route
 *  either), a 500, HTML from a proxy, a static host's directory listing, or a
 *  future server that renames the field. The cost of guessing wrong in the
 *  optimistic direction is a permanently reconnecting EventSource against a route
 *  that does not exist. */
export async function probeWatchEnabled(
  base: string,
  fetchFn: (url: string, init?: { signal?: AbortSignal }) => Promise<Response>,
  signal?: AbortSignal,
): Promise<boolean> {
  try {
    const res = await fetchFn(`${base}/api/health`, signal ? { signal } : undefined);
    if (!res.ok) return false;
    const body: unknown = await res.json();
    if (typeof body !== "object" || body === null) return false;
    return (body as Record<string, unknown>).watch_enabled === true;
  } catch {
    return false;
  }
}

/**
 * One live-reload session for one mounted application.
 *
 * Exactly one `EventSource` exists per instance, and `dispose()` closes it for
 * good. Callbacks check disposal before touching anything, so a message already
 * queued when the app unmounts cannot update a component that is gone.
 */
export class LiveReload {
  private source: EventSourceLike | null = null;
  private disposed = false;
  private readonly controller = new AbortController();
  private readonly base: string;
  private readonly fetchFn: (url: string, init?: { signal?: AbortSignal }) => Promise<Response>;
  private readonly createEventSource: (url: string) => EventSourceLike;

  constructor(
    private readonly handlers: LiveReloadHandlers,
    options: LiveReloadOptions = {},
  ) {
    this.base = options.base ?? "";
    this.fetchFn = options.fetchFn ?? ((url, init) => fetch(url, init));
    this.createEventSource =
      options.createEventSource ?? ((url) => new EventSource(url) as EventSourceLike);
  }

  /** Probes the capability and, only if watching, opens the stream.
   *
   *  Resolves with whether the stream was opened, which is what the tests assert
   *  on instead of waiting for a side effect to show up. */
  async start(): Promise<boolean> {
    const enabled = await probeWatchEnabled(this.base, this.fetchFn, this.controller.signal);
    // Unmounted while the probe was in flight: opening now would leak a stream
    // nobody will ever close.
    if (this.disposed || !enabled) return false;

    const source = this.createEventSource(`${this.base}/api/events`);
    this.source = source;

    // Every successful connection reloads once — including reconnects, which is
    // the whole convergence story. `open` is also the first thing that fires on a
    // fresh connection, so the initial reload costs no extra rule.
    source.addEventListener("open", () => {
      if (this.disposed) return;
      void this.handlers.onReload();
    });

    source.addEventListener(EVENT_STARTED, () => {
      if (this.disposed) return;
      this.handlers.onStarted();
    });

    source.addEventListener(EVENT_SUCCEEDED, () => {
      if (this.disposed) return;
      // `partial: true` is still a real document that is already being served, so
      // it reloads exactly like any other success. The payload is not read: the
      // reload discovers `partial` from the generation report it fetches, and
      // trusting the event instead would be two sources for one fact.
      void this.handlers.onReload();
    });

    source.addEventListener(EVENT_FAILED, (event: MessageEvent) => {
      if (this.disposed) return;
      // Nothing is fetched: a failed generation wrote nothing, so the artifacts on
      // disk are the ones already rendered.
      this.handlers.onFailed(parseFailure(event.data));
    });

    // An ordinary disconnect is not an application error. `EventSource` reconnects
    // on its own using the server's `retry:` hint, and the reconnect's `open` will
    // reload and converge. Closing here, or raising a banner per attempt, would
    // turn a laptop lid into a broken feature.
    source.onerror = () => {};

    return true;
  }

  /** Closes the stream permanently and silences every callback. */
  dispose(): void {
    this.disposed = true;
    this.controller.abort();
    this.source?.close();
    this.source = null;
  }
}

/** Parses a `generation-failed` payload without trusting it.
 *
 *  The server builds this from a stable code and a message written to be safe in a
 *  browser, but it arrives as text over a socket; a malformed frame must produce a
 *  banner, not an exception in an event listener. */
export function parseFailure(data: unknown): GenerationFailure {
  const fallback: GenerationFailure = {
    code: "watch_generation_failed",
    message: "Regeneration failed. See the terminal for details.",
  };
  if (typeof data !== "string") return fallback;
  try {
    const parsed: unknown = JSON.parse(data);
    if (typeof parsed !== "object" || parsed === null) return fallback;
    const record = parsed as Record<string, unknown>;
    return {
      code: typeof record.code === "string" && record.code ? record.code : fallback.code,
      message:
        typeof record.message === "string" && record.message
          ? record.message
          : fallback.message,
    };
  } catch {
    return fallback;
  }
}
