import { describe, it, expect } from "vitest";
import {
  loadArtifacts,
  ArtifactLoader,
  SERVER_ARTIFACT_ENDPOINTS,
  STATIC_ARTIFACT_ENDPOINTS,
  type ArtifactEndpoints,
  type FetchFn,
} from "../src/api/load.ts";

function jsonResponse(body: unknown, ok = true, status = 200): Response {
  return {
    ok,
    status,
    json: async () => body,
  } as unknown as Response;
}

const validDoc = {
  schema_version: "1.0",
  project: { id: "p", name: "p", description: "" },
  entities: [],
  relations: [],
  views: [],
};
const validGen = { generated_at: "t", partial: false };
const validDiag = { schema_version: "1.0", diagnostics: [] };

function router(map: Record<string, () => Response | Promise<Response>>): FetchFn {
  return async (url) => {
    const key = Object.keys(map).find((k) => url.endsWith(k));
    if (!key) throw new Error(`unexpected url ${url}`);
    return map[key]();
  };
}

describe("loadArtifacts degrade rules", () => {
  it("ok: all three load", async () => {
    const out = await loadArtifacts(
      SERVER_ARTIFACT_ENDPOINTS,
      router({
        "/api/document": () => jsonResponse(validDoc),
        "/api/generation": () => jsonResponse(validGen),
        "/api/diagnostics": () => jsonResponse(validDiag),
      }),
    );
    expect(out.status).toBe("ok");
    if (out.status === "ok") {
      expect(out.generationAvailable).toBe(true);
      expect(out.diagnosticsAvailable).toBe(true);
      expect(out.partial).toBe(false);
    }
  });

  it("document network failure -> blocking document-error", async () => {
    const out = await loadArtifacts(
      SERVER_ARTIFACT_ENDPOINTS,
      router({
        "/api/document": () => jsonResponse({}, false, 500),
        "/api/generation": () => jsonResponse(validGen),
        "/api/diagnostics": () => jsonResponse(validDiag),
      }),
    );
    expect(out.status).toBe("document-error");
  });

  it("malformed document -> blocking document-error", async () => {
    const out = await loadArtifacts(
      SERVER_ARTIFACT_ENDPOINTS,
      router({
        "/api/document": () => jsonResponse({ not: "a document" }),
        "/api/generation": () => jsonResponse(validGen),
        "/api/diagnostics": () => jsonResponse(validDiag),
      }),
    );
    expect(out.status).toBe("document-error");
  });

  it("unsupported major -> incompatible", async () => {
    const out = await loadArtifacts(
      SERVER_ARTIFACT_ENDPOINTS,
      router({
        "/api/document": () => jsonResponse({ ...validDoc, schema_version: "2.0" }),
        "/api/generation": () => jsonResponse(validGen),
        "/api/diagnostics": () => jsonResponse(validDiag),
      }),
    );
    expect(out.status).toBe("incompatible");
    if (out.status === "incompatible") expect(out.found).toBe("2.0");
  });

  it("generation failure -> graph usable, generation unavailable", async () => {
    const out = await loadArtifacts(
      SERVER_ARTIFACT_ENDPOINTS,
      router({
        "/api/document": () => jsonResponse(validDoc),
        "/api/generation": () => jsonResponse({}, false, 500),
        "/api/diagnostics": () => jsonResponse(validDiag),
      }),
    );
    expect(out.status).toBe("ok");
    if (out.status === "ok") {
      expect(out.generationAvailable).toBe(false);
      expect(out.diagnosticsAvailable).toBe(true);
    }
  });

  it("diagnostics failure -> graph usable, diagnostics unavailable", async () => {
    const out = await loadArtifacts(
      SERVER_ARTIFACT_ENDPOINTS,
      router({
        "/api/document": () => jsonResponse(validDoc),
        "/api/generation": () => jsonResponse(validGen),
        "/api/diagnostics": () => jsonResponse({}, false, 500),
      }),
    );
    expect(out.status).toBe("ok");
    if (out.status === "ok") expect(out.diagnosticsAvailable).toBe(false);
  });

  it("partial generation -> partial flag", async () => {
    const out = await loadArtifacts(
      SERVER_ARTIFACT_ENDPOINTS,
      router({
        "/api/document": () => jsonResponse(validDoc),
        "/api/generation": () => jsonResponse({ generated_at: "t", partial: true }),
        "/api/diagnostics": () => jsonResponse(validDiag),
      }),
    );
    expect(out.status).toBe("ok");
    if (out.status === "ok") expect(out.partial).toBe(true);
  });
});

describe("loadArtifacts uses the exact supplied endpoint triple", () => {
  /** Records every fetched URL and answers by exact URL match. */
  function recordingRouter(map: Record<string, () => Response>) {
    const seen: string[] = [];
    const fetchFn: FetchFn = async (url) => {
      seen.push(url);
      const handler = map[url];
      if (!handler) throw new Error(`unexpected url ${url}`);
      return handler();
    };
    return { fetchFn, seen };
  }

  const okBodies = {
    document: () => jsonResponse(validDoc),
    generation: () => jsonResponse(validGen),
    diagnostics: () => jsonResponse(validDiag),
  };

  it("fetches exactly the three server URLs, verbatim", async () => {
    const { fetchFn, seen } = recordingRouter({
      "/api/document": okBodies.document,
      "/api/generation": okBodies.generation,
      "/api/diagnostics": okBodies.diagnostics,
    });
    const out = await loadArtifacts(SERVER_ARTIFACT_ENDPOINTS, fetchFn);
    expect(out.status).toBe("ok");
    expect(seen.sort()).toEqual(["/api/diagnostics", "/api/document", "/api/generation"]);
  });

  it("fetches exactly the three relative static URLs, verbatim (no /api)", async () => {
    const { fetchFn, seen } = recordingRouter({
      "./document.json": okBodies.document,
      "./generation.json": okBodies.generation,
      "./diagnostics.json": okBodies.diagnostics,
    });
    const out = await loadArtifacts(STATIC_ARTIFACT_ENDPOINTS, fetchFn);
    expect(out.status).toBe("ok");
    expect(seen.sort()).toEqual(["./diagnostics.json", "./document.json", "./generation.json"]);
    expect(seen.some((u) => u.includes("/api/"))).toBe(false);
  });

  it("gives the same coherence outcome regardless of which endpoint set is used", async () => {
    // Header-less static triple (a file server stamps nothing) is coherent.
    const headerless: FetchFn = async () => jsonResponse(validDoc);
    const run = (endpoints: ArtifactEndpoints): Promise<string> =>
      loadArtifacts(endpoints, async (url) =>
        jsonResponse(
          url.includes("document") ? validDoc : url.includes("generation") ? validGen : validDiag,
        ),
      ).then((o) => o.status);
    expect(await run(SERVER_ARTIFACT_ENDPOINTS)).toBe("ok");
    expect(await run(STATIC_ARTIFACT_ENDPOINTS)).toBe("ok");
    // And a bare document-only static triple still loads (all-absent headers).
    const out = await loadArtifacts(STATIC_ARTIFACT_ENDPOINTS, headerless);
    expect(out.status).toBe("ok");
  });
});

describe("ArtifactLoader token/stale handling", () => {
  it("supersedes an earlier slow attempt (latest wins)", async () => {
    let resolveFirst: ((r: Response) => void) | null = null;
    let call = 0;
    const fetchFn: FetchFn = (url) => {
      if (url.endsWith("/api/document")) {
        call += 1;
        if (call === 1) {
          return new Promise<Response>((res) => {
            resolveFirst = res;
          });
        }
        return Promise.resolve(jsonResponse(validDoc));
      }
      return Promise.resolve(jsonResponse(url.endsWith("/api/generation") ? validGen : validDiag));
    };
    const loader = new ArtifactLoader(SERVER_ARTIFACT_ENDPOINTS, fetchFn);
    const first = loader.load();
    const second = loader.load(); // supersedes #1
    resolveFirst!(jsonResponse(validDoc));
    const r1 = await first;
    const r2 = await second;
    expect("stale" in r1 && r1.stale).toBe(true);
    expect("outcome" in r2 && r2.outcome.status).toBe("ok");
  });
});
