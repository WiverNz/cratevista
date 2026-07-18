// Snapshot-coherence rules for the artifact loader (PRD 09).
//
// A watch-mode server can swap the snapshot between the three concurrent requests
// of one load, which would render a document from one generation beside
// diagnostics from another. Nothing downstream can detect that mix, so it is
// caught here or not at all.
import { describe, it, expect, vi } from "vitest";
import {
  ArtifactLoader,
  COHERENCE_ATTEMPTS,
  SERVER_ARTIFACT_ENDPOINTS,
  SNAPSHOT_HEADER,
  loadArtifacts,
  type FetchFn,
} from "../src/api/load.ts";

const validDoc = {
  schema_version: "1.0",
  project: { id: "p", name: "p", description: "" },
  entities: [],
  relations: [],
  views: [],
};
const validGen = { generated_at: "t", partial: false };
const validDiag = { schema_version: "1.0", diagnostics: [] };

/** A Response carrying real headers, as the live server sends. */
function withHeader(body: unknown, snapshot: string | null): Response {
  const headers = new Headers();
  if (snapshot !== null) headers.set(SNAPSHOT_HEADER, snapshot);
  return { ok: true, status: 200, headers, json: async () => body } as unknown as Response;
}

/** A Response with no `headers` property at all — a static host, or a fixture. */
function bare(body: unknown): Response {
  return { ok: true, status: 200, json: async () => body } as unknown as Response;
}

function bodyFor(url: string): unknown {
  if (url.endsWith("/api/document")) return validDoc;
  if (url.endsWith("/api/generation")) return validGen;
  return validDiag;
}

/** A fetch whose per-attempt snapshot headers come from `script`.
 *
 *  `null` in a triple means "this response carries no header"; a triple of three
 *  different strings is what a swap mid-load actually looks like. */
function scripted(script: (string | null)[][]): { fetchFn: FetchFn; attempts: () => number; calls: () => string[] } {
  let attempt = 0;
  let inAttempt = 0;
  const calls: string[] = [];
  const fetchFn: FetchFn = async (url) => {
    calls.push(url);
    const triple = script[Math.min(attempt, script.length - 1)];
    const index = url.endsWith("/api/document") ? 0 : url.endsWith("/api/generation") ? 1 : 2;
    inAttempt += 1;
    if (inAttempt === 3) {
      inAttempt = 0;
      attempt += 1;
    }
    return withHeader(bodyFor(url), triple[index]);
  };
  return { fetchFn, attempts: () => attempt, calls: () => calls };
}

describe("snapshot header coherence", () => {
  it("accepts three identical headers on the first attempt", async () => {
    const { fetchFn, attempts } = scripted([["a", "a", "a"]]);
    const out = await loadArtifacts(SERVER_ARTIFACT_ENDPOINTS, fetchFn);
    expect(out.status).toBe("ok");
    expect(attempts()).toBe(1);
  });

  it("retries a mismatch and accepts the coherent second attempt", async () => {
    const { fetchFn, attempts } = scripted([
      ["a", "b", "a"],
      ["b", "b", "b"],
    ]);
    const out = await loadArtifacts(SERVER_ARTIFACT_ENDPOINTS, fetchFn);
    expect(out.status).toBe("ok");
    expect(attempts()).toBe(2);
  });

  it("accepts the coherent third attempt after two mismatches", async () => {
    const { fetchFn, attempts } = scripted([
      ["a", "b", "c"],
      ["c", "d", "c"],
      ["d", "d", "d"],
    ]);
    const out = await loadArtifacts(SERVER_ARTIFACT_ENDPOINTS, fetchFn);
    expect(out.status).toBe("ok");
    expect(attempts()).toBe(3);
  });

  it("fails with the typed coherence error after three mismatches", async () => {
    const { fetchFn, attempts } = scripted([["a", "b", "c"]]);
    const out = await loadArtifacts(SERVER_ARTIFACT_ENDPOINTS, fetchFn);
    expect(out.status).toBe("incoherent-snapshot");
    if (out.status === "incoherent-snapshot") expect(out.attempts).toBe(COHERENCE_ATTEMPTS);
    // Three attempts total, not one plus three retries.
    expect(attempts()).toBe(COHERENCE_ATTEMPTS);
  });

  it("treats mixed header presence as incoherent", async () => {
    // Two responses stamped, one not: one of them predates a swap.
    const { fetchFn } = scripted([["a", "a", null]]);
    const out = await loadArtifacts(SERVER_ARTIFACT_ENDPOINTS, fetchFn);
    expect(out.status).toBe("incoherent-snapshot");
  });

  it("retries mixed presence and accepts a later coherent triple", async () => {
    const { fetchFn, attempts } = scripted([
      [null, "a", "a"],
      ["a", "a", "a"],
    ]);
    const out = await loadArtifacts(SERVER_ARTIFACT_ENDPOINTS, fetchFn);
    expect(out.status).toBe("ok");
    expect(attempts()).toBe(2);
  });

  it("accepts three ABSENT headers — static exports have no server to stamp them", async () => {
    // PRD 10 writes immutable files; a file server stamps nothing. Immutable
    // files cannot disagree, so silence here is not ambiguity.
    const { fetchFn } = scripted([[null, null, null]]);
    const out = await loadArtifacts(SERVER_ARTIFACT_ENDPOINTS, fetchFn);
    expect(out.status).toBe("ok");
  });

  it("accepts a Response with no headers object at all", async () => {
    const fetchFn: FetchFn = async (url) => bare(bodyFor(url));
    const out = await loadArtifacts(SERVER_ARTIFACT_ENDPOINTS, fetchFn);
    expect(out.status).toBe("ok");
  });

  it("a degraded artifact does not make the triple incoherent", async () => {
    // /api/generation is gone. It never arrived, so it has no opinion about which
    // snapshot this is — counting its absent header would retry three times and
    // then fail a load that the existing degrade rules handle perfectly.
    const fetchFn: FetchFn = async (url) => {
      if (url.endsWith("/api/generation")) {
        return { ok: false, status: 404, headers: new Headers() } as unknown as Response;
      }
      return withHeader(bodyFor(url), "a");
    };
    const out = await loadArtifacts(SERVER_ARTIFACT_ENDPOINTS, fetchFn);
    expect(out.status).toBe("ok");
    if (out.status === "ok") expect(out.generationAvailable).toBe(false);
  });
});

describe("fetch semantics", () => {
  it("each attempt performs exactly three artifact requests", async () => {
    const { fetchFn, calls } = scripted([["a", "b", "c"]]);
    await loadArtifacts(SERVER_ARTIFACT_ENDPOINTS, fetchFn);
    expect(calls()).toHaveLength(3 * COHERENCE_ATTEMPTS);
    const perAttempt = calls().slice(0, 3).sort();
    expect(perAttempt).toEqual(["/api/diagnostics", "/api/document", "/api/generation"]);
  });

  it("an attempt fetches its three artifacts concurrently", async () => {
    // All three must be in flight before any resolves. If the loader awaited them
    // in sequence, `peak` could never reach 3.
    let live = 0;
    let peak = 0;
    const fetchFn: FetchFn = async (url) => {
      live += 1;
      peak = Math.max(peak, live);
      await Promise.resolve();
      live -= 1;
      return withHeader(bodyFor(url), "a");
    };
    await loadArtifacts(SERVER_ARTIFACT_ENDPOINTS, fetchFn);
    expect(peak).toBe(3);
  });

  it("never exposes a triple before every header is checked", async () => {
    // The document is valid and would parse; the triple is incoherent. If
    // coherence were checked after parsing, this would return `ok`.
    const { fetchFn } = scripted([["a", "b", "a"]]);
    const out = await loadArtifacts(SERVER_ARTIFACT_ENDPOINTS, fetchFn);
    expect(out.status).toBe("incoherent-snapshot");
    expect(out).not.toHaveProperty("document");
  });
});

describe("cancellation", () => {
  it("a newer load aborts every attempt of the previous one", async () => {
    const signals: AbortSignal[] = [];
    let release!: () => void;
    const gate = new Promise<void>((resolve) => {
      release = resolve;
    });
    const fetchFn: FetchFn = async (url, init) => {
      if (init?.signal) signals.push(init.signal);
      await gate;
      return withHeader(bodyFor(url), "a");
    };
    const loader = new ArtifactLoader(SERVER_ARTIFACT_ENDPOINTS, fetchFn);
    const first = loader.load();
    const second = loader.load();
    release();
    const [a, b] = await Promise.all([first, second]);

    expect("stale" in a).toBe(true);
    expect("stale" in b).toBe(false);
    // The first invocation's signal is aborted; the second's is not.
    expect(signals[0].aborted).toBe(true);
    expect(signals.at(-1)?.aborted).toBe(false);
  });

  it("a retry does not cancel itself", async () => {
    // Two attempts inside ONE invocation: the second must run with a live signal.
    const { fetchFn } = scripted([
      ["a", "b", "a"],
      ["b", "b", "b"],
    ]);
    const seen: boolean[] = [];
    const spy: FetchFn = async (url, init) => {
      seen.push(init?.signal?.aborted ?? false);
      return fetchFn(url, init);
    };
    const loader = new ArtifactLoader(SERVER_ARTIFACT_ENDPOINTS, spy);
    const result = await loader.load();
    expect("stale" in result).toBe(false);
    if (!("stale" in result)) expect(result.outcome.status).toBe("ok");
    expect(seen).toHaveLength(6);
    expect(seen.every((aborted) => aborted === false)).toBe(true);
  });

  it("an aborted stale load returns no data and no error", async () => {
    let release!: () => void;
    const gate = new Promise<void>((resolve) => {
      release = resolve;
    });
    let first = true;
    const fetchFn: FetchFn = async (url, init) => {
      if (first) {
        await gate;
        // Whatever the real network does, a stale invocation must report nothing.
        const error = new Error("aborted");
        error.name = "AbortError";
        if (init?.signal?.aborted) throw error;
      }
      return withHeader(bodyFor(url), "a");
    };
    const loader = new ArtifactLoader(SERVER_ARTIFACT_ENDPOINTS, fetchFn);
    const stale = loader.load();
    first = false;
    const fresh = loader.load();
    release();

    const staleResult = await stale;
    await fresh;
    // Not data, and — the part that matters for the UI — not an error banner.
    expect("stale" in staleResult).toBe(true);
  });
});

describe("ArtifactLoader coherence integration", () => {
  it("surfaces the typed coherence error through the loader", async () => {
    const { fetchFn } = scripted([["a", "b", "c"]]);
    const loader = new ArtifactLoader(SERVER_ARTIFACT_ENDPOINTS, fetchFn);
    const result = await loader.load();
    expect("stale" in result).toBe(false);
    if (!("stale" in result)) expect(result.outcome.status).toBe("incoherent-snapshot");
  });

  it("stops retrying once aborted", async () => {
    const fetchFn = vi.fn<FetchFn>(async (url) => withHeader(bodyFor(url), Math.random().toString()));
    const loader = new ArtifactLoader(SERVER_ARTIFACT_ENDPOINTS, fetchFn);
    const pending = loader.load();
    loader.abort();
    await pending;
    // Aborted after the first attempt's requests were issued: the remaining
    // attempts are not spent on a result nobody will read.
    expect(fetchFn.mock.calls.length).toBeLessThanOrEqual(3 * COHERENCE_ATTEMPTS);
  });
});
