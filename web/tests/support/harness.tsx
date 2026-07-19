import { render } from "@testing-library/react";
import { App } from "../../src/App.tsx";
import type { ArtifactSource, LoadOutcome } from "../../src/api/load.ts";
import type { LayoutEngine, LayoutInput } from "../../src/layout/client.ts";
import type { SourceClient } from "../../src/api/source.ts";
import type { ExplorerDocument } from "../../src/types/index.ts";
import type {
  DiagnosticsReport,
  GenerationReport,
} from "../../src/types/runtime.ts";

const WS = "workspace";
const PKG = "package:demo";
const MOD = "module:demo::app";
export const STRUCT = "item:struct:demo::app::Thing";
export const ENUM = "item:enum:demo::app::Color";

export function sampleDocument(): ExplorerDocument {
  return {
    schema_version: "1.0",
    project: { id: "demo", name: "Demo", description: "" },
    entities: [
      { id: WS, kind: "workspace", label: { default: "Demo WS" }, qualified_name: "demo", provenance: "discovered" },
      { id: PKG, kind: "package", label: { default: "demo" }, qualified_name: "demo", provenance: "discovered" },
      { id: MOD, kind: "module", label: { default: "app" }, qualified_name: "demo::app", provenance: "discovered", parent: PKG },
      {
        id: STRUCT,
        kind: "struct",
        label: { default: "Thing" },
        qualified_name: "demo::app::Thing",
        provenance: "discovered",
        parent: MOD,
        tags: ["public"],
        attributes: { visibility: "pub" },
        source: { path: "src/app.rs", span: { start_line: 10, start_col: 1, end_line: 20, end_col: 2 } },
        docs: { markdown: "A **thing**. [ex](https://example.com)", summary: "A thing.", documented: true },
      },
      { id: ENUM, kind: "enum", label: { default: "Color" }, qualified_name: "demo::app::Color", provenance: "discovered", parent: MOD },
    ],
    relations: [
      { id: "rel:contains:ws-pkg", kind: "contains", from: WS, to: PKG, provenance: "discovered" },
      { id: "rel:contains:pkg-mod", kind: "contains", from: PKG, to: MOD, provenance: "discovered" },
      { id: "rel:contains:mod-struct", kind: "contains", from: MOD, to: STRUCT, provenance: "discovered" },
      { id: "rel:has_field_type:struct-enum", kind: "has_field_type", from: STRUCT, to: ENUM, provenance: "discovered" },
    ],
    views: [
      { id: "view:workspace-overview", title: { default: "Workspace overview" }, entity_kinds: [], relation_kinds: [], stages: [], presentation: {} },
      { id: "view:types", title: { default: "Types" }, entity_kinds: ["struct", "enum"], relation_kinds: [], entity_ids: [STRUCT, ENUM], stages: [], presentation: {} },
      { id: "view:empty", title: { default: "Empty" }, entity_kinds: ["nonexistent"], relation_kinds: [], stages: [], presentation: {} },
      {
        id: "view:staged",
        title: { default: "Staged" },
        entity_kinds: [],
        relation_kinds: [],
        stages: [
          { id: "stage:a", order: 1, title: { default: "Stage A" } },
          { id: "stage:b", order: 2, title: { default: "Stage B" } },
        ],
        presentation: {},
      },
      { id: "view:focus", title: { default: "Focus" }, entity_kinds: [], relation_kinds: [], stages: [], default_focus: STRUCT, presentation: {} },
    ],
  } as unknown as ExplorerDocument;
}

export function unknownKindDocument(): ExplorerDocument {
  return {
    schema_version: "1.0",
    project: { id: "demo", name: "Demo", description: "" },
    entities: [
      { id: PKG, kind: "package", label: { default: "demo" }, qualified_name: "demo", provenance: "discovered" },
      { id: "item:widget", kind: "widget", label: { default: "Widget" }, qualified_name: "demo::Widget", provenance: "discovered" },
    ],
    relations: [
      { id: "rel:teleports:pkg-widget", kind: "teleports_to", from: PKG, to: "item:widget", provenance: "manual" },
    ],
    views: [
      { id: "view:workspace-overview", title: { default: "Workspace overview" }, entity_kinds: [], relation_kinds: [], stages: [], presentation: {} },
    ],
  } as unknown as ExplorerDocument;
}

export function okOutcome(overrides: Partial<Extract<LoadOutcome, { status: "ok" }>> = {}): LoadOutcome {
  const generation: GenerationReport = { generated_at: "t", partial: false };
  const diagnostics: DiagnosticsReport = {
    schema_version: "1.0",
    diagnostics: [
      { severity: "warning", code: "unresolved_type", message: "could not resolve X", occurrence_count: 1, entities: [STRUCT] },
    ],
  };
  return {
    status: "ok",
    document: sampleDocument(),
    generation,
    generationAvailable: true,
    diagnostics,
    diagnosticsAvailable: true,
    partial: false,
    ...overrides,
  };
}

export function fakeSource(outcome: LoadOutcome | { stale: true }): ArtifactSource & { calls: number } {
  return {
    calls: 0,
    async load() {
      this.calls += 1;
      return outcome;
    },
    abort() {},
  };
}

export function fakeLayout(mode: "ok" | "error" | "stale" = "ok"): {
  engine: LayoutEngine;
  calls: LayoutInput[];
} {
  const calls: LayoutInput[] = [];
  const engine: LayoutEngine = {
    layout(input) {
      calls.push(input);
      if (mode === "error") return Promise.resolve({ status: "error", error: "boom" });
      if (mode === "stale") return Promise.resolve({ status: "stale" });
      return Promise.resolve({
        status: "ok",
        result: {
          token: 0,
          nodes: input.request.nodes.map((n) => ({ id: n.id, x: 0, y: 0, width: n.width, height: n.height })),
          edges: [],
          width: 0,
          height: 0,
        },
      });
    },
    terminate() {},
  };
  return { engine, calls };
}

/** A `liveReload` transport that reports watch disabled and never touches the
 *  network. Component tests are not exercising live reload; without this the app's
 *  live-reload effect would fire a real `fetch("/api/health")` per mount, which is
 *  both a real network attempt in jsdom and needless async churn. The dedicated
 *  live-reload tests (`live-reload.test.ts`, `watch-ui.test.tsx`) inject their own
 *  transport instead. */
export const watchDisabledLiveReload = {
  fetchFn: async () =>
    ({ ok: true, status: 200, json: async () => ({ watch_enabled: false }) }) as Response,
  createEventSource: () => {
    throw new Error("component tests must not open an EventSource");
  },
};

export function renderApp(opts: {
  outcome?: LoadOutcome;
  search?: string;
  layoutMode?: "ok" | "error" | "stale";
  budget?: number;
  sourceClient?: SourceClient;
}) {
  const source = fakeSource(opts.outcome ?? okOutcome());
  const layout = fakeLayout(opts.layoutMode ?? "ok");
  const result = render(
    <App
      source={source}
      layout={layout.engine}
      initialSearch={opts.search ?? ""}
      budget={opts.budget}
      sourceClient={opts.sourceClient}
      liveReload={watchDisabledLiveReload}
    />,
  );
  return { ...result, source, layout };
}
