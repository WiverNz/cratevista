// Issue 15 Phase 3: redesigned card composition + deterministic-box contract.
import { describe, it, expect, afterEach } from "vitest";
import { render, screen, cleanup } from "@testing-library/react";
import { readFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

import { NodeCardView } from "../src/components/NodeCard.tsx";
import { cardSize, type NodeCard } from "../src/model/nodeCards.ts";

const HERE = dirname(fileURLToPath(import.meta.url));
const readSrc = (rel: string) => readFileSync(resolve(HERE, "../src", rel), "utf8");

function card(overrides: Partial<NodeCard> = {}): NodeCard {
  return {
    id: "package:demo",
    kind: "package",
    category: "package",
    known: true,
    kindLabel: "Package",
    title: "demo",
    fullTitle: "demo",
    hasSource: false,
    metrics: [],
    width: 252,
    height: 128,
    ...overrides,
  };
}

const box = () => document.querySelector(".cv-node") as HTMLElement;
const view = (c: NodeCard, zoom = 0.7, selected = false) =>
  render(
    <NodeCardView card={c} zoom={zoom} selected={selected} related={false} searchMatch={false} />,
  );

afterEach(cleanup);

describe("card identity (always present)", () => {
  it("shows the kind badge and title at every density", () => {
    for (const zoom of [0.3, 0.7, 1.3]) {
      view(card(), zoom);
      expect(screen.getByText("Package")).toBeInTheDocument();
      expect(screen.getByText("demo")).toBeInTheDocument();
      cleanup();
    }
  });

  it("keeps the full title in the accessible name when the title is visibly truncated", () => {
    const full = "a_very_long_fully_qualified_item_name_that_will_truncate";
    view(card({ title: full, fullTitle: full }));
    const title = document.querySelector(".cv-node-title") as HTMLElement;
    expect(title.getAttribute("title")).toBe(full); // full text on hover
    expect(box().getAttribute("aria-label")).toContain(full); // and in the a11y name
  });
});

describe("supporting description line", () => {
  it("renders one bounded description line at normal/detailed when available", () => {
    view(card({ description: "The command-line entry point." }), 0.7);
    expect(document.querySelector(".cv-node-desc")?.textContent).toBe(
      "The command-line entry point.",
    );
  });

  it("hides the description at compact density (identity stays)", () => {
    view(card({ description: "hidden when compact" }), 0.3);
    expect(box().dataset.level).toBe("compact");
    expect(document.querySelector(".cv-node-desc")).toBeNull();
    expect(screen.getByText("demo")).toBeInTheDocument();
  });

  it("leaves NO empty description block when the entity has no description", () => {
    view(card({ description: undefined }), 1.3); // detailed
    expect(box().dataset.level).toBe("detailed");
    expect(document.querySelector(".cv-node-desc")).toBeNull();
  });
});

describe("phase scope: no architectural role yet", () => {
  it("renders no role badge / role-specific element", () => {
    view(card({ category: "package" }), 1.3);
    expect(document.querySelector(".cv-node-role")).toBeNull();
    expect(document.querySelector("[data-role]")).toBeNull();
  });

  it("gives an unknown kind the polished generic fallback (badge + title, no crash)", () => {
    view(card({ kind: "widget", category: "unknown", known: false, kindLabel: "widget" }), 0.7);
    expect(box().className).toContain("cv-node--unknown");
    expect(screen.getByText("widget")).toBeInTheDocument();
    expect(screen.getByText("demo")).toBeInTheDocument();
    expect(screen.getByText(/\(unknown\)/)).toBeInTheDocument();
  });
});

describe("deterministic box: render root == cardSize (ELK receives the same)", () => {
  it("renders exactly the cardSize() width/height per category", () => {
    const cases: Array<[string, string]> = [
      ["package", "package"],
      ["target", "target"],
      ["module", "module"],
      ["struct", "type"],
    ];
    for (const [kind, category] of cases) {
      const s = cardSize(kind);
      view(card({ kind, category: category as NodeCard["category"], width: s.width, height: s.height }));
      const el = box();
      expect(el.style.width).toBe(`${s.width}px`);
      expect(el.style.height).toBe(`${s.height}px`); // fixed height (not min-height)
      cleanup();
    }
  });

  it("does not change dimensions between compact, normal and detailed", () => {
    const s = cardSize("package");
    const seen = new Set<string>();
    for (const zoom of [0.3, 0.7, 1.3]) {
      view(card({ width: s.width, height: s.height }), zoom);
      seen.add(`${box().style.width}|${box().style.height}`);
      cleanup();
    }
    expect([...seen]).toEqual([`${s.width}px|${s.height}px`]);
  });
});

describe("architecture guards", () => {
  it("the node card never measures itself to decide size (no per-node observer)", () => {
    const src = readSrc("components/NodeCard.tsx");
    expect(src).not.toMatch(/ResizeObserver/);
    expect(src).not.toMatch(/getBoundingClientRect/);
  });

  it("the graph node renders both source and target handles (routed-edge attachment)", () => {
    const src = readSrc("components/Graph.tsx");
    expect(src).toMatch(/Handle[\s\S]{0,40}type="target"/);
    expect(src).toMatch(/Handle[\s\S]{0,40}type="source"/);
  });
});
