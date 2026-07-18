// A controllable, watch-enabled server for the live-reload browser tests.
//
// # Why a double rather than the real binary
//
// The real `cargo-cratevista serve` is faithful for everything PRD 07 tested, but
// it cannot help here: it serves an immutable snapshot, reports
// `watch_enabled: false`, and exposes no `/api/events`. A real `open --watch`
// would speak the whole protocol, but only by running an actual `cargo doc`
// regeneration — which a browser test cannot trigger deterministically and which
// the prompt explicitly keeps out of these tests.
//
// So this server reproduces the SERVER-SIDE CONTRACT that already landed and is
// unit-tested in `cratevista-server`, and adds the one thing a test needs that no
// production path exposes: a control handle to swap the snapshot and publish an
// event on command. It is deliberately faithful where faithfulness is what the
// tests assert:
//
//   * it serves the REAL committed bundle from `crates/cratevista-server/embedded/`
//     — the same bytes the binary embeds — so the browser runs the real
//     application;
//   * it sends the EXACT production CSP string, copied from the Rust router, so
//     "zero CSP violations" means what it means against the binary;
//   * `/api/events` speaks the real SSE vocabulary (`generation-started` /
//     `-succeeded` / `-failed`), sends `retry: 1000` once, and never sends an
//     `id:` — replay is impossible by construction, exactly as in `events.rs`;
//   * every artifact response carries `X-CrateVista-Snapshot`, the header the
//     coherent loader checks.
//
// It runs in the Playwright worker process, so a test controls it by direct method
// calls rather than a second network channel.
import { createServer, type IncomingMessage, type Server, type ServerResponse } from "node:http";
import { readFileSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { AddressInfo } from "node:net";

const here = dirname(fileURLToPath(import.meta.url));
const webRoot = resolve(here, "../..");
const repoRoot = resolve(webRoot, "..");
// The authoritative bundle now lives inside the server crate (issue 10, Phase 5A);
// serve those exact bytes so the browser runs the real embedded application.
const distDir = join(repoRoot, "crates", "cratevista-server", "embedded");

// Copied verbatim from `crates/cratevista-server/src/router.rs`
// (`CONTENT_SECURITY_POLICY`). `connect-src 'self'` is what permits the
// same-origin EventSource; if the two ever drift, the security.spec.ts assertion
// against the real binary is the backstop.
const CONTENT_SECURITY_POLICY =
  "default-src 'self'; script-src 'self'; style-src 'self'; " +
  "style-src-attr 'unsafe-inline'; connect-src 'self'; worker-src 'self'; " +
  "base-uri 'self'; object-src 'none'; frame-ancestors 'none'";

const SNAPSHOT_HEADER = "x-cratevista-snapshot";

/** The three artifacts of one snapshot, plus the identity the header carries. */
export interface Snapshot {
  token: string;
  document: unknown;
  generation: unknown;
  diagnostics: unknown;
}

/** One SSE event, in the server's vocabulary. */
export type WatchEvent =
  | { name: "generation-started" }
  | { name: "generation-succeeded"; partial?: boolean }
  | { name: "generation-failed"; code: string; message: string };

const CONTENT_TYPES: Record<string, string> = {
  ".html": "text/html; charset=utf-8",
  ".js": "text/javascript; charset=utf-8",
  ".css": "text/css; charset=utf-8",
  ".json": "application/json; charset=utf-8",
  ".svg": "image/svg+xml",
  ".ico": "image/x-icon",
};

function extname(path: string): string {
  const dot = path.lastIndexOf(".");
  return dot === -1 ? "" : path.slice(dot);
}

/** A live SSE connection, so it can be pushed to and dropped on command. */
interface Connection {
  res: ServerResponse;
}

export class WatchServer {
  private server: Server;
  private baseURL: string | null = null;
  private snapshot: Snapshot;
  private connections = new Set<Connection>();
  /** watch_enabled reported by /api/health. */
  watchEnabled = true;
  /**
   * Static-export mode: no server APIs exist.
   *
   * `/api/health` and `/api/events` answer 404 (a file host has no such routes),
   * and artifacts are served WITHOUT `X-CrateVista-Snapshot` — an immutable file
   * has no server to stamp it. This is the shape PRD 10 will produce; the loader's
   * all-absent-headers rule and the health-probe's fail-closed default are what
   * must keep working under it.
   */
  staticExport = false;
  /** How many times the browser has connected to /api/events. */
  eventsConnections = 0;
  /**
   * A queue of per-attempt header overrides for the artifact triple.
   *
   * Each entry replaces the three responses' snapshot headers for ONE load
   * attempt: `["a","b","a"]` makes the second artifact disagree. `null` in a slot
   * means "send no header for this artifact". Consumed one triple per attempt;
   * once empty, the real coherent header is sent.
   */
  private headerScript: (string | null)[][] = [];
  private attemptCursor = 0;
  private artifactInAttempt = 0;

  constructor(initial: Snapshot) {
    this.snapshot = initial;
    this.server = createServer((req, res) => this.handle(req, res));
  }

  /** Binds an ephemeral loopback port. Idempotent: the URL is cached, so a
   *  fixture can ask for it again without re-binding. */
  async listen(): Promise<string> {
    if (this.baseURL) return this.baseURL;
    await new Promise<void>((done) => this.server.listen(0, "127.0.0.1", done));
    const { port } = this.server.address() as AddressInfo;
    this.baseURL = `http://127.0.0.1:${port}`;
    return this.baseURL;
  }

  async close(): Promise<void> {
    for (const connection of this.connections) connection.res.end();
    this.connections.clear();
    await new Promise<void>((done) => this.server.close(() => done()));
  }

  /** Replaces the served snapshot. The next load observes it. */
  setSnapshot(snapshot: Snapshot): void {
    this.snapshot = snapshot;
  }

  /** Publishes an event to every connected browser. */
  emit(event: WatchEvent): void {
    const data =
      event.name === "generation-succeeded"
        ? JSON.stringify({ partial: event.partial ?? false })
        : event.name === "generation-failed"
          ? JSON.stringify({ code: event.code, message: event.message })
          : "{}";
    const frame = `event: ${event.name}\ndata: ${data}\n\n`;
    for (const connection of this.connections) connection.res.write(frame);
  }

  /** Drops every live SSE connection. The browser's EventSource reconnects on its
   *  own after the `retry:` interval — no event is replayed, which is the whole
   *  point of the reconnect-convergence test. */
  dropConnections(): void {
    for (const connection of this.connections) connection.res.end();
    this.connections.clear();
  }

  /** Queues per-attempt header overrides for the next artifact loads. */
  scriptHeaders(script: (string | null)[][]): void {
    this.headerScript = script;
    this.attemptCursor = 0;
    this.artifactInAttempt = 0;
  }

  private nextHeaderFor(index: 0 | 1 | 2): string | null {
    if (this.attemptCursor < this.headerScript.length) {
      const triple = this.headerScript[this.attemptCursor];
      const value = triple[index];
      this.artifactInAttempt += 1;
      if (this.artifactInAttempt === 3) {
        this.artifactInAttempt = 0;
        this.attemptCursor += 1;
      }
      return value;
    }
    return this.snapshot.token;
  }

  private handle(req: IncomingMessage, res: ServerResponse): void {
    const url = (req.url ?? "/").split("?")[0];

    // The production CSP on EVERY response, assets included — the browser must run
    // the whole app under it or the "zero CSP violations" claim is hollow.
    res.setHeader("content-security-policy", CONTENT_SECURITY_POLICY);
    res.setHeader("x-content-type-options", "nosniff");

    if (url === "/api/health") return this.health(res);
    if (url === "/api/events") return this.events(req, res);
    if (url === "/api/document") return this.artifact(res, 0, this.snapshot.document);
    if (url === "/api/generation") return this.artifact(res, 1, this.snapshot.generation);
    if (url === "/api/diagnostics") return this.artifact(res, 2, this.snapshot.diagnostics);
    return this.asset(url, res);
  }

  private health(res: ServerResponse): void {
    if (this.staticExport) {
      res.statusCode = 404;
      res.end();
      return;
    }
    res.setHeader("content-type", CONTENT_TYPES[".json"]);
    res.end(
      JSON.stringify({
        schema_version: "1.1",
        watch_enabled: this.watchEnabled,
        partial: false,
      }),
    );
  }

  private artifact(res: ServerResponse, index: 0 | 1 | 2, body: unknown): void {
    const header = this.staticExport ? null : this.nextHeaderFor(index);
    if (header !== null) res.setHeader(SNAPSHOT_HEADER, header);
    res.setHeader("content-type", CONTENT_TYPES[".json"]);
    res.end(JSON.stringify(body));
  }

  private events(req: IncomingMessage, res: ServerResponse): void {
    // `/api/events` exists only when watching, mirroring the router: a client that
    // consulted health and connected anyway gets the honest 404.
    if (!this.watchEnabled || this.staticExport) {
      res.statusCode = 404;
      res.end();
      return;
    }
    this.eventsConnections += 1;
    res.setHeader("content-type", "text/event-stream");
    res.setHeader("cache-control", "no-cache");
    res.setHeader("connection", "keep-alive");
    res.statusCode = 200;
    // The one-time reconnect hint, before anything else — exactly as `events.rs`.
    res.write("retry: 1000\n\n");
    const connection: Connection = { res };
    this.connections.add(connection);
    req.on("close", () => this.connections.delete(connection));
  }

  private asset(url: string, res: ServerResponse): void {
    const relative = url === "/" ? "index.html" : url.replace(/^\/+/, "");
    try {
      const bytes = readFileSync(join(distDir, relative));
      res.setHeader("content-type", CONTENT_TYPES[extname(relative)] ?? "application/octet-stream");
      res.end(bytes);
    } catch {
      // An SPA: an unknown path is the app shell, never a hard 404 in the browser.
      try {
        const shell = readFileSync(join(distDir, "index.html"));
        res.setHeader("content-type", CONTENT_TYPES[".html"]);
        res.end(shell);
      } catch {
        res.statusCode = 404;
        res.end();
      }
    }
  }
}

/** Loads the committed `normal` fixture as a base snapshot. */
export function baseSnapshot(token: string): Snapshot {
  const dir = join(webRoot, "e2e", "fixtures", "normal");
  const read = (name: string): unknown => JSON.parse(readFileSync(join(dir, name), "utf8"));
  return {
    token,
    document: read("document.json"),
    generation: read("generation.json"),
    diagnostics: read("diagnostics.json"),
  };
}

/** A second snapshot that a test can tell apart from the base, by giving one
 *  entity a distinctive label that renders as a graph node. */
export function markedSnapshot(token: string, marker: string): Snapshot {
  const snapshot = baseSnapshot(token);
  const document = snapshot.document as {
    entities: { label?: { default?: string }; kind?: string }[];
  };
  // The workspace node is present in the default overview and is a stable target.
  const workspace = document.entities.find((entity) => entity.kind === "workspace");
  if (workspace) workspace.label = { default: marker };
  return { ...snapshot, document };
}
