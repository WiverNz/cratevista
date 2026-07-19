import { describe, it, expect } from "vitest";
import { renderHook, waitFor, act } from "@testing-library/react";
import { useLayout, type LayoutState } from "../src/app/AppContext.tsx";
import type { Projection } from "../src/app/AppContext.tsx";
import type { LayoutEngine, LayoutInput } from "../src/layout/client.ts";
import type { LayoutResult } from "../src/layout/types.ts";

const pt = (x: number, y: number) => ({ x, y });

/** Minimal projection carrying only what `useLayout` reads. */
function projection(key: string): Projection {
  return {
    cacheKey: key,
    graph: {
      nodes: [
        { id: "n1", kind: "struct", label: "n1" },
        { id: "n2", kind: "struct", label: "n2" },
      ],
      edges: [{ id: "e1", source: "n1", target: "n2", kind: "depends_on" }],
    },
    view: { stages: undefined },
  } as unknown as Projection;
}

/** An engine whose layout promises resolve only when the test says so. */
function deferredEngine() {
  const pending: Array<(o: { status: "ok"; result: LayoutResult }) => void> = [];
  const calls: LayoutInput[] = [];
  const engine: LayoutEngine = {
    layout(input) {
      calls.push(input);
      return new Promise((resolve) => pending.push(resolve));
    },
    terminate() {},
  };
  const resolveLast = (result: LayoutResult) =>
    act(() => {
      pending[pending.length - 1]({ status: "ok", result });
    });
  return { engine, calls, resolveLast };
}

function result(token: number, routePoints: { x: number; y: number }[]): LayoutResult {
  return {
    token,
    nodes: [
      { id: "n1", x: 0, y: 0, width: 10, height: 10 },
      { id: "n2", x: 100, y: 0, width: 10, height: 10 },
    ],
    edges: [{ id: "e1", points: routePoints }],
    width: 200,
    height: 50,
  };
}

describe("useLayout route surfacing", () => {
  it("exposes routes keyed by relation id, drawn from the same result as positions", async () => {
    const { engine, resolveLast } = deferredEngine();
    const { result: hook } = renderHook(() => useLayout(engine, projection("k1")));
    const route = [pt(0, 5), pt(50, 5), pt(50, 0), pt(100, 0)];
    await resolveLast(result(1, route));
    await waitFor(() => expect(hook.current.status).toBe("ok"));

    const state = hook.current as LayoutState;
    expect(state.routes.get("e1")).toEqual(route);
    // Positions and routes came from the same resolved result.
    expect(state.positions.has("n1")).toBe(true);
    expect(state.positions.has("n2")).toBe(true);
  });

  it("does not expose stale routes while a newer projection is pending", async () => {
    const { engine, resolveLast } = deferredEngine();
    const { result: hook, rerender } = renderHook(
      ({ key }: { key: string }) => useLayout(engine, projection(key)),
      { initialProps: { key: "k1" } },
    );
    await resolveLast(result(1, [pt(0, 0), pt(100, 0)]));
    await waitFor(() => expect(hook.current.status).toBe("ok"));
    expect(hook.current.routes.get("e1")).toBeTruthy();

    // Switch to a new projection: layout is now pending for k2.
    rerender({ key: "k2" });
    expect(hook.current.status).toBe("loading");
    // The k1 routes must NOT be handed out against a k2 projection.
    expect(hook.current.routes.size).toBe(0);

    // Once k2 resolves, its own routes appear.
    const route2 = [pt(0, 0), pt(60, 0), pt(60, 20), pt(100, 20)];
    await resolveLast(result(2, route2));
    await waitFor(() => expect(hook.current.status).toBe("ok"));
    expect(hook.current.routes.get("e1")).toEqual(route2);
  });

  it("does not request a new layout when only the projection object changes (same key)", async () => {
    const { engine, calls, resolveLast } = deferredEngine();
    const { rerender, result: hook } = renderHook(
      ({ p }: { p: Projection }) => useLayout(engine, p),
      { initialProps: { p: projection("stable") } },
    );
    await resolveLast(result(1, [pt(0, 0), pt(100, 0)]));
    await waitFor(() => expect(hook.current.status).toBe("ok"));
    expect(calls.length).toBe(1);

    // A fresh projection object with the SAME cache key (as a selection change
    // would produce) must not trigger another layout request.
    rerender({ p: projection("stable") });
    expect(calls.length).toBe(1);
  });
});
