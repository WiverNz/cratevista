// Issue 15 Phase 5: redesigned inspector rendering (sections, chips, bounded lists
// + Show more, selectable endpoints, empty states, entity/relation parity), and
// the invariant that expanding a group requests no layout and changes no URL.
import { describe, it, expect, beforeEach, vi } from "vitest";

vi.mock("@xyflow/react", () => import("./support/xyflow.tsx"));

import { screen, within, fireEvent, waitFor } from "@testing-library/react";
import { renderApp, okOutcome } from "./support/harness.tsx";

function hubDoc(childCount = 8) {
  const kids = Array.from({ length: childCount }, (_, i) => ({
    id: `item:struct:m::S${String(i).padStart(2, "0")}`,
    kind: "struct",
    label: { default: `Struct${String(i).padStart(2, "0")}` },
    qualified_name: `m::S${i}`,
    provenance: "discovered" as const,
    parent: "module:m",
  }));
  return {
    schema_version: "1.1",
    project: { id: "m", name: "M", description: "" },
    entities: [
      { id: "module:m", kind: "module", label: { default: "m" }, qualified_name: "m", provenance: "discovered" as const, attributes: { category: "service" } },
      { id: "item:struct:lonely", kind: "struct", label: { default: "Lonely" }, qualified_name: "lonely", provenance: "discovered" as const },
      ...kids,
    ],
    relations: kids.map((k, i) => ({ id: `rel:contains:m-${i}`, kind: "contains", from: "module:m", to: k.id, provenance: "discovered" as const })),
    views: [
      {
        id: "view:hub",
        title: { default: "Hub" },
        entity_kinds: [],
        relation_kinds: [],
        entity_ids: ["module:m", "item:struct:lonely", ...kids.map((k) => k.id)],
      },
    ],
  };
}

const HUB = "module:m";

async function ready() {
  return screen.findByRole("tablist", { name: "Views" });
}
async function selectHub() {
  fireEvent.click(await screen.findByTestId(`node-${HUB}`));
  return screen.findByRole("region", { name: "Entity inspector" });
}

beforeEach(() => {
  window.history.pushState(null, "", "/");
});

describe("entity inspector composition", () => {
  it("renders sectioned content with kind/role/provenance chips", async () => {
    renderApp({ outcome: okOutcome({ document: hubDoc() }) });
    await ready();
    const insp = await selectHub();
    // Section headings.
    for (const s of ["Identity", "Source & repository", "Hierarchy", "Outgoing relations", "Incoming relations"]) {
      expect(within(insp).getByRole("region", { name: s })).toBeInTheDocument();
    }
    // Chips: kind + role (authored "service" → "Service") + provenance.
    expect(within(insp).getByText("module")).toBeInTheDocument();
    expect(within(insp).getByText("Service")).toBeInTheDocument();
    expect(within(insp).getByText("discovered")).toBeInTheDocument();
  });

  it("bounds a large relation group and expands it with an accessible disclosure", async () => {
    renderApp({ outcome: okOutcome({ document: hubDoc(8) }) });
    await ready();
    const insp = await selectHub();
    const outgoing = within(insp).getByRole("region", { name: "Outgoing relations" });
    // 8 contains, preview limit 6 → 6 rows shown, a "Show 2 more" disclosure.
    expect(within(outgoing).getAllByRole("listitem").filter((li) => li.className.includes("cv-insp-rel-row"))).toHaveLength(6);
    const more = within(outgoing).getByRole("button", { name: /Show 2 more/ });
    expect(more).toHaveAttribute("aria-expanded", "false");
    fireEvent.click(more);
    expect(within(outgoing).getByRole("button", { name: /Show less/ })).toHaveAttribute("aria-expanded", "true");
    expect(within(outgoing).getAllByRole("listitem").filter((li) => li.className.includes("cv-insp-rel-row"))).toHaveLength(8);
  });

  it("shows an explicit empty state when a direction has no relations", async () => {
    renderApp({ outcome: okOutcome({ document: hubDoc() }) });
    await ready();
    const insp = await selectHub();
    const incoming = within(insp).getByRole("region", { name: "Incoming relations" });
    expect(within(incoming).getByText(/No incoming relations/i)).toBeInTheDocument();
  });

  it("selects the other endpoint when a relation row is activated", async () => {
    renderApp({ outcome: okOutcome({ document: hubDoc() }) });
    await ready();
    const insp = await selectHub();
    // Scope to the Outgoing relations group (children + a graph node share the label).
    const outgoing = within(insp).getByRole("region", { name: "Outgoing relations" });
    fireEvent.click(within(outgoing).getByRole("button", { name: "Struct00" }));
    await waitFor(() =>
      expect(screen.getByRole("region", { name: "Entity inspector" })).toHaveTextContent("Struct00"),
    );
  });

  it("shows diagnostic severity and represented occurrence count (no counts lost)", async () => {
    const document = hubDoc();
    const diagnostics = {
      schema_version: "1.1",
      diagnostics: [{ severity: "warning", code: "w1", message: "careful", entities: [HUB], occurrence_count: 5 }],
    };
    renderApp({ outcome: okOutcome({ document, diagnostics } as Parameters<typeof okOutcome>[0]) });
    await ready();
    const insp = await selectHub();
    const sec = within(insp).getByRole("region", { name: "Diagnostics" });
    expect(within(sec).getByText("warning")).toBeInTheDocument();
    expect(within(sec).getByText(/5 occurrences/)).toBeInTheDocument();
    expect(within(sec).getByText("w1")).toBeInTheDocument();
  });

  it("expanding a group requests no layout and changes no URL", async () => {
    const { layout } = renderApp({ outcome: okOutcome({ document: hubDoc() }) });
    await ready();
    const insp = await selectHub();
    await waitFor(() => expect(layout.calls.length).toBeGreaterThan(0));
    const before = layout.calls.length;
    const url = window.location.search;
    const outgoing = within(insp).getByRole("region", { name: "Outgoing relations" });
    fireEvent.click(within(outgoing).getByRole("button", { name: /Show 2 more/ }));
    expect(layout.calls.length).toBe(before);
    expect(window.location.search).toBe(url);
  });
});

describe("relation inspector parity", () => {
  it("shows direction with selectable endpoints", async () => {
    renderApp({ outcome: okOutcome({ document: hubDoc() }) });
    await ready();
    fireEvent.click(await screen.findByTestId("edge-rel:contains:m-0"));
    const insp = await screen.findByRole("region", { name: "Relation inspector" });
    const direction = within(insp).getByRole("region", { name: "Direction" });
    expect(within(direction).getByText("m")).toBeInTheDocument(); // from
    expect(within(direction).getByText("Struct00")).toBeInTheDocument(); // to
    // Endpoint is selectable.
    fireEvent.click(within(direction).getByRole("button", { name: "Struct00" }));
    await waitFor(() =>
      expect(screen.getByRole("region", { name: "Entity inspector" })).toHaveTextContent("Struct00"),
    );
  });
});
