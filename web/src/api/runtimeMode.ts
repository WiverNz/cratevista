// Runtime mode: whether the app is served by a live CrateVista server (with
// `/api/*`, health, events and opt-in source) or as an immutable static export
// (files only, no server behind them).
//
// The mode is decided **once**, from the marker the Rust materializer injects into
// the copied `index.html` (Decision 4). It is never inferred from a hostname, a
// query parameter, an environment variable, a Vite define, a build flag or a
// failed request — those would all be either mutable, guessable, or a fetch that
// static mode must never make. A CSP-safe `<meta>` element is the whole signal.

/** The one runtime mode. */
export type RuntimeMode = "server" | "static";

/** The marker the static builder injects: `<meta name="cratevista-mode" content="static" />`. */
export const MODE_META_NAME = "cratevista-mode";
/** The only recognized static value. */
export const STATIC_MODE_VALUE = "static";

/**
 * Detects the runtime mode from `document`'s head, purely and testably.
 *
 * Static mode requires an **exact** `<meta name="cratevista-mode" content="static">`.
 * Anything else — no marker, an unrelated meta, or `cratevista-mode` with any other
 * content — is server mode, which is the embedded server's marker-free index.
 */
export function detectRuntimeMode(doc: Document): RuntimeMode {
  const meta = doc.querySelector(`meta[name="${MODE_META_NAME}"]`);
  const content = meta?.getAttribute("content");
  return content === STATIC_MODE_VALUE ? "static" : "server";
}
