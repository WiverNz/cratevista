import { describe, it, expect } from "vitest";
import { LayoutClient, type WorkerLike } from "../src/layout/client.ts";
import { layoutCacheKey, type LayoutCacheKeyParts } from "../src/layout/cache.ts";
import type { LayoutRequest, LayoutResponse, LayoutResult } from "../src/layout/types.ts";

class MockWorker implements WorkerLike {
  onmessage: ((e: { data: LayoutResponse }) => void) | null = null;
  onerror: ((e: unknown) => void) | null = null;
  posted: LayoutRequest[] = [];
  postMessage(m: LayoutRequest): void {
    this.posted.push(m);
  }
  terminate(): void {}
  respond(token: number): void {
    const result: LayoutResult = { token, nodes: [], edges: [], width: 10, height: 10 };
    this.onmessage?.({ data: { token, ok: true, result } });
  }
  respondError(token: number, error: string): void {
    this.onmessage?.({ data: { token, ok: false, error } });
  }
  crash(): void {
    this.onerror?.({});
  }
}

function input(key: string) {
  return { key, request: { nodes: [], edges: [] } };
}

describe("LayoutClient", () => {
  it("propagates a monotonically increasing token", async () => {
    const worker = new MockWorker();
    const client = new LayoutClient(() => worker);
    const p1 = client.layout(input("a"));
    worker.respond(worker.posted[0].token);
    await p1;
    const p2 = client.layout(input("b"));
    worker.respond(worker.posted[1].token);
    await p2;
    expect(worker.posted[0].token).toBe(1);
    expect(worker.posted[1].token).toBe(2);
  });

  it("serves an identical key from cache (no second post)", async () => {
    const worker = new MockWorker();
    const client = new LayoutClient(() => worker);
    const p = client.layout(input("same"));
    worker.respond(1);
    await p;
    const cached = await client.layout(input("same"));
    expect(cached.status).toBe("ok");
    expect(worker.posted.length).toBe(1); // no second worker call
  });

  it("re-lays out when the key changes", async () => {
    const worker = new MockWorker();
    const client = new LayoutClient(() => worker);
    await (async () => {
      const p = client.layout(input("k1"));
      worker.respond(1);
      await p;
    })();
    const p2 = client.layout(input("k2"));
    worker.respond(2);
    await p2;
    expect(worker.posted.length).toBe(2);
  });

  it("discards a stale response and accepts the latest", async () => {
    const worker = new MockWorker();
    const client = new LayoutClient(() => worker);
    const p1 = client.layout(input("k1")); // token 1
    const p2 = client.layout(input("k2")); // token 2 supersedes → p1 stale
    worker.respond(1); // stale token, ignored by client
    worker.respond(2); // latest, accepted
    expect((await p1).status).toBe("stale");
    expect((await p2).status).toBe("ok");
  });

  it("turns a worker crash into a recoverable error (no hang)", async () => {
    const worker = new MockWorker();
    const client = new LayoutClient(() => worker);
    const p = client.layout(input("k"));
    worker.crash();
    const outcome = await p;
    expect(outcome.status).toBe("error");
  });

  it("turns a worker error response into a recoverable error", async () => {
    const worker = new MockWorker();
    const client = new LayoutClient(() => worker);
    const p = client.layout(input("k"));
    worker.respondError(1, "elk failed");
    const outcome = await p;
    expect(outcome).toEqual({ status: "error", error: "elk failed" });
  });
});

describe("layoutCacheKey", () => {
  const base: LayoutCacheKeyParts = {
    identity: "id1",
    viewId: "view:types",
    kinds: ["struct", "enum"],
    edgeMode: "all",
    expanded: [],
    stage: null,
    nodeIds: ["b", "a"],
    edgeIds: ["y", "x"],
  };

  it("is order-independent (deterministic normalization)", () => {
    const k1 = layoutCacheKey({ ...base, kinds: ["enum", "struct"], nodeIds: ["a", "b"], edgeIds: ["x", "y"] });
    expect(layoutCacheKey(base)).toBe(k1);
  });

  it("changes when view/filter/neighborhood/stage/nodes change", () => {
    const k = layoutCacheKey(base);
    expect(layoutCacheKey({ ...base, viewId: "view:public-api" })).not.toBe(k);
    expect(layoutCacheKey({ ...base, kinds: ["struct"] })).not.toBe(k);
    expect(layoutCacheKey({ ...base, expanded: ["n1"] })).not.toBe(k);
    expect(layoutCacheKey({ ...base, stage: "s1" })).not.toBe(k);
    // Hide-focus reduces the node set → different nodeIds → different key.
    expect(layoutCacheKey({ ...base, nodeIds: ["a"] })).not.toBe(k);
    expect(layoutCacheKey({ ...base, edgeIds: ["x"] })).not.toBe(k);
  });

  it("does not depend on selection/hover/focus (they are not key inputs)", () => {
    // Focus reaches the key ONLY through nodeIds/edgeIds: with the same node/edge
    // set (as dim-focus keeps), the key is identical regardless of focus. There
    // are no focus/selection/hover fields on the key at all.
    expect(layoutCacheKey(base)).toBe(layoutCacheKey({ ...base }));
  });
});
