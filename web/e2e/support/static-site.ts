// Produces a REAL static site through the actual CLI and serves it over HTTP at a
// non-root subpath, so the Playwright suite exercises the true static export — the
// injected `cratevista-mode` marker, the sibling `./*.json` artifacts, and the
// complete absence of any server behind them — rather than a server emulating one.
//
// The site is built by `cargo cratevista build` on the committed metadata-only
// fixture: a bin-only crate whose default RustdocPlan is empty, so the build is a
// metadata-only success needing **no nightly and no network**.
import { createServer, type Server, type ServerResponse } from "node:http";
import { execFileSync } from "node:child_process";
import { existsSync, mkdirSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { AddressInfo } from "node:net";
import { join } from "node:path";
import { binaryPath, repoRoot, webRoot } from "./harness";

/** The subpath the static site is mounted under, to prove non-root hosting. */
export const STATIC_MOUNT = "/cratevista/";

// The exact production CSP (copied from the server router / watch-server double),
// so "zero CSP violations" against a static host means what it means: the produced
// site runs under the strictest policy with only relative, same-origin assets.
const CONTENT_SECURITY_POLICY =
  "default-src 'self'; script-src 'self'; style-src 'self'; " +
  "style-src-attr 'unsafe-inline'; connect-src 'self'; worker-src 'self'; " +
  "base-uri 'self'; object-src 'none'; frame-ancestors 'none'";

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

/**
 * Builds the metadata-only static site via the real binary and returns the output
 * directory. Requires the binary (built AFTER the production dist) and a stable
 * `cargo` on PATH.
 */
export function buildStaticSite(): string {
  const binary = binaryPath();
  if (!existsSync(binary)) {
    throw new Error(
      `static E2E: the binary is missing at ${binary}.\n` +
        "Build it AFTER the production dist: npm run build && cargo build -p cargo-cratevista",
    );
  }
  const root = join(webRoot, "e2e", ".tmp", "static-build");
  rmSync(root, { recursive: true, force: true });
  mkdirSync(join(root, "src"), { recursive: true });
  writeFileSync(
    join(root, "Cargo.toml"),
    // A real `repository` so the generated document carries a genuine
    // `Project.repository_url` (proving the repository-link path end to end). An
    // empty `[workspace]` table makes this its own workspace root, so it is not
    // rejected as a stray package inside the CrateVista workspace it lives under.
    '[package]\nname = "e2estatic"\nversion = "0.0.0"\nedition = "2021"\n' +
      'repository = "https://github.com/example/example"\n\n[workspace]\n',
  );
  writeFileSync(join(root, "src", "main.rs"), "fn main() {}\n");

  const site = join(root, "site");
  // Real generation + materialization; absolute --output so nothing depends on cwd.
  execFileSync(binary, ["build", "--manifest-path", join(root, "Cargo.toml"), "--output", site], {
    cwd: repoRoot,
    stdio: "pipe",
  });
  return site;
}

/** A running static file server. */
export interface StaticSiteServer {
  /** The base URL including the mount subpath, e.g. `http://127.0.0.1:PORT/cratevista/`. */
  baseURL: string;
  close: () => Promise<void>;
}

/**
 * Serves `dir` over loopback HTTP under [`STATIC_MOUNT`]. Everything outside the
 * mount — including any `/api/*` path — is a plain 404: there is no server behind
 * a static export, so a stray API request has nothing to answer it.
 */
export async function serveStaticSite(dir: string): Promise<StaticSiteServer> {
  const server: Server = createServer((req, res) => {
    res.setHeader("content-security-policy", CONTENT_SECURITY_POLICY);
    res.setHeader("x-content-type-options", "nosniff");

    const path = (req.url ?? "/").split("?")[0];
    if (!path.startsWith(STATIC_MOUNT)) {
      res.statusCode = 404;
      res.end();
      return;
    }
    const relative = path.slice(STATIC_MOUNT.length) || "index.html";
    serveFile(dir, relative, res);
  });

  await new Promise<void>((done) => server.listen(0, "127.0.0.1", done));
  const { port } = server.address() as AddressInfo;
  const baseURL = `http://127.0.0.1:${port}${STATIC_MOUNT}`;
  return {
    baseURL,
    close: () => new Promise<void>((done) => server.close(() => done())),
  };
}

/**
 * Serves `dir` over loopback HTTP at the URL **root** (`/`), to prove root hosting in
 * addition to the subpath case. Any `/api/*` path is a plain 404 — there is no server
 * behind a static export.
 */
export async function serveStaticSiteAtRoot(dir: string): Promise<StaticSiteServer> {
  const server: Server = createServer((req, res) => {
    res.setHeader("content-security-policy", CONTENT_SECURITY_POLICY);
    res.setHeader("x-content-type-options", "nosniff");
    const path = (req.url ?? "/").split("?")[0];
    const relative = path === "/" ? "index.html" : path.replace(/^\/+/, "");
    serveFile(dir, relative, res);
  });
  await new Promise<void>((done) => server.listen(0, "127.0.0.1", done));
  const { port } = server.address() as AddressInfo;
  return {
    baseURL: `http://127.0.0.1:${port}/`,
    close: () => new Promise<void>((done) => server.close(() => done())),
  };
}

function serveFile(dir: string, relative: string, res: ServerResponse): void {
  try {
    const bytes = readFileSync(join(dir, relative));
    res.setHeader("content-type", CONTENT_TYPES[extname(relative)] ?? "application/octet-stream");
    res.end(bytes);
  } catch {
    // A static host has no SPA fallback for a genuinely missing file; return 404 so
    // a wrong relative URL is a visible failure rather than a silent index.html.
    res.statusCode = 404;
    res.end();
  }
}

/** Removes the throwaway build workspace. */
export function cleanStaticSite(): void {
  rmSync(join(webRoot, "e2e", ".tmp", "static-build"), { recursive: true, force: true });
}
