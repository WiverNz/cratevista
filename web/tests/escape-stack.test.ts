// Issue 15 Phase 5: the shared, ordered Escape-handler stack.
import { describe, it, expect, beforeEach } from "vitest";
import { pushEscape, escapeDepth } from "../src/app/escapeStack.ts";

function pressEscape() {
  window.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape", bubbles: true, cancelable: true }));
}

beforeEach(() => {
  // Drain any handlers a prior test left (defensive; each test disposes its own).
  while (escapeDepth() > 0) pressEscape();
});

describe("escapeStack", () => {
  it("dispatches Escape to the TOP handler only (LIFO), never the layers beneath", () => {
    const calls: string[] = [];
    const disposeInner = pushEscape(() => calls.push("inner"));
    const disposeOuterNever = pushEscape(() => calls.push("outer"));
    // Reorder: outer pushed last → it is the top. Fix the intent explicitly:
    disposeOuterNever();
    const disposeOuter = pushEscape(() => calls.push("outer"));
    // Stack (bottom→top): inner, outer.
    expect(escapeDepth()).toBe(2);
    pressEscape();
    expect(calls).toEqual(["outer"]); // top only
    disposeOuter();
    pressEscape();
    expect(calls).toEqual(["outer", "inner"]); // now the layer beneath
    disposeInner();
    pressEscape();
    expect(calls).toEqual(["outer", "inner"]); // empty stack → nothing
  });

  it("does nothing (and does not throw) with an empty stack", () => {
    expect(escapeDepth()).toBe(0);
    expect(() => pressEscape()).not.toThrow();
  });

  it("dispose removes exactly one handler by identity", () => {
    const h = () => {};
    const d1 = pushEscape(h);
    pushEscape(() => {});
    expect(escapeDepth()).toBe(2);
    d1();
    expect(escapeDepth()).toBe(1);
    // Cleanup the second.
    pressEscape();
    expect(escapeDepth()).toBe(0);
  });
});
