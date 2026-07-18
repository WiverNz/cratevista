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
export const BaseEdge = () => null;

export const Position = {
  Left: "left",
  Right: "right",
  Top: "top",
  Bottom: "bottom",
} as const;

export function getStraightPath(): [string, number, number] {
  return ["M0,0", 0, 0];
}

const flow = {
  fitView: () => controlCalls.push("fitView"),
  zoomIn: () => controlCalls.push("zoomIn"),
  zoomOut: () => controlCalls.push("zoomOut"),
  setViewport: () => controlCalls.push("setViewport"),
};
export const useReactFlow = () => flow;
export const useViewport = () => ({ x: 0, y: 0, zoom: 1 });
