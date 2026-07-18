// Regenerates web/src/fixtures/all_views.document.json from the checked-in
// sample workspace. GATED: this invokes `cargo cratevista generate`, which uses
// the pinned nightly toolchain (nightly-2026-07-01) for rustdoc JSON. The normal
// test suite reads the COMMITTED fixture and never runs this. Cross-platform
// (Node child_process). Usage: `npm run refresh:fixtures`.
import { spawnSync } from "node:child_process";
import { copyFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const here = dirname(fileURLToPath(import.meta.url));
const repoRoot = resolve(here, "../..");
const manifest = resolve(repoRoot, "web/fixtures/sample-workspace/Cargo.toml");
const generated = resolve(
  repoRoot,
  "web/fixtures/sample-workspace/target/cratevista/document.json",
);
const dest = resolve(repoRoot, "web/src/fixtures/all_views.document.json");

const result = spawnSync(
  "cargo",
  ["run", "-q", "-p", "cargo-cratevista", "--", "generate", "--manifest-path", manifest],
  { cwd: repoRoot, stdio: "inherit" },
);
if (result.status !== 0) {
  console.error("refresh:fixtures — `cargo cratevista generate` failed.");
  process.exit(result.status ?? 1);
}
copyFileSync(generated, dest);
console.log(`refresh:fixtures — wrote ${dest}`);
