import { describe, it, expect } from "vitest";
import { render, screen, within, cleanup } from "@testing-library/react";

import { NodeCardView } from "../src/components/NodeCard.tsx";
import type { NodeCard, CardMetric } from "../src/model/nodeCards.ts";

function card(overrides: Partial<NodeCard> = {}): NodeCard {
  return {
    id: "package:flighttrace-api",
    kind: "package",
    category: "package",
    known: true,
    kindLabel: "Package",
    title: "flighttrace-api",
    fullTitle: "flighttrace-api",
    hasSource: false,
    metrics: [],
    width: 216,
    height: 92,
    ...overrides,
  };
}

const metric = (key: string, label: string, value: string, minLevel: CardMetric["minLevel"]): CardMetric => ({
  key,
  label,
  value,
  minLevel,
});

const pkgMetrics: CardMetric[] = [
  metric("version", "v", "0.1.6", "normal"),
  metric("deps", "deps", "3", "normal"),
  metric("targets", "targets", "2", "detailed"),
  metric("docs", "docs", "66%", "detailed"),
];

function box() {
  return document.querySelector(".cv-node") as HTMLElement;
}

describe("progressive disclosure", () => {
  it("compact shows only the kind badge and title", () => {
    render(<NodeCardView card={card({ metrics: pkgMetrics })} zoom={0.3} selected={false} related={false} searchMatch={false} />);
    expect(box().dataset.level).toBe("compact");
    expect(screen.getByText("Package")).toBeInTheDocument();
    expect(screen.getByText("flighttrace-api")).toBeInTheDocument();
    // No metrics rendered at compact.
    expect(document.querySelector(".cv-node-metrics")).toBeNull();
  });

  it("normal shows key metrics but hides detailed ones", () => {
    render(<NodeCardView card={card({ metrics: pkgMetrics })} zoom={0.7} selected={false} related={false} searchMatch={false} />);
    expect(box().dataset.level).toBe("normal");
    expect(screen.getByText("0.1.6")).toBeInTheDocument(); // version (normal)
    expect(screen.getByText("3")).toBeInTheDocument(); // deps (normal)
    expect(screen.queryByText("66%")).toBeNull(); // docs (detailed) hidden
  });

  it("detailed (or selected) shows richer metrics and indicators", () => {
    render(
      <NodeCardView
        card={card({ metrics: pkgMetrics, visibility: "public", documented: true, hasSource: true, category: "type", kind: "struct", kindLabel: "Struct", context: "flighttrace_api::dto" })}
        zoom={0.2}
        selected={true}
        related={false}
        searchMatch={false}
      />,
    );
    expect(box().dataset.level).toBe("detailed");
    expect(screen.getByText("66%")).toBeInTheDocument(); // detailed metric visible
    expect(screen.getByText("public")).toBeInTheDocument();
    expect(screen.getByText("documented")).toBeInTheDocument();
    expect(screen.getByText("source")).toBeInTheDocument();
    expect(screen.getByText("flighttrace_api::dto")).toBeInTheDocument();
  });
});

describe("package vs target distinction", () => {
  it("renders distinct kind badges, categories and target crate kind", () => {
    render(<NodeCardView card={card()} zoom={1} selected={false} related={false} searchMatch={false} />);
    const pkg = box();
    expect(pkg.className).toMatch(/cv-node--package/);
    expect(within(pkg).getByText("Package")).toBeInTheDocument();
    cleanup();

    render(
      <NodeCardView
        card={card({ id: "t", kind: "target", category: "target", kindLabel: "lib", title: "flighttrace_api (lib)", fullTitle: "flighttrace_api (lib)", width: 208, height: 80 })}
        zoom={1}
        selected={false}
        related={false}
        searchMatch={false}
      />,
    );
    const tgt = box();
    expect(tgt.className).toMatch(/cv-node--target/);
    expect(within(tgt).getByText("lib")).toBeInTheDocument();
  });
});

describe("long-name truncation + accessible full name", () => {
  it("keeps the full name in title + aria-label even when displayed name is truncated", () => {
    const full = "a_really_long_qualified_entity_name_that_will_visually_truncate";
    render(
      <NodeCardView
        card={card({ title: full, fullTitle: full, kind: "struct", category: "type", kindLabel: "Struct" })}
        zoom={1}
        selected={false}
        related={false}
        searchMatch={false}
      />,
    );
    const titleEl = document.querySelector(".cv-node-title") as HTMLElement;
    expect(titleEl.getAttribute("title")).toBe(full); // hover tooltip
    expect(box().getAttribute("aria-label")).toContain(full); // screen reader
  });
});

describe("missing optional metrics omitted cleanly", () => {
  it("does not render a metric that is absent (no N/A placeholder)", () => {
    render(
      <NodeCardView
        card={card({ metrics: [metric("version", "v", "1.0.0", "normal")] })}
        zoom={0.7}
        selected={false}
        related={false}
        searchMatch={false}
      />,
    );
    expect(screen.getByText("1.0.0")).toBeInTheDocument();
    expect(screen.queryByText(/N\/A/)).toBeNull();
    expect(screen.queryByText("deps")).toBeNull();
  });
});

describe("diagnostic occurrence badge + severity", () => {
  it("shows the severity symbol, occurrence count and an accessible label", () => {
    render(
      <NodeCardView
        card={card({ diagnostic: { severity: "warning", occurrences: 5, records: 2, label: "2 warnings (5 occurrences)" } })}
        zoom={0.7}
        selected={false}
        related={false}
        searchMatch={false}
      />,
    );
    const badge = screen.getByRole("img", { name: "2 warnings (5 occurrences)" });
    expect(badge.textContent).toContain("⚠");
    expect(badge.textContent).toContain("5"); // occurrences shown at normal
    expect(badge.getAttribute("aria-label")).toMatch(/5 occurrences/);
  });

  it("compact badge shows the marker without the count but keeps the a11y label", () => {
    render(
      <NodeCardView
        card={card({ diagnostic: { severity: "error", occurrences: 3, records: 3, label: "3 errors (3 occurrences)" } })}
        zoom={0.3}
        selected={false}
        related={false}
        searchMatch={false}
      />,
    );
    const badge = screen.getByRole("img", { name: /3 occurrences/ });
    expect(badge.textContent).toContain("✕");
    expect(badge.querySelector(".cv-node-diag-count")).toBeNull();
  });
});

describe("unknown kind fallback", () => {
  it("marks unknown kinds textually and with the unknown category", () => {
    render(
      <NodeCardView
        card={card({ kind: "teleporter", category: "unknown", known: false, kindLabel: "teleporter", title: "Gizmo", fullTitle: "Gizmo" })}
        zoom={1}
        selected={false}
        related={false}
        searchMatch={false}
      />,
    );
    expect(box().className).toMatch(/cv-node--unknown/);
    expect(screen.getByText(/\(unknown\)/)).toBeInTheDocument();
  });
});

describe("stable, deterministic dimensions", () => {
  it("uses the card's box regardless of density level", () => {
    render(<NodeCardView card={card()} zoom={0.3} selected={false} related={false} searchMatch={false} />);
    const compact = box();
    expect(compact.style.width).toBe("216px");
    expect(compact.style.minHeight).toBe("92px");
    cleanup();
    render(<NodeCardView card={card()} zoom={2} selected={true} related={false} searchMatch={false} />);
    const detailed = box();
    expect(detailed.style.width).toBe("216px"); // unchanged by zoom/selection
    expect(detailed.style.minHeight).toBe("92px");
  });
});

describe("meaning not by colour alone", () => {
  it("conveys kind and diagnostic severity as text, not only colour", () => {
    render(
      <NodeCardView
        card={card({ diagnostic: { severity: "error", occurrences: 1, records: 1, label: "1 error (1 occurrence)" } })}
        zoom={1}
        selected={false}
        related={false}
        searchMatch={false}
      />,
    );
    // Kind is a visible textual badge.
    expect(screen.getByText("Package")).toBeInTheDocument();
    // Diagnostic severity is available as text (aria-label), not colour only.
    expect(screen.getByRole("img", { name: /error/ })).toBeInTheDocument();
  });
});

describe("selected state priority", () => {
  it("selected dominates related, search and diagnostic emphasis", () => {
    render(
      <NodeCardView
        card={card({ diagnostic: { severity: "error", occurrences: 1, records: 1, label: "1 error (1 occurrence)" } })}
        zoom={1}
        selected={true}
        related={true}
        searchMatch={true}
      />,
    );
    expect(box().dataset.state).toBe("selected");
    // The diagnostic badge is still present alongside the selected emphasis.
    expect(screen.getByRole("img", { name: /error/ })).toBeInTheDocument();
  });
});
