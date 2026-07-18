# PRD — Build the interactive architecture explorer UI

## Status

**Implemented / Verified.** All phases have landed; every acceptance criterion has implementation + test evidence; all frontend and Rust gates pass; the large-graph benchmark is recorded and the 1,500-node budget decided (`docs/benchmarks/prd-07-large-graph.md`). See "## Implementation status". Originally finalized against the implemented PRDs 02/05/06: the frontend stack and version policy, the **approved PRD-06 CSP amendment** (`style-src-attr 'unsafe-inline'` + `worker-src 'self'`, no other `unsafe-inline`/`unsafe-eval`/remote origins), Zustand UI-state store, `json-schema-to-typescript` codegen (document only) + hand-written diagnostics/generation types, concurrent three-artifact loading with degrade rules, query-string URL state, an ELK module Web Worker with tokened/cached deterministic layout, the 1,500-node reduced-mode policy, the three-fixture strategy, `react-markdown`+`rehype-sanitize` Markdown security, the `web/src`→committed-`web/dist` workflow with `check:dist`, and a WCAG 2.1 AA baseline. **No blocking design questions or deferred implementation measurements remain.** The one measurement that was deferred to implementation — the benchmarked large-graph default — is recorded, and retained the 1,500-node default (`docs/benchmarks/prd-07-large-graph.md`).

See "## Explorer UI decisions" for the concept mapping and visual acceptance criteria.

## Source issue

`ISSUES/issue_07_interactive_explorer_ui.md`

## Summary

Build `web/`: a **React 19 + TypeScript 7 + Vite 8 + `@xyflow/react` 12** SPA (Zustand 5 UI state) that renders the CrateVista explorer document as an interactive architecture map — data-driven view tabs, search, entity-kind filters, optional flow/stage steps, **ELK layout in a same-origin module Web Worker**, focus/related modes, legend, and a details inspector. A dedicated pure adapter converts schema entities/relations to React Flow nodes/edges; rustdoc JSON is never exposed to components. Markdown is rendered safely (`react-markdown` + `rehype-sanitize`). Built with `base: "./"` to a **committed** `web/dist` and embedded by the server (issue 06) under the amended CSP.

## Problem statement

The generated document must become a polished, accessible, browser-based architecture explorer — driven entirely by the CrateVista schema and automatically generated data.

## Goals

- Render every MVP view; search by label/qualified name; predictable kind filters.
- Node selection → inspector; relation selection → relation details.
- Pan/zoom/fit/reset; automatic ELK layout; focus-path and related-only modes; edge visibility controls.
- Legend reflecting only categories present in the active view.
- Deterministic-enough layout for repeatable tests; large-graph threshold + fallback.
- TypeScript strict; unit/component + E2E smoke tests; accessibility baseline.

## Non-goals

- Backend/serving (issue 06). Manual flow authoring UI (issue 08 provides data; UI just renders manual entities/flows). Editing source. Coordinates in the schema (layout computed client-side).

## Approved technology stack and version policy

**Runtime/tooling:** Node **24 LTS**; **npm** with a committed **`package-lock.json`** as the exact dependency authority (`npm ci` in CI).

**Libraries (approved):** React **19** + React DOM **19**; TypeScript **7**; Vite **8** + `@vitejs/plugin-react` **6**; `@xyflow/react` **12**; `elkjs` (current compatible stable release); Zustand **5**; Vitest **4** + Testing Library; Playwright **1**; `json-schema-to-typescript` **15**; `react-markdown` **10** + `remark-gfm` + `rehype-sanitize`; ESLint + `typescript-eslint`; **plain CSS or CSS Modules**.

**Explicitly disallowed:** Redux, styled-components, Emotion, or any other **runtime CSS-in-JS** system (they break the `style-src 'self'` policy and add weight).

Exact compatible **patch** versions are resolved into `package-lock.json` during implementation and recorded in the completion report. Major versions above are fixed by this approval; patch/minor pinning is `package-lock.json`'s job.

> **Implementation note (2026-07-14): TypeScript 7 compiler with a TypeScript 6 compatibility API.** The project uses stable TypeScript 7 for the authoritative project type-check. TypeScript 7.0 does not yet expose the stable programmatic compiler API required by typescript-eslint, so — following the official side-by-side transition model — the npm package name `typescript` is aliased to `@typescript/typescript6` (`"typescript": "npm:@typescript/typescript6@^6.0.0"`), while a second alias installs stable TypeScript 7 (`"typescript-7": "npm:typescript@7.0.2"`). TypeScript 7's compiler runs the authoritative `npm run typecheck`; `tsc6` (and any tool importing `typescript`, i.e. typescript-eslint) uses the TypeScript 6 compatibility API. **No peer-dependency bypass flags** (`--legacy-peer-deps`/`--force`) are used. Both the TypeScript 7 check and the TypeScript 6 compatibility check (`npm run typecheck:compat`) pass on the same `tsconfig` and source. The compatibility alias can be removed once TypeScript 7 exposes its new programmatic API and typescript-eslint supports it. *(Binary note: the compat package bundles the TS6 compiler as `@typescript/old`, whose `tsc` bin wins the bare-`tsc` name collision, so the authoritative TS7 gate is invoked explicitly via `node ./node_modules/typescript-7/bin/tsc`; `tsc6` is unambiguous. No bypass flags — see "## Implementation status".)*

## Current repository state

`web/` is **fully implemented and verified** (see "## Implementation status" for the full ledger). Implemented and verified:

- `web/package.json` + committed `web/package-lock.json`; the **TypeScript 7 authoritative compiler + TypeScript 6 compatibility API** toolchain (alias layout above); Vite 8 / React 19 / Vitest 4 / flat ESLint scaffold with `base: "./"`;
- generated `ExplorerDocument` types (`web/src/types/generated/`) + `generate:types`/`check:types` drift scripts; hand-written runtime artifact types + guards (`web/src/types/runtime.ts`);
- immutable document model + indexes (`web/src/model/`); the pure `documentToGraph` adapter + kind styles with the generic **unknown-kind fallback** (`web/src/adapter/`); URL parse/serialize + Zustand UI store (`web/src/state/`); the concurrent artifact loader with token/stale handling (`web/src/api/load.ts`); `SafeMarkdown` (`web/src/markdown/`);
- the **React application shell + GraphCanvas/`LayoutClient` wiring**, the **ELK worker layer** (`src/layout/`), the complete **graph controls** (fit/zoom/reset/edge-modes/related/double-click focus), and **large-graph reduced mode** (`adapter/reduce.ts` + banner/list) — implemented, component-tested, and now **verified executing in real Chromium**;
- the checked-in **sample workspace** (`web/fixtures/sample-workspace/`) and the **real generated** `all_views.document.json` (**101 entities, 166 relations, 8 views**), alongside `schema_parity.document.json` + `unknown_kinds.document.json`;
- the **source-content action**, the **URL-normalization + history** boundary, and the **accessibility baseline** (axe + keyboard/focus); **176 frontend unit/component tests pass** across 15 files (TS7 + TS6 typechecks, lint, Vitest);
- the committed **production `web/dist`** (real Vite bundle; the PRD-06 placeholder `app.js`/`style.css` are removed). Assets are hex-content-hashed so the server's `is_fingerprinted` rule grants immutable caching; repeat builds are byte-identical;
- **`check:dist`** (`web/scripts/check-dist.mjs`): builds into an isolated temp dir and compares recursively and byte-exactly against the committed baseline, without Git. **Proven to fail on intentional drift**, then restored;
- **`check:embed-rebuild`** (`web/scripts/check-embed-rebuild.mjs`): proves the PRD-06 build-correctness amendment (below) end-to-end;
- **E2E snapshots** (`web/e2e/fixtures/{normal,partial}`): two complete, real three-artifact snapshots. `normal` (101 entities, `partial:false`); `partial` (73 entities, `partial:true`) generated from a workspace containing a genuinely uncompilable crate via `--keep-going`, carrying a real `target_failed` diagnostic. Both are **generated and deterministically path-normalized**, and both load through the **real server loader** (`crates/cratevista-server/tests/e2e_fixtures.rs`);
- the **real-server harness** (`web/e2e/support/harness.ts`): runs the actual `cargo-cratevista serve` binary on an **ephemeral loopback port**, polls `/api/health` for a strict `200`, captures output, and always reaps the process;
- **70 real-browser tests pass, zero skipped**, against the actual embedded production bundle, real same-origin APIs, real CSP headers and the real ELK worker. Nothing is mocked in Playwright.

**Four production defects were found in the browser that every unit test missed** — the direct justification for this phase. Two broke functionality outright; two broke interaction through layout:

1. **The production ELK worker never ran.** `elk.worker.ts` imported `elkjs/lib/elk.bundled.js`, whose default export is the **Node** variant; its fallback reads `require("./elk-worker.min.js").Worker`, but elk-worker only exports that when it believes it is *not* itself the worker script (`typeof document === "undefined"`). Inside our worker that guard is true, so elk-worker self-installed `self.onmessage` — hijacking our message channel — and left the factory undefined. Chromium threw `o is not a constructor`, layout never completed, and every node sat at the `(0,0)` placeholder. Corrected to `elkjs/lib/elk-api.js` + elkjs's own bundler worker (`elk-worker.min.js?worker`), which is exactly what that guard exists for. Vite emits it as a same-origin fingerprinted asset (never `blob:`), so `worker-src 'self'` holds.
2. **`/api/source` always reported a network failure.** `HttpSourceClient`'s default `fetchFn = fetch` captured a bare reference, and it is invoked as `this.fetchFn(...)`; the browser requires a `Window` receiver and threw `Illegal invocation`, which the catch reported as "Could not reach the server." Tests inject a mock, so only a real browser exposed it. The default is now wrapped. (`load.ts` is unaffected: it invokes `fetchFn` as a free call, where the receiver defaults to the global — which is why document loading always worked.)
3. **`fitView` ran before the real layout and never re-fit.** `<ReactFlow fitView>` fits **once, at init** — when every node still sits at the `(0,0)` placeholder — so it zoomed to fit a degenerate point-graph. When the real ELK coordinates arrived nothing re-fit: the graph overflowed its `overflow: hidden` viewport (nodes reached x≈2184 against a 1080-wide region) and the clipped parts, **including whole edges, could not be clicked**. `useFitOnLayout` now re-fits whenever a fresh layout lands, keyed on the `positions` Map identity — selection and inspector changes never touch it, and fitting only moves the viewport, never requests a layout.
4. **The absolute graph layer covered the reduced-mode banner** — the interaction/layout defect paired with (3), found while fixing it. `.cv-graph` was `position: absolute; inset: 0`, so it covered the **whole** `.cv-canvas` including any banner rendered above it, leaving the reduced-mode banner's own **"Render full graph" button unclickable**. `.cv-canvas` is now a flex column and `.cv-graph` a `flex: 1` child, so banners sit above the graph and stay interactive. `.cv-canvas` also gained a stable 320px `min-width`/`min-height`.

**For the record, the graph and inspector never overlapped**: they are disjoint grid columns (canvas `0–1080`, inspector `1080–1440`), which `e2e/tests/layout.spec.ts` now asserts. The unclickable edges were caused by the missing re-fit in (3), not by a panel overlaying the canvas. The floating React Flow controls and legend do legitimately float **above** the canvas — a node may sit under one — which is why the benchmark hit-tests before clicking a node, while `layout.spec.ts` proves **every rendered edge** hit-tests to itself.

Real layout now produces **finite, distinct, left-to-right coordinates with routed edges** (`workspace(x=12) → packages(x=252) → targets(x=492/732)`), and is **deterministic across both reload and a fresh browser context** (same node ids, same relative ordering, coordinates within a documented ±0.5px tolerance). The **CSP** and **same-origin worker execution** are verified in-browser.

**Nothing is pending.** The large-graph benchmark is recorded and the 1,500-node budget decided ([`docs/benchmarks/prd-07-large-graph.md`](../docs/benchmarks/prd-07-large-graph.md)); CI enforces the full ordered pipeline; the documentation pass (`README.md`, `web/README.md`, `CHANGELOG.md`, `docs/accessibility.md`) is complete; and `PRD/INDEX.md` is synchronized. Real-browser keyboard selection in the **GraphList** is proven in `e2e/tests/reduced-mode.spec.ts`, which serves the large benchmark fixture so reduced mode is entered through the **normal 1,500-node policy** rather than a test-only budget override — no required test is skipped.

**Server contract (implemented PRD 06):** the server is a loopback axum app that serves `GET/HEAD /api/{document,generation,diagnostics,health}` (exact stored canonical bytes) and a guarded, off-by-default `GET /api/source`; unknown `/api/*` → JSON `404`; non-API paths fall back to `index.html` (SPA). `/api/document` returns the issue-02 `ExplorerDocument`; `/api/diagnostics` returns the `DiagnosticsReport`; `/api/health` returns `{status, schema_version, partial}`. The server **embeds `web/dist` at compile time** (`rust-embed`, `debug-embed`), so a release must build the frontend **before** the server crate is compiled (build ordering is issue 10's concern; this PRD only produces the `web/dist` bundle). Every response carries the strict PRD-06 CSP — see "### Content-Security-Policy compatibility" below.

### PRD-06 build-correctness amendment (discovered during PRD-07 verification)

Authorized **in addition to** the CSP amendment; these two are the only Rust
production changes in PRD 07.

`cratevista-server` embeds `../../web/dist`, but that directory was **not a Cargo
package input**, so Cargo had no reason to rebuild the crate when the bundle
changed: `npm run build && cargo build` could silently keep serving the
previously embedded UI. The fix is `crates/cratevista-server/build.rs`,
containing only:

```rust
fn main() {
    println!("cargo::rerun-if-changed=../../web/dist");
}
```

It runs no npm, mutates no files, adds no build dependency, generates no Rust,
and changes no runtime behavior. It adds no watch, SSE, or static-export
functionality.

**Why a build script is required at all**, given that rust-embed's derive expands
to `include_bytes!` (whose paths rustc records as dependencies, so *modifying* an
embedded file already triggers a rebuild): that tracking only covers files that
were present at the last compile. **Adding or removing** a file does not, and
that is exactly what `npm run build` does whenever a content hash changes. This
was verified empirically — without `build.rs` the modify probe passes while the
add-file probe fails, serving the SPA fallback instead of the new asset.

`web/scripts/check-embed-rebuild.mjs` is the regression guard. It builds with the
committed dist, modifies an asset, rebuilds **without `cargo clean`**, and
asserts the **served bytes** changed (never merely an mtime); then it adds a new
asset and asserts it is served; then it restores the original bytes in a
`finally` block, rebuilds, asserts the restored bytes are embedded, and runs
`check:dist`. The repository is restored even when an intermediate assertion
fails. Playwright additionally byte-compares every served asset against the
committed `web/dist` as defense in depth.

The UX and data-shape decisions below (React Flow, node/edge adapter, inspector, timeline, focus/related toggles, legend, localization) are driven entirely by the issue-02 schema. See "## Explorer UI decisions".

## Terminology

Schema (issue 02) terms. **Adapter**: schema→React Flow transform. **Focus mode**: highlight selected entity + neighbors. **Related-only**: hide unrelated nodes.

## User-visible behavior

- Loads `/api/document` (fixture in tests/dev), shows view tabs; default view = Workspace overview.
- Left→right/ELK-laid graph; click node = inspect; double-click = focus/expand where applicable; hover edge = relation label.
- Search box filters/locates entities; kind filter chips; reset; language selector (en default, data model localization-ready).

## Functional requirements

1. Data loading (serve mode): fetch `/api/document`, `/api/generation`, and `/api/diagnostics` **concurrently**, with **`AbortController`** cancellation on unmount/retry. Loading/empty/error states per the rules in "### Runtime data loading". **No silent bundled demo data in production** — the fixture fallback is allowed **only** in explicit dev/test mode.
2. Schema adapter (`src/adapter/`): `documentToGraph(view, document)` → `{nodes, edges}` typed for React Flow; pure, unit-tested; **no rustdoc types**; entity kind → node category/visual style; relation kind → edge style/label.
3. Layout: ELK (`elkjs`) layered layout in a **dedicated same-origin module Web Worker**, computed from adapter output; deterministic options for test stability; tokened + cached; layout **not** stored in schema. See "### ELK layout".
4. Views: render **every view present in the document** as a tab (the eight generated views, plus any issue-08 manual-flow views when configured — the tab bar is data-driven, not a hard-coded list of 8); switching reprojects (the document carries all entities/relations plus each view's `entity_kinds`/`relation_kinds`/`entity_ids` filters; the client applies the active view's filters).
5. Search: match label + qualified_name (+ tags); highlight/focus matches.
6. Filters: entity-kind multiselect updates visible graph predictably; legend updates to present categories only.
7. Graph controls: fit, zoom in/out, reset, focus selected path, related-only, show all/related/hide edges, visible selection state, directional edge styling, stage lanes where a view defines stages.
8. Inspector: for entity — label, qualified name, kind, tags, description/rustdoc (Markdown rendered through a **sanitizing** renderer — see "### Security and privacy"), **source location (always shown when present in the document)** plus an open-source action, parent/container, related entities grouped by relation kind, attributes, documentation status. For relation — kind, source/target, label, provenance, attributes.

   **Open-source action availability (contract clarification).** `/api/health` does **not** advertise whether `/api/source` is enabled, and no other endpoint does. The UI therefore does **not** know in advance; it renders the open-source action optimistically and, on activation, `GET /api/source?path=…`. A `403` with code `source_disabled` is a normal, non-error outcome: the UI then shows the source **location only** and disables/annotates the action ("source contents disabled on this server"). Other stable source errors (`source_path_invalid`, `source_outside_root`, `source_not_file`, `source_too_large`, `source_not_utf8`) surface as inline, non-fatal messages. The action never blocks inspector rendering.
9. Large-graph handling: a **configurable** visible-node budget — **final default 1,500 visible nodes**, benchmarked and decided ([`docs/benchmarks/prd-07-large-graph.md`](../docs/benchmarks/prd-07-large-graph.md)) — drives an explicit reduced mode (focused-neighborhood render + banner + expand actions + Render-full-graph escape hatch), never silently dropping content. The budget is a **frontend engineering default**, not a schema limit and not a server limit: documents may contain any number of entities, and the budget only decides how many are drawn at once. See "### Large-graph policy". Documented + tested at the configured budget.
10. **Unknown-kind fallback:** the adapter and legend must render entity/relation kinds they do not recognize using a generic node/edge style and a generic legend entry (issue 02 kinds are open). Unknown kinds must never break rendering; they round-trip and appear with a neutral style + the raw kind string as a label/badge.
11. Accessibility: target **WCAG 2.1 AA** — see "### Accessibility".

## Technical design

### Module/component boundaries

```
web/src/
  api/                       # fetch document/generation/diagnostics (AbortController); source probe
  types/
    generated/               # explorer-document.ts (json-schema-to-typescript; committed, never hand-edited)
    diagnostics.ts, generation.ts   # small hand-written types + runtime guards (not schematized)
  model/        # immutable frontend document model + indexes (id→entity, relations-by-endpoint, by-kind)
  adapter/      # documentToGraph, kind→style maps (pure)
  layout/       # elk layout wrapper + elk.worker.ts (module Web Worker) + layout cache/token
  components/   # GraphCanvas, Node, Edge, Inspector, Toolbar, ViewTabs, StageLanes, Legend, Search, Filters
  state/        # Zustand store (UI state only — see "### State management")
  i18n/         # en dictionary + localization scaffolding
  fixtures/     # schema_parity / all_views / unknown_kinds document fixtures
  App.tsx, main.tsx
```

### Type generation (approved)

The `ExplorerDocument` types are **generated**, committed, and never hand-edited:

```
crates/cratevista-schema/schema/cratevista-document.schema.json
    --( json-schema-to-typescript )-->
web/src/types/generated/explorer-document.ts
```

Scripts: **`npm run generate:types`** (regenerate) and **`npm run check:types`** (regenerate to a temp file and diff — **fails when the committed file is stale**), wired into CI.

**Scope caveat (do not overstate).** That checked-in JSON Schema covers **`ExplorerDocument` only**. `GenerationReport` and `DiagnosticsReport` are **not** in the checked-in schema, so codegen does **not** cover them. Keep **small hand-written** types for only the fields the UI reads (`DiagnosticsReport { schema_version, diagnostics: DocumentDiagnostic[] }`; the `GenerationReport` fields used by the generation panel) and validate them through **explicit runtime guards** at the fetch boundary.

### Data model (client)

Mirror schema types; `GraphNode`/`GraphEdge` are React-Flow-facing and produced only by the adapter. The fetched `ExplorerDocument` is treated as **immutable**: it is wrapped in a separate frontend model (`model/`) that builds read-only **indexes** (id→entity, relations-by-endpoint, entities-by-kind, view lookup) once per load. Nothing mutates the document.

### State management (approved: Zustand)

A single **Zustand 5** store owns **UI state only**:

- active view;
- selected entity/relation;
- search query;
- entity-kind filters;
- focus mode;
- edge visibility;
- active stage;
- large-graph mode (normal / reduced);
- theme + language preference.

The store **never mutates `ExplorerDocument`**. Document indexes live in the separate immutable `model/`, not the store. Derived graph state (projected nodes/edges, legend categories, search matches) comes from **pure selectors/memoized adapters** over `(document model, UI state)`, not stored copies.

### Control flow

load artifacts → build immutable model + indexes → select view → adapter (memoized) → elk layout worker (tokened, cached) → render → user interactions update the Zustand store → selectors re-project / re-layout **only when the layout cache key changes** (see "### ELK layout"). Selection, inspector expansion, and hover **must not** re-layout.

### Runtime data loading

Serve mode fetches the three artifacts concurrently (`AbortController` for cancellation) and applies these rules:

| Condition | Behavior |
|---|---|
| `/api/document` fails or is malformed | **Blocking explorer error** (no graph). |
| Unsupported schema **major** version | **Blocking incompatibility screen**. |
| `/api/generation` fails | Graph may render; generation status **unavailable** + a visible warning. |
| `/api/diagnostics` fails | Graph may render; diagnostics panel shows an **unavailable** state. |
| `generation.partial == true` | Persistent **partial-generation banner**. |
| Empty document / empty active view | Explicit **empty state**. |
| Retry | Reloads **all three** artifacts coherently (one AbortController generation). |

**No silent bundled demo data in production.** A bundled fixture is used only in explicit dev/test mode. Source-content probing is unchanged: location always visible; the source action requests `/api/source`; `403 source_disabled` → normal location-only mode; other source errors → inline non-fatal state (functional requirement 8).

### Error handling

Fetch failure/malformed document/empty view → explicit states (table above). The adapter tolerates unknown attribute keys AND unknown entity/relation kinds (forward-compat with schema minor bumps — issue 02 kinds are open; unknown kinds render with a generic fallback style, never a crash).

### Views and URL state

The view tab bar is **entirely data-driven** and must support: the eight current generated views; **unknown future views**; **manual views** (PRD 08); **empty views**; and views that define **Stages**.

**Default view selection** (in order): (1) the document's default/focus information when usable; (2) `view:workspace-overview` when present; (3) the first view in document order.

**Approved query-string state** (shareable, restorable): `view`, `entity` or `relation`, `q`, `kinds`, `focus`, `edges`, `stage`. Browser **back/forward restores** this state. Invalid/stale ids **degrade safely** (fall back to defaults, no crash). **Do not** encode hover state or viewport coordinates in the URL.

### ELK layout

Layout runs in a **dedicated same-origin module Web Worker** (`layout/elk.worker.ts`; `worker-src 'self'`). Semantic ELK configuration:

- layered algorithm; `elk.direction = RIGHT`;
- orthogonal edge routing;
- deterministic spacing and ordering (stable input order — the document is already id-sorted);
- parent hierarchy represented as **nested groups** where useful;
- disconnected components handled deterministically;
- **Stage lanes only** for a view that defines stages;
- **no hard-coded entity-kind columns**.

**Tokening:** every layout request carries a monotonically increasing token; stale results are discarded. **Layout cache key** = (document identity/hash, active view, filters, focus/related mode, expanded-neighborhood state, stage). **Selection, inspector expansion, and hover must not trigger relayout.**

### Large-graph policy

**Final visible-node budget = 1,500** (configurable). Benchmarked and decided on
2026-07-16 — full data and rationale in
[`docs/benchmarks/prd-07-large-graph.md`](../docs/benchmarks/prd-07-large-graph.md).

Measured in the pinned Chromium against the real embedded bundle and real
generated fixtures (i9-13900K / 32 cores; medians of three warm runs):

| Visible nodes | First usable graph | Click → inspector | JS heap |
| --- | --- | --- | --- |
| 64 | 165 ms | 16 ms | 28 MB |
| 1,212 (full render, below budget) | **1,019 ms** | **206 ms** | 202 MB |
| 69 (reduced, from 3,232 projected) | 505 ms | 802 ms | 28 MB |
| 3,232 (full render, forced) | **3,662 ms** | **3,414 ms** | 468 MB |

**Rationale.** Interaction latency, not first paint, decides the value: a 1,212-node
full render stays responsive (206 ms per selection), while 3,232 rendered fully
costs **3.4 s on every click** and 468 MB — on the fastest hardware measured, so
those are best-case numbers. Reduced mode holds first-usable near 0.5 s
**independent of document size**. Model/index construction (≤ 4.3 ms) and adapter
projection (≤ 1 ms) are negligible at every size, which is why the budget governs
**visible** nodes rather than document size. Honest limit: the knee was not
bisected — 1,212 measures good, 3,232 measures poor, and 1,500 sits between them
without a direct full-render measurement at that exact size.

**The benchmark found no need for clustering in the MVP. The final configurable
default remains 1,500 visible nodes.** Reduced mode already holds first-usable
near 0.5 s independent of document size, so clustering would add complexity
without addressing a measured problem.

The budget is a **frontend engineering default** (`DEFAULT_LARGE_GRAPH_BUDGET`,
overridable via `<App budget>`), **not a schema limit**, not a server limit, and
not a guarantee that every hardware configuration behaves identically.

When the projected graph exceeds the budget:

- show a visible **reduced-mode banner** with full vs visible counts;
- render a **focused neighborhood** around the default focus, the selected entity, or the search result;
- keep **every** entity reachable via a searchable/list representation;
- provide explicit **expand-neighborhood** actions;
- provide an explicit **Render full graph** action with a performance warning;
- truncate relations **only together with** their hidden endpoints;
- **never silently drop** graph content.

**No clustering in the MVP** unless benchmark evidence shows it necessary. During implementation, benchmark representative generated documents and record: adapter time, layout-worker time, first usable render, interaction responsiveness, peak memory, and the chosen normal/reduced threshold.

### Compatibility

Consumes the document `schema_version`. An **unsupported MAJOR** version is a **blocking incompatibility screen** (see "### Runtime data loading"); a newer **MINOR** is accepted (forward-compatible). Unknown/newer entity/relation kinds render via the generic fallback (a MINOR-compatible change per issue 02). No coordinates consumed (layout is client-side). Works as an embedded SPA with **`base: "./"`** relative asset paths (static-build subpath coordinated with issue 10).

### Security and privacy

- Only calls the documented same-origin API endpoints; **no external network calls; no telemetry**.
- Source *locations* are shown from the document without any extra request; source *contents* are fetched only via the opt-in `/api/source` endpoint, and a `403 source_disabled` degrades gracefully (see functional requirement 8).
- **Markdown/rustdoc sanitization (required, approved stack).** rustdoc descriptions originate from the analyzed workspace **and its dependencies**, so doc comments can contain arbitrary Markdown **including raw HTML**. Render Markdown with **`react-markdown` + `remark-gfm` + `rehype-sanitize`**, and specifically: **no `rehype-raw`**, **no `dangerouslySetInnerHTML`**, explicit safe-link handling, and any allowed external links get `rel="noopener noreferrer"`. The pipeline must reject/neutralize `<script>`, event-handler attributes (`onerror`, …), `javascript:` URLs, and unsafe embedded HTML. The strict CSP is defense-in-depth, not the primary control. Hostile-input **unit and component** tests feed `<img src=x onerror=…>`, `<script>`, and `[x](javascript:…)` and assert they are neutralized.

### Content-Security-Policy — approved PRD-06 amendment

**Correction.** An earlier draft claimed React `style`-prop / CSSOM assignments are outside CSP enforcement. That is wrong: the implemented PRD-06 policy `style-src 'self'` **blocks inline `style` attributes, including dynamically assigned `style` attributes** — which is exactly how React Flow positions nodes (`transform: translate(x,y)` on each node wrapper). Under the current strict policy the graph would not lay out. This is a real, required policy change.

**Approved amendment (deliberate, minimal).** PRD 07 implementation may update the single CSP constant in `crates/cratevista-server/src/router.rs` (the reserved PRD-07 extension point) to:

```
default-src 'self'; script-src 'self'; style-src 'self'; style-src-attr 'unsafe-inline'; connect-src 'self'; worker-src 'self'; base-uri 'self'; object-src 'none'; frame-ancestors 'none'
```

This is recorded as an **approved PRD-06 security-policy amendment**, not an accidental relaxation. Constraints kept:

- **no `script-src 'unsafe-inline'`**; **no `style-src 'unsafe-inline'`** (only the narrower **`style-src-attr 'unsafe-inline'`**, scoped to inline `style` *attributes* for React Flow geometry — it does **not** permit `<style>` elements or inline scripts);
- **no `unsafe-eval`**; **no remote origins** (`connect-src 'self'`, `worker-src 'self'`); **no permissive CORS**.

**Stylesheets stay external.** Do **not** use a runtime CSS-in-JS system (styled-components/Emotion) that injects `<style>` elements — those are governed by `style-src` (still `'self'`) and would be blocked. Use plain CSS / CSS Modules / imported stylesheets (e.g. `@xyflow/react/dist/style.css`); Vite emits them as `<link rel="stylesheet">`.

**ELK worker.** Use a Vite-emitted, **same-origin module worker** — `new Worker(new URL("./elk.worker.ts", import.meta.url), { type: "module" })` — which is covered by `worker-src 'self'`. Do **not** use a `blob:` worker unless implementation evidence proves it necessary; if so, that is a further deliberate, documented CSP amendment.

**Required test.** A Playwright test loads the built app **served by the real `cratevista` server** (so the actual CSP headers apply), **fails on any CSP violation** (console/`securitypolicyviolation`), and verifies graph **positioning**, **pan/zoom**, and **inspector** rendering.

### Accessibility

Target **WCAG 2.1 AA** for text and application controls. Required:

- semantic **buttons / tabs / dialogs** (not clickable `<div>`s);
- **visible focus** on every interactive element;
- keyboard-operable **view tabs, search, filters, toolbar, and inspector**;
- **Escape** clears the current selection or closes transient UI;
- **reduced-motion** support (`prefers-reduced-motion` disables edge/transition animation);
- **no information conveyed by color alone** (kinds also carry a label/badge/shape);
- **accessible labels** for node and relation kinds;
- a **searchable/list alternative** for graph content (also the reduced-mode and non-pointer path);
- **focus restored predictably** after inspector close / view switch.

## CLI/API/configuration changes

None to the Rust CLI/API. Adds the frontend quality gates required by CLAUDE.md.

### Source / dist workflow and CI

**`web/src` is the authoritative source.** `web/dist` **remains committed** because `cratevista-server` embeds it and a **clean `cargo build` must not need Node.js**. Vite is configured with **`base: "./"`** (relative asset paths — works embedded and under a static-build subpath, coordinated with issue 10).

Required npm scripts (all **cross-platform** — no bash-only constructs; use Node/`cross-env`-style tooling for Linux/macOS/Windows/WSL):

- `npm run lint`
- `npm run typecheck` (TypeScript 7 — authoritative)
- `npm run typecheck:compat` (TypeScript 6 compatibility guard for typescript-eslint)
- `npm run test`
- `npm run build` (→ `web/dist`)
- `npm run e2e`
- `npm run generate:types`
- `npm run check:types` — fails when `web/src/types/generated/**` is stale
- `npm run check:dist` — rebuilds from source and **fails when the committed `web/dist` differs**

**`cargo build` must never invoke `npm`.** Authoritative CI order (implemented as
the single `explorer` job in `.github/workflows/ci.yml`):

1. `npm ci`;
2. `npm run generate:types`;
3. `npm run check:types`;
4. `npm run typecheck` (TypeScript 7 — authoritative);
5. `npm run typecheck:compat` (TypeScript 6 compatibility guard);
6. `npm run lint`;
7. `npm run test`;
8. `npm run check:dist` — **against the untouched committed `web/dist`**;
9. `npm run build`;
10. compile the Rust E2E binary (`cargo build -p cargo-cratevista`) **after** that
    exact dist exists, so it embeds it;
11. `npm run e2e` (Playwright against the real server and that binary);
12. `npm run check:embed-rebuild`;
13. `cargo fmt --all -- --check`;
14. `cargo clippy --workspace --all-targets --all-features -- -D warnings`;
15. `cargo test --workspace --all-features`;
16. `cargo +1.97.0 check --workspace --all-features`.

Two ordering constraints are load-bearing, and both were originally wrong:

- **`check:dist` must precede `build`.** It compares the committed baseline
  against a fresh isolated build; building first would overwrite that baseline,
  and the check could never fail.
- **The Rust binary must be compiled after `build`.** `cratevista-server` embeds
  `web/dist` at compile time, so a binary compiled earlier would serve a
  different bundle than the one under test. This is why CI runs the whole
  pipeline as **one job**: transferring `dist` between jobs would reintroduce the
  ambiguity. `crates/cratevista-server/build.rs` makes Cargo honour the
  dependency, and Playwright byte-compares every served asset against
  `web/dist` as defence in depth.

**A third, subtler one:** step 2 (`generate:types`) **overwrites** the committed
generated types, while step 3 (`check:types`) compares a fresh generation against
the *committed* file. Run in that order, step 3 can no longer fail. CI therefore
asserts `git diff --exit-code -- web/src/types/generated` immediately after step
2 — that check is what actually catches generated-type drift; `check:types`
remains as the Git-free guard for local use.

Caching is limited to inputs that cannot mask drift: the npm **download** cache
(never `node_modules` or `dist`) and `Swatinem/rust-cache` (registry and
dependency artifacts; it discards the workspace's own crates).

## Files and modules to create or modify

- `web/{package.json,package-lock.json,tsconfig.json,vite.config.ts,eslint config,index.html}`
- `web/src/**` per the structure above, including `web/src/types/generated/explorer-document.ts` (generated, committed) and `web/src/types/{diagnostics,generation}.ts` (hand-written).
- `web/src/fixtures/{schema_parity.document.json, all_views.document.json, unknown_kinds.document.json}` (see "### Fixtures") and the small checked-in sample workspace that `all_views` is generated from.
- `web/tests/**` (Vitest + Testing Library) and `web/e2e/**` (Playwright).
- `web/scripts/{gen-types.ts, check-types.ts, check-dist.ts}` (or equivalent, cross-platform Node scripts).
- CI: extend `.github/workflows/ci.yml` with a frontend job (Node 24 pinned; `npx playwright install --with-deps`) per the CI order in "### Source / dist workflow and CI".

## Testing strategy

### Unit tests (Vitest)

- Adapter: entity/relation kinds → nodes/edges/styles; no rustdoc leakage; unknown attrs tolerated.
- Search/filter/legend logic (pure selectors).
- i18n resolution.

### Integration/component tests (Testing Library)

- Load the `all_views.document.json` fixture → each of the eight generated views (by its real slug) renders as a tab and projects the expected entities/relations.
- Selecting a node populates the inspector (all fields).
- Kind filter updates visible nodes; legend reflects present categories.
- Fit/reset/focus/related controls change state predictably.

### End-to-end tests (Playwright)

- Smoke: open a view, select a node, search, change filters — against the built app **served by the real `cratevista` server** on a complete generated snapshot (component/integration tests use stubbed endpoints).
- **CSP + rendering:** load under the real server, **fail on any CSP violation**, and assert graph **positioning**, **pan/zoom**, and **inspector** render (see "### Content-Security-Policy — approved PRD-06 amendment").
- **URL state:** deep-link `?view=…&entity=…&q=…&kinds=…&focus=…&edges=…&stage=…` restores state; **back/forward** works; stale ids degrade safely.

### Fixtures

Three fixtures under `web/src/fixtures/`, each asserted to validate against `cratevista-document.schema.json`:

1. **`schema_parity.document.json`** — a **byte copy** of the schema crate's `full_mvp.document.json` (contract-parity guard). It has only two views (`view:overview`, `view:types`) with non-generated ids, so it is **not** used for the "renders every MVP view" tests.
2. **`all_views.document.json`** — **generated** from a small **checked-in representative Rust workspace** (so it tracks the real builder output); contains all **eight** real generated view ids (`view:workspace-overview`, `view:crate-dependencies`, `view:module-hierarchy`, `view:types`, `view:traits-and-impls`, `view:type-relationships`, `view:public-api`, `view:documentation-coverage`). Consumed by **stable frontend tests** (the committed JSON is what tests read — no nightly at test time). **Refreshing** the fixture may be a **gated command** that uses the pinned nightly (rustdoc JSON); a test asserts its view-id set equals the eight generated slugs.
3. **`unknown_kinds.document.json`** — small **hand-authored**, schema-valid fixture containing **unknown** entity and relation kinds; drives the generic-rendering and legend-fallback tests.

**Normal frontend tests must not require nightly** — they read the committed fixture JSON.

**Real-server E2E fixture.** The implemented server loads a **hash-verified** snapshot, so a lone `document.json` is **not** a valid server fixture. Real-server E2E must provide a **complete matching snapshot**: `document.json`, `generation.json` **with valid `artifact_hashes`**, and `diagnostics.json` (produced by `cargo cratevista generate` on the sample workspace). Non-server component/integration tests instead stub the endpoints with the fixtures above.

## Performance considerations

Memoize adapter + layout; virtualize/simplify above threshold; avoid re-layout on selection/inspector changes; lazy-load heavy panels. Deterministic ELK options both help tests and perf caching.

## Observability and diagnostics

Fetch diagnostics separately (`/api/diagnostics`, the `DiagnosticsReport` of `DocumentDiagnostic`s) and render them in a panel — diagnostics are **not** embedded in `ExplorerDocument`/`document.json`; a diagnostics-fetch failure shows an unavailable state, not a blocking error. An **unsupported MAJOR** `schema_version` is a **blocking incompatibility screen** (not a console warning); `generation.partial` shows a persistent banner. Empty/error states are explicit.

## Documentation changes

`web/README.md` (dev/build/test); README screenshots/GIF placeholder (issue 10); accessibility notes.

## Rollout and migration

New `web/` app; its `dist` (built with `base: "./"`) **has replaced** the PRD-06 placeholder and is embedded by the server. The committed `web/dist` stays in sync via `check:dist`, and Cargo rebuilds the embedding crate when it changes via `crates/cratevista-server/build.rs`; base-path/hosting-subpath handling is coordinated with issue 10.

## Risks and mitigations

- **Schema/type drift** → generate TS types from JSON Schema + fixture-validates-against-schema test.
- **Non-deterministic layout breaking tests** → fixed ELK options + assert on graph structure, not pixel positions.
- **Large graphs janky** → explicit threshold + fallback + test.
- **rustdoc leakage** → adapter boundary + lint/test forbidding raw rustdoc shapes.

## Alternatives considered

- Dagre / hand-rolled column layout: rejected as the primary layout. A fixed-column layout keyed to a small, curated set of blocks does not scale to generated Rust graphs, which are far larger and denser and have no curated column assignment. CrateVista uses **ELK** layered layout (issue mandate) with deterministic options; a column-style ELK configuration can still approximate left→right lanes.
- Storing layout in schema: rejected — violates issue 02 (no coordinates).
- Hand-authored UI data: rejected — the explorer is driven entirely by the generated schema document, never by hand-authored domain content.

## Implementation sequence

1. Vite/TS/ESLint scaffold (Node 24, `package-lock.json`) + strict config + CI job + `base: "./"`.
2. Type codegen (`generate:types`/`check:types`) + hand-written diagnostics/generation types + immutable model/indexes + concurrent artifact loader (AbortController) + fixtures (incl. the sample workspace).
3. Adapter + kind style maps + unit tests (no rustdoc leakage; unknown-kind fallback).
4. ELK **module Web Worker** + tokened/cached layout + GraphCanvas + controls (fit/zoom/reset/focus/related/edge-visibility).
5. ViewTabs (data-driven) + URL-state sync + Search + Filters + Legend + StageLanes.
6. Inspector (entity + relation) + sanitized Markdown + source action (403-degrade).
7. Large-graph reduced mode + benchmark + accessibility pass (WCAG 2.1 AA).
8. **Approved CSP amendment** to the PRD-06 constant + component/E2E tests (incl. CSP-violation-fails E2E) + `check:dist`.

## Acceptance criteria

- [x] UI loads a fixture document and renders every MVP generated view. *(driven by `all_views.document.json` — **real** `cargo cratevista generate` output from the checked-in `web/fixtures/sample-workspace/` (101 entities / 166 relations / 8 views). Tests: JSON-Schema-validates (ajv 2020), view-id set equals the eight generated slugs, each view projects without crashing, and the app renders exactly eight selectable view tabs.)*
- [x] Search locates entities by label and qualified name. *(unit `searchEntities` + component: search box → result option → selects the entity and populates the inspector)*
- [x] Entity-kind filters update the visible graph predictably. *(component: checking a kind narrows the visible nodes)*
- [x] Selecting a node populates the inspector. *(component: click node → Entity inspector shows label/kind/qualified-name/source/grouped relations/diagnostics; click edge → Relation inspector)*
- [x] Fit/reset/focus/related controls work. *(component: Fit/Zoom-in/Zoom-out invoke the flow instance; **Reset** clears search/filters/selection/stage, keeps the active view, and fits (state + rendered assertions); edge modes all/hidden/related change the rendered edge set; **Related only** hides unrelated nodes; double-click focuses around a node — 6 control tests)*
- [x] Legend reflects only categories present in the active view. *(unit `legendForGraph` + component: legend shows the active view's categories, incl. a generic entry for unknown kinds)*
- [x] Layout deterministic enough for repeatable tests. *(fixed ELK options; structural assertions)* — **verified in real Chromium against the real worker** (`e2e/tests/worker.spec.ts`, "deterministic layout"): for the same view/state, both a **reload** and a **fresh browser context** yield the **same node ids in the same relative ordering** (not merely the same set) and coordinates equal within a documented **±0.5px tolerance**; every coordinate is finite and the all-zero placeholder is excluded (distinct positions == node count). No hard-coded pixel coordinates are asserted.
- [x] Large graph handling uses a **configurable** visible-node budget and the reduced-mode behavior (banner with full/visible counts + deterministic focus + bounded neighborhood + Expand + Render-full-graph/Return + complete searchable entity list; endpoint-containment prevents dangling edges; never silently drops content). *(pure `reduceGraph` — 10 unit tests: budget threshold, focus order selected→search→default→first, bounded BFS neighborhood, expand grows, recenter, reachability, no-silent-omission; + 4 reduced-mode component tests at a configured budget.)* **Benchmarked and decided (2026-07-16): the default stays `1,500`, and is no longer provisional.** Measured in the pinned Chromium against the real embedded bundle and real generated fixtures (`docs/benchmarks/prd-07-large-graph.md`): a **1,212-node** full render is usable (**1.0 s** to first usable graph warm, **206 ms** click-to-inspector), while **3,232** nodes rendered fully costs **3.7 s** to first usable, **3.4 s per selection** and **468 MB** of JS heap — selection latency degrades far worse than first paint, which is what decides the value. Reduced mode holds first-usable at **336 ms** (1,616 projected) / **505 ms** (3,232 projected) — independent of document size. Model/index (**≤ 4.3 ms**) and adapter (**≤ 1 ms**) are negligible at every size: the cost is layout and rendering, which is why the budget governs **visible** nodes, not document size. Honest limits: the knee was not bisected (1,212 good, 3,232 poor, 1,500 between), and the numbers come from one fast desktop (i9-13900K/32 cores), so they are a best case. The value is a **configurable frontend default** (`<App budget>`), not a schema or server limit.
- [x] Unknown entity/relation kinds render via a generic fallback and never break rendering. *(adapter/legend unit tests + component test with an unknown-kind document: generic legend entry, node renders and stays selectable)*
- [x] TypeScript strict mode passes. *(strict `tsconfig`; **authoritative** `npm run typecheck` on TypeScript 7 exit 0 **and** the `npm run typecheck:compat` TypeScript 6 compatibility check exit 0)*
- [x] Unit/component tests cover the schema adapter and major interactions. *(Vitest: **176 tests** across 15 files — model/adapter/reduce/selectors/url-normalize/guards/store/loader/source/SafeMarkdown/layout/dist-compare units + App-component, controls, reduced-mode, history, a11y and fixture suites)*
- [x] E2E smoke covers opening a view, selecting a node, searching, changing filters. *(Playwright)* — **full inventory against the `normal` snapshot and the embedded production dist** (`e2e/tests/smoke.spec.ts`), nothing mocked: application load; toolbar; view tabs; the optional stage region (correctly **absent** — no generated view defines stages); graph + inspector regions; **all eight** generated views each rendering a non-empty graph; view switching; entity selection; relation selection; search by **label** and by **qualified name**; kind filters (narrow + clear, reflected in `kinds`); the dynamic legend; fit/zoom-in/zoom-out/reset; focus / related-only; edge modes **all/related/hidden**; the **source action with source disabled by default** (`serve` without `--source` → real `403 source_disabled` → "Source contents are disabled on this server; showing the location only."); the diagnostics region; and **no partial banner**. Against the `partial` snapshot: the **partial banner persists** across view change and reload, the real `target_failed`/`cvbroken` diagnostic is represented, and the graph and entity inspector both remain usable.
- [x] The built app renders under the amended PRD-06 CSP (no `script-src`/`style-src` `unsafe-inline`, no `unsafe-eval`, no remote origins) with **no CSP violations**; graph positioning, pan/zoom, and inspector work. *(Playwright against the real server; fails on any `securitypolicyviolation`)* — **verified against the real served headers** (`e2e/tests/security.spec.ts`): the header carries **exactly the nine approved directives**; **exactly one `unsafe-inline` token**, and it belongs to **`style-src-attr`** (bare `script-src`/`style-src` stay `'self'`); **no `unsafe-eval`**, no `blob:`, **no remote origin**, no `*`, and no `Access-Control-Allow-Origin`. Instrumentation installed **before any application script** records `securitypolicyviolation`, console errors, uncaught page errors and failed same-origin requests, and the fixture fails the test on any of them: the app boots with **zero CSP violations**, zero unexpected console/page errors, and **every request same-origin**.
- [x] Rendered rustdoc Markdown is sanitized (`react-markdown` + `rehype-sanitize`, no `rehype-raw`/`dangerouslySetInnerHTML`) — a hostile doc string cannot inject active content. *(component tests in `web/tests/markdown.test.tsx` render the real `SafeMarkdown` and verify `<script>`, `onerror`, `javascript:`, encoded-`javascript:`, and malformed-HTML are neutralized, plus safe-markdown/external-link hardening — 8 tests pass)*
- [x] The open-source action degrades gracefully when `/api/source` is disabled (`403 source_disabled` → location-only, no error state). *(19 tests in `web/tests/source.test.tsx`: `HttpSourceClient` URL-encodes the repo-relative path (no line/col params), maps 403 `source_disabled` → capability absence, maps all five stable codes without echoing server text, maps malformed/network → generic retryable; component: no request before explicit activation, contents render with the repo-relative path, **no absolute path**, `source_disabled` → location-only with the inspector still usable and no global error, stable-error inline message, retry, abort on selection change / inspector close, late (stale) response ignored)*
- [x] Concurrent artifact loading follows the rules table (document/major-version blocking; generation/diagnostics degraded; partial banner; empty state; coherent retry); no silent demo data in production. *(unit loader tests + component tests per row: blocking document-error+retry, incompatible-major screen, generation-unavailable warning, diagnostics-unavailable, partial banner, empty state; production requires an injected `ServerArtifactSource` with no fixture fallback)*
- [x] URL query-state (`view`/`entity`/`relation`/`q`/`kinds`/`focus`/`edges`/`stage`) is shareable and restored on back/forward; stale ids degrade safely. *(E2E + unit)* — **logical contract COMPLETE** (25 unit tests): one `normalizeUrlState` boundary applied on **both** initial load and popstate (view fallback chain; relation-over-entity; invalid-relation→valid-entity; stale entity/relation/stage/focus removal; unknown-kind removal + dedupe + deterministic order; edges/q validation; transient keys never serialized; idempotent round-trip), plus history semantics. **Now also proven in the real browser against the real history stack** (`e2e/tests/history.spec.ts`, no mocked history object): a full deep link (`view`+`entity`+`q`+`kinds`+`focus`+`edges`) is honoured and **visible in the UI** (selected tab, open inspector, search value, edge mode, `aria-pressed` focus toggle); two meaningful `pushState` steps followed by `page.goBack()` restoring the full prior durable state and `page.goForward()` restoring the next; **Back to a relation selection restores the relation inspector**; stale **view** falls back to a real view, stale **entity**/**relation**/**stage** are dropped; **relation-over-entity** priority holds; typing six characters adds **zero** history entries (replacement, not push) so one Back leaves the search behind; **popstate pushes no duplicate entry** (`history.length` unchanged across two Backs); refresh preserves the normalized query state exactly; and a **non-API SPA path without a trailing slash** (`/explore`) serves `index.html` and loads its relative assets with no failed requests, while an unknown `/api/*` path is **not** swallowed by the fallback.
- [x] Layout runs in a same-origin module Web Worker, is tokened (stale results discarded), and selection/inspector/hover do **not** relayout. *(component test asserting layout invocation count)* — **verified with the real worker in real Chromium** (`e2e/tests/worker.spec.ts`): a Worker is actually created; its URL is **HTTP same-origin** and **not `blob:`**; the worker asset responds `200`; the graph **leaves the layout-loading state** with finite, non-placeholder coordinates and non-zero rendered dimensions; **selection does not issue another layout request** and **opening/closing the inspector does not relayout** (counted by instrumenting `Worker.postMessage` before app scripts), while a **view change does** trigger a new layout; and no worker error or "Layout failed." state occurs. This criterion is no longer inferred from unit tests — doing so is precisely what hid the `elk.bundled.js` defect.
- [x] Generated `explorer-document.ts` is not stale (`check:types`) and the committed `web/dist` matches a fresh build (`check:dist`). *(CI)* — `check:types` passes; `web/dist` holds the **real production bundle** (placeholder removed); `check:dist` performs an **isolated temp-dir build** and a **recursive exact-byte** comparison against the untouched committed baseline (no Git), and was **proven to fail on intentional drift** (naming the changed file) before being restored. Its comparison helper is unit-tested (byte-exact, not text- or length-based). Both checks are enforced by the `explorer` job in `.github/workflows/ci.yml`, which runs `check:dist` against the untouched checkout **before** any build.
- [x] Accessibility baseline (WCAG 2.1 AA): semantic controls, visible focus, keyboard-operable tabs/search/filters/toolbar/inspector, Escape behavior, reduced-motion, non-color-only encoding, list alternative. *(21 tests in `web/tests/a11y.test.tsx`.* **Automated (axe-core, jsdom):** no violations across 9 states — loaded explorer, entity inspector, relation inspector, loading, blocking error, incompatible schema, empty view, reduced mode, source-disabled; the `region` landmark rule is **enabled**. **Component/keyboard:** landmarks (banner/nav/main/complementary), tablist↔tabpanel (`aria-controls`/`aria-labelledby`), roving `tabindex` + Arrow/Home/End with activation-follows-focus, labelled toolbar, keyboard-operable kind filters, listbox/option search results, Escape clears selection without trapping focus, inspector accessible heading, GraphList keyboard-reaches every (incl. hidden) entity, non-color kind encoding (text badge + legend label + "(unknown)" marker), and the `prefers-reduced-motion` / `:focus-visible` stylesheet contract. **Documented baseline — NOT full WCAG conformance:** `color-contrast` cannot run in jsdom and remains a **manual** check; the *rendered* reduced-motion effect and real screen-reader/AT behavior are **manual/E2E** and are not claimed here.)*

Verification:

```bash
cd web && npm ci && npm run generate:types && npm run check:types && npm run typecheck && npm run typecheck:compat && npm run lint && npm run test && npm run build && npm run check:dist && npm run e2e
```

## Explorer UI decisions

1. **App shell**
   - A **four-region shell**: toolbar / view tabs / stage timeline / graph + right-side inspector, in a polished dark visual language.
   - A wide graph canvas with a floating legend bottom-left and a right-side inspector ≈ 320–400px.

2. **Graph rendering**
   - **React Flow** with a **single custom node component** styled by entity category, showing title + kind label + up to ~2 compact secondary lines (qualified name / source hints); nodes non-draggable.
   - A pure **`documentToGraph` adapter** builds React Flow nodes/edges from the schema; rustdoc shapes never reach components.
   - **ELK** layered layout with fixed, deterministic options (for testability), replacing any hand-rolled column layout.
   - Labeled directional edges; hover reveals the relation label; active-path edges are visually distinct.

3. **Controls and interaction**
   - Focus/related and edge-visibility toggles; fit / zoom / reset controls with a zoom-% indicator.
   - A floating legend that reflects only the entity categories actually present.
   - Click a node → select it (entity inspector); double-click → an expanded detail view with "Back to system map"; `Esc` clears focus.
   - A timeline of numbered stages with connecting arrows; clicking a stage highlights related entities/edges and populates the step inspector.

4. **Inspector**
   - Collapsible sections grouped by relation kind; a **relation inspector** for edges and a **step inspector** showing related entities/relationships.
   - Rust-specific data (visibility, documentation coverage, trait impls, function-signature types).
   - Source shown as a validated repo-relative span (`SourceLocation`); file contents appear only when the opt-in server endpoint is enabled.

5. **Localization**
   - Dictionary-based i18n with a persisted language selection, using issue-02 `LocalizedText` for labels and descriptions.

6. **Data model**
   - The whole UI is driven by the **generated `document.json`** (issue 05), never hand-authored data.
   - Entity/relation kinds come from the **open schema with a generic fallback**, not a fixed subtype enum.
   - Views are generated projections (Workspace overview, Crate dependencies, Module hierarchy, Types, Traits & impls, Type relationships, Public API, Documentation coverage) plus manual flows (issue 08).
   - Graphs are larger and denser than a small curated set, so ELK layout is paired with a **configurable large-graph fallback threshold** (benchmark ~1,500 nodes, justified by measurement).

7. **Visual & interaction acceptance criteria**
   - Colored, typed node cards with title + kind label + up to ~2 secondary lines; a clear selected state (glow/outline).
   - Fit / zoom / reset controls and a zoom-% indicator; focus/related and edge-visibility toggles present and functional.
   - Inspector sections collapsible and grouped by relation kind; code/source shown as a reference (path + span), with contents only when the opt-in endpoint is enabled.
   - Consistent spacing, borders, and typography, with visible focus states across controls.

8. **Screens/states the explorer must support**
   - Overview with a view selected and a timeline step's step-inspector.
   - Focus mode with a step selected, showing related entities and relationships.
   - A detail/expanded view with "Back to system map", an entity-kind filter, and an inspector source reference.
   - An "all entities" mode with a fully populated entity inspector (source, incoming connections, data structures, etc.).

## Open questions

**None. Every PRD-07 design question is resolved, and every one is now implemented
and verified.** No PRD-07 measurement is deferred.

- **State lib** — resolved and **implemented**: **Zustand 5** (UI state only; the document stays immutable).
- **Large-graph budget/fallback** — resolved and **implemented**, and the benchmark that was the one deferred measurement is **complete**: it **retained the 1,500** visible-node default (configurable), together with the reduced-mode behavior in "### Large-graph policy". Evidence: [`docs/benchmarks/prd-07-large-graph.md`](../docs/benchmarks/prd-07-large-graph.md). **No clustering in the MVP** — the benchmark did not prove it necessary: reduced mode holds first-usable near 0.5 s independent of document size.
- **TS types** — resolved and **implemented**: **codegen** `ExplorerDocument` via `json-schema-to-typescript` (committed + drift-guarded by `check:types`); hand-written diagnostics/generation types with runtime guards.
- **ELK left→right lanes** — resolved: `elk.direction = RIGHT`, layered, orthogonal routing, **no hard-coded entity-kind columns**; stage lanes only when a view defines stages.

No design decisions and no measurements remain outstanding. The one item that was
deferred to implementation — the benchmarked large-graph default — was measured
and **retained at 1,500**.

## Traceability

Issue-07 checkboxes → tests above. Consumes issue-06 `/api/document` + issue-02 schema; renders issue-05 views + issue-08 manual flows; `web/dist` embedded by issues 06/10; live-reload events (issue 09) integrate via the api/store seam.

## Implementation status

**Status: Implemented / Verified (2026-07-16).** Every acceptance criterion above
has implementation + test evidence; the full frontend and Rust gates pass; the
large-graph benchmark is recorded and the node budget decided.

**Final evidence:**

- **176** frontend unit/component tests (15 files); **70** real-browser tests,
  **zero skipped** — the GraphList keyboard test is no longer skipped; **239**
  Rust tests.
- **Gates, all by exit code:** `npm ci`, `generate:types`, `check:types`,
  `typecheck` (TS7), `typecheck:compat` (TS6), `lint`, `test`, `check:dist`,
  `build`, `e2e`, `check:embed-rebuild`; `cargo fmt --all -- --check`,
  `cargo clippy --workspace --all-targets --all-features -- -D warnings`,
  `cargo test --workspace --all-features`,
  `cargo +1.97.0 check --workspace --all-features`.
- **Final node budget: 1,500, benchmarked and no longer provisional** —
  [`docs/benchmarks/prd-07-large-graph.md`](../docs/benchmarks/prd-07-large-graph.md).
- **CI** enforces the complete ordered pipeline as the single `explorer` job in
  `.github/workflows/ci.yml`, including the `check:dist`-before-`build` and
  compile-binary-after-`build` orderings.
- **Documentation** is current: `web/README.md`, `README.md`, `CHANGELOG.md`,
  `docs/accessibility.md`, `docs/server.md`, `docs/adr/0006-server-and-security.md`,
  and the PRD-06 build-correctness amendment.

**Two Rust production changes**, both authorized: the CSP constant amendment
(`crates/cratevista-server/src/router.rs`) and the build-correctness amendment
(`crates/cratevista-server/build.rs`). No PRD-08/09/10 functionality was added.

**Known follow-up, not a PRD-07 blocker:**
[`ISSUES/issue_11_source_path_duplication.md`](../ISSUES/issue_11_source_path_duplication.md)
— rustdoc-span source paths duplicate their package prefix
(`crates/cvapp/crates/cvapp/src/lib.rs`). It is a `cratevista-rustdoc` defect, the
explorer operates normally (source is disabled by default and a bad path degrades
to a stable, non-fatal error), and **no frontend workaround was added**.

**Landed and verified (2026-07-16) — browser-verification phase:**

- **Production `web/dist` committed; PRD-06 placeholder removed.** Deterministic repeat builds; `hashCharacters: "hex"` (both `build` and `worker` outputs) so the server's hex-only `is_fingerprinted` rule grants immutable caching — Vite's default base64url hashes would have silently degraded every asset to `no-cache`. Three placeholder-era Rust tests were rewritten against the real bundle, discovering the bundle's hashed names via `Assets::iter()` rather than hard-coding names that change each build. **No npm is invoked from Cargo or `build.rs`.**
- **Second authorized Rust change — `crates/cratevista-server/build.rs`** (see "### PRD-06 build-correctness amendment"), proven by `check:embed-rebuild`, including a **negative control**: with `build.rs` removed the add-file probe fails and the guard exits 1, having still restored the repository.
- **Two real production defects found and fixed in this phase** — the broken production ELK worker and `HttpSourceClient`'s `Illegal invocation`. Both passed every unit test; only a real browser exposed them. (The final phase found two more — the `fitView` overflow and the absolute graph layer covering the reduced-mode banner — for **four in total**; all are detailed under "## Current repository state".)
- **70 real-browser tests pass, zero skipped** (`security` / `worker` / `smoke` / `history` / `a11y` / `layout` / `reduced-mode`), plus **176 frontend unit tests** across 15 files and **239 Rust tests**.

### Additive increment (2026-07-16) — view docs/examples rendering (PRD-08 Amendment C)

> **Status: Implemented / Verified.** PRD 07 remains **Implemented / Verified**; this is an **additive** rendering increment required by PRD 08 (see its "### Amendment C"). No Rust runtime behaviour changed.

Schema `1.1` (PRD-08 Amendment A) added optional `View::docs` / `View::examples`; without a renderer they would be dead data. Added `web/src/components/ViewDocs.tsx`, mounted in the inspector column:

- **`View::description`** — previously unrendered anywhere — is now shown for the active view.
- **`View::docs`** renders through the **existing** `SafeMarkdown` pipeline (`react-markdown` + `remark-gfm` + `rehype-sanitize`; no `rehype-raw`, no `dangerouslySetInnerHTML`, **no new dependency**).
- **`View::examples`** render as native **`<details>`/`<summary>`** disclosures — keyboard-operable and announced as disclosures with no JavaScript and no hand-rolled ARIA that could drift out of sync with the open state. Each shows its title, its `language` **as text** (never colour alone), an optional description, and its `content` in `<pre><code>`.
- **The content is a React text child**, so React escapes it. An example may legitimately contain `<script>` or `</code></pre>` as sample payload; it must appear as characters, never as markup. `language` is a display hint that nothing interprets.
- **No `/api/source` request** is made: contents are embedded in the document, so examples render whether or not the server was started with `--source`.
- **Appearance is preserved** when a view has no description/docs/examples: the component returns `null`, so the eight generated views are unchanged.

Tests: **9 component tests** (`web/tests/view-docs.test.tsx`) covering description/docs/examples rendering, the empty and whitespace-only cases, disclosure + summary focus, and **three hostile-content tests** (scripts/`onerror`/`javascript:`/`iframe` stripped from docs and nothing executed; hostile example content rendered as text with no elements created; example content not treated as Markdown). **3 real-browser tests** (`web/e2e/tests/view-docs.spec.ts`) against the real server and CSP: rendering with **zero `/api/source` requests** and zero CSP violations; keyboard-only disclosure (Enter to open and close); and a regression that the generated views render no panel.

`web/dist` was rebuilt and committed; `check:dist` and `check:embed-rebuild` pass.

**Landed and verified (2026-07-16) — final phase:**

- **Graph/panel layout defect fixed.** Two real defects, both in the same class
  and both found by pointer hit-testing in the browser:
  1. `fitView` on `<ReactFlow>` fits **once, at init** — when every node is still
     at the `(0,0)` placeholder. It zoomed to fit a degenerate point-graph; when
     the real ELK coordinates arrived nothing re-fit, so the graph overflowed its
     `overflow: hidden` viewport (nodes ran to x≈2184 against a 1080-wide region)
     and the clipped parts, including whole edges, could not be clicked.
     `useFitOnLayout` now re-fits whenever a fresh layout lands, keyed on the
     `positions` Map identity — selection and inspector changes never touch it,
     and fitting only moves the viewport, never requests a layout.
  2. `.cv-graph` was `position: absolute; inset: 0`, so it **covered the whole
     canvas including the reduced-mode banner**, making the banner's own
     "Render full graph" button unclickable. `.cv-canvas` is now a flex column and
     `.cv-graph` a `flex: 1` child, so banners sit above the graph and stay
     interactive.

  Note: the graph and inspector never actually overlapped — they are disjoint
  grid columns (canvas `0–1080`, inspector `1080–1440`), which `layout.spec.ts`
  now asserts. The unclickable edges were caused by the missing re-fit, not by an
  overlay. `.cv-canvas` also gained a stable `min-width`/`min-height` of 320px.
- **Relation selection is natural.** `layout.spec.ts` clicks the **first** edge's
  midpoint with no searching, and separately asserts that **every** rendered edge
  hit-tests to itself — including the one nearest the inspector. The E2E helper
  was simplified accordingly: it no longer hunts for a conveniently clickable
  edge, which would have hidden exactly this defect.
- **Large-graph benchmark fixtures.** A deterministic generator
  (`web/scripts/gen-benchmark-workspace.mjs`) emits realistic Rust workspaces —
  multiple crates with real dependencies, nested public/private modules, structs
  with fields, enums with variants, traits, trait **and** inherent impls,
  functions/methods, cross-crate type references, and a deliberate
  documented/undocumented mix — not isolated synthetic nodes. Three scales are
  committed as full snapshots (`bench-near`/`bench-at`/`bench-large`), all
  validated through the real server loader.
- **The GraphList accessibility test is no longer skipped.** `reduced-mode.spec.ts`
  serves `bench-large` so reduced mode is entered through the **normal 1,500-node
  policy** (not a test-only budget), and proves keyboard-only operation.
- **Local benchmark instrumentation** (`web/src/app/perf.ts`): User Timing marks
  and an in-memory `window.__cratevistaPerf` hook for local automation. No
  telemetry, no network, no paths, no debug console, and no change to rendering.
- **CI** enforces the full ordered pipeline as one job (see "### Build + dist workflow").
- **Full gates green:** `npm ci`, `generate:types`, `check:types`, `typecheck` (TS7), `typecheck:compat` (TS6), `lint`, `test`, `check:dist`, `build`, `e2e`, `check:embed-rebuild`; and `cargo fmt --check`, `cargo clippy --workspace --all-targets --all-features -D warnings`, `cargo test --workspace --all-features`, `cargo +1.97.0 check --workspace --all-features`. The E2E binary is compiled **after** the final restored production dist, and Playwright byte-compares every served asset against the committed `web/dist` to prove it.

**Landed and verified (2026-07-14):**

- **Authorized Rust change — done & verified.** The single CSP constant in `crates/cratevista-server/src/router.rs` is amended to `… style-src 'self'; style-src-attr 'unsafe-inline'; connect-src 'self'; worker-src 'self'; …` (no `script`/`style` `unsafe-inline`, no `unsafe-eval`, no remote origins); the header test asserts exactly one `unsafe-inline` token (the `style-src-attr` one). `cargo test -p cratevista-server` (10 router tests), `clippy`, and `fmt` pass.
- **Frontend toolchain (Phase 1) — done.** `web/` scaffold (`package.json`, committed `package-lock.json`, `tsconfig.json`, `vite.config.ts` with `base: "./"`, flat ESLint, `index.html`, cross-platform Node scripts). Clean `npm install` (**no** `--legacy-peer-deps`/`--force`). **TypeScript 7 + TS6 compatibility bridge** (see stack section): `typescript`→`@typescript/typescript6` **6.0.2** (compat API reports TS **6.0.3**; provides `tsc6`), `typescript-7`→`typescript` **7.0.2** (authoritative `tsc`). typescript-eslint / parser / eslint-plugin **8.64.0** consume the TS6 compat API. Verified: `npm run typecheck` (TS7) exit 0; `npm run typecheck:compat` (TS6) exit 0; `require("typescript").version` → 6.0.3; `npm run lint` clean (no ERESOLVE / unsupported-version warning). Other approved majors resolved: React 19.2.7, Vite 8.1.4, `@vitejs/plugin-react` 6.0.3, `@xyflow/react` 12.11.2, Zustand 5.0.14, Vitest 4.1.10, Playwright 1.61.1, `json-schema-to-typescript` 15.0.4, `react-markdown` 10.1.0, `remark-gfm` 4.0.1, `rehype-sanitize` 6.0.0, elkjs 0.11.1.
- **Type generation (Phase 2) — done.** `generate:types` / `check:types` produce and drift-guard the committed `web/src/types/generated/explorer-document.ts` (16 types); hand-written `DiagnosticsReport`/`GenerationReport`/`HealthResponse`/`ApiErrorBody` types + runtime guards (reject malformed required fields, tolerate unknown fields).
- **Pure core (Phases 4/6/7) — done & unit-tested.** Immutable `model/` (id/incoming/outgoing/children/by-kind/diagnostics indexes + stable `identity`); pure `adapter/` (`documentToGraph` + `viewEntityIds` + kind→style with generic unknown-kind fallback; edge emitted only when both endpoints visible); `state/selectors` (search by label/qualified-name/tags, legend of present categories, kinds-in-graph); `state/url` (query-string encode/decode with entity/relation mutual exclusivity, default-drop).
- **Zustand store (Phase 7) — done & tested.** UI-only vanilla store (`createUiStore`): `activeViewId`, discriminated-union `selection` (none/entity/relation), search, kind filters, focus, edge mode, active stage, reduced mode, expanded neighborhoods, theme, language; actions `initialize/switchView/select*/clearSelection/setSearch/toggleKind/setFocus/setEdgeMode/setStage/setReducedMode/expandNeighborhood/reset`; `switchView` clears stage (and selection unless kept); `toUrlState` derives durable URL state only. Never mutates the document; no duplicated projected nodes/edges.
- **Concurrent loader (Phase 8) — done & tested.** `loadArtifacts` fetches `/api/{document,generation,diagnostics}` concurrently; `ArtifactLoader` aborts the prior attempt, bumps a monotonic token, and ignores stale results (latest wins). Degrade rules: document failure/malformed → blocking `document-error`; unsupported major → `incompatible`; generation/diagnostics failure → `ok` with `*Available:false`; `partial` surfaced. Documented that cross-request coherence across a future PRD-09 hot swap is PRD 09's concern.
- **SafeMarkdown (Phase 9) — done & tested.** `react-markdown` + `remark-gfm` + `rehype-sanitize` (no `rehype-raw`, no `dangerouslySetInnerHTML`); `<script>`/`onerror`/`javascript:`/encoded-`javascript:`/malformed HTML neutralized; external links hardened with `rel="noopener noreferrer" target="_blank"`; relative links untouched.
- **ELK worker layer (Phase 10) — logic done & unit-tested.** `layout/{types,cache,client,elk.worker}.ts`: the same-origin ES-module worker (`new Worker(new URL("./elk.worker.ts", import.meta.url), { type: "module" })`, no blob) running layered/`RIGHT`/orthogonal ELK with deterministic ordering + stage partitioning (lanes only when stages exist, no kind columns); `LayoutClient` with monotonic request tokens, stale-response discard, an injectable worker seam, a deterministic cache key (identity/view/kinds/focus/related/edgeMode/expanded/stage/nodeIds/edgeIds — **excludes** selection/hover/inspector), and recoverable error handling (worker crash/error → error outcome, never a hung promise). **9 unit tests** via a mock worker. *(The actual in-browser ELK layout still needs the real-server E2E to exercise it end-to-end.)*
- **React application shell + wiring (Phases 3–8) — done & component-tested.** `src/main.tsx`, `src/App.tsx` (loader orchestration + four-region shell), `src/app/AppContext.tsx` (context + `useProjection`/`useLayout`/`useUrlSync` + initial-view/`default_focus` helpers), and components `src/components/{Chrome,Graph,Panels}.tsx` (Toolbar, ViewTabs, Search, KindFilters, StageBar, GraphCanvas + EntityNode + RelationEdge + CanvasControls, Legend, Inspector (entity+relation), GenerationStatus, DiagnosticsPanel, PartialBanner, Loading/Error/Incompatibility/Empty states) + `src/styles.css`. The pure model→view→adapter→`LayoutClient`→React Flow pipeline is wired; the store never mutates the document and holds no projected nodes/edges; layout is keyed by the cache key so selection/inspector/hover never relayout. Loader states (blocking document-error+retry, incompatible-major, generation/diagnostics degraded, partial banner, empty), initial view (URL → `workspace-overview` → first) + gated `default_focus`, popstate view restore, search, kind filters, dynamic legend, unknown-kind rendering, entity/relation selection → inspector, Escape-clears-selection, layout ok/error/stale + fit control, and the stage seam are all covered by a **25-test App component suite** (React Flow mocked; injected `ArtifactSource` + `LayoutEngine`).
- **Fixtures (Phase 13, real generation):** `schema_parity.document.json` (byte copy) + hand-authored `unknown_kinds.document.json` + **`all_views.document.json`** — real `cargo cratevista generate` output (via the pinned nightly) from the checked-in `web/fixtures/sample-workspace/` (2 crates; public/private modules; structs/enums/traits/impls/methods; cross-crate dep + typed relations; documented + undocumented public items → 101 entities / 166 relations / 8 views). A gated **`npm run refresh:fixtures`** regenerates it (nightly-only); normal tests read the committed JSON. Tests: ajv-2020 schema validation of all three fixtures, exact eight-view id set, per-view projection, and eight selectable view tabs.
- **Graph controls + Reset (Phase 6 completion) — done & component-tested.** Fit/zoom-in/zoom-out/reset/zoom-%; `resetView()` (clears search/filters/selection/focus/edge/stage/reduced, keeps the view, then fits); edge modes all/related/hidden (rendered-edge assertions); Related-only (rendered-node assertions); double-click focus.
- **Large-graph reduced mode (Phase 14) — done & tested.** Pure `adapter/reduce.ts` (`reduceGraph`: configurable budget, deterministic focus order, bounded BFS neighborhood, expand, recenter, counts) wired into `useProjection` (default budget `DEFAULT_LARGE_GRAPH_BUDGET = 1500`, overridable) + `ReducedModeBanner`/`GraphList` (counts, Expand, Render-full/Return, complete searchable entity list). 10 unit + 4 component tests.
- **Source-content action (Phase 8 completion) — done & tested.** `api/source.ts` (`SourceClient`/`HttpSourceClient`, injectable): `GET /api/source?path=<encoded repo-relative>` (no line/col params — PRD 06 exposes none), explicit activation only, `AbortController` with abort on selection change / inspector close / unmount, stale responses ignored. `403 source_disabled` → capability absence (location-only, no global error); the five stable codes → specific inline non-fatal messages that never echo server paths/raw text; network/malformed → generic retryable. Contents render in a `<pre><code>` (no `dangerouslySetInnerHTML`); only the repo-relative path is ever shown.
- **URL normalization + history (Phases 6/7 completion) — done & tested.** One `state/normalize.ts` boundary (`chooseView`, `normalizeUrlState`, `differsOnlyBySearch`) applied on **both** initial load and popstate; history semantics: init → `replaceState`, meaningful navigation → `pushState`, search typing → `replaceState`, popstate restoration pushes **no** duplicate (guarded against Zustand↔history loops).
- **Accessibility (Phase 12/15) — implemented with automated + component evidence.** Landmarks, tablist/tab/tabpanel with roving tabindex + Arrow/Home/End, labelled toolbar/filters/search-listbox, Escape semantics, GraphList keyboard alternative, non-color kind encoding, `prefers-reduced-motion` + `:focus-visible` CSS. axe-core clean across 9 states (`color-contrast` is jsdom-impossible → manual; rendered reduced-motion + screen-reader behavior → manual/E2E).
- **Gates green for this scope:** `generate:types` + `check:types` + `typecheck` (TS7) + `typecheck:compat` (TS6) + `lint` (0 errors) pass; **170 Vitest tests** across 14 files pass; Rust regression gates (`fmt`, server `clippy -D warnings`, `cargo test -p cratevista-core -p cratevista-server`) pass with **no Rust production changes**.

*(Historical note: at the end of this phase the production build/`check:dist`, the real-server E2E snapshots and process orchestration, Playwright real-server E2E, the ELK worker's in-browser execution, the large-graph benchmark, CI wiring and the documentation pass were all still outstanding. **All of them subsequently landed** — see the two "Landed and verified (2026-07-16)" ledgers above — and the status is now **Implemented / Verified**.)*

## Review record

- Reviewed at: 2026-07-14
- Result: Changes required → corrections applied in place → **finalized & Approved** (see "### Finalization & approval" below)
- Reviewed against: `CLAUDE.md`, `ISSUES/CONTEXT.md`, the source issue, and the implemented PRDs 02/05/06 code (`crates/cratevista-schema`, `crates/cratevista-graph/src/views.rs`, `crates/cratevista-server`).
- Major findings (all corrected in this PRD):
  - **Stale current state:** claimed "`web/` does not yet exist", but the implemented PRD 06 checks in and embeds a placeholder `web/dist/{index.html,app.js,style.css}` (and un-ignored `web/dist`). Rewrote "Current repository state" and recorded the compile-time embedding / build-ordering constraint.
  - **Untestable "renders every MVP view" criterion:** the schema's `full_mvp.document.json` contains only two views (`view:overview`, `view:types`) whose ids do **not** match the eight generated slugs (`workspace-overview`, `crate-dependencies`, `module-hierarchy`, `types`, `traits-and-impls`, `type-relationships`, `public-api`, `documentation-coverage`). Added a required `all_views.document.json` fixture (generated from a sample workspace, or hand-authored to the same shape) that carries all eight real view ids + an unknown-kind entity; retargeted the tests and acceptance criterion.
  - **Missing CSP compatibility:** PRD 06 serves a strict CSP with **no `unsafe-inline`**; the PRD said nothing about it. Added a CSP subsection + E2E criterion. *(Superseded at finalization: the review's claim that React-Flow's `style`-prop/CSSOM writes are outside CSP was **wrong** — `style-src 'self'` blocks dynamically assigned `style` attributes; the finalization approved a deliberate CSP amendment instead — see below.)*
  - **Markdown/rustdoc XSS:** rendering third-party-crate doc Markdown (which may contain raw HTML) required a sanitization mandate + a hostile-input test; added to Security and acceptance criteria.
  - **Source-capability discovery unspecified:** no endpoint advertises whether `/api/source` is enabled (`/api/health` = `{status, schema_version, partial}` only). Specified probe-and-degrade behavior (`403 source_disabled` → location-only) instead of "active only when enabled".
  - **Codegen scope:** the checked-in JSON Schema is `ExplorerDocument`-only; `DiagnosticsReport`/`GenerationReport` are not schematized. Corrected the schema path (`crates/cratevista-schema/schema/…`) and specified hand-written diagnostics/generation types.
  - **E2E vs hash-verified server:** you cannot `serve` a lone `document.json` (the loader requires a matching `generation.json`+`artifact_hashes`); documented the two valid E2E setups.
  - **Minor:** views tab bar made data-driven (8 generated + manual, not hard-coded 8); cross-platform npm-script requirement added.
- Scope check: no out-of-scope leakage — backend/serving (06), manual-flow authoring/data (08), source editing, schema coordinates, and static-build base-path/ordering (10) remain deferred with only the necessary seams referenced.

### Finalization & approval (2026-07-14)

- Result: **Approved — safe to implement.**
- Decisions locked in: stack + version policy (Node 24, npm/`package-lock.json`, React 19, TS 7, Vite 8, `@xyflow/react` 12, elkjs, Zustand 5, Vitest 4, Playwright 1, `json-schema-to-typescript` 15, `react-markdown` 10 + `remark-gfm` + `rehype-sanitize`; no CSS-in-JS/Redux); the **approved PRD-06 CSP amendment** (`style-src-attr 'unsafe-inline'` + `worker-src 'self'`; no `script`/`style` `unsafe-inline`, no `unsafe-eval`, no remote origins); Zustand UI-only store + immutable document model; codegen (document only) + hand-written diagnostics/generation types with runtime guards; concurrent three-artifact loading with the degrade-rules table; query-string URL state; ELK same-origin module worker with layout tokening/caching; the 1,500-node configurable reduced-mode policy; the three fixtures + real-server snapshot requirement; `react-markdown`/`rehype-sanitize` Markdown security; `web/src`→committed-`web/dist` workflow with `check:types`/`check:dist` and Cargo-never-invokes-npm CI order; WCAG 2.1 AA.
- All four prior open questions resolved (see "## Open questions"). Only remaining item is an **implementation measurement** (benchmarked large-graph default), not a blocker.
- Note for implementers: PRD 07 is authorized to make **one** production-code change outside `web/` — updating the single CSP constant in `crates/cratevista-server/src/router.rs` to the amended policy (and its header test). That is an approved PRD-06 security-policy amendment; no other Rust code changes.
