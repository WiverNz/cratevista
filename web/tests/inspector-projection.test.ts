// Issue 15 Phase 5: pure inspector projection — discriminated models, direction,
// deterministic ordering, indexed lookup, role via the single parse.
import { describe, it, expect } from "vitest";
import { readFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { buildModel } from "../src/model/model.ts";
import { entityInspection, relationInspection } from "../src/model/inspectorProjection.ts";
import { DEFAULT_LAYOUT_OPTIONS } from "../src/layout/types.ts";
import type { DiagnosticsReport } from "../src/types/runtime.ts";

const HERE = dirname(fileURLToPath(import.meta.url));
const readSrc = (rel: string) => readFileSync(resolve(HERE, "../src", rel), "utf8");

function e(id: string, kind: string, label: string, extra: Record<string, unknown> = {}) {
  return { id, kind, label: { default: label }, qualified_name: label, provenance: "discovered", ...extra };
}
function r(id: string, kind: string, from: string, to: string, extra: Record<string, unknown> = {}) {
  return { id, kind, from, to, provenance: "discovered", ...extra };
}

const doc = {
  schema_version: "1.1",
  project: { id: "x", name: "X", description: "" },
  entities: [
    e("A", "module", "alpha", { attributes: { category: "service" }, parent: undefined }),
    e("B", "struct", "bravo", { parent: "A" }),
    e("C", "struct", "charlie", { parent: "A" }),
    e("D", "package", "delta"),
    e("Z", "struct", "zulu"),
  ],
  relations: [
    r("r1", "contains", "A", "B"),
    r("r2", "contains", "A", "C"),
    r("r3", "depends_on", "A", "D"),
    r("r4", "widgetises", "A", "Z"), // unknown kind
    r("r5", "uses", "D", "A"), // incoming to A
    r("r6", "contains", "D", "Z"), // does NOT touch A
  ],
  views: [{ id: "view:v", title: { default: "V" }, entity_kinds: [], relation_kinds: [], entity_ids: ["A", "B", "C", "D", "Z"] }],
};
const diagnostics: DiagnosticsReport = {
  schema_version: "1.1",
  diagnostics: [{ severity: "warning", code: "w", message: "m", entities: ["A"], occurrence_count: 3 }],
} as unknown as DiagnosticsReport;

const model = buildModel(doc as Parameters<typeof buildModel>[0], diagnostics);

describe("entityInspection", () => {
  const insp = entityInspection(model, model.entityById.get("A")!, "en");

  it("is discriminated as an entity model", () => {
    expect(insp.kind).toBe("entity");
  });

  it("resolves the role from the single-parse categoryById", () => {
    expect(insp.roleLabel).toBe("Service");
    expect(insp.roleKnown).toBe(true);
  });

  it("keeps outgoing and incoming as SEPARATE, direction-preserving groups", () => {
    // Outgoing: contains(B,C), depends_on(D), widgetises(Z) — grouped by kind.
    const outKinds = insp.outgoing.map((g) => g.kind).sort();
    expect(outKinds).toEqual(["contains", "depends_on", "widgetises"]);
    expect(insp.outgoingTotal).toBe(4);
    // Incoming: uses(from D). r6 (D→Z) does NOT touch A and must be absent.
    expect(insp.incoming.map((g) => g.kind)).toEqual(["uses"]);
    expect(insp.incomingTotal).toBe(1);
    // The other endpoint of the incoming `uses` is D (source), not A.
    expect(insp.incoming[0].rows[0].otherId).toBe("D");
  });

  it("orders groups and rows deterministically", () => {
    const contains = insp.outgoing.find((g) => g.kind === "contains")!;
    expect(contains.rows.map((r) => r.otherLabel)).toEqual(["bravo", "charlie"]); // by label
  });

  it("preserves an unknown relation kind (generic fallback, not discarded)", () => {
    const unknown = insp.outgoing.find((g) => g.kind === "widgetises")!;
    expect(unknown.known).toBe(false);
    expect(unknown.label).toBe("widgetises");
  });

  it("sorts children and carries owned diagnostics", () => {
    expect(insp.children.map((c) => c.label)).toEqual(["bravo", "charlie"]);
    expect(insp.diagnostics).toHaveLength(1);
    expect(insp.diagnostics[0].code).toBe("w");
  });

  it("has no role for an entity without an authored category", () => {
    const d = entityInspection(model, model.entityById.get("D")!, "en");
    expect(d.roleLabel).toBeUndefined();
  });
});

describe("relationInspection", () => {
  const insp = relationInspection(model, model.relationById.get("r3")!, "en");
  it("is discriminated and carries directed endpoints", () => {
    expect(insp.kind).toBe("relation");
    expect(insp.fromId).toBe("A");
    expect(insp.fromLabel).toBe("alpha");
    expect(insp.toId).toBe("D");
    expect(insp.toLabel).toBe("delta");
    expect(insp.known).toBe(true);
  });
  it("labels an unknown relation kind generically", () => {
    const u = relationInspection(model, model.relationById.get("r4")!, "en");
    expect(u.known).toBe(false);
  });
});

describe("architecture / determinism guards", () => {
  it("derives hierarchy from the parent index, never from qualified-name prefixes", () => {
    const src = readSrc("model/inspectorProjection.ts");
    expect(src).toMatch(/childrenByParent/);
    expect(src).not.toMatch(/qualified_name/);
  });

  it("ELK spacing is a fixed deterministic scalar, independent of the viewport", () => {
    expect(DEFAULT_LAYOUT_OPTIONS.spacing).toBe(60);
    for (const file of ["layout/elk.worker.ts", "layout/client.ts", "layout/types.ts"]) {
      const s = readSrc(file);
      expect(s).not.toMatch(/innerWidth|innerHeight|matchMedia|clientWidth|clientHeight/);
    }
  });

  it("neither the inspector projection nor the inspector component parses the raw role attribute", () => {
    // Role reaches the inspector via the single-parse `model.categoryById` →
    // `roleStyleFor`; the raw parser is never called here.
    expect(readSrc("model/inspectorProjection.ts")).not.toMatch(/authoredRole/);
    expect(readSrc("components/Panels.tsx")).not.toMatch(/authoredRole/);
  });
});
