import { afterEach, beforeEach, describe, expect, it } from "vitest";

import { applyProjectTitle, projectTabTitle } from "../src/app/documentTitle.ts";

describe("projectTabTitle", () => {
  it("falls back to CrateVista when there is no project name", () => {
    expect(projectTabTitle(undefined)).toBe("CrateVista");
    expect(projectTabTitle(null)).toBe("CrateVista");
    expect(projectTabTitle("")).toBe("CrateVista");
  });

  it("renders `CV · <name>` for a real project", () => {
    expect(projectTabTitle("FlightTrace")).toBe("CV · FlightTrace");
  });

  it("trims surrounding whitespace", () => {
    expect(projectTabTitle("  FlightTrace  ")).toBe("CV · FlightTrace");
  });

  it("renders no separator for a whitespace-only name", () => {
    expect(projectTabTitle("   ")).toBe("CrateVista");
    expect(projectTabTitle("\t\n")).toBe("CrateVista");
  });
});

describe("applyProjectTitle", () => {
  let title: string;
  beforeEach(() => {
    title = document.title;
    document.title = "CrateVista";
  });
  afterEach(() => {
    document.title = title;
  });

  it("sets the tab title from the project name", () => {
    applyProjectTitle("FlightTrace");
    expect(document.title).toBe("CV · FlightTrace");
  });

  it("keeps the fallback for an empty/whitespace name", () => {
    applyProjectTitle("   ");
    expect(document.title).toBe("CrateVista");
  });

  it("updates when the project name changes (e.g. after a reload)", () => {
    applyProjectTitle("FlightTrace");
    expect(document.title).toBe("CV · FlightTrace");
    applyProjectTitle("OtherProject");
    expect(document.title).toBe("CV · OtherProject");
  });

  it("does not rewrite the title when it has not changed", () => {
    applyProjectTitle("FlightTrace");
    // Track writes via a property spy: a redundant apply must not assign again.
    let writes = 0;
    let current = document.title;
    Object.defineProperty(document, "title", {
      configurable: true,
      get: () => current,
      set: (value: string) => {
        writes += 1;
        current = value;
      },
    });
    try {
      applyProjectTitle("FlightTrace");
      expect(writes).toBe(0);
      applyProjectTitle("Changed");
      expect(writes).toBe(1);
    } finally {
      delete (document as unknown as { title?: unknown }).title;
      document.title = current;
    }
  });
});
