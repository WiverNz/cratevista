import { describe, it, expect, beforeEach, vi } from "vitest";

vi.mock("@xyflow/react", () => import("./support/xyflow.tsx"));

import { readFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import Ajv2020 from "ajv/dist/2020";
import { screen, fireEvent, within } from "@testing-library/react";

import allViews from "../src/fixtures/all_views.document.json" with { type: "json" };
import schemaParity from "../src/fixtures/schema_parity.document.json" with { type: "json" };
import unknownKinds from "../src/fixtures/unknown_kinds.document.json" with { type: "json" };
import { buildModel } from "../src/model/model.ts";
import { documentToGraph } from "../src/adapter/adapter.ts";
import type { ExplorerDocument, View } from "../src/types/index.ts";
import { renderApp, okOutcome } from "./support/harness.tsx";

const here = dirname(fileURLToPath(import.meta.url));
const schema = JSON.parse(
  readFileSync(
    resolve(here, "../../crates/cratevista-schema/schema/cratevista-document.schema.json"),
    "utf8",
  ),
) as object;

const EIGHT_VIEWS = [
  "view:workspace-overview",
  "view:crate-dependencies",
  "view:module-hierarchy",
  "view:types",
  "view:traits-and-impls",
  "view:type-relationships",
  "view:public-api",
  "view:documentation-coverage",
];

const allViewsDoc = allViews as unknown as ExplorerDocument;

describe("fixture schema validation", () => {
  const ajv = new Ajv2020({ strict: false, allErrors: true });
  const validate = ajv.compile(schema);

  it("all_views.document.json validates against the ExplorerDocument schema", () => {
    const ok = validate(allViews);
    if (!ok) console.error(validate.errors);
    expect(ok).toBe(true);
  });
  it("schema_parity.document.json validates", () => {
    expect(validate(schemaParity)).toBe(true);
  });
  it("unknown_kinds.document.json validates", () => {
    expect(validate(unknownKinds)).toBe(true);
  });
});

describe("all_views fixture shape", () => {
  it("contains exactly the eight generated view ids", () => {
    const ids = allViewsDoc.views.map((v) => v.id).sort();
    expect(ids).toEqual([...EIGHT_VIEWS].sort());
  });

  it("is real generated content (structs/traits/impls + cross-crate deps present)", () => {
    const kinds = new Set(allViewsDoc.entities.map((e) => e.kind));
    for (const k of ["workspace", "package", "module", "struct", "enum", "trait", "impl", "method"]) {
      expect(kinds.has(k)).toBe(true);
    }
    // cross-crate dependency + typed relations from real rustdoc.
    expect(allViewsDoc.relations.some((r) => r.kind === "depends_on")).toBe(true);
    expect(allViewsDoc.relations.some((r) => r.kind === "implements")).toBe(true);
    expect(allViewsDoc.relations.some((r) => r.kind === "returns_type")).toBe(true);
  });

  it("does not contain unknown-kind test data", () => {
    expect(allViewsDoc.entities.every((e) => e.kind !== "widget")).toBe(true);
  });

  it("each view projects without crashing", () => {
    const model = buildModel(allViewsDoc);
    for (const view of allViewsDoc.views as View[]) {
      const v = model.viewById.get(view.id)!;
      expect(() => documentToGraph(model, v, {})).not.toThrow();
    }
  });

  it("the types/traits views are non-empty and empty views still project", () => {
    const model = buildModel(allViewsDoc);
    const types = model.viewById.get("view:types")!;
    expect(documentToGraph(model, types, {}).nodes.length).toBeGreaterThan(0);
    const traits = model.viewById.get("view:traits-and-impls")!;
    expect(documentToGraph(model, traits, {}).nodes.length).toBeGreaterThan(0);
  });
});

describe("all eight views render in the application", () => {
  beforeEach(() => {
    window.history.pushState(null, "", "/");
  });

  it("renders a tab per generated view and each is selectable", async () => {
    renderApp({ outcome: okOutcome({ document: allViewsDoc }) });
    const tablist = await screen.findByRole("tablist", { name: "Views" });
    const tabs = within(tablist).getAllByRole("tab");
    // None of the eight generated views define stages, so the only tablist is
    // "Views" with exactly eight tabs.
    expect(tabs.length).toBe(8);
    // Switch to each view without crashing.
    for (const tab of tabs) {
      fireEvent.click(tab);
      expect(tab).toHaveAttribute("aria-selected", "true");
    }
  });
});
