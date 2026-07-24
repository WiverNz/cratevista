// Authored architectural-role parsing + the centralized role-style registry
// (Issue 15, Phase 4).
//
// Role is an ADDITIVE presentation layer authored by a maintainer via
// `attributes.category` (see `[[override]].category` in cratevista-config). It is
// never inferred from a kind, label, qualified name, module path, dependency or
// relation. This module is the single place that reads the raw attribute and the
// single source of a role's label, colour token, decorative cue and known/unknown
// status — no role switch statements are duplicated in components or CSS helpers.

/** The exact, locked lowercase role vocabulary. Adding/removing a role is a change
 *  here and nowhere else. `data-store` is deliberately NOT a member — `database`
 *  is the canonical key. */
export const KNOWN_ROLES = [
  "service",
  "client",
  "database",
  "cache",
  "observability",
  "external",
  "infra",
  "shared",
  "domain",
] as const;

export type KnownRole = (typeof KNOWN_ROLES)[number];

/** The bounded set of non-colour decorative cues a card can carry. */
export type RoleCue =
  | "band" // solid inset top band (service)
  | "corner-tab" // rounded inner corner tab (client)
  | "stacked-lines" // stacked curved lines suggesting storage layers (database)
  | "stacked-bars" // short stacked horizontal bars (cache)
  | "double-border" // inner double border (observability)
  | "dashed-border" // inner dashed border (external)
  | "corner-bracket" // corner-bracket marks (infra)
  | "dashed-band" // dashed inset top band (shared)
  | "top-rule" // stronger solid inner top rule (domain)
  | "neutral"; // neutral dotted inner cue (unknown)

/** A fully-resolved role presentation. Immutable; shared by the card model, the
 *  registry consumers and (through its token) the stylesheet. */
export interface RoleStyle {
  /** The known role key, or `"unknown"` for a non-empty unrecognised value. */
  readonly key: KnownRole | "unknown";
  /** CSS custom-property name for the role colour (dark + light aware). */
  readonly token: string;
  /** User-facing label. For a known role this is the concise name (e.g. "Service");
   *  for an unknown role it is the trimmed authored value verbatim. */
  readonly label: string;
  /** The deterministic non-colour decorative cue. */
  readonly cue: RoleCue;
  /** Whether the authored value matched the locked vocabulary exactly. */
  readonly known: boolean;
}

/** The minimal shape the parser needs; values are unknown at runtime. */
export type AttributeBag = Readonly<Record<string, unknown>> | null | undefined;

/**
 * The single, strict parser for the authored role value.
 *
 * Returns the trimmed authored string, or `undefined` when there is no usable
 * value. It accepts ONLY a runtime string: a boolean, number, array, object or
 * `null` — and a whitespace-only / empty string — are all treated as missing. It
 * never lowercases or otherwise normalises the value (an incorrectly-cased authored
 * value stays as authored and is later classified as unknown), and never throws.
 */
export function authoredRole(attributes: AttributeBag): string | undefined {
  const value: unknown = attributes ? attributes["category"] : undefined;
  if (typeof value !== "string") return undefined;
  const trimmed = value.trim();
  return trimmed === "" ? undefined : trimmed;
}

const KNOWN: Record<KnownRole, { label: string; cue: RoleCue }> = {
  service: { label: "Service", cue: "band" },
  client: { label: "Client", cue: "corner-tab" },
  database: { label: "Database", cue: "stacked-lines" },
  cache: { label: "Cache", cue: "stacked-bars" },
  observability: { label: "Observability", cue: "double-border" },
  external: { label: "External", cue: "dashed-border" },
  infra: { label: "Infrastructure", cue: "corner-bracket" },
  shared: { label: "Shared", cue: "dashed-band" },
  domain: { label: "Domain", cue: "top-rule" },
};

const KNOWN_SET = new Set<string>(KNOWN_ROLES);

/**
 * Resolves an already-parsed authored value to its role style.
 *
 * - `undefined` (missing) → `undefined`: the card carries no role badge or cue and
 *   stays the polished kind-based card.
 * - an EXACT lowercase vocabulary match → the known role style.
 * - any other non-empty string (including a wrong-case "Service") → the neutral
 *   unknown style, carrying the authored value verbatim as its label; the value is
 *   preserved, never discarded, and no colour is fabricated from the string.
 */
export function roleStyleFor(authored: string | undefined): RoleStyle | undefined {
  if (authored === undefined) return undefined;
  if (KNOWN_SET.has(authored)) {
    const key = authored as KnownRole;
    return { key, token: `--role-${key}`, label: KNOWN[key].label, cue: KNOWN[key].cue, known: true };
  }
  return { key: "unknown", token: "--role-unknown", label: authored, cue: "neutral", known: false };
}
