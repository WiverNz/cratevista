// Entity/relation kind → visual style, with a generic fallback for unknown kinds
// (issue-02 kinds are open; unknown kinds must render, never crash).

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

const RELATION_STYLES: Record<string, { category: string; color: string }> = {
  contains: { category: "contains", color: "#4a5568" },
  depends_on: { category: "depends on", color: "#e0a020" },
  implements: { category: "implements", color: "#e055a0" },
  implemented_for: { category: "implemented for", color: "#a05ce0" },
  has_field_type: { category: "has field type", color: "#33b1a3" },
  accepts_type: { category: "accepts type", color: "#5cb85c" },
  returns_type: { category: "returns type", color: "#5c9bf7" },
  error_type: { category: "error type", color: "#b05050" },
  re_exports: { category: "re-exports", color: "#7a5cf0" },
  imports: { category: "imports", color: "#60a0a0" },
  references_type: { category: "references type", color: "#8a94a2" },
  manual: { category: "manual", color: "#7a8a6a" },
};

const GENERIC_COLOR = "#9aa4b2";

export function entityStyle(kind: string): KindStyle {
  const s = ENTITY_STYLES[kind];
  return s
    ? { ...s, known: true }
    : { category: kind, color: GENERIC_COLOR, known: false };
}

export function relationStyle(kind: string): KindStyle {
  const s = RELATION_STYLES[kind];
  return s
    ? { ...s, known: true }
    : { category: kind, color: GENERIC_COLOR, known: false };
}
