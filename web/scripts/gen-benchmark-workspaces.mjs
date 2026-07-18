// Regenerates all three PRD-07 benchmark workspaces at their pinned scales.
//
// The scales are chosen to place each fixture at a specific point relative to
// the large-graph budget, measured on the `traits-and-impls` view (the widest
// projection the generated documents produce):
//
//   near  — 3 crates → ~1,212 projected entities  (below, but near the budget)
//   at    — 4 crates → ~1,616 projected entities  (the boundary, just above)
//   large — 8 crates → ~3,232 projected entities  (clearly above → reduced mode)
//
// Deterministic: the same scales always produce byte-identical Rust sources.
// This only writes the Rust workspaces; run `npm run refresh:e2e-snapshots`
// afterwards (gated, needs the pinned nightly) to regenerate their snapshots.
import { spawnSync } from "node:child_process";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const here = dirname(fileURLToPath(import.meta.url));
const webRoot = resolve(here, "..");

const SCALES = [
  { name: "near", crates: 3, mods: 4, types: 4 },
  { name: "at", crates: 4, mods: 4, types: 4 },
  { name: "large", crates: 8, mods: 4, types: 4 },
];

for (const { name, crates, mods, types } of SCALES) {
  const out = join(webRoot, "fixtures", `benchmark-${name}-workspace`);
  const result = spawnSync(
    process.execPath,
    [join(here, "gen-benchmark-workspace.mjs"), out, String(crates), String(mods), String(types)],
    { stdio: "inherit" },
  );
  if (result.status !== 0) process.exit(result.status ?? 1);
}
console.log(
  "gen:benchmark-workspaces — done. Refresh their snapshots with `npm run refresh:e2e-snapshots`.",
);
