// Concurrent artifact loader. Fetches /api/document, /api/generation, and
// /api/diagnostics together under one AbortController + monotonic token; stale
// results are ignored; the three frontend states commit together per attempt.
//
// SNAPSHOT COHERENCE (PRD 09). Under PRD 06 the server snapshot was immutable
// after startup, so any three responses necessarily described one document. Watch
// mode breaks that: a regeneration can swap the snapshot *between* two of the
// three requests, and the result would be a document from one generation rendered
// beside diagnostics from another. Nothing about such a mix is detectable
// downstream — it just looks like a document whose diagnostics are wrong.
//
// So every response carries `X-CrateVista-Snapshot`, and an attempt is accepted
// only if the three agree. A disagreement is not an error: it means a swap landed
// mid-load, and the fix is to ask again. Hence up to three attempts per logical
// load, all under the caller's single token and AbortController.
import type { ExplorerDocument } from "../types/index.ts";
import {
  parseDiagnosticsReport,
  parseGenerationReport,
  type DiagnosticsReport,
  type GenerationReport,
} from "../types/runtime.ts";

export const SUPPORTED_SCHEMA_MAJOR = 1;

/** The per-response snapshot identity a live server stamps on every artifact. */
export const SNAPSHOT_HEADER = "x-cratevista-snapshot";

/**
 * Attempts per logical load — **three attempts total**, not one plus three
 * retries.
 *
 * A collision needs a snapshot swap to land inside the sub-millisecond window
 * between three concurrent same-origin requests. Two in a row means the workspace
 * is being rewritten continuously; three means retrying is not the answer, and
 * saying so beats spinning.
 */
export const COHERENCE_ATTEMPTS = 3;

export type FetchFn = (
  url: string,
  init?: { signal?: AbortSignal },
) => Promise<Response>;

/** The three exact URLs one logical load fetches. The loader uses them verbatim —
 *  it never appends a route name to a base string — so the same coherence, retry
 *  and cancellation logic serves both the live server and a static export. */
export interface ArtifactEndpoints {
  document: string;
  generation: string;
  diagnostics: string;
}

/** The live server's same-origin API routes. */
export const SERVER_ARTIFACT_ENDPOINTS: ArtifactEndpoints = {
  document: "/api/document",
  generation: "/api/generation",
  diagnostics: "/api/diagnostics",
};

/** The static export's sibling files, resolved relative to the current document
 *  (so an injected `<base href>` and any hosting subpath are honoured by the
 *  browser's own URL resolution — never by string surgery here). */
export const STATIC_ARTIFACT_ENDPOINTS: ArtifactEndpoints = {
  document: "./document.json",
  generation: "./generation.json",
  diagnostics: "./diagnostics.json",
};

export type LoadOutcome =
  | {
      status: "ok";
      document: ExplorerDocument;
      generation: GenerationReport | null;
      generationAvailable: boolean;
      diagnostics: DiagnosticsReport | null;
      diagnosticsAvailable: boolean;
      partial: boolean;
    }
  | { status: "document-error"; message: string }
  | { status: "incompatible"; found: string; supportedMajor: number }
  /** Every attempt saw a mid-load snapshot swap. Distinct from `document-error`
   *  because nothing is broken: the artifacts are fine and the next load will
   *  almost certainly succeed, so the UI keeps what it has rather than treating
   *  this as a failed initial load. */
  | { status: "incoherent-snapshot"; attempts: number };

function majorOf(version: string): number | null {
  const first = version.split(".")[0];
  const n = Number(first);
  return Number.isInteger(n) ? n : null;
}

function isDocument(value: unknown): value is ExplorerDocument {
  if (typeof value !== "object" || value === null) return false;
  const v = value as Record<string, unknown>;
  return (
    typeof v.schema_version === "string" &&
    Array.isArray(v.entities) &&
    Array.isArray(v.relations) &&
    Array.isArray(v.views) &&
    typeof v.project === "object" &&
    v.project !== null
  );
}

/** One artifact response: its body, and the snapshot it belongs to. */
type Fetched =
  | { ok: true; value: unknown; snapshot: string | null }
  | { ok: false; reason: unknown };

/**
 * Reads the snapshot header, tolerating a `Response` that has no `headers` at
 * all.
 *
 * Test doubles and some static hosts hand back bare objects; a loader that threw
 * on them would fail for a reason that has nothing to do with coherence.
 */
function snapshotOf(res: Response): string | null {
  try {
    return res.headers?.get?.(SNAPSHOT_HEADER) ?? null;
  } catch {
    return null;
  }
}

async function fetchArtifact(
  fetchFn: FetchFn,
  url: string,
  signal?: AbortSignal,
): Promise<Fetched> {
  try {
    const res = await fetchFn(url, signal ? { signal } : undefined);
    if (!res.ok) return { ok: false, reason: new Error(`HTTP ${res.status}`) };
    // The header is read BEFORE the body: a `Response` is consumed by `json()`,
    // and reading it after would work by luck rather than by contract.
    const snapshot = snapshotOf(res);
    return { ok: true, value: await res.json(), snapshot };
  } catch (reason) {
    return { ok: false, reason };
  }
}

/**
 * Whether the three responses describe one snapshot.
 *
 * Only responses that actually **arrived** get an opinion. A `/api/generation`
 * that 404s degrades under the existing rules and says nothing about coherence —
 * counting its missing header as "absent" would turn every degraded load into an
 * incoherent one and retry it three times for nothing.
 *
 * | arrived responses | verdict |
 * | --- | --- |
 * | all carry the same header | coherent |
 * | all carry a header, not all equal | **incoherent** — a swap landed mid-load |
 * | some carry one, some do not | **incoherent** — one response predates a swap |
 * | none carries one | coherent (see below) |
 *
 * The all-absent case is the static-export rule, and it is a compatibility
 * requirement rather than a shortcut: PRD 10 writes the three artifacts as
 * immutable files, and a file server stamps no header of its own. Immutable files
 * cannot disagree, so their silence is not ambiguity. A live CrateVista server
 * always stamps all three, so this rule can never mask a real collision there.
 */
function isCoherent(parts: Fetched[]): boolean {
  const arrived = parts.filter((part): part is Extract<Fetched, { ok: true }> => part.ok);
  if (arrived.length === 0) return true;
  const headers = arrived.map((part) => part.snapshot);
  if (headers.every((header) => header === null)) return true;
  const first = headers[0];
  return first !== null && headers.every((header) => header === first);
}

/**
 * One logical load: up to [`COHERENCE_ATTEMPTS`] attempts, each fetching the three
 * artifacts concurrently, returning the first coherent triple.
 *
 * Only an **incoherent triple** is retried. An HTTP, JSON, schema or network
 * failure means the same thing on the second try as on the first, and this loader
 * has never had a general retry contract — inventing one here would quietly change
 * how every existing failure behaves.
 */
export async function loadArtifacts(
  endpoints: ArtifactEndpoints,
  fetchFn: FetchFn,
  signal?: AbortSignal,
): Promise<LoadOutcome> {
  for (let attempt = 1; attempt <= COHERENCE_ATTEMPTS; attempt += 1) {
    const outcome = await attemptLoad(endpoints, fetchFn, signal);
    if (outcome !== "incoherent") return outcome;
    // Aborted mid-attempt: stop rather than spend the remaining attempts on a
    // load whose result nobody will read.
    if (signal?.aborted) break;
  }
  return { status: "incoherent-snapshot", attempts: COHERENCE_ATTEMPTS };
}

/** One attempt. `"incoherent"` means the triple was discarded whole. */
async function attemptLoad(
  endpoints: ArtifactEndpoints,
  fetchFn: FetchFn,
  signal?: AbortSignal,
): Promise<LoadOutcome | "incoherent"> {
  const [docResult, genResult, diagResult] = await Promise.all([
    fetchArtifact(fetchFn, endpoints.document, signal),
    fetchArtifact(fetchFn, endpoints.generation, signal),
    fetchArtifact(fetchFn, endpoints.diagnostics, signal),
  ]);

  // Coherence is decided BEFORE anything is parsed or returned, so a mixed triple
  // is never partially exposed — not as a document, not as a parse error.
  if (!isCoherent([docResult, genResult, diagResult])) return "incoherent";

  // Document is blocking.
  if (!docResult.ok) {
    return { status: "document-error", message: String(docResult.reason) };
  }
  if (!isDocument(docResult.value)) {
    return { status: "document-error", message: "malformed document" };
  }
  const document = docResult.value;
  const major = majorOf(document.schema_version);
  if (major !== SUPPORTED_SCHEMA_MAJOR) {
    return {
      status: "incompatible",
      found: document.schema_version,
      supportedMajor: SUPPORTED_SCHEMA_MAJOR,
    };
  }

  // Generation degrades.
  let generation: GenerationReport | null = null;
  let generationAvailable = false;
  if (genResult.ok) {
    try {
      generation = parseGenerationReport(genResult.value);
      generationAvailable = true;
    } catch {
      generationAvailable = false;
    }
  }

  // Diagnostics degrades.
  let diagnostics: DiagnosticsReport | null = null;
  let diagnosticsAvailable = false;
  if (diagResult.ok) {
    try {
      diagnostics = parseDiagnosticsReport(diagResult.value);
      diagnosticsAvailable = true;
    } catch {
      diagnosticsAvailable = false;
    }
  }

  return {
    status: "ok",
    document,
    generation,
    generationAvailable,
    diagnostics,
    diagnosticsAvailable,
    partial: generation?.partial ?? false,
  };
}

/** Manages one logical load: aborts the previous, bumps a token, and ignores
 *  stale results so only the latest load commits.
 *
 *  One token and one `AbortController` cover **all** coherence attempts of one
 *  invocation, so a retry never cancels itself and a newer invocation cancels the
 *  whole of an older one. */
export class ArtifactLoader {
  private token = 0;
  private controller: AbortController | null = null;

  constructor(
    private readonly endpoints: ArtifactEndpoints,
    private readonly fetchFn: FetchFn = fetch,
  ) {}

  /** Starts a load; resolves with `{ token, outcome }` or `{ token, stale: true }`
   *  if a newer attempt superseded it. */
  async load(): Promise<
    { token: number; outcome: LoadOutcome } | { token: number; stale: true }
  > {
    this.controller?.abort();
    const controller = new AbortController();
    this.controller = controller;
    const token = ++this.token;
    try {
      const outcome = await loadArtifacts(
        this.endpoints,
        this.fetchFn,
        controller.signal,
      );
      if (token !== this.token) return { token, stale: true };
      return { token, outcome };
    } catch (error) {
      if (token !== this.token) return { token, stale: true };
      return {
        token,
        outcome: { status: "document-error", message: String(error) },
      };
    }
  }

  abort(): void {
    this.controller?.abort();
  }
}

/** The artifact-loading capability the UI depends on (implemented by
 *  `ServerArtifactSource`; faked in tests). */
export interface ArtifactSource {
  load(): Promise<LoadOutcome | { stale: true }>;
  abort(): void;
}

/** Loads from the real server (`/api/*`), coalescing the loader's stale marker. */
export class ServerArtifactSource implements ArtifactSource {
  private readonly loader: ArtifactLoader;
  constructor(fetchFn: FetchFn = fetch) {
    this.loader = new ArtifactLoader(SERVER_ARTIFACT_ENDPOINTS, fetchFn);
  }
  async load(): Promise<LoadOutcome | { stale: true }> {
    const result = await this.loader.load();
    return "stale" in result ? { stale: true } : result.outcome;
  }
  abort(): void {
    this.loader.abort();
  }
}

/** Loads from the sibling static files (`./document.json`, …) using the **same**
 *  coherence/retry/cancellation loader — it adds no fetching of its own, opens no
 *  `EventSource`, and inspects no `window.location`: the relative URLs are handed
 *  to the browser exactly as written, so an injected `<base href>` and any hosting
 *  subpath just work. */
export class StaticArtifactSource implements ArtifactSource {
  private readonly loader: ArtifactLoader;
  constructor(fetchFn: FetchFn = fetch) {
    this.loader = new ArtifactLoader(STATIC_ARTIFACT_ENDPOINTS, fetchFn);
  }
  async load(): Promise<LoadOutcome | { stale: true }> {
    const result = await this.loader.load();
    return "stale" in result ? { stale: true } : result.outcome;
  }
  abort(): void {
    this.loader.abort();
  }
}
