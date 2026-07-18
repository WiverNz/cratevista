// Fails when the committed embedded bundle is stale.
//
// The authoritative bundle now lives INSIDE the server crate at
// `crates/cratevista-server/embedded/` (issue 10, Phase 5A). This treats it as the
// baseline, builds into an isolated temporary directory, then compares recursively
// and byte-exactly — so a missing file, a changed byte or an extra stale asset all
// fail. It never overwrites the baseline, never shells out to git, and always
// cleans the temp directory. Usage: `npm run check:dist`.
import { mkdtempSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { spawnSync } from "node:child_process";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { compareDirs, formatDiff, isDirectory, isIdentical } from "./dist-compare.mjs";

const here = dirname(fileURLToPath(import.meta.url));
const webRoot = resolve(here, "..");
const repoRoot = resolve(webRoot, "..");
const baseline = join(repoRoot, "crates", "cratevista-server", "embedded");

if (!isDirectory(baseline)) {
  console.error(
    "check:dist: the embedded bundle (crates/cratevista-server/embedded) is missing. " +
      "Run `npm run build` and commit it.",
  );
  process.exit(1);
}

// Build into an isolated temp dir; the committed dist is never touched.
const temp = mkdtempSync(join(tmpdir(), "cratevista-dist-"));
let exitCode = 0;
try {
  const viteBin = join(webRoot, "node_modules", "vite", "bin", "vite.js");
  const build = spawnSync(
    process.execPath,
    [viteBin, "build", "--outDir", temp, "--emptyOutDir"],
    { cwd: webRoot, stdio: "inherit" },
  );
  if (build.status !== 0) {
    console.error("check:dist: the verification build failed.");
    process.exit(build.status ?? 1);
  }

  const diff = compareDirs(baseline, temp);
  if (isIdentical(diff)) {
    console.log(
      "check:dist: the committed embedded bundle matches a fresh production build.",
    );
  } else {
    console.error(
      "check:dist: the committed embedded bundle is STALE. Run `npm run build` and commit the result.\n" +
        formatDiff(diff),
    );
    exitCode = 1;
  }
} finally {
  rmSync(temp, { recursive: true, force: true });
}
process.exit(exitCode);
