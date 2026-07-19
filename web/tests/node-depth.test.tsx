import { describe, it, expect, beforeEach } from "vitest";
import { readFileSync } from "node:fs";
import { render, cleanup } from "@testing-library/react";
import { NodeCardView } from "../src/components/NodeCard.tsx";
import type { NodeCard, NodeCategory } from "../src/model/nodeCards.ts";

const css = readFileSync("src/styles.css", "utf8");

/** The `.cv-node { … }` base rule body. */
function cvNodeRule(): string {
  const start = css.indexOf(".cv-node {");
  return css.slice(start, css.indexOf("}", start) + 1);
}

beforeEach(() => cleanup());

// =====================================================================
// Token + CSS-contract tests
// =====================================================================
describe("depth tokens — one coherent, bounded system", () => {
  it("defines a single tint token in dark and light", () => {
    expect(css).toMatch(/--node-tint:\s*\d+%/);
    // Present under the light overrides too (>= 2 total definitions: dark + light).
    expect((css.match(/--node-tint:/g) ?? []).length).toBeGreaterThanOrEqual(2);
  });
  it("defines the three bounded elevation tokens in dark and light", () => {
    for (const t of ["--node-elevation", "--node-elevation-raised", "--node-elevation-selected"]) {
      expect((css.match(new RegExp(`${t}:`, "g")) ?? []).length).toBeGreaterThanOrEqual(2);
    }
  });
  it("elevation shadows are neutral (no coloured glow / accent in the shadow)", () => {
    // Grab every elevation token definition and confirm each is an rgba(0..)/(neutral)
    // offset shadow, never referencing a category accent or a large blur.
    const defs = [...css.matchAll(/--node-elevation[\w-]*:\s*([^;]+);/g)].map((m) => m[1]);
    expect(defs.length).toBeGreaterThanOrEqual(6); // 3 tokens × ≥2 themes
    for (const d of defs) {
      expect(d).toMatch(/rgba\(/); // colour is a neutral rgba…
      expect(d).not.toMatch(/var\(--(accent|node-(workspace|package|type|module|trait|function|impl|manual|unknown))/);
      // Bounded blur: the blur radius (3rd length) is not extreme.
      const nums = d.match(/-?\d+(\.\d+)?px/g)?.map((n) => parseFloat(n)) ?? [];
      for (const n of nums) expect(Math.abs(n)).toBeLessThanOrEqual(24);
    }
  });
});

describe("card surface — accent-derived, no per-category gradient literal", () => {
  it("derives the top surface from --accent + --node-bg via color-mix (one rule)", () => {
    const rule = cvNodeRule();
    expect(rule).toMatch(/background-image:\s*linear-gradient\(/);
    expect(rule).toMatch(
      /color-mix\(in srgb, var\(--accent\) var\(--node-tint\), var\(--node-bg\)\)/,
    );
  });
  it("keeps a flat background-color fallback BEFORE the gradient (compatibility)", () => {
    const rule = cvNodeRule();
    const bgColor = rule.indexOf("background-color: var(--node-bg)");
    const bgImage = rule.indexOf("background-image:");
    expect(bgColor).toBeGreaterThanOrEqual(0);
    expect(bgImage).toBeGreaterThan(bgColor); // fallback declared first
  });
  it("applies a base elevation on the card", () => {
    expect(cvNodeRule()).toMatch(/box-shadow:\s*var\(--node-elevation\)/);
  });
  it("keeps each category's distinct accent (hierarchy not flattened to one cue)", () => {
    for (const cat of ["workspace", "package", "target", "module", "type", "trait", "function", "impl", "manual", "unknown"]) {
      expect(css).toMatch(new RegExp(`\\.cv-node--${cat}\\b[^{]*\\{[^}]*--accent:\\s*var\\(--node-${cat}\\)`));
    }
  });
  it("has NO per-category gradient/background literal (categories only set --accent/border)", () => {
    // Every `.cv-node--<category>` rule must not set a background or gradient.
    const catRules = [...css.matchAll(/\.cv-node--(?:workspace|package|target|module|type|trait|function|impl|manual|unknown)\b[^{]*\{([^}]*)\}/g)];
    expect(catRules.length).toBeGreaterThanOrEqual(10);
    for (const [, body] of catRules) {
      expect(body).not.toMatch(/background/);
      expect(body).not.toMatch(/linear-gradient/);
    }
  });
});

describe("state composition — depth + rings, selection strongest", () => {
  function ruleBody(selector: string): string {
    const start = css.indexOf(selector + " {");
    return css.slice(start, css.indexOf("}", start) + 1);
  }
  it("selected composes the focus ring with the strongest elevation", () => {
    const sel = ruleBody(".cv-node--state-selected");
    expect(sel).toMatch(/box-shadow:[\s\S]*var\(--focus\)[\s\S]*var\(--node-elevation-selected\)/);
  });
  it("diagnostic + search rings keep the base elevation composed in", () => {
    for (const s of [".cv-node--state-diagnostic-warning", ".cv-node--state-diagnostic-error", ".cv-node--state-search"]) {
      expect(ruleBody(s)).toMatch(/var\(--node-elevation\)/);
    }
  });
  it("overview cards read slightly more raised", () => {
    const start = css.indexOf(".cv-node--workspace,");
    const body = css.slice(start, css.indexOf("}", start) + 1);
    expect(body).toMatch(/var\(--node-elevation-raised\)/);
  });
  it("no state or hover rule changes border-width/padding (dimension safety)", () => {
    for (const s of [
      ".cv-node--state-selected",
      ".cv-node--state-search",
      ".cv-node--state-related",
      ".cv-node--state-diagnostic-warning",
      ".cv-node:not(.cv-node--state-selected):hover",
    ]) {
      const start = css.indexOf(s + " {");
      const body = css.slice(start, css.indexOf("}", start) + 1);
      expect(body).not.toMatch(/border-width/);
      expect(body).not.toMatch(/padding/);
      expect(body).not.toMatch(/(^|[^-])\bborder:\s/); // no border shorthand (which sets width)
    }
  });
});

describe("forced colors — strips decoration, keeps structure", () => {
  const fc = css.slice(css.indexOf("@media (forced-colors: active)", css.indexOf(".cv-node {")));
  const block = fc.slice(0, fc.indexOf("\n}\n\n") + 3);
  it("removes the gradient and shadows", () => {
    expect(block).toMatch(/background-image:\s*none/);
    expect(block).toMatch(/box-shadow:\s*none/);
  });
  it("retains the selected outline", () => {
    expect(block).toMatch(/outline:\s*3px solid Highlight/);
  });
});

// =====================================================================
// Rendering + dimension-invariance tests
// =====================================================================
function card(overrides: Partial<NodeCard> = {}): NodeCard {
  return {
    id: "n",
    kind: "package",
    category: "package",
    known: true,
    kindLabel: "Package",
    title: "demo",
    fullTitle: "demo",
    hasSource: false,
    metrics: [],
    width: 216,
    height: 92,
    ...overrides,
  };
}
function box(): HTMLElement {
  return document.querySelector(".cv-node") as HTMLElement;
}

describe("depth applies via classes to every category (never inline colour)", () => {
  const categories: [NodeCategory, string][] = [
    ["workspace", "cv-node--workspace"],
    ["package", "cv-node--package"],
    ["target", "cv-node--target"],
    ["module", "cv-node--module"],
    ["type", "cv-node--type"],
    ["trait", "cv-node--trait"],
    ["function", "cv-node--function"],
    ["impl", "cv-node--impl"],
    ["manual", "cv-node--manual"],
    ["unknown", "cv-node--unknown"],
  ];
  it("each category gets its category class and no inline background/shadow", () => {
    for (const [category, cls] of categories) {
      render(<NodeCardView card={card({ category })} zoom={1} selected={false} related={false} searchMatch={false} />);
      const el = box();
      expect(el.className).toContain(cls);
      // Depth is token/class-driven — no per-card colour computed into inline style.
      expect(el.style.background).toBe("");
      expect(el.style.boxShadow).toBe("");
      cleanup();
    }
  });
});

describe("dimensions are byte-for-byte identical across every visual state", () => {
  const states = [
    { selected: false, related: false, searchMatch: false },
    { selected: true, related: false, searchMatch: false },
    { selected: false, related: true, searchMatch: false },
    { selected: false, related: false, searchMatch: true },
  ];
  it("width/minHeight never change with state, selection or zoom", () => {
    const seen = new Set<string>();
    for (const z of [0.3, 1, 2]) {
      for (const s of states) {
        render(<NodeCardView card={card({ diagnostic: { severity: "error", occurrences: 1, records: 1, label: "1 error" } })} zoom={z} {...s} />);
        seen.add(`${box().style.width}|${box().style.minHeight}`);
        cleanup();
      }
    }
    // Exactly one distinct (width,minHeight) pair across all states/zooms.
    expect([...seen]).toEqual(["216px|92px"]);
  });
});

describe("depth performance (smoke) — token-driven, no per-card colour in JS", () => {
  it("renders 500 cards with no inline background/shadow, within a generous bound", () => {
    const cats: NodeCategory[] = ["workspace", "package", "target", "module", "type", "trait", "function", "impl", "manual", "unknown"];
    const start = performance.now();
    for (let i = 0; i < 500; i++) {
      render(<NodeCardView card={card({ id: `n${i}`, category: cats[i % cats.length] })} zoom={1} selected={i % 7 === 0} related={false} searchMatch={false} />);
    }
    const ms = performance.now() - start;
    // No inline colour was computed onto any card — depth is entirely CSS/token.
    for (const el of document.querySelectorAll<HTMLElement>(".cv-node")) {
      expect(el.style.background).toBe("");
      expect(el.style.boxShadow).toBe("");
    }
    cleanup();
    // Smoke ceiling only, not a production guarantee.
    expect(ms).toBeLessThan(4000);
  });
});

describe("selected remains strongest while diagnostic/search info persists", () => {
  it("selected state class present; badge + kind text still rendered", () => {
    render(
      <NodeCardView
        card={card({ diagnostic: { severity: "error", occurrences: 2, records: 1, label: "2 errors" } })}
        zoom={1}
        selected={true}
        related={true}
        searchMatch={true}
      />,
    );
    expect(box().dataset.state).toBe("selected");
    expect(document.querySelector(".cv-node-diag")).not.toBeNull();
    expect(box().textContent).toContain("Package");
  });
});
