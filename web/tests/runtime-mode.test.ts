import { describe, it, expect } from "vitest";
import { detectRuntimeMode } from "../src/api/runtimeMode.ts";

/** A fresh, isolated Document with the given `<head>` HTML. */
function docWithHead(headHtml: string): Document {
  const doc = document.implementation.createHTMLDocument("t");
  doc.head.innerHTML = headHtml;
  return doc;
}

describe("detectRuntimeMode", () => {
  it("returns static for the exact injected marker", () => {
    const doc = docWithHead(`<meta name="cratevista-mode" content="static" />`);
    expect(detectRuntimeMode(doc)).toBe("static");
  });

  it("returns server when no marker is present (the embedded server index)", () => {
    const doc = docWithHead(`<meta charset="utf-8" /><title>CrateVista</title>`);
    expect(detectRuntimeMode(doc)).toBe("server");
  });

  it("returns server for an unrelated meta element", () => {
    const doc = docWithHead(`<meta name="viewport" content="width=device-width" />`);
    expect(detectRuntimeMode(doc)).toBe("server");
  });

  it("returns server for cratevista-mode with a different content value", () => {
    const doc = docWithHead(`<meta name="cratevista-mode" content="dynamic" />`);
    expect(detectRuntimeMode(doc)).toBe("server");
  });

  it("returns server for an empty cratevista-mode content", () => {
    const doc = docWithHead(`<meta name="cratevista-mode" content="" />`);
    expect(detectRuntimeMode(doc)).toBe("server");
  });
});
