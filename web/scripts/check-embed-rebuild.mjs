// Regression guard for the PRD-06 build-correctness amendment.
//
// `cratevista-server` embeds `crates/cratevista-server/embedded/` at compile time.
// Cargo only knows to rebuild when `crates/cratevista-server/build.rs` declares
// `cargo::rerun-if-changed=embedded`. Without it, `cargo build` after a frontend
// rebuild silently keeps serving the OLD embedded UI.
//
// This proves the dependency is live, end to end and without `cargo clean`:
//   1. build with the committed dist
//   2. change one dist asset deterministically
//   3. `cargo build` (incremental) and assert the SERVED bytes changed
//   4. restore the asset, rebuild, and assert the original bytes are served again
//
// The asset is always restored, including when an assertion fails. The final
// assertion compares served bytes — never merely file mtimes.
//
// Usage: `npm run check:embed-rebuild`.
import { spawn, spawnSync } from "node:child_process";
import { copyFileSync, mkdirSync, readFileSync, rmSync, statSync, writeFileSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { listFilesRecursive } from "./dist-compare.mjs";

const here = dirname(fileURLToPath(import.meta.url));
const webRoot = resolve(here, "..");
const repoRoot = resolve(webRoot, "..");
const distDir = join(repoRoot, "crates", "cratevista-server", "embedded");
const binary = join(
  repoRoot,
  "target",
  "debug",
  process.platform === "win32" ? "cargo-cratevista.exe" : "cargo-cratevista",
);

const step = (message) => console.log(`check:embed-rebuild — ${message}`);
const fail = (message) => {
  throw new Error(`check:embed-rebuild FAILED: ${message}`);
};

function build() {
  const result = spawnSync("cargo", ["build", "-p", "cargo-cratevista"], {
    cwd: repoRoot,
    stdio: "inherit",
  });
  if (result.status !== 0) fail("`cargo build -p cargo-cratevista` failed");
}

/** A throwaway workspace holding the committed `normal` snapshot. */
function prepareWorkspace() {
  const root = join(webRoot, "e2e", ".tmp", "embed-rebuild");
  rmSync(root, { recursive: true, force: true });
  const artifacts = join(root, "target", "cratevista");
  mkdirSync(artifacts, { recursive: true });
  writeFileSync(join(root, "Cargo.toml"), '[workspace]\nresolver = "2"\nmembers = []\n');
  for (const artifact of ["document.json", "generation.json", "diagnostics.json"]) {
    copyFileSync(join(webRoot, "e2e", "fixtures", "normal", artifact), join(artifacts, artifact));
  }
  return root;
}

/** Starts the built server, fetches `assetPath`, and always stops the server. */
async function fetchServedAsset(workspace, assetPath) {
  const child = spawn(
    binary,
    ["serve", "--manifest-path", join(workspace, "Cargo.toml"), "--port", "0"],
    { cwd: repoRoot, stdio: ["ignore", "pipe", "pipe"] },
  );
  let output = "";
  child.stdout.on("data", (c) => (output += c.toString()));
  child.stderr.on("data", (c) => (output += c.toString()));

  const stop = () =>
    new Promise((done) => {
      if (child.exitCode !== null) return done();
      child.once("exit", () => done());
      if (process.platform === "win32") {
        spawn("taskkill", ["/pid", String(child.pid), "/T", "/F"], { stdio: "ignore" });
      } else {
        child.kill("SIGTERM");
      }
    });

  try {
    const deadline = Date.now() + 60_000;
    for (;;) {
      if (Date.now() > deadline) fail(`server did not become ready.\n${output}`);
      const baseURL = /http:\/\/127\.0\.0\.1:\d+/.exec(output)?.[0];
      if (baseURL) {
        try {
          const health = await fetch(`${baseURL}/api/health`);
          if (health.status === 200) {
            const response = await fetch(`${baseURL}/${assetPath}`);
            if (response.status !== 200) fail(`${assetPath} served ${response.status}`);
            return Buffer.from(await response.arrayBuffer());
          }
        } catch {
          /* not up yet */
        }
      }
      await new Promise((r) => setTimeout(r, 100));
    }
  } finally {
    await stop();
    rmSync(workspace, { recursive: true, force: true });
  }
}

// Deterministically pick the first hashed JS asset.
const assetPath = listFilesRecursive(distDir)
  .filter((p) => p.startsWith("assets/") && p.endsWith(".js"))
  .sort()[0];
if (!assetPath) fail("no hashed JS asset found in the embedded bundle — run `npm run build` first");
const assetFile = join(distDir, ...assetPath.split("/"));
const original = readFileSync(assetFile);

// A fingerprint-shaped name so the server's `is_fingerprinted` rule treats it
// like any other emitted asset. Never committed: always removed on the way out.
const addedPath = "assets/embed-probe.0123456789abcdef.js";
const addedFile = join(distDir, ...addedPath.split("/"));

let exitCode = 0;
try {
  step(`baseline build with the committed dist (probe asset: ${assetPath})`);
  build();
  const baselineMtime = statSync(binary).mtimeMs;
  const servedBefore = await fetchServedAsset(prepareWorkspace(), assetPath);
  if (!servedBefore.equals(original)) {
    fail(`the baseline binary does not serve the committed ${assetPath}`);
  }

  step("modifying the dist asset and rebuilding WITHOUT cargo clean");
  const probe = Buffer.concat([original, Buffer.from("\n/* embed-rebuild-probe */\n")]);
  writeFileSync(assetFile, probe);
  build();

  // Primary proof: the SERVED bytes changed. (mtime is corroborating only.)
  const servedAfter = await fetchServedAsset(prepareWorkspace(), assetPath);
  if (servedAfter.equals(servedBefore)) {
    fail(
      "the rebuilt binary still serves the OLD asset bytes.\n" +
        "crates/cratevista-server/build.rs must declare:\n" +
        "  cargo::rerun-if-changed=embedded",
    );
  }
  if (!servedAfter.equals(probe)) fail(`the served ${assetPath} does not match the modified dist`);
  if (statSync(binary).mtimeMs === baselineMtime) {
    fail("the binary was not relinked after the dist changed");
  }
  step("PASS: changing the embedded bundle triggers a rebuild and changes the served bytes");

  // The decisive probe. Modifying an existing file is already covered by
  // rust-embed's generated `include_bytes!`, whose paths rustc records as
  // dependencies — that probe passes even without build.rs. Only ADDING a file
  // exercises the folder-level `rerun-if-changed`, and adding files is exactly
  // what `npm run build` does whenever a content hash changes.
  step("adding a new dist asset and rebuilding WITHOUT cargo clean");
  const addedBytes = Buffer.from("/* embed-rebuild-added-probe */\n");
  writeFileSync(addedFile, addedBytes);
  build();
  const servedAdded = await fetchServedAsset(prepareWorkspace(), addedPath);
  if (!servedAdded.equals(addedBytes)) {
    fail(
      `a newly added ${addedPath} is not embedded (the server fell back to index.html).\n` +
        "crates/cratevista-server/build.rs must declare:\n" +
        "  cargo::rerun-if-changed=embedded",
    );
  }
  step("PASS: a newly added dist asset is embedded after an incremental rebuild");
} catch (error) {
  console.error(String(error instanceof Error ? error.message : error));
  exitCode = 1;
} finally {
  // Always leave the repository exactly as we found it.
  writeFileSync(assetFile, original);
  rmSync(addedFile, { force: true });
  step("restored the original asset bytes and removed the added probe");
}

try {
  step("rebuilding with the restored dist");
  build();
  const servedRestored = await fetchServedAsset(prepareWorkspace(), assetPath);
  if (!servedRestored.equals(original)) fail(`the restored ${assetPath} is not embedded`);
  step("PASS: the restored committed asset is embedded again");

  const check = spawnSync(process.execPath, [join(here, "check-dist.mjs")], {
    cwd: webRoot,
    stdio: "inherit",
  });
  if (check.status !== 0) fail("check:dist failed after restoration");
  step("PASS: check:dist is clean after restoration");
} catch (error) {
  console.error(String(error instanceof Error ? error.message : error));
  exitCode = 1;
}

process.exit(exitCode);
