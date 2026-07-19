// Project-aware browser-tab title. The title is derived ONLY from the loaded
// CrateVista document's authoritative project name — never from the URL, filesystem
// path, hostname, a query parameter, or repository metadata. Before any document
// loads (and whenever there is no valid project name) the fallback is `CrateVista`,
// matching the embedded `<title>CrateVista</title>`.

const FALLBACK_TITLE = "CrateVista";

/**
 * The tab title for a project: `CV · <name>` with surrounding whitespace trimmed,
 * or the plain `CrateVista` fallback when the name is missing or whitespace-only
 * (no separator is rendered for an empty name).
 */
export function projectTabTitle(projectName: string | null | undefined): string {
  const trimmed = (projectName ?? "").trim();
  return trimmed ? `CV · ${trimmed}` : FALLBACK_TITLE;
}

/**
 * Sets `document.title` from the project name as plain text (never HTML). Idempotent:
 * it writes only when the computed title actually differs, so repeated loads/reloads
 * with the same project name cause no update.
 */
export function applyProjectTitle(projectName: string | null | undefined): void {
  const next = projectTabTitle(projectName);
  if (document.title !== next) {
    document.title = next;
  }
}
