// ELK layout Web Worker (same-origin ES module). Receives a LayoutRequest,
// runs a deterministic layered/RIGHT/orthogonal layout, and posts a
// token-tagged LayoutResponse. Stage lanes are produced via ELK partitioning
// only when the request carries stages. No hard-coded entity-kind columns.
// `elk.bundled.js` cannot be used here. Its default export is the Node variant,
// whose fallback worker factory reads `require("./elk-worker.min.js").Worker` —
// but elk-worker only exports that when it believes it is NOT itself the worker
// script (`typeof document === "undefined"`). Inside this worker that guard is
// true, so elk-worker instead self-installs `self.onmessage`, hijacking our own
// message channel and leaving the factory undefined ("o is not a constructor").
// Instead we use the API entry point and hand it elkjs's own worker, which is
// exactly what that guard is written for. Vite emits it as a same-origin,
// fingerprinted asset — never a blob: URL, so `worker-src 'self'` holds.
import ELK from "elkjs/lib/elk-api.js";
import ElkWorker from "elkjs/lib/elk-worker.min.js?worker";
import type { LayoutRequest, LayoutResponse, PositionedNode, RoutedEdge } from "./types.ts";
import { sectionsToPoints } from "./routes.ts";

interface ElkChild {
  id: string;
  width?: number;
  height?: number;
  x?: number;
  y?: number;
  layoutOptions?: Record<string, string>;
}
interface ElkEdgeSection {
  // Only ever read back from ELK's result (which always assigns an id); we never
  // construct sections on the request side.
  id: string;
  startPoint: { x: number; y: number };
  endPoint: { x: number; y: number };
  bendPoints?: { x: number; y: number }[];
}
interface ElkEdge {
  id: string;
  sources: string[];
  targets: string[];
  sections?: ElkEdgeSection[];
}
interface ElkGraph {
  id: string;
  layoutOptions?: Record<string, string>;
  children: ElkChild[];
  edges: ElkEdge[];
  width?: number;
  height?: number;
}

const elk = new ELK({ workerFactory: () => new ElkWorker() });

function toElkGraph(request: LayoutRequest): ElkGraph {
  const spacing = String(request.options.spacing);
  const layoutOptions: Record<string, string> = {
    "elk.algorithm": "layered",
    "elk.direction": "RIGHT",
    "elk.edgeRouting": "ORTHOGONAL",
    "elk.spacing.nodeNode": spacing,
    "elk.layered.spacing.nodeNodeBetweenLayers": spacing,
    // Deterministic ordering (no randomized crossing minimization seed).
    "elk.layered.crossingMinimization.strategy": "LAYER_SWEEP",
    "elk.layered.considerModelOrder.strategy": "NODES_AND_EDGES",
  };
  const stageOrder = new Map<string, number>();
  if (request.stages && request.stages.length > 0) {
    layoutOptions["elk.partitioning.activate"] = "true";
    for (const s of request.stages) stageOrder.set(s.id, s.order);
  }

  // Stable input ordering (already id-sorted upstream, but re-assert here).
  const children: ElkChild[] = [...request.nodes]
    .sort((a, b) => a.id.localeCompare(b.id))
    .map((n) => {
      const child: ElkChild = { id: n.id, width: n.width, height: n.height };
      const stageId = request.nodeStage?.[n.id];
      if (stageId && stageOrder.has(stageId)) {
        child.layoutOptions = {
          "elk.partitioning.partition": String(stageOrder.get(stageId)),
        };
      }
      return child;
    });

  const edges: ElkEdge[] = [...request.edges]
    .sort((a, b) => a.id.localeCompare(b.id))
    .map((e) => ({ id: e.id, sources: [e.source], targets: [e.target] }));

  return { id: "root", layoutOptions, children, edges };
}

function fromElk(token: number, laid: ElkGraph): LayoutResponse {
  const nodes: PositionedNode[] = laid.children.map((c) => ({
    id: c.id,
    x: c.x ?? 0,
    y: c.y ?? 0,
    width: c.width ?? 0,
    height: c.height ?? 0,
  }));
  // Join whatever routing sections ELK produced into one ordered polyline. A
  // single section is the common case; a malformed/disconnected set yields `[]`,
  // which the renderer reads as "no route" and draws its computed fallback.
  const edges: RoutedEdge[] = laid.edges.map((e) => ({
    id: e.id,
    points: sectionsToPoints(e.sections),
  }));
  return {
    token,
    ok: true,
    result: { token, nodes, edges, width: laid.width ?? 0, height: laid.height ?? 0 },
  };
}

const ctx = self as unknown as {
  onmessage: ((event: { data: LayoutRequest }) => void) | null;
  postMessage(message: LayoutResponse): void;
};

ctx.onmessage = (event) => {
  const request = event.data;
  elk
    .layout(toElkGraph(request))
    .then((laid) => ctx.postMessage(fromElk(request.token, laid as ElkGraph)))
    .catch((error: unknown) =>
      ctx.postMessage({ token: request.token, ok: false, error: String(error) }),
    );
};

export {};
