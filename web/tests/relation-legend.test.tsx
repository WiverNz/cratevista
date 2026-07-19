import { describe, it, expect } from "vitest";
import { render, screen, within } from "@testing-library/react";
import { readFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

import { Legend } from "../src/components/Panels.tsx";
import { relationLegendForGraph } from "../src/state/selectors.ts";
import { relationStyleFor } from "../src/adapter/relationStyle.ts";
import type { Graph, GraphEdge } from "../src/adapter/adapter.ts";

function edge(kind: string): GraphEdge {
  return { id: `e:${kind}`, source: "a", target: "b", kind, style: { category: kind, color: "", known: true } };
}
function relationsFor(kinds: string[]) {
  const graph: Graph = { nodes: [], edges: kinds.map(edge) };
  return relationLegendForGraph(graph);
}

function renderLegend(kinds: string[]) {
  return render(<Legend entries={[]} relations={relationsFor(kinds)} />);
}

describe("relation legend: active kinds only", () => {
  it("shows exactly the relation kinds present in the view", () => {
    renderLegend(["contains", "depends_on"]);
    const group = screen.getByRole("group", { name: "Relation types" });
    expect(within(group).getByText("depends on")).toBeInTheDocument();
    expect(within(group).getByText("contains")).toBeInTheDocument();
    // A relation not in the active view is absent.
    expect(within(group).queryByText("implements")).not.toBeInTheDocument();
  });
});

describe("relation legend: driven by the central registry", () => {
  it("each sample's rendered style matches relationStyleFor(kind) exactly", () => {
    renderLegend(["contains", "depends_on", "implements"]);
    for (const kind of ["contains", "depends_on", "implements"]) {
      const style = relationStyleFor(kind);
      const sample = document.querySelector(`.cv-rel-sample[data-kind="${kind}"]`) as HTMLElement;
      expect(sample).toBeTruthy();
      expect(sample.dataset.pattern).toBe(style.pattern);
      expect(sample.dataset.marker).toBe(style.marker);
      expect(sample.dataset.width).toBe(String(style.states.normal.width));
      expect(sample.dataset.strokeToken).toBe(style.strokeToken);
    }
  });

  it("distinguishes contains from depends_on by pattern and marker, not colour alone", () => {
    renderLegend(["contains", "depends_on"]);
    const contains = document.querySelector('.cv-rel-sample[data-kind="contains"]') as HTMLElement;
    const depends = document.querySelector('.cv-rel-sample[data-kind="depends_on"]') as HTMLElement;
    expect(contains.dataset.pattern).not.toBe(depends.dataset.pattern);
    expect(contains.dataset.marker).not.toBe(depends.dataset.marker);
    expect(contains.dataset.width).not.toBe(depends.dataset.width);
  });
});

describe("relation legend: keyboard + screen-reader accessibility", () => {
  it("each sample is focusable and has an accessible name describing relation and direction", () => {
    renderLegend(["depends_on", "contains"]);
    const depends = screen.getByRole("img", { name: /depends on/i });
    expect(depends).toHaveAttribute("tabindex", "0");
    expect(depends.getAttribute("aria-label")).toMatch(/arrow shows direction from source to target/i);

    const contains = screen.getByRole("img", { name: /contains/i });
    expect(contains).toHaveAttribute("tabindex", "0");
    expect(contains.getAttribute("aria-label")).toMatch(/no direction marker/i);
  });

  it("includes visible text labels alongside the samples", () => {
    renderLegend(["depends_on"]);
    const group = screen.getByRole("group", { name: "Relation types" });
    expect(within(group).getByText("depends on")).toBeInTheDocument();
  });
});

describe("relation legend: dark + light token availability", () => {
  const here = dirname(fileURLToPath(import.meta.url));
  const css = readFileSync(resolve(here, "../src/styles.css"), "utf8");

  it("defines relation stroke tokens in both the default (dark) and a light palette", () => {
    // Default (dark) palette.
    expect(css).toMatch(/:root\s*\{[^}]*--rel-depends-on:/s);
    // A light palette (media query and/or explicit data-theme override).
    expect(css).toMatch(/prefers-color-scheme:\s*light/);
    expect(css).toMatch(/data-theme="light"\]\s*\{[^}]*--rel-depends-on:/s);
    // Label halo tokens exist for both themes.
    expect(css).toMatch(/--rel-label-fg:/);
    expect(css).toMatch(/--rel-label-bg:/);
  });
});
