// Hand-written types + runtime guards for the artifacts that are NOT covered by
// the checked-in ExplorerDocument JSON Schema (PRD 02 schematizes only the
// document). We validate only the fields the UI consumes and tolerate unknown
// forward-compatible fields; malformed *required* fields are rejected.

export interface DocumentDiagnostic {
  severity: "error" | "warning" | "info";
  code: string;
  message: string;
  entities?: string[];
  relations?: string[];
}

export interface DiagnosticsReport {
  schema_version: string;
  diagnostics: DocumentDiagnostic[];
}

export interface GenerationReport {
  generated_at: string;
  schema_version?: string; // NOTE: generation.json has no schema_version field
  toolchain?: string | null;
  rustdoc_format_version?: number | null;
  partial: boolean;
  // other fields exist but the UI does not depend on them
}

export interface HealthResponse {
  status: string;
  schema_version: string;
  partial: boolean;
}

export interface ApiErrorBody {
  error: { code: string; message: string };
}

function isObject(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

function isStringArray(value: unknown): value is string[] {
  return Array.isArray(value) && value.every((v) => typeof v === "string");
}

/** Rejects malformed required fields; ignores unknown extra fields. */
export function parseDiagnosticsReport(value: unknown): DiagnosticsReport {
  if (!isObject(value)) throw new TypeError("diagnostics: not an object");
  if (typeof value.schema_version !== "string")
    throw new TypeError("diagnostics: schema_version missing");
  if (!Array.isArray(value.diagnostics))
    throw new TypeError("diagnostics: diagnostics not an array");
  const diagnostics = value.diagnostics.map((d, i): DocumentDiagnostic => {
    if (!isObject(d)) throw new TypeError(`diagnostics[${i}]: not an object`);
    if (
      d.severity !== "error" &&
      d.severity !== "warning" &&
      d.severity !== "info"
    )
      throw new TypeError(`diagnostics[${i}]: bad severity`);
    if (typeof d.code !== "string" || typeof d.message !== "string")
      throw new TypeError(`diagnostics[${i}]: bad code/message`);
    return {
      severity: d.severity,
      code: d.code,
      message: d.message,
      entities: isStringArray(d.entities) ? d.entities : undefined,
      relations: isStringArray(d.relations) ? d.relations : undefined,
    };
  });
  return { schema_version: value.schema_version, diagnostics };
}

export function parseGenerationReport(value: unknown): GenerationReport {
  if (!isObject(value)) throw new TypeError("generation: not an object");
  if (typeof value.generated_at !== "string")
    throw new TypeError("generation: generated_at missing");
  if (typeof value.partial !== "boolean")
    throw new TypeError("generation: partial missing");
  return {
    generated_at: value.generated_at,
    partial: value.partial,
    toolchain: typeof value.toolchain === "string" ? value.toolchain : null,
    rustdoc_format_version:
      typeof value.rustdoc_format_version === "number"
        ? value.rustdoc_format_version
        : null,
  };
}

export function parseHealth(value: unknown): HealthResponse {
  if (!isObject(value)) throw new TypeError("health: not an object");
  if (
    typeof value.status !== "string" ||
    typeof value.schema_version !== "string" ||
    typeof value.partial !== "boolean"
  )
    throw new TypeError("health: malformed");
  return {
    status: value.status,
    schema_version: value.schema_version,
    partial: value.partial,
  };
}

/** Extracts a stable error code from an API JSON error body, if present. */
export function apiErrorCode(value: unknown): string | undefined {
  if (isObject(value) && isObject(value.error)) {
    const code = value.error.code;
    if (typeof code === "string") return code;
  }
  return undefined;
}
