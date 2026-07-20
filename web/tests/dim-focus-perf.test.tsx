import { describe, it, expect, beforeEach, vi } from "vitest";

vi.mock("@xyflow/react", () => import("./support/xyflow.tsx"));

import { render, screen, fireEvent, waitFor, cleanup } from "@testing-library/react";
import { App } from "../src/App.tsx";
import { fakeLayout, watchDisabledLiveReload } from "./support/harness.tsx";
import { fakeSource } from "./support/harness.tsx";
import type { ExplorerDocument } from "../src/types/index.ts";
import type { LoadOutcome } from "../src/api/load.ts";

/**
 * A synthetic graph: 500 packages, one high-degree anchor connected to 200 of
 * them, ~1000 relations total, some diagnostics, plus a couple of manual flow
 * relations (eligible + ineligible). Rendered via the jsdom React Flow stub.
 */
function bigDocument(): ExplorerDocument {
  const N = 500;
  const entities = Array.from({ length: N }, (_, i) => ({
    id: `package:p${i}`,
    kind: "package",
    label: { default: `p${i}` },
    qualified_name: `p${i}`,
    provenance: i < 2 ? "manual" : "discovered",
  }));
  const relations: unknown[] = [];
  // High-degree anchor p0 → p1..p200.
  for (let i = 1; i <= 200; i++) {
    relations.push({ id: `r:a${i}`, kind: "depends_on", from: "package:p0", to: `package:p${i}`, provenance: "discovered" });
  }
  // A ring of the remaining to reach ~1000 edges.
  for (let i = 0; i < N; i++) {
    relations.push({ id: `r:ring${i}`, kind: "depends_on", from: `package:p${i}`, to: `package:p${(i + 1) % N}`, provenance: "discovered" });
  }
  for (let i = 0; i < 300; i++) {
    relations.push({ id: `r:x${i}`, kind: "uses", from: `package:p${i}`, to: `package:p${(i + 7) % N}`, provenance: "discovered" });
  }
  // One eligible + one ineligible manual flow relation.
  relations.push({ id: "r:flow", kind: "manual", from: "package:p0", to: "package:p1", provenance: "manual", attributes: { flow: "active" } });
  relations.push({ id: "r:plain", kind: "manual", from: "package:p1", to: "package:p2", provenance: "manual" });
  return {
    schema_version: "1.0",
    project: { id: "big", name: "Big", description: "" },
    entities,
    relations,
    views: [{ id: "view:workspace-overview", title: { default: "All" }, entity_kinds: [], relation_kinds: [], stages: [], presentation: {} }],
  } as unknown as ExplorerDocument;
}

function renderBig() {
  const outcome: LoadOutcome = {
    status: "ok",
    document: bigDocument(),
    generation: { generated_at: "t", partial: false },
    generationAvailable: true,
    diagnostics: { schema_version: "1.0", diagnostics: [{ severity: "warning", code: "x", message: "m", occurrence_count: 1, entities: ["package:p5"] }] },
    diagnosticsAvailable: true,
    partial: false,
  } as unknown as LoadOutcome;
  const source = fakeSource(outcome);
  const layout = fakeLayout("ok");
  // A large budget so the whole graph renders (dim is a full-projection mode).
  render(<App source={source} layout={layout.engine} initialSearch="" budget={2000} liveReload={watchDisabledLiveReload} />);
  return { layout };
}

beforeEach(() => {
  cleanup();
  window.history.pushState(null, "", "/");
});

describe("dim focus performance (500 nodes / ~1000 relations)", () => {
  it("dim anchor changes over the full projection request no new layout; smoke timing", async () => {
    const start = performance.now();
    const { layout } = renderBig();
    await screen.findByRole("tablist", { name: "Views" });
    await waitFor(() => expect(document.querySelectorAll('[data-testid^="node-"]').length).toBeGreaterThan(400));
    await waitFor(() => {});
    const base = layout.calls.length;

    // Enter dim on the high-degree anchor.
    fireEvent.click(screen.getByTestId("node-package:p0"));
    fireEvent.click(screen.getByRole("button", { name: "Dim unrelated" }));
    await waitFor(() => {});
    expect(layout.calls.length).toBe(base); // dim keeps the full projection

    // Move the dim anchor several times → still full projection → no relayout.
    for (const id of ["package:p10", "package:p250", "package:p3"]) {
      fireEvent.click(screen.getByTestId(`node-${id}`));
      await waitFor(() => {});
    }
    expect(layout.calls.length).toBe(base);

    // Clearing dim keeps the full projection too → still no new layout.
    fireEvent.click(screen.getByRole("button", { name: "Clear focus" }));
    await waitFor(() => {});
    expect(layout.calls.length).toBe(base);

    const ms = performance.now() - start;
    // Smoke ceiling only, not a production guarantee.
    expect(ms).toBeLessThan(15000);
  });
});
