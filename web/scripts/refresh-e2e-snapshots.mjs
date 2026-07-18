// Regenerates the committed Playwright E2E snapshots under web/e2e/fixtures/.
//
// GATED: this invokes `cargo cratevista generate`, which uses the pinned nightly
// toolchain (nightly-2026-07-01) for rustdoc JSON. The E2E suite and CI read the
// COMMITTED snapshots and never run this — `npm run e2e` needs no nightly.
// Cross-platform (Node child_process). Usage: `npm run refresh:e2e-snapshots`.
//
// Two snapshots, both produced by REAL generation runs:
//   normal  — web/fixtures/sample-workspace   (partial: false)
//   partial — web/fixtures/partial-workspace  (partial: true, via --keep-going;
//             that workspace contains a deliberately uncompilable crate)
//
// `generation.json` embeds BLAKE3 hashes over the exact bytes of document.json
// and diagnostics.json, so the three files of a snapshot must always be copied
// together. Never hand-edit them: any edit breaks integrity and the server
// refuses to load the snapshot.
import { spawnSync } from "node:child_process";
import { mkdirSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const here = dirname(fileURLToPath(import.meta.url));
const repoRoot = resolve(here, "../..");

const ARTIFACTS = ["document.json", "generation.json", "diagnostics.json"];
const SNAPSHOTS = [
  { name: "normal", workspace: "web/fixtures/sample-workspace", extraArgs: [] },
  { name: "partial", workspace: "web/fixtures/partial-workspace", extraArgs: ["--keep-going"] },
  // Large-graph benchmark fixtures (PRD 07 phase 15). Their Rust sources are
  // themselves generated, deterministically, by gen-benchmark-workspace.mjs —
  // regenerate them with `npm run gen:benchmark-workspaces` before refreshing.
  { name: "bench-near", workspace: "web/fixtures/benchmark-near-workspace", extraArgs: [] },
  { name: "bench-at", workspace: "web/fixtures/benchmark-at-workspace", extraArgs: [] },
  { name: "bench-large", workspace: "web/fixtures/benchmark-large-workspace", extraArgs: [] },
];

for (const { name, workspace, extraArgs } of SNAPSHOTS) {
  const manifest = resolve(repoRoot, workspace, "Cargo.toml");
  const result = spawnSync(
    "cargo",
    ["run", "-q", "-p", "cargo-cratevista", "--", "generate", "--manifest-path", manifest, ...extraArgs],
    { cwd: repoRoot, stdio: "inherit" },
  );
  if (result.status !== 0) {
    console.error(`refresh:e2e-snapshots — generation failed for the '${name}' snapshot.`);
    process.exit(result.status ?? 1);
  }

  const from = resolve(repoRoot, workspace, "target/cratevista");
  const to = resolve(repoRoot, "web/e2e/fixtures", name);
  mkdirSync(to, { recursive: true });

  // Normalize + re-commit rather than copy. `generate` records the rustdoc
  // command verbatim in the `target_failed` diagnostic, and cargo resolves it to
  // an absolute path, which must never be committed. The helper rewrites the
  // fixture-workspace prefix to `<fixture-workspace>` and re-commits through the
  // production writer, which recomputes artifact_hashes over the exact
  // normalized bytes. Fixtures are never hand-edited.
  const normalize = spawnSync(
    "cargo",
    [
      "run", "-q", "-p", "cratevista-core", "--example", "gen_e2e_fixtures", "--",
      from, to, resolve(repoRoot, workspace),
    ],
    { cwd: repoRoot, stdio: "inherit" },
  );
  if (normalize.status !== 0) {
    console.error(`refresh:e2e-snapshots — normalization failed for the '${name}' snapshot.`);
    process.exit(normalize.status ?? 1);
  }
  console.log(`refresh:e2e-snapshots — wrote ${to}`);
}

console.log(
  "refresh:e2e-snapshots — done. Verify with `cargo test -p cratevista-server --test e2e_fixtures`.",
);
