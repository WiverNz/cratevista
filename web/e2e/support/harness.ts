// Real-server process orchestration for the Playwright suite.
//
// Runs the ACTUAL `cargo-cratevista serve` binary against a committed snapshot —
// no mock server, no dev server, no stubbed APIs. The browser therefore exercises
// the real embedded production bundle, the real same-origin APIs, the real CSP
// headers and the real ELK worker asset.
//
// Guarantees (PRD 07):
//   * binds 127.0.0.1 only (the server's own default; never a public interface)
//   * an isolated port — ephemeral by default, overridable via CRATEVISTA_E2E_PORT
//   * stdout/stderr captured and surfaced on failure
//   * readiness = polling /api/health until it returns exactly 200
//   * bounded startup timeout, after which we fail with the captured logs
//   * the process is always terminated (test end, failure, SIGINT/SIGTERM)
//   * cross-platform: Linux, macOS, Windows
import { spawn, type ChildProcessWithoutNullStreams } from "node:child_process";
import { copyFileSync, mkdirSync, rmSync, writeFileSync } from "node:fs";
import { existsSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const here = dirname(fileURLToPath(import.meta.url));
export const webRoot = resolve(here, "../..");
export const repoRoot = resolve(webRoot, "..");

/**
 * Snapshot names under `web/e2e/fixtures/`.
 *
 * `normal`/`partial` drive the E2E suite; the `bench-*` fixtures are the
 * large-graph benchmark documents (see docs/benchmarks/prd-07-large-graph.md).
 */
export type SnapshotName =
  | "normal"
  | "partial"
  | "flow"
  | "bench-near"
  | "bench-at"
  | "bench-large";

const ARTIFACTS = ["document.json", "generation.json", "diagnostics.json"] as const;
const STARTUP_TIMEOUT_MS = 60_000;
const HEALTH_POLL_INTERVAL_MS = 100;

/** The compiled binary under test. Must be built AFTER the production dist. */
export function binaryPath(): string {
  const override = process.env.CRATEVISTA_E2E_BINARY;
  if (override) return override;
  const name = process.platform === "win32" ? "cargo-cratevista.exe" : "cargo-cratevista";
  return join(repoRoot, "target", "debug", name);
}

/**
 * Materialises a throwaway Cargo workspace whose `target/cratevista` holds the
 * committed snapshot. `serve` locates artifacts via `cargo locate-project
 * --workspace`, so a virtual manifest is all that is needed — nothing is compiled.
 */
export function prepareWorkspace(snapshot: SnapshotName): string {
  const root = join(webRoot, "e2e", ".tmp", snapshot);
  rmSync(root, { recursive: true, force: true });
  const artifacts = join(root, "target", "cratevista");
  mkdirSync(artifacts, { recursive: true });
  writeFileSync(
    join(root, "Cargo.toml"),
    // A virtual manifest: `serve` never builds this workspace, it only needs a
    // workspace root to resolve `target/cratevista` against.
    '[workspace]\nresolver = "2"\nmembers = []\n',
  );
  const from = join(webRoot, "e2e", "fixtures", snapshot);
  for (const artifact of ARTIFACTS) {
    copyFileSync(join(from, artifact), join(artifacts, artifact));
  }
  return root;
}

/** A running real server. */
export interface ServerHandle {
  baseURL: string;
  stop: () => Promise<void>;
  logs: () => string;
}

/** Kills the process tree, cross-platform, and resolves once it is gone. */
function terminate(child: ChildProcessWithoutNullStreams): Promise<void> {
  return new Promise((resolveDone) => {
    if (child.exitCode !== null || child.signalCode !== null) return resolveDone();
    child.once("exit", () => resolveDone());
    if (process.platform === "win32" && child.pid !== undefined) {
      // Windows has no signals; taskkill /T reaps the whole tree so no orphan
      // survives the run.
      spawn("taskkill", ["/pid", String(child.pid), "/T", "/F"], { stdio: "ignore" });
    } else {
      child.kill("SIGTERM");
      // Escalate if it ignores SIGTERM.
      setTimeout(() => child.kill("SIGKILL"), 5_000).unref();
    }
  });
}

/** True once GET /api/health answers with exactly 200. */
async function isHealthy(baseURL: string): Promise<boolean> {
  try {
    const response = await fetch(`${baseURL}/api/health`);
    return response.status === 200;
  } catch {
    return false;
  }
}

/** Reads the bound URL the server prints on startup (supports `--port 0`). */
function firstUrl(text: string): string | undefined {
  return /http:\/\/127\.0\.0\.1:(\d+)\//.exec(text)?.[0].replace(/\/$/, "");
}

/**
 * Starts the real server for `snapshot` and resolves once /api/health returns
 * 200. Rejects — with the captured server output — on early exit or timeout.
 */
export async function startServer(snapshot: SnapshotName): Promise<ServerHandle> {
  const binary = binaryPath();
  if (!existsSync(binary)) {
    throw new Error(
      `E2E: the server binary is missing at ${binary}.\n` +
        "Build it AFTER the production dist:\n" +
        "  npm run build && cargo build -p cargo-cratevista",
    );
  }
  const workspace = prepareWorkspace(snapshot);
  // An ephemeral port by default keeps parallel/CI runs isolated; the server
  // prints the port it actually bound. CRATEVISTA_E2E_PORT pins it when needed.
  const port = process.env.CRATEVISTA_E2E_PORT ?? "0";

  const child = spawn(
    binary,
    // `serve` binds 127.0.0.1 by default and never regenerates; --source is
    // deliberately omitted so /api/source stays disabled (the shipped default).
    ["serve", "--manifest-path", join(workspace, "Cargo.toml"), "--port", port],
    { cwd: repoRoot, stdio: ["ignore", "pipe", "pipe"] },
  ) as ChildProcessWithoutNullStreams;

  let output = "";
  const capture = (chunk: Buffer) => {
    output += chunk.toString();
  };
  child.stdout.on("data", capture);
  child.stderr.on("data", capture);

  const logs = () => output;
  const fail = (reason: string): never => {
    throw new Error(
      `E2E: the ${snapshot} server ${reason}.\n--- captured server output ---\n${output || "(none)"}\n---`,
    );
  };

  let exited = false;
  child.once("exit", () => {
    exited = true;
  });

  // Always reap the child, even if the runner is interrupted.
  const onSignal = () => void terminate(child);
  process.once("SIGINT", onSignal);
  process.once("SIGTERM", onSignal);
  process.once("exit", onSignal);

  const deadline = Date.now() + STARTUP_TIMEOUT_MS;
  try {
    let baseURL: string | undefined;
    while (Date.now() < deadline) {
      if (exited && !baseURL) fail(`exited before becoming ready (code ${child.exitCode})`);
      baseURL ??= firstUrl(output);
      if (baseURL && (await isHealthy(baseURL))) {
        return { baseURL, stop: async () => void (await terminate(child)), logs };
      }
      await new Promise((r) => setTimeout(r, HEALTH_POLL_INTERVAL_MS));
    }
    fail(`did not report /api/health 200 within ${STARTUP_TIMEOUT_MS}ms`);
  } catch (error) {
    await terminate(child);
    throw error;
  }
  throw new Error("unreachable");
}

/** Removes the throwaway workspaces. */
export function cleanWorkspaces(): void {
  rmSync(join(webRoot, "e2e", ".tmp"), { recursive: true, force: true });
}
