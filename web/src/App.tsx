// Top-level app: drives artifact loads, renders blocking/degraded states, and
// assembles the four-region shell once a valid document loads.
//
// # Initial load and reload are not the same failure
//
// With nothing rendered, a failed load is fatal: there is nothing to show, so the
// blocking `ErrorState` is the honest answer. Once a document is on screen the
// same failure means something much smaller — the *newer* document could not be
// fetched — and throwing away a working graph to say so would destroy the user's
// place in it to report a problem they cannot act on. So a reload failure is a
// banner, the last good snapshot stays, and the next success clears it.
import { useCallback, useEffect, useRef, useState } from "react";

import {
  ServerArtifactSource,
  StaticArtifactSource,
  type ArtifactSource,
} from "./api/load.ts";
import {
  LiveReload,
  type GenerationFailure,
  type LiveReloadHandlers,
  type LiveReloadOptions,
} from "./api/liveReload.ts";
import { HttpSourceClient, type SourceClient } from "./api/source.ts";
import { detectRuntimeMode, type RuntimeMode } from "./api/runtimeMode.ts";
import { LayoutClient, type LayoutEngine } from "./layout/client.ts";
import { buildModel } from "./model/model.ts";
import { measure, record } from "./app/perf.ts";
import { createUiStore, toUrlState } from "./state/store.ts";
import { parseUrlState, serializeUrlState } from "./state/url.ts";
import { normalizeUrlState } from "./state/normalize.ts";
import {
  AppProvider,
  DEFAULT_LARGE_GRAPH_BUDGET,
  applyDefaultFocus,
  chooseInitialView,
  useApp,
  useLayout,
  useProjection,
  useUi,
  useUrlSync,
  type AppData,
} from "./app/AppContext.tsx";
import {
  GRAPH_PANEL_ID,
  StageBar,
  Toolbar,
  ViewTabs,
  viewTabId,
} from "./components/Chrome.tsx";
import { GraphCanvas } from "./components/Graph.tsx";
import { ViewDocs } from "./components/ViewDocs.tsx";
import {
  DiagnosticsPanel,
  EmptyState,
  ErrorState,
  GenerationFailedBanner,
  GenerationStatus,
  GraphList,
  IncompatibilityState,
  Inspector,
  LoadingState,
  PartialBanner,
  ReducedModeBanner,
  RegeneratingIndicator,
  ReloadErrorBanner,
} from "./components/Panels.tsx";

type Phase =
  | { kind: "loading" }
  | { kind: "error"; message: string }
  | { kind: "incompatible"; found: string }
  | { kind: "ready"; data: AppData };

/** Transient watch-mode state. None of it can hide or replace the graph. */
interface WatchState {
  regenerating: boolean;
  generationError: GenerationFailure | null;
  reloadError: string | null;
}

const NO_WATCH_STATE: WatchState = {
  regenerating: false,
  generationError: null,
  reloadError: null,
};

/** Shown when three attempts all saw a snapshot swap mid-load. Deliberately not
 *  phrased as a fault: nothing is broken, the workspace is simply changing faster
 *  than it can be read. */
const INCOHERENT_MESSAGE =
  "The document changed while it was being loaded. Retrying did not settle it.";

/** The live-reload lifecycle the app drives. `LiveReload` implements it; static
 *  mode never constructs one. */
export interface LiveReloadLike {
  start(): Promise<boolean>;
  dispose(): void;
}

export interface AppProps {
  source?: ArtifactSource;
  layout?: LayoutEngine;
  /** Overrides `window.location.search` in tests. */
  initialSearch?: string;
  /** Large-graph reduced-mode budget (default 1,500). */
  budget?: number;
  /** Injectable source-content client (defaults to the real `/api/source` in
   *  server mode). Static mode never constructs one. */
  sourceClient?: SourceClient;
  /** Injectable live-reload transport (defaults to the real `/api/health` +
   *  `EventSource`). Tests pass fakes; nothing else sets this. */
  liveReload?: LiveReloadOptions;
  /** The runtime mode. Defaults to detecting the injected static marker; tests set
   *  it explicitly. Read **once**, before any source/live-reload construction. */
  mode?: RuntimeMode;
  /** Injectable live-reload factory, so tests can prove it is constructed exactly
   *  once in server mode and **never** in static mode. */
  liveReloadFactory?: (
    handlers: LiveReloadHandlers,
    options?: LiveReloadOptions,
  ) => LiveReloadLike;
}

export function App(props: AppProps) {
  // The mode is decided ONCE, before any source, source client or live-reload is
  // constructed, so a static export never wires up a single `/api` capability.
  const [mode] = useState<RuntimeMode>(() => props.mode ?? detectRuntimeMode(document));
  const [source] = useState<ArtifactSource>(
    () =>
      props.source ??
      (mode === "static" ? new StaticArtifactSource() : new ServerArtifactSource()),
  );
  const [layout] = useState<LayoutEngine>(() => props.layout ?? new LayoutClient());
  // Static mode has no source-content capability at all: there is no `/api/source`
  // to call, so no client is constructed and the inspector renders no source action.
  const [sourceClient] = useState<SourceClient | null>(() =>
    mode === "static" ? null : (props.sourceClient ?? new HttpSourceClient()),
  );
  // Captured once, like `source` and `layout` above. Read as a prop it would be a
  // fresh object on every parent render, and the effect below would tear down the
  // stream and open a new one each time — one EventSource per mounted app is the
  // contract, not one per render.
  const [liveReloadOptions] = useState<LiveReloadOptions | undefined>(() => props.liveReload);
  const [liveReloadFactory] = useState(
    () =>
      props.liveReloadFactory ??
      ((handlers: LiveReloadHandlers, options?: LiveReloadOptions): LiveReloadLike =>
        new LiveReload(handlers, options)),
  );
  const [phase, setPhase] = useState<Phase>({ kind: "loading" });
  const [watch, setWatch] = useState<WatchState>(NO_WATCH_STATE);
  // What is currently rendered. A ref rather than state because `load` must read
  // it without being rebuilt every time a document commits — and rebuilding `load`
  // would restart the live-reload effect and open a second EventSource.
  const rendered = useRef<AppData | null>(null);

  // Async load: the first `setPhase` happens after `await`, so it never runs
  // synchronously inside the mount effect.
  const load = useCallback(async () => {
    const fetchStarted = performance.now();
    const outcome = await source.load();
    // A newer load superseded this one. It commits nothing and — this is the part
    // that matters — reports nothing either: a stale failure must never replace a
    // newer success with an error banner.
    if ("stale" in outcome) return;
    record("cv.artifacts.load", performance.now() - fetchStarted);

    const current = rendered.current;
    const failure =
      outcome.status === "document-error"
        ? outcome.message
        : outcome.status === "incoherent-snapshot"
          ? INCOHERENT_MESSAGE
          : outcome.status === "incompatible"
            ? `Unsupported document schema ${outcome.found}.`
            : null;

    if (failure !== null) {
      // Something is already rendered: keep it. The graph, its layout, the
      // viewport and the selection are all still valid — they describe the
      // snapshot that *did* load.
      if (current) {
        setWatch((w) => ({ ...w, regenerating: false, reloadError: failure }));
        return;
      }
      if (outcome.status === "incompatible") {
        setPhase({ kind: "incompatible", found: outcome.found });
        return;
      }
      setPhase({ kind: "error", message: failure });
      return;
    }
    if (outcome.status !== "ok") return;

    const model = measure(
      "cv.model.build",
      () => buildModel(outcome.document, outcome.diagnostics),
      { entities: outcome.document.entities.length, relations: outcome.document.relations.length },
    );

    // The store is REUSED across reloads. It holds the selection, the filters and
    // the active view; recreating it would silently reset the user's place in the
    // graph on every rebuild, which is exactly what watch mode must not do. It is
    // re-pointed only when the new document invalidates what it names.
    let store = current?.store;
    if (!store) {
      store = createUiStore();
      // Normalize the requested URL against the real document before it can reach
      // the store, then record the normalized form with replaceState (no history
      // entry for initialization).
      const url = normalizeUrlState(
        parseUrlState(props.initialSearch ?? window.location.search),
        model,
      );
      store.getState().initialize({ activeViewId: chooseInitialView(model, url), url });
      applyDefaultFocus(store, model, url);
      window.history.replaceState(
        null,
        "",
        serializeUrlState(toUrlState(store.getState())) || window.location.pathname,
      );
    } else {
      // The regenerated document may no longer contain the view being looked at
      // (a crate was deleted, a flow renamed). Re-normalizing against the new
      // model repoints only what has actually become dangling and leaves
      // everything else — including the viewport — untouched.
      const active = store.getState().activeViewId;
      if (!active || !model.viewById.has(active)) {
        const url = normalizeUrlState(toUrlState(store.getState()), model);
        store.getState().initialize({ activeViewId: chooseInitialView(model, url), url });
      }
    }

    const data: AppData = {
      store,
      model,
      generation: outcome.generation,
      generationAvailable: outcome.generationAvailable,
      diagnostics: outcome.diagnostics,
      diagnosticsAvailable: outcome.diagnosticsAvailable,
      budget: props.budget ?? DEFAULT_LARGE_GRAPH_BUDGET,
      sourceClient,
    };
    rendered.current = data;
    setPhase({ kind: "ready", data });
    // A successful load is the only thing that clears the transient watch state:
    // whatever the last error was, it is now answered by a document.
    setWatch(NO_WATCH_STATE);
  }, [source, sourceClient, props.initialSearch, props.budget]);

  const retry = useCallback(() => {
    setPhase({ kind: "loading" });
    void load();
  }, [load]);

  useEffect(() => {
    // `load` is async: any setState runs after `await`, not synchronously here.
    // eslint-disable-next-line react-hooks/set-state-in-effect
    void load();
    return () => {
      source.abort();
      layout.terminate();
    };
  }, [load, source, layout]);

  // Live reload — server mode only. A static export is immutable files with no
  // server behind them: no `/api/health` probe, no `EventSource`, no `/api/events`,
  // no reconnect. The effect returns before constructing anything, so the factory
  // is never called in static mode. In server mode exactly one EventSource exists
  // per mounted app: the dependencies are stable for the component's lifetime, and
  // `dispose()` closes the stream and silences every callback before React drops it.
  useEffect(() => {
    if (mode !== "server") return;
    const live = liveReloadFactory(
      {
        onStarted: () => setWatch((w) => ({ ...w, regenerating: true })),
        // Reuses the one loader, so the reload inherits its token and abort
        // semantics rather than racing them with a second implementation.
        onReload: () => load(),
        // A failed generation wrote nothing, so there is nothing to fetch: the
        // artifacts on disk are the ones already on screen.
        onFailed: (generationError) =>
          setWatch((w) => ({ ...w, regenerating: false, generationError })),
      },
      liveReloadOptions,
    );
    void live.start();
    return () => live.dispose();
  }, [load, liveReloadOptions, mode, liveReloadFactory]);

  if (phase.kind === "loading") return <LoadingState />;
  if (phase.kind === "error")
    return <ErrorState message={phase.message} onRetry={retry} />;
  if (phase.kind === "incompatible")
    return <IncompatibilityState found={phase.found} onRetry={retry} />;

  return (
    <AppProvider value={phase.data}>
      <AppShell layout={layout} watch={watch} />
    </AppProvider>
  );
}

function AppShell(props: { layout: LayoutEngine; watch: WatchState }) {
  const { store, model, generation, generationAvailable } = useApp();
  useUrlSync(store, model);
  const projection = useProjection();
  const layoutState = useLayout(props.layout, projection);
  const partial = generation?.partial ?? false;
  const activeViewId = useUi((s) => s.activeViewId);

  const { regenerating, generationError, reloadError } = props.watch;

  return (
    <div className="cv-shell">
      {/* Every one of these sits above the shell and pushes nothing aside: the
          graph, its layout and its viewport are untouched by any of them. */}
      {regenerating && <RegeneratingIndicator />}
      {generationError && (
        <GenerationFailedBanner
          code={generationError.code}
          message={generationError.message}
        />
      )}
      {reloadError && <ReloadErrorBanner message={reloadError} />}
      {partial && <PartialBanner />}
      <header className="cv-region-toolbar">
        <Toolbar />
      </header>
      <nav className="cv-region-tabs" aria-label="Views">
        <ViewTabs />
      </nav>
      <div className="cv-region-stage">
        <StageBar />
      </div>
      <main className="cv-region-body">
        <section
          className="cv-canvas"
          id={GRAPH_PANEL_ID}
          role="tabpanel"
          tabIndex={-1}
          aria-labelledby={activeViewId ? viewTabId(activeViewId) : undefined}
          aria-label={activeViewId ? undefined : "Architecture graph"}
        >
          {!generationAvailable && <GenerationStatus />}
          {projection && <ReducedModeBanner projection={projection} />}
          {projection && projection.graph.nodes.length > 0 ? (
            <GraphCanvas projection={projection} layoutState={layoutState} />
          ) : (
            <EmptyState />
          )}
        </section>
        <aside className="cv-inspector" aria-label="Details inspector">
          {/* Renders nothing unless the active view carries description/docs/
              examples, so the eight generated views are unchanged. */}
          <ViewDocs />
          {projection && <GraphList projection={projection} />}
          <Inspector />
          <DiagnosticsPanel />
        </aside>
      </main>
    </div>
  );
}
