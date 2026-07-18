import { describe, it, expect } from "vitest";
import { createUiStore, toUrlState } from "../src/state/store.ts";
import { parseUrlState } from "../src/state/url.ts";

describe("ui store", () => {
  it("initializes from url state", () => {
    const store = createUiStore();
    store
      .getState()
      .initialize({ activeViewId: "view:types", url: parseUrlState("?view=view:public-api&entity=e1&q=foo&kinds=struct,enum&edges=related") });
    const s = store.getState();
    expect(s.activeViewId).toBe("view:public-api");
    expect(s.selection).toEqual({ kind: "entity", id: "e1" });
    expect(s.search).toBe("foo");
    expect([...s.kindFilters]).toEqual(["struct", "enum"]);
    expect(s.edgeMode).toBe("related");
  });

  it("selection is a discriminated union — entity replaces relation", () => {
    const store = createUiStore();
    store.getState().selectRelation("r1");
    expect(store.getState().selection).toEqual({ kind: "relation", id: "r1" });
    store.getState().selectEntity("e1");
    expect(store.getState().selection).toEqual({ kind: "entity", id: "e1" });
    store.getState().clearSelection();
    expect(store.getState().selection).toEqual({ kind: "none" });
  });

  it("switchView clears stage and (by default) selection", () => {
    const store = createUiStore();
    store.getState().selectEntity("e1");
    store.getState().setStage("s1");
    store.getState().switchView("view:types");
    expect(store.getState().activeViewId).toBe("view:types");
    expect(store.getState().activeStage).toBeNull();
    expect(store.getState().selection).toEqual({ kind: "none" });
  });

  it("switchView can keep a still-valid selection", () => {
    const store = createUiStore();
    store.getState().selectEntity("e1");
    store.getState().switchView("view:types", { keepSelection: true });
    expect(store.getState().selection).toEqual({ kind: "entity", id: "e1" });
  });

  it("toggleKind adds/removes", () => {
    const store = createUiStore();
    store.getState().toggleKind("struct");
    expect([...store.getState().kindFilters]).toEqual(["struct"]);
    store.getState().toggleKind("struct");
    expect([...store.getState().kindFilters]).toEqual([]);
  });

  it("toUrlState round-trips through parseUrlState", () => {
    const store = createUiStore();
    store.getState().initialize({ activeViewId: "view:types" });
    store.getState().selectRelation("r1");
    store.getState().setSearch("q");
    store.getState().setEdgeMode("hidden");
    const url = toUrlState(store.getState());
    expect(parseUrlState(new URLSearchParams(Object.entries(url).flatMap(([k, v]) => (Array.isArray(v) ? [[k, v.join(",")]] : [[k, String(v)]])) as [string, string][]).toString())).toMatchObject({
      relation: "r1",
      q: "q",
      edges: "hidden",
    });
  });
});
