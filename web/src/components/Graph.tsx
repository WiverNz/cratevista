// React Flow graph canvas wired to the projection + LayoutClient positions.
import { useEffect } from "react";
import {
  ReactFlow,
  ReactFlowProvider,
  Background,
  Controls,
  Panel,
  Handle,
  Position,
  BaseEdge,
  EdgeLabelRenderer,
  getStraightPath,
  useReactFlow,
  useViewport,
  type Node,
  type Edge,
  type NodeProps,
  type EdgeProps,
} from "@xyflow/react";

import { useApp, useUi, type Projection, type LayoutState } from "../app/AppContext.tsx";
import { mark } from "../app/perf.ts";
import { Legend } from "./Panels.tsx";
import type { GraphNode, GraphEdge } from "../adapter/adapter.ts";

type EntityNodeData = { node: GraphNode; label: string };
type RelationEdgeData = { edge: GraphEdge; label?: string };
type EntityRfNode = Node<EntityNodeData, "entity">;
type RelationRfEdge = Edge<RelationEdgeData, "relation">;

const NODE_WIDTH = 180;
const NODE_HEIGHT = 56;

export function EntityNode({ data, selected }: NodeProps<EntityRfNode>) {
  const { node } = data;
  return (
    <div
      className={`cv-node${selected ? " cv-node-selected" : ""}${node.style.known ? "" : " cv-node-generic"}`}
      style={{ borderColor: node.style.color }}
      aria-label={`${node.kind}: ${node.label}`}
    >
      <Handle type="target" position={Position.Left} />
      <div className="cv-node-title">{node.label}</div>
      <div className="cv-node-kind" style={{ color: node.style.color }}>
        {node.style.category}
        {!node.style.known && " (unknown)"}
      </div>
      <Handle type="source" position={Position.Right} />
    </div>
  );
}

export function RelationEdge({
  id,
  sourceX,
  sourceY,
  targetX,
  targetY,
  data,
  selected,
}: EdgeProps<RelationRfEdge>) {
  const [path, labelX, labelY] = getStraightPath({ sourceX, sourceY, targetX, targetY });
  const label = data?.label;
  return (
    <>
      <BaseEdge id={id} path={path} style={selected ? { strokeWidth: 2 } : undefined} />
      {label && (
        <EdgeLabelRenderer>
          <div
            className="cv-edge-label"
            style={{
              position: "absolute",
              transform: `translate(-50%, -50%) translate(${labelX}px, ${labelY}px)`,
            }}
          >
            {label}
          </div>
        </EdgeLabelRenderer>
      )}
    </>
  );
}

const NODE_TYPES = { entity: EntityNode };
const EDGE_TYPES = { relation: RelationEdge };

function CanvasControls() {
  const { store } = useApp();
  const flow = useReactFlow();
  const { zoom } = useViewport();
  return (
    <div className="cv-canvas-controls" role="group" aria-label="Canvas controls">
      <button type="button" onClick={() => flow.fitView()}>
        Fit
      </button>
      <button type="button" onClick={() => flow.zoomIn()}>
        Zoom in
      </button>
      <button type="button" onClick={() => flow.zoomOut()}>
        Zoom out
      </button>
      <button
        type="button"
        onClick={() => {
          store.getState().resetView();
          flow.setViewport({ x: 0, y: 0, zoom: 1 });
          flow.fitView();
        }}
      >
        Reset
      </button>
      <span className="cv-zoom" aria-live="polite">
        {Math.round(zoom * 100)}%
      </span>
    </div>
  );
}

export function GraphCanvas(props: { projection: Projection; layoutState: LayoutState }) {
  return (
    <ReactFlowProvider>
      <GraphInner {...props} />
    </ReactFlowProvider>
  );
}

/**
 * Re-fits the viewport whenever a fresh layout lands.
 *
 * `fitView` on `<ReactFlow>` only fits once, at init — when every node is still
 * at the (0,0) placeholder. It therefore zooms to fit a degenerate point-graph,
 * and when the real ELK coordinates arrive nothing re-fits: the graph overflows
 * its (overflow-hidden) viewport and the clipped parts, including whole edges,
 * become unreachable. `positions` is a new Map per resolved layout, so it is a
 * precise trigger: selection and inspector changes do not touch it, and fitting
 * only moves the viewport — it never requests a layout.
 */
function useFitOnLayout(layoutState: LayoutState) {
  const flow = useReactFlow();
  const { positions, status } = layoutState;
  useEffect(() => {
    if (status !== "ok" || positions.size === 0) return;
    // Fit after the browser has committed the new node positions.
    const frame = requestAnimationFrame(() => {
      flow.fitView();
      // The graph is laid out, painted and fitted: the first point at which a
      // user can actually read and interact with it.
      mark("cv.firstUsableGraph");
    });
    return () => cancelAnimationFrame(frame);
  }, [flow, status, positions]);
}

function GraphInner({
  projection,
  layoutState,
}: {
  projection: Projection;
  layoutState: LayoutState;
}) {
  const { store } = useApp();
  useFitOnLayout(layoutState);
  const selection = useUi((s) => s.selection);
  const edgeMode = useUi((s) => s.edgeMode);
  const focusId = useUi((s) => s.focusId);
  const selectedEntity = selection.kind === "entity" ? selection.id : null;
  const selectedRelation = selection.kind === "relation" ? selection.id : null;

  const nodes: EntityRfNode[] = projection.graph.nodes.map((n) => {
    const pos = layoutState.positions.get(n.id) ?? { x: 0, y: 0 };
    return {
      id: n.id,
      type: "entity",
      position: { x: pos.x, y: pos.y },
      selected: n.id === selectedEntity,
      width: NODE_WIDTH,
      height: NODE_HEIGHT,
      data: { node: n, label: n.label },
    };
  });

  const anchor = selectedEntity ?? focusId ?? null;
  const visibleEdges =
    edgeMode === "hidden"
      ? []
      : edgeMode === "related" && anchor
        ? projection.graph.edges.filter((e) => e.source === anchor || e.target === anchor)
        : projection.graph.edges;
  const edges: RelationRfEdge[] = visibleEdges.map((e) => ({
    id: e.id,
    source: e.source,
    target: e.target,
    type: "relation",
    selected: e.id === selectedRelation,
    data: { edge: e, label: e.label ?? e.kind },
  }));

  return (
    <div className="cv-graph">
      {layoutState.status === "loading" && (
        <div className="cv-graph-status" role="status">
          Computing layout…
        </div>
      )}
      {layoutState.status === "error" && (
        <div className="cv-graph-status cv-error" role="alert">
          Layout failed.{" "}
          <button type="button" onClick={() => layoutState.retry()}>
            Retry layout
          </button>
        </div>
      )}
      <ReactFlow
        nodes={nodes}
        edges={edges}
        nodeTypes={NODE_TYPES}
        edgeTypes={EDGE_TYPES}
        onNodeClick={(_, node) => store.getState().selectEntity(node.id)}
        onNodeDoubleClick={(_, node) => store.getState().setFocus(node.id, true)}
        onEdgeClick={(_, edge) => store.getState().selectRelation(edge.id)}
        fitView
        proOptions={{ hideAttribution: true }}
      >
        <Background />
        <Controls />
        <Panel position="top-left">
          <CanvasControls />
        </Panel>
        <Panel position="bottom-left">
          <Legend entries={projection.legend} />
        </Panel>
      </ReactFlow>
    </div>
  );
}
