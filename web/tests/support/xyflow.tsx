// Lightweight @xyflow/react mock for jsdom component tests. Renders each node
// and edge as an accessible button wired to onNodeClick/onEdgeClick, and renders
// panel/control children (Legend, CanvasControls) normally. No real ELK/browser.
import type { ReactNode } from "react";

interface MockNode {
  id: string;
  data?: { label?: string };
}
interface MockEdge {
  id: string;
  data?: { label?: string };
}
interface ReactFlowProps {
  nodes?: MockNode[];
  edges?: MockEdge[];
  onNodeClick?: (event: unknown, node: MockNode) => void;
  onNodeDoubleClick?: (event: unknown, node: MockNode) => void;
  onEdgeClick?: (event: unknown, edge: MockEdge) => void;
  children?: ReactNode;
}

export const controlCalls: string[] = [];

export function ReactFlow(props: ReactFlowProps) {
  return (
    <div data-testid="rf">
      {props.children}
      <div data-testid="rf-nodes">
        {(props.nodes ?? []).map((n) => (
          <button
            key={n.id}
            data-testid={`node-${n.id}`}
            onClick={(e) => props.onNodeClick?.(e, n)}
            onDoubleClick={(e) => props.onNodeDoubleClick?.(e, n)}
          >
            {n.data?.label ?? n.id}
          </button>
        ))}
      </div>
      <div data-testid="rf-edges">
        {(props.edges ?? []).map((ed) => (
          <button
            key={ed.id}
            data-testid={`edge-${ed.id}`}
            onClick={(e) => props.onEdgeClick?.(e, ed)}
          >
            {ed.data?.label ?? ed.id}
          </button>
        ))}
      </div>
    </div>
  );
}

export function ReactFlowProvider({ children }: { children?: ReactNode }) {
  return <>{children}</>;
}
export function Panel({ children }: { children?: ReactNode }) {
  return <div>{children}</div>;
}
export function EdgeLabelRenderer({ children }: { children?: ReactNode }) {
  return <>{children}</>;
}
export const Background = () => null;
export const Controls = () => null;
export const Handle = () => null;

/** Renders the edge path so tests can inspect the registry-derived stroke,
 *  width, dash pattern, opacity and arrow marker of a `RelationEdge`. */
export function BaseEdge(props: {
  id?: string;
  path?: string;
  className?: string;
  markerEnd?: string;
  style?: {
    stroke?: string;
    strokeWidth?: number | string;
    strokeDasharray?: string;
    opacity?: number | string;
  };
}) {
  const s = (props.style ?? {}) as Record<string, unknown>;
  const flowDashVar = s["--edge-flow-dash"];
  const flowCycleVar = s["--edge-flow-dash-cycle"];
  return (
    <path
      data-testid={props.id ? `edgepath-${props.id}` : "edgepath"}
      className={props.className}
      data-d={props.path ?? ""}
      data-classname={props.className ?? ""}
      data-stroke={(s.stroke as string) ?? ""}
      data-width={s.strokeWidth == null ? "" : String(s.strokeWidth)}
      data-dash={(s.strokeDasharray as string) ?? ""}
      data-flowdash={flowDashVar == null ? "" : String(flowDashVar)}
      data-flowcycle={flowCycleVar == null ? "" : String(flowCycleVar)}
      data-opacity={s.opacity == null ? "" : String(s.opacity)}
      data-marker={props.markerEnd ?? ""}
    />
  );
}

export const MarkerType = { Arrow: "arrow", ArrowClosed: "arrowclosed" } as const;

export const Position = {
  Left: "left",
  Right: "right",
  Top: "top",
  Bottom: "bottom",
} as const;

export function getStraightPath(): [string, number, number] {
  return ["M0,0", 0, 0];
}

/** Deterministic stepped path, distinct in shape from a straight line, so tests
 *  can tell "fallback smooth-step was used" from "routed" and from "straight". */
export function getSmoothStepPath(params: {
  sourceX: number;
  sourceY: number;
  targetX: number;
  targetY: number;
}): [string, number, number, number, number] {
  const { sourceX, sourceY, targetX, targetY } = params;
  const midX = (sourceX + targetX) / 2;
  const d = `M${sourceX},${sourceY} L${midX},${sourceY} L${midX},${targetY} L${targetX},${targetY}`;
  return [d, midX, (sourceY + targetY) / 2, 0, 0];
}

const flow = {
  fitView: () => controlCalls.push("fitView"),
  zoomIn: () => controlCalls.push("zoomIn"),
  zoomOut: () => controlCalls.push("zoomOut"),
  setViewport: () => controlCalls.push("setViewport"),
};
export const useReactFlow = () => flow;

/** Mutable zoom so component tests can exercise zoom-dependent label rules. */
let mockZoom = 1;
export function setMockZoom(z: number): void {
  mockZoom = z;
}
export const useViewport = () => ({ x: 0, y: 0, zoom: mockZoom });
