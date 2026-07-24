// Issue 15 Phase 4: role presentation on the node card (rendering + state + a11y).
import { describe, it, expect, afterEach } from "vitest";
import { render, screen, cleanup, within } from "@testing-library/react";
import { readFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

import { NodeCardView } from "../src/components/NodeCard.tsx";
import { cardSize, type NodeCard, type CardRole } from "../src/model/nodeCards.ts";

const HERE = dirname(fileURLToPath(import.meta.url));
const readSrc = (rel: string) => readFileSync(resolve(HERE, "../src", rel), "utf8");

function role(overrides: Partial<CardRole> = {}): CardRole {
  return { authoredValue: "service", label: "Service", token: "--role-service", cue: "band", known: true, ...overrides };
}
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
const view = (c: NodeCard, zoom = 0.7, selected = false, extra: Partial<Parameters<typeof NodeCardView>[0]> = {}) =>
  render(
    <NodeCardView card={c} zoom={zoom} selected={selected} related={false} searchMatch={false} {...extra} />,
  );

afterEach(cleanup);

describe("role badge + cue", () => {
  it("renders a role badge (label) and the role cue for a known role, alongside the kind badge", () => {
    view(card({ role: role() }));
    expect(screen.getByText("Package")).toBeInTheDocument(); // kind identity kept
    expect(screen.getByText("Service")).toBeInTheDocument(); // role badge
    const cue = document.querySelector(".cv-node-role-cue");
    expect(cue).not.toBeNull();
    expect(cue!.getAttribute("data-cue")).toBe("band");
    expect(box().getAttribute("data-role")).toBe("service");
  });

  it("renders an unknown role badge with the authored value verbatim + neutral cue", () => {
    view(card({ role: role({ authoredValue: "scheduler", label: "scheduler", token: "--role-unknown", cue: "neutral", known: false }) }));
    const badge = document.querySelector(".cv-node-role")!;
    expect(badge.textContent).toBe("scheduler");
    expect(badge.classList.contains("cv-node-role--unknown")).toBe(true);
    expect(document.querySelector(".cv-node-role-cue")!.getAttribute("data-cue")).toBe("neutral");
    expect(box().getAttribute("data-role")).toBe("unknown");
  });

  it("bounds a long unknown value but keeps it complete in title + accessible name", () => {
    const longValue = "orchestration-control-plane-supervisor";
    view(card({ role: role({ authoredValue: longValue, label: longValue, token: "--role-unknown", cue: "neutral", known: false }) }));
    const badge = document.querySelector(".cv-node-role") as HTMLElement;
    expect(badge.classList.contains("cv-node-role--unknown")).toBe(true); // width-bounded class
    expect(badge.getAttribute("title")).toBe(longValue);
    expect(box().getAttribute("aria-label")).toContain(longValue);
  });

  it("renders NO role badge or cue when no role is authored (no blank slot)", () => {
    view(card({ role: undefined }));
    expect(document.querySelector(".cv-node-role")).toBeNull();
    expect(document.querySelector(".cv-node-role-cue")).toBeNull();
    expect(box().getAttribute("data-role")).toBeNull();
  });
});

describe("role never changes the deterministic box", () => {
  it("has identical dimensions with and without a role, across densities", () => {
    const s = cardSize("package");
    const dims = new Set<string>();
    for (const r of [undefined, role(), role({ cue: "double-border", label: "Observability" })]) {
      for (const zoom of [0.3, 0.7, 1.3]) {
        view(card({ role: r, width: s.width, height: s.height }), zoom);
        dims.add(`${box().style.width}|${box().style.height}`);
        cleanup();
      }
    }
    expect([...dims]).toEqual([`${s.width}px|${s.height}px`]);
  });

  it("works on the smallest code/default card without changing its box", () => {
    const s = cardSize("struct");
    view(card({ kind: "struct", category: "type", kindLabel: "Struct", width: s.width, height: s.height, role: role({ label: "Domain", cue: "top-rule" }) }));
    expect(box().style.width).toBe(`${s.width}px`);
    expect(box().style.height).toBe(`${s.height}px`);
    expect(screen.getByText("Domain")).toBeInTheDocument();
  });
});

describe("state composition + accessibility", () => {
  it("keeps selection dominant while the role stays present", () => {
    view(card({ role: role() }), 1, true);
    expect(box().getAttribute("data-state")).toBe("selected");
    expect(screen.getByText("Service")).toBeInTheDocument();
  });

  it("keeps a diagnostic recognizable and the role subordinate to it", () => {
    view(card({ role: role(), diagnostic: { severity: "error", occurrences: 2, records: 1, label: "1 error (2 occurrences)" } }), 1);
    expect(box().getAttribute("data-state")).toBe("diagnostic-error");
    expect(screen.getByText("Service")).toBeInTheDocument(); // role coexists
    expect(within(box()).getByRole("img", { name: /error/i })).toBeInTheDocument();
  });

  it("keeps the role text present (readable) when the card is dimmed", () => {
    view(card({ role: role() }), 1, false, { dimmed: true });
    expect(box().getAttribute("data-dimmed")).toBe("true");
    expect(screen.getByText("Service")).toBeInTheDocument();
  });

  it("names the architectural role in the accessible label (full value for unknown)", () => {
    view(card({ role: role({ authoredValue: "scheduler", label: "scheduler", known: false }) }));
    expect(box().getAttribute("aria-label")).toContain("architectural role: scheduler");
  });
});

describe("architecture guards", () => {
  it("the card component never parses the raw authored attribute itself", () => {
    const src = readSrc("components/NodeCard.tsx");
    // Role must arrive as the pre-projected `card.role`; the component must not
    // call the parser or read raw `attributes` (it may still use the kind-derived
    // `card.category`, which is unrelated to the authored role).
    expect(src).not.toMatch(/authoredRole/);
    expect(src).not.toMatch(/roleStyleFor/);
    expect(src).not.toMatch(/attributes/);
  });

  it("the role CSS uses no clip-path and no external asset", () => {
    const css = readSrc("styles.css").replace(/\/\*[\s\S]*?\*\//g, "");
    const roleBlock = css.slice(css.indexOf(".cv-node-role"));
    expect(roleBlock).not.toMatch(/clip-path/i);
    expect(roleBlock).not.toMatch(/url\(\s*['"]?https?:/i);
    expect(roleBlock).not.toMatch(/backdrop-filter/i);
  });

  it("no role rule sets outer padding/border-width on the card root (layout safety)", () => {
    const css = readSrc("styles.css").replace(/\/\*[\s\S]*?\*\//g, "");
    // A `.cv-node[data-role…]` rule must never change the card's own padding or
    // border width — that would move the fixed box.
    const roleRootRule = /\.cv-node\[data-role[^{]*\{[^}]*(padding|border-width|border:)/;
    expect(css).not.toMatch(roleRootRule);
  });

  it("keeps the decorative cue below content in the stacking order (cannot cover a diagnostic)", () => {
    const css = readSrc("styles.css").replace(/\/\*[\s\S]*?\*\//g, "");
    const cueRule = css.slice(css.indexOf(".cv-node-role-cue {"));
    const z = cueRule.match(/z-index:\s*(\d+)/);
    expect(z).not.toBeNull();
    expect(Number(z![1])).toBeLessThanOrEqual(2); // never above the card content/diag
  });
});
