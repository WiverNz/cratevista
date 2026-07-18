// Central type surface: generated ExplorerDocument types + hand-written runtime
// types. Components import from here.
export type {
  ExplorerDocument,
  Entity,
  Relation,
  View,
  Project,
  Stage,
} from "./generated/explorer-document.ts";

export type {
  DocumentDiagnostic,
  DiagnosticsReport,
  GenerationReport,
  HealthResponse,
  ApiErrorBody,
} from "./runtime.ts";

/** Localization-ready text value (may be a plain string in some positions). */
export interface Localized {
  default: string;
  translations?: Record<string, string>;
}

/** Resolves a possibly-localized value to a display string for `lang`. */
export function localized(value: unknown, lang = "en"): string {
  if (typeof value === "string") return value;
  if (value && typeof value === "object") {
    const v = value as Localized;
    if (typeof v.default === "string") {
      const t = v.translations?.[lang];
      return typeof t === "string" ? t : v.default;
    }
  }
  return "";
}
