// Issue 15 Phase 4: strict authored-role parser + centralized role registry.
import { describe, it, expect } from "vitest";
import {
  authoredRole,
  roleStyleFor,
  KNOWN_ROLES,
  type RoleCue,
} from "../src/adapter/roleStyle.ts";
import { buildModel } from "../src/model/model.ts";
import { documentToGraph } from "../src/adapter/adapter.ts";

describe("authoredRole — strict, total parser", () => {
  const parse = (v: unknown) => authoredRole({ category: v } as Record<string, unknown>);

  it("accepts an exact lowercase known string", () => {
    expect(parse("service")).toBe("service");
  });
  it("trims surrounding whitespace", () => {
    expect(parse("  service  ")).toBe("service");
  });
  it("treats an empty / whitespace-only string as missing", () => {
    expect(parse("")).toBeUndefined();
    expect(parse("   ")).toBeUndefined();
  });
  it("preserves a wrong-case value verbatim (does NOT normalize)", () => {
    expect(parse("Service")).toBe("Service");
  });
  it("preserves an unknown non-empty value", () => {
    expect(parse("scheduler")).toBe("scheduler");
  });
  it("ignores non-string types (boolean, number, array, object, null)", () => {
    for (const v of [true, false, 0, 1, [], ["service"], {}, { x: 1 }, null]) {
      expect(parse(v)).toBeUndefined();
    }
  });
  it("is missing when there is no category / no attributes", () => {
    expect(authoredRole({})).toBeUndefined();
    expect(authoredRole(null)).toBeUndefined();
    expect(authoredRole(undefined)).toBeUndefined();
  });
  it("reads ONLY attributes.category (no inference from other fields)", () => {
    expect(authoredRole({ kind: "package", label: "database", name: "cache" } as Record<string, unknown>)).toBeUndefined();
  });
});

describe("role registry — locked vocabulary", () => {
  it("has exactly the nine locked roles, includes database, excludes data-store", () => {
    expect(KNOWN_ROLES).toHaveLength(9);
    expect([...KNOWN_ROLES]).toEqual([
      "service",
      "client",
      "database",
      "cache",
      "observability",
      "external",
      "infra",
      "shared",
      "domain",
    ]);
    expect(KNOWN_ROLES).toContain("database");
    expect(KNOWN_ROLES as readonly string[]).not.toContain("data-store");
  });

  it("gives every known role a unique token, a non-empty label and a deterministic cue", () => {
    const tokens = new Set<string>();
    const cues = new Set<RoleCue>();
    for (const r of KNOWN_ROLES) {
      const s = roleStyleFor(r)!;
      expect(s.known).toBe(true);
      expect(s.key).toBe(r);
      expect(s.token).toBe(`--role-${r}`);
      expect(s.label.length).toBeGreaterThan(0);
      expect(roleStyleFor(r)).toEqual(s); // deterministic
      tokens.add(s.token);
      cues.add(s.cue);
    }
    expect(tokens.size).toBe(9); // unique per role
    expect(cues.size).toBe(9); // a distinct cue per role
  });

  it("labels known roles with concise user-facing names", () => {
    expect(roleStyleFor("infra")!.label).toBe("Infrastructure");
    expect(roleStyleFor("database")!.label).toBe("Database");
    expect(roleStyleFor("observability")!.label).toBe("Observability");
  });

  it("maps a non-empty unknown value to the neutral style, keeping the authored value", () => {
    const s = roleStyleFor("scheduler")!;
    expect(s.known).toBe(false);
    expect(s.key).toBe("unknown");
    expect(s.token).toBe("--role-unknown"); // fixed token, never generated from the string
    expect(s.label).toBe("scheduler");
    expect(s.cue).toBe("neutral");
  });

  it("classifies a wrong-case known value as unknown (not silently normalized)", () => {
    const s = roleStyleFor("Service")!;
    expect(s.known).toBe(false);
    expect(s.key).toBe("unknown");
    expect(s.label).toBe("Service");
  });

  it("returns undefined for a missing role (no visible role style)", () => {
    expect(roleStyleFor(undefined)).toBeUndefined();
  });

  it("uses the same fixed neutral token for every unknown value (no dynamic colour)", () => {
    expect(roleStyleFor("scheduler")!.token).toBe(roleStyleFor("anything-else")!.token);
  });
});

describe("adapter surfaces GraphNode.category via the shared parser", () => {
  const doc = {
    schema_version: "1.1",
    project: { id: "x", name: "X", description: "" },
    entities: [
      { id: "a", kind: "package", label: { default: "A" }, qualified_name: "a", provenance: "discovered", attributes: { category: "  service  " } },
      { id: "b", kind: "package", label: { default: "B" }, qualified_name: "b", provenance: "discovered" },
      { id: "c", kind: "package", label: { default: "C" }, qualified_name: "c", provenance: "discovered", attributes: { category: 7 } },
    ],
    relations: [],
    views: [{ id: "view:v", title: { default: "V" }, entity_kinds: [], relation_kinds: [], entity_ids: ["a", "b", "c"] }],
  };

  it("parses the authored value (trimmed) and omits missing/non-string ones", () => {
    const model = buildModel(doc as Parameters<typeof buildModel>[0]);
    const graph = documentToGraph(model, model.views[0], {});
    const byId = new Map(graph.nodes.map((n) => [n.id, n]));
    expect(byId.get("a")!.category).toBe("service"); // trimmed
    expect(byId.get("b")!.category).toBeUndefined(); // missing
    expect(byId.get("c")!.category).toBeUndefined(); // non-string ignored
  });
});
