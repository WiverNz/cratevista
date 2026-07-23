import { describe, it, expect } from "vitest";

import { buildModel } from "../src/model/model.ts";
import {
  buildNodeCards,
  getNodeCards,
  cardLevel,
  cardSize,
  nodeVisualState,
  nodeCategory,
} from "../src/model/nodeCards.ts";
import type { ExplorerDocument, Entity, DocumentDiagnostic } from "../src/types/index.ts";

type EntitySpec = Partial<Entity> & { id: string; kind: string };

function entity(spec: EntitySpec): Entity {
  return {
    provenance: "discovered",
    label: { default: spec.id },
    qualified_name: spec.id,
    ...spec,
  } as Entity;
}

function model(entities: EntitySpec[], relations: unknown[] = [], diagnostics: DocumentDiagnostic[] = []) {
  const doc = {
    schema_version: "1.0",
    project: { id: "demo", name: "Demo", description: "" },
    entities: entities.map(entity),
    relations,
    views: [],
  } as unknown as ExplorerDocument;
  return buildModel(doc, { schema_version: "1.0", diagnostics } as never);
}

/** A FlightTrace-shaped slice: workspace → package(api) → target(lib) → module → struct,
 *  plus a second package with a depends_on edge from api. */
function flighttraceLike() {
  const entities: EntitySpec[] = [
    { id: "workspace", kind: "workspace", label: { default: "FlightTrace" }, qualified_name: "FlightTrace" },
    {
      id: "package:flighttrace-api",
      kind: "package",
      parent: "workspace",
      label: { default: "flighttrace-api" },
      qualified_name: "flighttrace-api",
      attributes: { version: "0.1.6", doc_coverage: { documented: 23, percent: 66, total: 35 } },
    },
    {
      id: "package:flighttrace-core",
      kind: "package",
      parent: "workspace",
      label: { default: "flighttrace-core" },
      qualified_name: "flighttrace-core",
      attributes: { version: "0.2.0", doc_coverage: { documented: 10, percent: 50, total: 20 } },
    },
    {
      id: "target:api:lib",
      kind: "target",
      parent: "package:flighttrace-api",
      label: { default: "flighttrace_api (lib)" },
      qualified_name: "flighttrace-api::flighttrace_api",
      attributes: { crate_types: ["lib"] },
    },
    {
      id: "target:app:bin",
      kind: "target",
      parent: "package:flighttrace-api",
      label: { default: "flighttrace (bin)" },
      qualified_name: "flighttrace-api::flighttrace",
      attributes: { crate_types: ["bin"] },
    },
    {
      id: "module:api::dto",
      kind: "module",
      parent: "target:api:lib",
      label: { default: "dto" },
      qualified_name: "flighttrace_api::dto",
      attributes: { visibility: "public", doc_coverage: { documented: 3, percent: 20, total: 15 } },
    },
    {
      id: "item:struct:api::dto::Thing",
      kind: "struct",
      parent: "module:api::dto",
      label: { default: "AssertionResultDto" },
      qualified_name: "flighttrace_api::dto::AssertionResultDto",
      attributes: { visibility: "public" },
      source: { path: "crates/flighttrace-api/src/dto.rs" },
      docs: { documented: true, markdown: "x" },
    },
  ];
  const relations = [
    {
      id: "rel:dep",
      kind: "depends_on",
      from: "package:flighttrace-api",
      to: "package:flighttrace-core",
      provenance: "discovered",
    },
  ];
  return model(entities, relations);
}

describe("cardLevel (progressive disclosure)", () => {
  it("compact when zoomed out, normal at mid, detailed when zoomed in", () => {
    expect(cardLevel({ zoom: 0.3, selected: false })).toBe("compact");
    expect(cardLevel({ zoom: 0.7, selected: false })).toBe("normal");
    expect(cardLevel({ zoom: 1.3, selected: false })).toBe("detailed");
  });
  it("selection forces detailed regardless of zoom", () => {
    expect(cardLevel({ zoom: 0.2, selected: true })).toBe("detailed");
  });
});

describe("nodeVisualState priority", () => {
  it("selected dominates related, search and diagnostics", () => {
    expect(
      nodeVisualState({ selected: true, searchMatch: true, related: true, diagnosticSeverity: "error" }),
    ).toBe("selected");
  });
  it("orders search over diagnostics, error over warning over related", () => {
    expect(nodeVisualState({ selected: false, searchMatch: true, related: true, diagnosticSeverity: "error" })).toBe("search");
    expect(nodeVisualState({ selected: false, searchMatch: false, related: true, diagnosticSeverity: "error" })).toBe("diagnostic-error");
    expect(nodeVisualState({ selected: false, searchMatch: false, related: true, diagnosticSeverity: "warning" })).toBe("diagnostic-warning");
    expect(nodeVisualState({ selected: false, searchMatch: false, related: true })).toBe("related");
    expect(nodeVisualState({ selected: false, searchMatch: false, related: false })).toBe("normal");
  });
});

describe("cardSize (bounded, deterministic)", () => {
  it("is deterministic and bounded", () => {
    expect(cardSize("package")).toEqual(cardSize("package"));
    for (const kind of ["workspace", "package", "target", "module", "struct", "trait", "function", "impl", "weird"]) {
      const s = cardSize(kind);
      expect(s.width).toBeGreaterThan(0);
      expect(s.width).toBeLessThanOrEqual(260);
      expect(s.height).toBeGreaterThan(0);
      expect(s.height).toBeLessThanOrEqual(136);
    }
  });
  it("code-entity cards are smaller than package overview cards", () => {
    expect(cardSize("struct").height).toBeLessThan(cardSize("package").height);
  });
});

describe("nodeCategory", () => {
  it("maps kinds to categories and falls back to unknown", () => {
    expect(nodeCategory("package", "discovered")).toBe("package");
    expect(nodeCategory("struct", "discovered")).toBe("type");
    expect(nodeCategory("method", "discovered")).toBe("function");
    expect(nodeCategory("teleporter", "discovered")).toBe("unknown");
    expect(nodeCategory("package", "manual")).toBe("manual");
  });
});

describe("buildNodeCards — workspace / package / target", () => {
  const cards = buildNodeCards(flighttraceLike());

  it("workspace card carries the name, kind and package count", () => {
    const ws = cards.get("workspace")!;
    expect(ws.kindLabel).toBe("Workspace");
    expect(ws.title).toBe("FlightTrace");
    expect(ws.metrics.find((m) => m.key === "packages")?.value).toBe("2");
  });

  it("package card carries version (normal), deps and detailed metrics", () => {
    const pkg = cards.get("package:flighttrace-api")!;
    expect(pkg.kindLabel).toBe("Package");
    const version = pkg.metrics.find((m) => m.key === "version");
    expect(version).toMatchObject({ value: "0.1.6", minLevel: "normal" });
    expect(pkg.metrics.find((m) => m.key === "deps")?.value).toBe("1");
    expect(pkg.metrics.find((m) => m.key === "targets")).toMatchObject({ value: "2", minLevel: "detailed" });
    expect(pkg.metrics.find((m) => m.key === "docs")).toMatchObject({ value: "66%", minLevel: "detailed" });
  });

  it("target card shows the exact Cargo target kind and public-item count", () => {
    const lib = cards.get("target:api:lib")!;
    expect(lib.category).toBe("target");
    expect(lib.kindLabel).toBe("lib");
    // One public struct lives under the lib target's module subtree.
    expect(lib.metrics.find((m) => m.key === "public")).toMatchObject({ value: "1", minLevel: "detailed" });

    const bin = cards.get("target:app:bin")!;
    expect(bin.kindLabel).toBe("bin");
    // The bin target has no public items → the metric is omitted (no "0").
    expect(bin.metrics.find((m) => m.key === "public")).toBeUndefined();
  });

  it("package and target cards are visually distinct", () => {
    expect(cards.get("package:flighttrace-api")!.category).toBe("package");
    expect(cards.get("target:api:lib")!.category).toBe("target");
    expect(cards.get("package:flighttrace-api")!.kindLabel).not.toBe(cards.get("target:api:lib")!.kindLabel);
  });
});

describe("buildNodeCards — code entities", () => {
  const cards = buildNodeCards(flighttraceLike());
  it("carries kind, visibility, doc state, source availability and short context", () => {
    const s = cards.get("item:struct:api::dto::Thing")!;
    expect(s.category).toBe("type");
    expect(s.kindLabel).toBe("Struct");
    expect(s.visibility).toBe("public");
    expect(s.documented).toBe(true);
    expect(s.hasSource).toBe(true);
    // Short owning context, not the full qualified name.
    expect(s.context).toBe("flighttrace_api::dto");
    expect(s.context).not.toBe(s.fullTitle);
  });
});

describe("buildNodeCards — omitted metrics", () => {
  it("omits zero-count metrics cleanly (no deps/targets shown as 0)", () => {
    const cards = buildNodeCards(
      model([
        { id: "p", kind: "package", label: { default: "solo" }, qualified_name: "solo", attributes: { version: "1.0.0" } },
      ]),
    );
    const pkg = cards.get("p")!;
    expect(pkg.metrics.find((m) => m.key === "version")).toBeTruthy();
    expect(pkg.metrics.find((m) => m.key === "deps")).toBeUndefined();
    expect(pkg.metrics.find((m) => m.key === "targets")).toBeUndefined();
  });
});

describe("buildNodeCards — diagnostic ownership + occurrence badge", () => {
  it("attaches a badge only to the entity a diagnostic explicitly references", () => {
    const m = model(
      [{ id: "p", kind: "package", qualified_name: "p", attributes: { version: "1" } }],
      [],
      [{ severity: "warning", code: "c", message: "m", occurrence_count: 5, entities: ["p"] } as DocumentDiagnostic],
    );
    const badge = buildNodeCards(m).get("p")!.diagnostic!;
    expect(badge.severity).toBe("warning");
    // Occurrences, not record count.
    expect(badge.occurrences).toBe(5);
    expect(badge.records).toBe(1);
    expect(badge.label).toMatch(/5 occurrences/);
  });

  it("does NOT attach global (unassociated) diagnostics to any node", () => {
    const m = model(
      [{ id: "p", kind: "package", qualified_name: "p", attributes: { version: "1" } }],
      [],
      // A global aggregated external-reference summary with no entity association.
      [{ severity: "info", code: "external_crate_reference", message: "1924 refs", occurrence_count: 1924 } as DocumentDiagnostic],
    );
    expect(buildNodeCards(m).get("p")!.diagnostic).toBeUndefined();
  });

  it("prioritises error over warning over info and sums that severity's occurrences", () => {
    const m = model(
      [{ id: "p", kind: "package", qualified_name: "p" }],
      [],
      [
        { severity: "info", code: "i", message: "i", occurrence_count: 3, entities: ["p"] } as DocumentDiagnostic,
        { severity: "error", code: "e", message: "e", occurrence_count: 2, entities: ["p"] } as DocumentDiagnostic,
        { severity: "warning", code: "w", message: "w", occurrence_count: 9, entities: ["p"] } as DocumentDiagnostic,
      ],
    );
    const badge = buildNodeCards(m).get("p")!.diagnostic!;
    expect(badge.severity).toBe("error");
    expect(badge.occurrences).toBe(2);
  });
});

describe("getNodeCards memoization", () => {
  it("returns the same map for the same model (metrics not rebuilt on selection)", () => {
    const m = flighttraceLike();
    expect(getNodeCards(m)).toBe(getNodeCards(m));
  });
});

describe("500-node performance / projection", () => {
  it("builds 500 cards quickly and memoizes by model", () => {
    const entities: EntitySpec[] = [{ id: "workspace", kind: "workspace", qualified_name: "WS" }];
    const kinds = ["package", "target", "module", "struct", "enum", "trait", "function", "impl"];
    for (let i = 0; i < 500; i++) {
      entities.push({
        id: `e${i}`,
        kind: kinds[i % kinds.length],
        parent: i === 0 ? "workspace" : `e${i - 1}`,
        label: { default: `a_very_long_entity_name_that_must_truncate_${i}` },
        qualified_name: `crate::mod${i % 7}::Name${i}`,
        attributes: { visibility: "public", version: "1.2.3" },
      });
    }
    const m = model(entities);
    const t0 = performance.now();
    const cards = buildNodeCards(m);
    const elapsed = performance.now() - t0;
    expect(cards.size).toBe(501);
    expect(elapsed).toBeLessThan(500); // generous bound; typically a few ms
    // Every card has a bounded, deterministic box.
    for (const c of cards.values()) {
      expect(c.width).toBeLessThanOrEqual(260);
      expect(c.height).toBeLessThanOrEqual(136);
    }
    // Memoized: no rebuild on repeated access.
    expect(getNodeCards(m)).toBe(getNodeCards(m));
  });
});
