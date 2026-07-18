import { describe, it, expect } from "vitest";
import type { ExplorerDocument } from "../src/types/index.ts";
import { buildModel } from "../src/model/model.ts";
import {
  documentToGraph,
  viewEntityIds,
  type Graph,
} from "../src/adapter/adapter.ts";
import {
  searchEntities,
  legendForGraph,
  kindsInGraph,
} from "../src/state/selectors.ts";
import { parseUrlState, serializeUrlState } from "../src/state/url.ts";
import {
  parseDiagnosticsReport,
  parseGenerationReport,
  parseHealth,
  apiErrorCode,
} from "../src/types/runtime.ts";
import parity from "../src/fixtures/schema_parity.document.json" with { type: "json" };
import unknown from "../src/fixtures/unknown_kinds.document.json" with { type: "json" };

const parityDoc = parity as unknown as ExplorerDocument;
const unknownDoc = unknown as unknown as ExplorerDocument;

describe("model indexes", () => {
  const model = buildModel(parityDoc);
  it("indexes entities/relations/views by id", () => {
    expect(model.entityById.size).toBe(parityDoc.entities.length);
    expect(model.relationById.size).toBe(parityDoc.relations.length);
    expect(model.viewById.size).toBe(parityDoc.views.length);
  });
  it("builds incoming/outgoing and children indexes", () => {
    const ws = "workspace";
    expect(model.outgoing.get(ws)?.length ?? 0).toBeGreaterThan(0);
    // struct Thing has module parent → appears under childrenByParent.
    const anyChildren = [...model.childrenByParent.values()].some(
      (c) => c.length > 0,
    );
    expect(anyChildren).toBe(true);
  });
  it("groups entities by kind", () => {
    expect(model.entitiesByKind.get("package")?.length ?? 0).toBeGreaterThan(0);
  });
  it("has a stable identity string", () => {
    expect(buildModel(parityDoc).identity).toBe(model.identity);
  });
});

describe("adapter projection", () => {
  const model = buildModel(parityDoc);
  const overview = model.viewById.get("view:overview")!;
  const types = model.viewById.get("view:types")!;

  it("projects only view entity_ids when explicit", () => {
    const ids = viewEntityIds(model, types);
    // full_mvp 'types' view lists struct/enum/union explicitly.
    expect(ids.size).toBe(3);
  });

  it("emits an edge only when both endpoints are visible", () => {
    const graph = documentToGraph(model, types);
    for (const edge of graph.edges) {
      const ids = new Set(graph.nodes.map((n) => n.id));
      expect(ids.has(edge.source)).toBe(true);
      expect(ids.has(edge.target)).toBe(true);
    }
  });

  it("applies an extra kind filter", () => {
    const graph = documentToGraph(model, overview, {
      kindFilter: new Set(["package"]),
    });
    expect(graph.nodes.every((n) => n.kind === "package")).toBe(true);
  });

  it("related-only narrows to focus neighborhood", () => {
    const graph = documentToGraph(model, overview, {
      relatedOnly: true,
      focusId: "workspace",
    });
    expect(graph.nodes.some((n) => n.id === "workspace")).toBe(true);
    // only workspace + direct neighbors
    expect(graph.nodes.length).toBeLessThan(parityDoc.entities.length);
  });
});

describe("unknown kinds render via generic fallback", () => {
  const model = buildModel(unknownDoc);
  const view = model.viewById.get("view:workspace-overview")!;
  const graph: Graph = documentToGraph(model, view);

  it("keeps unknown entity kinds with generic style + raw label", () => {
    const widget = graph.nodes.find((n) => n.kind === "widget")!;
    expect(widget.style.known).toBe(false);
    expect(widget.style.category).toBe("widget");
  });
  it("keeps unknown relation kinds", () => {
    const edge = graph.edges.find((e) => e.kind === "teleports_to")!;
    expect(edge.style.known).toBe(false);
  });
  it("legend includes only present categories (incl. unknown)", () => {
    const legend = legendForGraph(graph);
    const categories = legend.map((l) => l.category);
    expect(categories).toContain("widget");
    expect(kindsInGraph(graph)).toContain("quantum_gizmo");
  });
});

describe("search", () => {
  const model = buildModel(parityDoc);
  it("matches label and qualified name", () => {
    expect(searchEntities(model, "Thing")).toContain(
      "item:struct:demo::app::Thing",
    );
    expect(searchEntities(model, "demo::app::Color")).toContain(
      "item:enum:demo::app::Color",
    );
  });
  it("empty query yields no matches", () => {
    expect(searchEntities(model, "  ")).toEqual([]);
  });
});

describe("url state", () => {
  it("round-trips durable state", () => {
    const s = {
      view: "view:types",
      entity: "item:struct:demo::app::Thing",
      q: "Thing",
      kinds: ["struct", "enum"],
      edges: "related" as const,
      stage: "s1",
    };
    expect(parseUrlState(serializeUrlState(s))).toEqual(s);
  });
  it("relation wins over entity (mutual exclusivity)", () => {
    const parsed = parseUrlState("?entity=a&relation=r");
    expect(parsed.relation).toBe("r");
    expect(parsed.entity).toBeUndefined();
  });
  it("drops default edge mode and empty kinds", () => {
    expect(serializeUrlState({ view: "v", edges: "all", kinds: [] })).toBe(
      "?view=v",
    );
  });
});

describe("runtime guards", () => {
  it("parses valid diagnostics/generation/health", () => {
    expect(
      parseDiagnosticsReport({ schema_version: "1.0", diagnostics: [] })
        .diagnostics,
    ).toEqual([]);
    expect(
      parseGenerationReport({ generated_at: "t", partial: true }).partial,
    ).toBe(true);
    expect(
      parseHealth({ status: "ok", schema_version: "1.0", partial: false })
        .status,
    ).toBe("ok");
  });
  it("rejects malformed required fields", () => {
    expect(() => parseHealth({ status: "ok" })).toThrow();
    expect(() => parseGenerationReport({})).toThrow();
    expect(() => parseDiagnosticsReport({ diagnostics: [] })).toThrow();
  });
  it("tolerates unknown extra fields", () => {
    const d = parseDiagnosticsReport({
      schema_version: "1.0",
      diagnostics: [],
      future: 42,
    });
    expect(d.schema_version).toBe("1.0");
  });
  it("extracts api error code", () => {
    expect(apiErrorCode({ error: { code: "source_disabled", message: "x" } })).toBe(
      "source_disabled",
    );
    expect(apiErrorCode({})).toBeUndefined();
  });
});
