import { describe, it, expect } from "vitest";
import { buildModel } from "../src/model/model.ts";
import { normalizeUrlState, chooseView, differsOnlyBySearch } from "../src/state/normalize.ts";
import { parseUrlState, serializeUrlState } from "../src/state/url.ts";
import { sampleDocument, STRUCT, ENUM } from "./support/harness.tsx";

const model = buildModel(sampleDocument());
const REL = "rel:contains:mod-struct";

function norm(search: string) {
  return normalizeUrlState(parseUrlState(search), model);
}

describe("chooseView", () => {
  it("prefers a valid requested view", () => {
    expect(chooseView("view:types", model)).toBe("view:types");
  });
  it("falls back to workspace-overview for a stale view", () => {
    expect(chooseView("view:does-not-exist", model)).toBe("view:workspace-overview");
    expect(chooseView(undefined, model)).toBe("view:workspace-overview");
  });
  it("falls back to the first view when workspace-overview is absent", () => {
    const doc = sampleDocument();
    doc.views = doc.views.filter((v) => v.id !== "view:workspace-overview");
    expect(chooseView("nope", buildModel(doc))).toBe(doc.views[0].id);
  });
});

describe("normalizeUrlState", () => {
  it("keeps a valid requested view", () => {
    expect(norm("?view=view:types").view).toBe("view:types");
  });

  it("replaces a stale view with workspace-overview", () => {
    expect(norm("?view=view:ghost").view).toBe("view:workspace-overview");
  });

  it("relation wins when both relation and entity are valid", () => {
    const s = norm(`?relation=${REL}&entity=${STRUCT}`);
    expect(s.relation).toBe(REL);
    expect(s.entity).toBeUndefined();
  });

  it("falls back to a valid entity when the relation is invalid", () => {
    const s = normalizeUrlState({ relation: "rel:ghost", entity: STRUCT }, model);
    expect(s.relation).toBeUndefined();
    expect(s.entity).toBe(STRUCT);
  });

  it("removes stale entity and relation ids", () => {
    const s = normalizeUrlState({ entity: "item:ghost", relation: "rel:ghost" }, model);
    expect(s.entity).toBeUndefined();
    expect(s.relation).toBeUndefined();
  });

  it("removes a stage not present in the selected view", () => {
    // stage:a exists only on view:staged.
    expect(normalizeUrlState({ view: "view:types", stage: "stage:a" }, model).stage).toBeUndefined();
    expect(normalizeUrlState({ view: "view:staged", stage: "stage:a" }, model).stage).toBe("stage:a");
  });

  it("removes unknown kinds and dedupes/orders the rest", () => {
    const s = normalizeUrlState({ kinds: ["enum", "struct", "struct", "ghostkind"] }, model);
    expect(s.kinds).toEqual(["enum", "struct"]);
  });

  it("drops a kinds parameter that becomes empty", () => {
    expect(normalizeUrlState({ kinds: ["ghostkind"] }, model).kinds).toBeUndefined();
  });

  it("removes a stale focus id", () => {
    expect(normalizeUrlState({ focus: "item:ghost" }, model).focus).toBeUndefined();
    expect(normalizeUrlState({ focus: ENUM }, model).focus).toBe(ENUM);
  });

  it("normalizes edges: invalid dropped, default `all` not serialized", () => {
    expect(normalizeUrlState({ edges: "sideways" as never }, model).edges).toBeUndefined();
    expect(normalizeUrlState({ edges: "all" }, model).edges).toBeUndefined();
    expect(normalizeUrlState({ edges: "related" }, model).edges).toBe("related");
  });

  it("drops a whitespace-only query", () => {
    expect(normalizeUrlState({ q: "   " }, model).q).toBeUndefined();
    expect(normalizeUrlState({ q: "Thing" }, model).q).toBe("Thing");
  });

  it("is idempotent and round-trips through the query string", () => {
    const once = norm(`?view=view:types&entity=${STRUCT}&kinds=struct,struct,ghost&edges=related&q=Th`);
    const twice = normalizeUrlState(parseUrlState(serializeUrlState(once)), model);
    expect(twice).toEqual(once);
  });

  it("never carries transient state (only durable keys are produced)", () => {
    const s = norm(`?view=view:types&entity=${STRUCT}&hover=x&zoom=2&expanded=y`);
    expect(Object.keys(s).sort()).toEqual(["entity", "view"]);
  });
});

describe("differsOnlyBySearch", () => {
  it("true when only q changed", () => {
    expect(differsOnlyBySearch({ view: "v", q: "a" }, { view: "v", q: "b" })).toBe(true);
  });
  it("false when q is unchanged", () => {
    expect(differsOnlyBySearch({ view: "v", q: "a" }, { view: "v", q: "a" })).toBe(false);
  });
  it("false when another durable field also changed", () => {
    expect(differsOnlyBySearch({ view: "v1", q: "a" }, { view: "v2", q: "b" })).toBe(false);
  });
});
