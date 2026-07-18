// ELK layout worker contract (shared by the client and the worker).

export interface LayoutNode {
  id: string;
  width: number;
  height: number;
  /** Structural parent for nested groups (e.g. module hierarchy), if any. */
  parent?: string;
}

export interface LayoutEdge {
  id: string;
  source: string;
  target: string;
}

export interface StageInfo {
  id: string;
  order: number;
}

export interface LayoutOptions {
  algorithm: "layered";
  direction: "RIGHT";
  edgeRouting: "ORTHOGONAL";
  spacing: number;
}

export const DEFAULT_LAYOUT_OPTIONS: LayoutOptions = {
  algorithm: "layered",
  direction: "RIGHT",
  edgeRouting: "ORTHOGONAL",
  spacing: 60,
};

export interface LayoutRequest {
  token: number;
  nodes: LayoutNode[];
  edges: LayoutEdge[];
  /** Present only when the active view defines stages (→ stage lanes). */
  stages?: StageInfo[];
  /** node id → stage id (only when `stages` present). */
  nodeStage?: Record<string, string>;
  options: LayoutOptions;
}

export interface Point {
  x: number;
  y: number;
}

export interface PositionedNode {
  id: string;
  x: number;
  y: number;
  width: number;
  height: number;
}

export interface RoutedEdge {
  id: string;
  /** Bend points, when the layout produced routing sections. */
  points: Point[];
}

export interface LayoutResult {
  token: number;
  nodes: PositionedNode[];
  edges: RoutedEdge[];
  width: number;
  height: number;
}

export type LayoutResponse =
  | { token: number; ok: true; result: LayoutResult }
  | { token: number; ok: false; error: string };
