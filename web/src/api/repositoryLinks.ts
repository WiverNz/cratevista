// Safe, provider-aware repository and source links.
//
// CrateVista never copies source content into a static site (issue 13 is
// deferred); the most it offers is a *link* to a repository host. That link is
// built only from a repository URL the document declares, and only when that URL
// is a safe, credential-free `https:` URL. Everything is parsed with the standard
// URL API and every path segment is encoded on its own, so no untrusted string is
// ever concatenated into an href.

/** A recognized repository host, or `"other"` for a valid HTTPS host we do not
 *  know how to deep-link into. */
export type RepositoryProvider = "github" | "gitlab" | "other";

export interface RepositoryLinks {
  /** The normalized repository root URL (always safe to render). */
  repository: string;
  /** A deep link to the selected file+line, when the provider and data allow. */
  source?: string;
  provider: RepositoryProvider;
}

/** The minimal project fields a link needs. */
export interface ProjectLike {
  repository_url?: string | null;
  default_branch?: string | null;
}

/** The minimal source-location fields a file link needs. */
export interface SourceLocationLike {
  path: string;
  span?: { start_line?: number | null } | null;
}

/** Recognizes the provider from an exact host. Unknown hosts get root-only links —
 *  we never guess a self-hosted GitHub/GitLab deep-link layout. */
function providerOf(host: string): RepositoryProvider {
  if (host === "github.com") return "github";
  if (host === "gitlab.com") return "gitlab";
  return "other";
}

/**
 * Parses and normalizes a repository URL into a safe root, or `null` when it is
 * not a safe `https:` URL.
 *
 * Rejected: any non-`https:` scheme (`ssh:`, `git:`, `file:`, `http:`), a URL
 * carrying username/password credentials, an empty host, an empty repository path,
 * and anything the URL parser cannot parse (so `git@…`, relative and malformed
 * inputs are all excluded). Normalized: one trailing slash removed, and a single
 * trailing `.git` stripped from the path.
 */
function normalizeRepository(raw: string): { root: string; host: string } | null {
  let url: URL;
  try {
    url = new URL(raw);
  } catch {
    return null;
  }
  if (url.protocol !== "https:") return null;
  // Credentials in a URL are a phishing/exfiltration vector; never render them.
  if (url.username !== "" || url.password !== "") return null;
  if (url.hostname === "") return null;

  // Work from pathname only (drop any query/fragment the source URL carried).
  let path = url.pathname;
  // Strip a single trailing slash, then a single trailing `.git`.
  if (path.endsWith("/")) path = path.slice(0, -1);
  if (path.endsWith(".git")) path = path.slice(0, -".git".length);
  // A repository needs a non-empty path (`https://github.com/` alone is not one).
  if (path === "" || path === "/") return null;

  const root = `https://${url.host}${path}`;
  return { root, host: url.hostname };
}

/** Encodes a branch as a single path value: slashes and reserved characters are
 *  percent-encoded so a `feature/x` branch cannot escape its position. */
function encodeBranch(branch: string): string {
  return encodeURIComponent(branch);
}

/** Splits a repo-relative path on `/` and encodes each component independently, so
 *  a component can never contain an un-encoded separator or reserved character.
 *  Empty components (from `//` or a leading `/`) are dropped. */
function encodePath(path: string): string {
  return path
    .split("/")
    .filter((segment) => segment.length > 0)
    .map((segment) => encodeURIComponent(segment))
    .join("/");
}

/** A positive 1-based start line, or `null`. */
function startLine(location: SourceLocationLike): number | null {
  const line = location.span?.start_line;
  return typeof line === "number" && Number.isInteger(line) && line > 0 ? line : null;
}

/** Builds the provider-specific file deep link, or `null` when the provider is not
 *  one we deep-link into or the data is insufficient. */
function fileLink(
  provider: RepositoryProvider,
  root: string,
  branch: string,
  location: SourceLocationLike,
): string | null {
  const encodedPath = encodePath(location.path);
  if (encodedPath === "") return null;
  const line = startLine(location);
  const fragment = line !== null ? `#L${line}` : "";
  const encodedBranch = encodeBranch(branch);
  if (provider === "github") {
    return `${root}/blob/${encodedBranch}/${encodedPath}${fragment}`;
  }
  if (provider === "gitlab") {
    return `${root}/-/blob/${encodedBranch}/${encodedPath}${fragment}`;
  }
  return null;
}

/**
 * Computes the safe repository/source links for a project and an optional selected
 * source location.
 *
 * Returns `null` when there is no safe repository URL at all. Otherwise always
 * returns a root link; the `source` deep link is present only for a recognized
 * provider **and** a non-empty default branch **and** a source location with a
 * usable path.
 */
export function repositoryLinks(
  project: ProjectLike,
  location?: SourceLocationLike | null,
): RepositoryLinks | null {
  const raw = project.repository_url;
  if (typeof raw !== "string" || raw === "") return null;

  const normalized = normalizeRepository(raw);
  if (!normalized) return null;

  const provider = providerOf(normalized.host);
  const links: RepositoryLinks = { repository: normalized.root, provider };

  const branch = project.default_branch;
  if (
    (provider === "github" || provider === "gitlab") &&
    typeof branch === "string" &&
    branch !== "" &&
    location
  ) {
    const source = fileLink(provider, normalized.root, branch, location);
    if (source) links.source = source;
  }

  return links;
}
