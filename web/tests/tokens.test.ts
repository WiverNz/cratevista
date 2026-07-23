// Issue 15 Phase 1: styling-contract + invariant checks.
//
// jsdom cannot evaluate real CSS layout, so — like the reduced-motion contract in
// a11y.test.tsx — these assert the *shipped stylesheet text* carries the token
// system and the policy bans (no backdrop-filter, no external/data-URI font), and
// that the Phase-1 node-card dimension invariant is unchanged.
import { describe, it, expect } from "vitest";
import { readFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { cardSize } from "../src/model/nodeCards.ts";

const css = readFileSync(
  resolve(dirname(fileURLToPath(import.meta.url)), "../src/styles.css"),
  "utf8",
);
// Policy bans must inspect real CSS, not prose: comments legitimately NAME the
// banned features ("no backdrop-filter…"), so strip comments before asserting.
const cssNoComments = css.replace(/\/\*[\s\S]*?\*\//g, "");

describe("centralized token declarations exist", () => {
  const tokens = [
    // typography
    "--font-ui:",
    "--fs-project-title:",
    "--fs-control:",
    "--graph-label-min:",
    // spacing
    "--sp-3:",
    "--gap-control:",
    // shape
    "--radius-pill:",
    "--radius-input:",
    "--radius-chip:",
    // surfaces
    "--surface-header:",
    "--surface-overlay:",
    "--surface-control:",
    "--elevation-overlay:",
    // interaction
    "--state-hover-bg:",
    "--state-disabled-opacity:",
    "--ring-focus:",
  ];
  for (const token of tokens) {
    it(`declares ${token}`, () => {
      expect(css).toContain(token);
    });
  }
});

describe("asset + visual policy", () => {
  it("uses a tuned system-ui font stack", () => {
    const line = css.split("\n").find((l) => l.includes("--font-ui:"));
    expect(line).toBeTruthy();
    expect(line!).toContain("system-ui");
  });

  it("embeds no font (no @font-face, no data:-URI font)", () => {
    expect(cssNoComments).not.toMatch(/@font-face/i);
    expect(cssNoComments).not.toMatch(/url\(\s*['"]?data:/i);
  });

  it("makes no external asset request (no @import, no http(s) url)", () => {
    expect(cssNoComments).not.toMatch(/@import/i);
    expect(cssNoComments).not.toMatch(/url\(\s*['"]?https?:/i);
  });

  it("uses no backdrop-filter blur", () => {
    expect(cssNoComments).not.toMatch(/backdrop-filter/i);
  });

  it("declares a minimum readable graph-label size token", () => {
    expect(css).toContain("--graph-label-min:");
  });

  it("routes controls through the shared pill radius primitive", () => {
    expect(css).toMatch(/border-radius:\s*var\(--radius-pill\)/);
  });
});

describe("dark / light / forced-colors coverage", () => {
  it("keeps the maintained light-theme token blocks", () => {
    expect(css).toMatch(/prefers-color-scheme:\s*light/);
    expect(css).toMatch(/\[data-theme="light"\]/);
  });

  it("keeps a forced-colors fallback", () => {
    expect(css).toMatch(/forced-colors:\s*active/);
  });
});

describe("responsive foundation (CSS contract)", () => {
  it("view navigation is a single scrolling row (no multi-row wrap)", () => {
    const block = css.slice(css.indexOf(".cv-tabs,"));
    expect(block).toMatch(/flex-wrap:\s*nowrap/);
    expect(block).toMatch(/overflow-x:\s*auto/);
  });

  it("the body grid lets the graph column shrink (no page overflow)", () => {
    const block = css.slice(css.indexOf(".cv-region-body {"));
    expect(block).toMatch(/grid-template-columns:\s*minmax\(0,\s*1fr\)/);
    expect(block).toMatch(/min-width:\s*0/);
  });
});

describe("Phase-2 overlay + responsive-inspector CSS contract", () => {
  it("declares the deterministic z-index scale tokens", () => {
    for (const t of [
      "--z-canvas:",
      "--z-overlays:",
      "--z-inspector-trigger:",
      "--z-inspector-drawer:",
      "--z-source-dialog:",
    ]) {
      expect(css).toContain(t);
    }
  });

  it("shares one overlay primitive that opts panels into pointer events", () => {
    const block = css.slice(css.indexOf(".cv-overlay-panel {"));
    expect(block).toMatch(/pointer-events:\s*auto/);
    expect(block).toMatch(/z-index:\s*var\(--z-overlays\)/);
  });

  it("collapses to a single inspector-less grid column below 1200px", () => {
    expect(cssNoComments).toMatch(/@media\s*\(max-width:\s*1199px\)/);
    expect(css).toMatch(/grid-template-columns:\s*minmax\(0,\s*1fr\)\s*clamp\(320px/);
  });

  it("suppresses the drawer transition under reduced motion", () => {
    const rm = css.slice(css.lastIndexOf("prefers-reduced-motion"));
    expect(rm).toMatch(/\.cv-inspector-dialog--medium\s*\{\s*transition:\s*none/);
  });

  it("adds no backdrop-filter in Phase 2 either", () => {
    expect(cssNoComments).not.toMatch(/backdrop-filter/i);
  });
});

describe("Phase-3 node-card dimensions (locked, within approved ranges)", () => {
  it("returns the exact locked per-category boxes", () => {
    expect(cardSize("workspace")).toEqual({ width: 252, height: 128 });
    expect(cardSize("package")).toEqual({ width: 252, height: 128 });
    expect(cardSize("target")).toEqual({ width: 236, height: 116 });
    expect(cardSize("module")).toEqual({ width: 224, height: 108 });
    expect(cardSize("manual_block", "manual")).toEqual({ width: 224, height: 108 });
    expect(cardSize("struct")).toEqual({ width: 216, height: 104 });
    expect(cardSize("trait")).toEqual({ width: 216, height: 104 });
    expect(cardSize("function")).toEqual({ width: 216, height: 104 });
  });

  it("every locked box is within its PRD-approved range", () => {
    const inRange = (v: number, lo: number, hi: number) => v >= lo && v <= hi;
    const wp = cardSize("package");
    expect(inRange(wp.width, 240, 260) && inRange(wp.height, 120, 136)).toBe(true);
    const tg = cardSize("target");
    expect(inRange(tg.width, 228, 244) && inRange(tg.height, 108, 124)).toBe(true);
    const md = cardSize("module");
    expect(inRange(md.width, 216, 232) && inRange(md.height, 100, 116)).toBe(true);
    const cd = cardSize("struct");
    expect(inRange(cd.width, 208, 224) && inRange(cd.height, 96, 112)).toBe(true);
  });
});
