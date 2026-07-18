// Local-only performance instrumentation for the PRD-07 large-graph benchmark.
//
// Everything here is in-memory and same-page. It performs **no** network
// requests, records **no** paths or document content, and reports nothing
// anywhere: `performance.measure` writes to the browser's own User Timing
// buffer, which only this page can read. There is no telemetry.
//
// It also must not change what the app renders. `measure` runs the wrapped
// function synchronously and returns its value unchanged; if the User Timing API
// is unavailable it degrades to a plain call.
//
// The benchmark reads the results through `window.__cratevistaPerf`, a hook
// exposed for local automation only. Nothing in the application reads it, and no
// debug console is required.

/** A single recorded duration, in milliseconds. */
export interface PerfEntry {
  name: string;
  duration: number;
  /** Optional counts recorded alongside the timing (e.g. visible nodes). */
  detail?: Record<string, number>;
}

interface PerfHook {
  entries: PerfEntry[];
  /** Counters the benchmark reads directly (visible nodes/edges, etc.). */
  counts: Record<string, number>;
  clear: () => void;
}

declare global {
  interface Window {
    __cratevistaPerf?: PerfHook;
  }
}

function hook(): PerfHook | undefined {
  if (typeof window === "undefined") return undefined;
  window.__cratevistaPerf ??= {
    entries: [],
    counts: {},
    clear() {
      this.entries = [];
      this.counts = {};
    },
  };
  return window.__cratevistaPerf;
}

/**
 * Times `body`, records it under `name`, and returns its result unchanged.
 *
 * Uses the User Timing API so the measurement is visible to devtools and to the
 * benchmark alike, and falls back to a direct call when it is unavailable.
 */
export function measure<T>(name: string, body: () => T, detail?: Record<string, number>): T {
  const store = hook();
  if (!store || typeof performance === "undefined" || !performance.mark) return body();

  const start = `${name}:start`;
  performance.mark(start);
  const started = performance.now();
  const result = body();
  const duration = performance.now() - started;
  try {
    performance.measure(name, { start });
  } catch {
    // A missing mark must never break rendering.
  }
  store.entries.push({ name, duration, detail });
  return result;
}

/** Records a duration measured elsewhere (e.g. across a worker round-trip). */
export function record(name: string, duration: number, detail?: Record<string, number>): void {
  hook()?.entries.push({ name, duration, detail });
}

/** Records a count the benchmark reads directly. */
export function count(name: string, value: number): void {
  const store = hook();
  if (store) store.counts[name] = value;
}

/** Marks a point in time (e.g. first usable graph). */
export function mark(name: string): void {
  if (typeof performance !== "undefined" && performance.mark) {
    try {
      performance.mark(name);
    } catch {
      /* never break rendering */
    }
  }
  const store = hook();
  if (store) store.counts[`${name}:at`] = typeof performance !== "undefined" ? performance.now() : 0;
}
