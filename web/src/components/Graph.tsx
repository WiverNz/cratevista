// React Flow graph canvas wired to the projection + LayoutClient positions.
import { useEffect, useState } from "react";
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
import {
  allRelationStyles,
  edgeVisual,
  edgeZIndex,
  markerId,
  relationStyleFor,
  shouldShowEdgeLabel,
  type EdgeState,
} from "../adapter/relationStyle.ts";

type EntityNodeData = { node: GraphNode; label: string };
type RelationEdgeData = {
  edge: GraphEdge;
  label?: string;
  state: EdgeState;
  repeated: boolean;
  hovered?: boolean;
};
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

/**
 * A relation edge, styled entirely from the central relation-style registry.
 *
 * Stroke colour token, width, dash pattern, opacity and the directional arrow
 * marker all come from `relationStyleFor(kind)` resolved for the edge's
 * interaction `state` (normal / related / selected / faded). The label carries a
 * readable halo and is shown or hidden by `shouldShowEdgeLabel`, so repeated
 * labels stop forming a wall at low zoom yet stay reachable on hover, selection
 * or a useful zoom level.
 */
export function RelationEdge({
  id,
  sourceX,
  sourceY,
  targetX,
  targetY,
  data,
}: EdgeProps<RelationRfEdge>) {
  const [path, labelX, labelY] = getStraightPath({ sourceX, sourceY, targetX, targetY });
  const { zoom } = useViewport();

  const edge = data?.edge;
  const state: EdgeState = data?.state ?? "normal";
  const style = relationStyleFor(edge?.kind ?? "");
  const visual = edgeVisual(style, state);
  const marker = markerId(style);
  const label = data?.label;
  const showLabel =
    !!label &&
    shouldShowEdgeLabel({ zoom, state, hovered: !!data?.hovered, repeated: !!data?.repeated });

  return (
    <>
      <BaseEdge
        id={id}
        path={path}
        markerEnd={marker ? `url(#${marker})` : undefined}
        style={{
          stroke: visual.stroke,
          strokeWidth: visual.strokeWidth,
          strokeDasharray: visual.strokeDasharray,
          opacity: visual.opacity,
        }}
      />
      {showLabel && (
        <EdgeLabelRenderer>
          {/* pointer-events:none (via CSS) so the label never intercepts clicks
              meant for the edge underneath it. Hover is detected on the edge
              itself via React Flow's onEdgeMouseEnter. */}
          <div
            className={`cv-edge-label cv-edge-label-${state}`}
            style={{
              position: "absolute",
              transform: `translate(-50%, -50%) translate(${labelX}px, ${labelY}px)`,
              color: visual.labelFg,
              background: visual.labelBg,
            }}
          >
            {label}
          </div>
        </EdgeLabelRenderer>
      )}
    </>
  );
}

/** The shared arrow-marker `<defs>`, rendered once per canvas. Each recognized
 *  relation (and the neutral unknown fallback) gets one marker coloured by its
 *  stroke token, so markers stay in sync with edge colours in dark and light. */
export function EdgeMarkerDefs() {
  return (
    <svg className="cv-edge-defs" aria-hidden="true" width="0" height="0" focusable="false">
      <defs>
        {allRelationStyles().map((style) => {
          const id = markerId(style);
          if (!id) return null;
          return (
            <marker
              key={id}
              id={id}
              viewBox="0 0 8 8"
              refX="7"
              refY="4"
              markerWidth="7"
              markerHeight="7"
              orient="auto"
              markerUnits="userSpaceOnUse"
            >
              <path
                d={style.marker === "arrow-closed" ? "M0,0 L8,4 L0,8 Z" : "M0,0 L8,4 L0,8 L2.5,4 Z"}
                style={{ fill: `var(${style.strokeToken})` }}
              />
            </marker>
          );
        })}
      </defs>
    </svg>
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
  const [hoveredEdge, setHoveredEdge] = useState<string | null>(null);
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

  // A label is "repeated" when its text appears on more than one visible edge —
  // these are the labels that pile into a wall, so they are the ones the zoom
  // rule thins out first.
  const labelFrequency = new Map<string, number>();
  for (const e of visibleEdges) {
    const text = e.label ?? e.kind;
    labelFrequency.set(text, (labelFrequency.get(text) ?? 0) + 1);
  }

  // Selecting a node emphasizes the relations touching it and fades the rest;
  // with no anchor every edge draws normally.
  const edgeState = (e: GraphEdge): EdgeState => {
    if (e.id === selectedRelation) return "selected";
    if (!anchor) return "normal";
    return e.source === anchor || e.target === anchor ? "related" : "faded";
  };

  const edges: RelationRfEdge[] = visibleEdges.map((e) => {
    const state = edgeState(e);
    const text = e.label ?? e.kind;
    return {
      id: e.id,
      source: e.source,
      target: e.target,
      type: "relation",
      selected: e.id === selectedRelation,
      zIndex: edgeZIndex(relationStyleFor(e.kind), state),
      data: {
        edge: e,
        label: text,
        state,
        repeated: (labelFrequency.get(text) ?? 0) > 1,
        hovered: e.id === hoveredEdge,
      },
    };
  });

  return (
    <div className="cv-graph">
      <EdgeMarkerDefs />
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
        onEdgeMouseEnter={(_, edge) => setHoveredEdge(edge.id)}
        onEdgeMouseLeave={() => setHoveredEdge(null)}
        fitView
        proOptions={{ hideAttribution: true }}
      >
        <Background />
        <Controls />
        <Panel position="top-left">
          <CanvasControls />
        </Panel>
        <Panel position="bottom-left">
          <Legend entries={projection.legend} relations={projection.relationLegend} />
        </Panel>
      </ReactFlow>
    </div>
  );
}
