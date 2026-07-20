import { describe, it, expect } from "vitest";
import { parseUrlState, serializeUrlState, type UrlState } from "../src/state/url.ts";
import { createUiStore, toUrlState } from "../src/state/store.ts";

describe("focus URL parsing + normalization (Phase 4 contract)", () => {
  it("no focus → no focus mode", () => {
    const s = parseUrlState("?view=v");
    expect(s.focus).toBeUndefined();
    expect(s.focusmode).toBeUndefined();
  });
  it("focus alone → hide (no focusmode stored)", () => {
    const s = parseUrlState("?focus=e1");
    expect(s.focus).toBe("e1");
    expect(s.focusmode).toBeUndefined();
  });
  it("focus + explicit hide → focusmode omitted (hide is the default)", () => {
    const s = parseUrlState("?focus=e1&focusmode=hide");
    expect(s.focus).toBe("e1");
    expect(s.focusmode).toBeUndefined();
  });
  it("focus + dim → preserves focusmode=dim", () => {
    const s = parseUrlState("?focus=e1&focusmode=dim");
    expect(s.focusmode).toBe("dim");
  });
  it("unknown focusmode + anchor → dropped (→ hide)", () => {
    expect(parseUrlState("?focus=e1&focusmode=sideways").focusmode).toBeUndefined();
  });
  it("focusmode=all → invalid, dropped", () => {
    expect(parseUrlState("?focus=e1&focusmode=all").focusmode).toBeUndefined();
  });
  it("focusmode without anchor → dropped", () => {
    const s = parseUrlState("?focusmode=dim");
    expect(s.focus).toBeUndefined();
    expect(s.focusmode).toBeUndefined();
  });
});

describe("focus URL serialization", () => {
  const ser = (u: UrlState) => serializeUrlState(u);
  it("focus alone serializes a bare focus (byte-for-byte legacy)", () => {
    expect(ser({ focus: "e1" })).toBe("?focus=e1");
  });
  it("explicit hide is NOT serialized (omitted default)", () => {
    expect(ser({ focus: "e1", focusmode: "hide" })).toBe("?focus=e1");
  });
  it("dim serializes focusmode=dim alongside the anchor", () => {
    expect(ser({ focus: "e1", focusmode: "dim" })).toBe("?focus=e1&focusmode=dim");
  });
  it("focusmode without an anchor is never serialized", () => {
    expect(ser({ focusmode: "dim" })).toBe("");
  });
  it("all as focusmode never appears (not a representable value)", () => {
    // The type forbids it; even a coerced object omits it because value != "dim".
    expect(ser({ focus: "e1", focusmode: "all" as unknown as "dim" })).toBe("?focus=e1");
  });
  it("round-trips dim deterministically and preserves other params", () => {
    const url = "?view=view:types&focus=e1&focusmode=dim&edges=related";
    const again = serializeUrlState(parseUrlState(url));
    expect(parseUrlState(again)).toEqual(parseUrlState(url));
    expect(again).toContain("focusmode=dim");
    expect(again).toContain("edges=related");
  });
  it("existing non-focus URLs round-trip unchanged (no focus params added)", () => {
    const url = "?view=view:types&q=Thing&edges=hidden";
    const out = serializeUrlState(parseUrlState(url));
    expect(parseUrlState(out)).toEqual(parseUrlState(url));
    expect(out).not.toContain("focus");
  });
});

describe("store ↔ URL focus mode (toUrlState)", () => {
  it("hide focus serializes a bare focus", () => {
    const store = createUiStore();
    store.getState().setFocus("e1", "hide");
    const url = toUrlState(store.getState());
    expect(url.focus).toBe("e1");
    expect(url.focusmode).toBeUndefined();
  });
  it("dim focus serializes focusmode=dim", () => {
    const store = createUiStore();
    store.getState().setFocus("e1", "dim");
    expect(toUrlState(store.getState()).focusmode).toBe("dim");
  });
  it("clearFocus removes the anchor AND any focus mode", () => {
    const store = createUiStore();
    store.getState().setFocus("e1", "dim");
    store.getState().clearFocus();
    const url = toUrlState(store.getState());
    expect(url.focus).toBeUndefined();
    expect(url.focusmode).toBeUndefined();
    expect(store.getState().focusMode).toBe("hide"); // reset to default
  });
  it("setFocus(null) clears anchor and resets mode to hide", () => {
    const store = createUiStore();
    store.getState().setFocus("e1", "dim");
    store.getState().setFocus(null, "dim");
    expect(store.getState().focusId).toBeNull();
    expect(toUrlState(store.getState()).focusmode).toBeUndefined();
  });
  it("initialize restores dim from the URL and hide from a bare focus", () => {
    const dimStore = createUiStore();
    dimStore.getState().initialize({ activeViewId: "v", url: { focus: "e1", focusmode: "dim" } });
    expect(dimStore.getState().focusMode).toBe("dim");
    expect(dimStore.getState().focusId).toBe("e1");

    const hideStore = createUiStore();
    hideStore.getState().initialize({ activeViewId: "v", url: { focus: "e1" } });
    expect(hideStore.getState().focusMode).toBe("hide");
    expect(hideStore.getState().focusId).toBe("e1");
  });
  it("focusmode without focus in the URL is ignored on init", () => {
    const store = createUiStore();
    store.getState().initialize({ activeViewId: "v", url: { focusmode: "dim" } });
    expect(store.getState().focusId).toBeNull();
    expect(store.getState().focusMode).toBe("hide");
    expect(toUrlState(store.getState()).focusmode).toBeUndefined();
  });
});
