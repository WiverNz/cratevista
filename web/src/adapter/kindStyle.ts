// Entity/relation kind → visual style, with a generic fallback for unknown kinds
// (issue-02 kinds are open; unknown kinds must render, never crash).
//
// Relation styling lives in the centralized relation-style registry
// (`relationStyle.ts`); this module keeps only entity kinds and re-exposes the
// relation registry through the shared `KindStyle` shape so there is a single
// source of relation-style constants.
import { relationStyleFor } from "./relationStyle.ts";

export interface KindStyle {
  category: string;
  color: string;
  /** Whether this kind was recognized (vs the generic fallback). */
  known: boolean;
}

const ENTITY_STYLES: Record<string, { category: string; color: string }> = {
  workspace: { category: "Workspace", color: "#f74c00" },
  package: { category: "Package", color: "#e0a020" },
  target: { category: "Target", color: "#c08040" },
  module: { category: "Module", color: "#4c8bf7" },
  struct: { category: "Struct", color: "#33b1a3" },
  enum: { category: "Enum", color: "#7a5cf0" },
  union: { category: "Union", color: "#8a6cf0" },
  trait: { category: "Trait", color: "#e055a0" },
  impl: { category: "Impl", color: "#a05ce0" },
  function: { category: "Function", color: "#5cb85c" },
  method: { category: "Method", color: "#7cc87c" },
  type_alias: { category: "Type alias", color: "#60a0a0" },
  constant: { category: "Constant", color: "#9aa4b2" },
  static: { category: "Static", color: "#8a94a2" },
  macro: { category: "Macro", color: "#d08030" },
  external_system: { category: "External system", color: "#b05050" },
  infrastructure: { category: "Infrastructure", color: "#5070b0" },
  stage: { category: "Stage", color: "#6a7a8a" },
  manual_block: { category: "Manual block", color: "#7a8a6a" },
};

const GENERIC_COLOR = "#9aa4b2";

export function entityStyle(kind: string): KindStyle {
  const s = ENTITY_STYLES[kind];
  return s
    ? { ...s, known: true }
    : { category: kind, color: GENERIC_COLOR, known: false };
}

/** Relation style in the shared `KindStyle` shape, sourced from the central
 *  relation-style registry (no duplicated relation constants live here). */
export function relationStyle(kind: string): KindStyle {
  const s = relationStyleFor(kind);
  return { category: s.label, color: `var(${s.strokeToken})`, known: s.known };
}
