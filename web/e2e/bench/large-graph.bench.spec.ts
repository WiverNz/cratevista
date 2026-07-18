// The PRD-07 large-graph benchmark.
//
// Runs the real embedded production bundle in the pinned Playwright Chromium
// against the real `cargo-cratevista serve` binary and the committed benchmark
// snapshots. Everything measured comes from the app's own local instrumentation
// (`window.__cratevistaPerf`, User Timing) or from the browser directly — nothing
// is simulated, and unavailable numbers are reported as unavailable rather than
// invented.
//
// Writes docs/benchmarks/prd-07-large-graph.json; the narrative report and the
// budget decision live in docs/benchmarks/prd-07-large-graph.md.
import { execFileSync } from "node:child_process";
import { appendFileSync } from "node:fs";
import { mkdirSync, writeFileSync } from "node:fs";
import { cpus, totalmem, platform, release, arch } from "node:os";
import { join } from "node:path";
import { test, expect, type Page } from "@playwright/test";
import { startServer, repoRoot, type SnapshotName } from "../support/harness";

/** Warm repeats per case (plus one cold run). A single run proves nothing. */
const REPEATS = 3;
/** The widest projection these generated documents produce. */
const VIEW = "view:traits-and-impls";

interface Case {
  id: string;
  snapshot: SnapshotName;
  description: string;
  /** Click "Render full graph" before measuring. */
  renderFull?: boolean;
}

const CASES: Case[] = [
  { id: "sample-101", snapshot: "normal", description: "the existing 101-entity sample" },
  { id: "near-1212", snapshot: "bench-near", description: "below but near the budget" },
  { id: "at-1616", snapshot: "bench-at", description: "at approximately 1,500 visible entities" },
  { id: "large-3232", snapshot: "bench-large", description: "above the budget, reduced mode" },
  {
    id: "large-3232-full",
    snapshot: "bench-large",
    description: "above the budget, Render full graph",
    renderFull: true,
  },
];

interface Sample {
  cold: boolean;
  modelBuildMs: number | null;
  adapterMs: number | null;
  reduceMs: number | null;
  workerMs: number | null;
  firstUsableGraphMs: number | null;
  artifactsLoadMs: number | null;
  selectionToInspectorMs: number | null;
  panZoomMs: number | null;
  fullNodes: number | null;
  visibleNodes: number | null;
  visibleEdges: number | null;
  reduced: boolean | null;
  jsHeapBytes: number | null;
}

/** Progress goes to a file as it happens: Playwright buffers console output
 *  until a test ends, which makes a long benchmark completely opaque. */
const PROGRESS = process.env.CRATEVISTA_BENCH_LOG;
function progress(message: string) {
  console.log(`benchmark — ${message}`);
  if (PROGRESS) appendFileSync(PROGRESS, `${new Date().toISOString()} ${message}
`);
}

const median = (values: number[]) => {
  const sorted = [...values].sort((a, b) => a - b);
  const mid = Math.floor(sorted.length / 2);
  return sorted.length % 2 ? sorted[mid] : (sorted[mid - 1] + sorted[mid]) / 2;
};
const round = (value: number) => Math.round(value * 100) / 100;

/** Summarises a numeric field across samples, or null when never available. */
function summarise(samples: Sample[], key: keyof Sample) {
  const values = samples
    .map((s) => s[key])
    .filter((v): v is number => typeof v === "number" && Number.isFinite(v));
  if (values.length === 0) return null;
  return {
    median: round(median(values)),
    min: round(Math.min(...values)),
    max: round(Math.max(...values)),
    samples: values.length,
  };
}

/** Reads the app's local instrumentation buffer. */
async function readPerf(page: Page) {
  return page.evaluate(() => {
    const hook = window.__cratevistaPerf;
    if (!hook) return null;
    const pick = (name: string) => {
      const entries = hook.entries.filter((e) => e.name === name);
      return entries.length ? entries[entries.length - 1].duration : null;
    };
    return {
      modelBuildMs: pick("cv.model.build"),
      adapterMs: pick("cv.adapter.project"),
      reduceMs: pick("cv.reduce"),
      workerMs: pick("cv.layout.worker"),
      artifactsLoadMs: pick("cv.artifacts.load"),
      firstUsableGraphMs: hook.counts["cv.firstUsableGraph:at"] ?? null,
      fullNodes: hook.counts.fullNodes ?? null,
      visibleNodes: hook.counts.visibleNodes ?? null,
      visibleEdges: hook.counts.visibleEdges ?? null,
      reduced: hook.counts.reduced === 1,
    };
  });
}

/**
 * The JS heap, when the browser exposes it. Chromium's `performance.memory` is
 * non-standard and coarse (quantised) without --enable-precise-memory-info; it
 * is reported as-is, and as null when absent, never estimated.
 */
async function readHeap(page: Page): Promise<number | null> {
  return page.evaluate(() => {
    const memory = (performance as unknown as { memory?: { usedJSHeapSize?: number } }).memory;
    return typeof memory?.usedJSHeapSize === "number" ? memory.usedJSHeapSize : null;
  });
}

async function waitForGraphReady(page: Page) {
  await expect(page.locator(".cv-graph")).toBeVisible();
  await expect(page.getByText("Computing layout…")).toBeHidden({ timeout: 600_000 });
  await expect(page.locator(".react-flow__node").first()).toBeVisible({ timeout: 600_000 });
}

test("large-graph benchmark", async ({ browser }) => {
  const results: Record<string, unknown>[] = [];

  for (const testCase of CASES) {
    const server = await startServer(testCase.snapshot);
    try {
      const samples: Sample[] = [];

      /** Measures the currently-loaded page. */
      const collect = async (page: Page, cold: boolean): Promise<Sample> => {
        if (testCase.renderFull) {
          await page.getByRole("button", { name: /Render full graph/i }).click();
          await waitForGraphReady(page);
        }
        const perf = await readPerf(page);
        expect(perf, "the app must expose its local instrumentation").not.toBeNull();

        // Selection → visible inspector, measured around a real click.
        //
        // The node is chosen by hit-testing rather than taking the first in DOM
        // order: the floating controls and legend legitimately float above the
        // canvas, so after fitView an arbitrary node may sit underneath one and
        // never receive a click. We are measuring selection latency here, not
        // proving hit-testability (layout.spec.ts does that for edges).
        const target = await page.evaluate(() => {
          for (const node of document.querySelectorAll<HTMLElement>(".react-flow__node")) {
            const rect = node.getBoundingClientRect();
            const point = { x: rect.x + rect.width / 2, y: rect.y + rect.height / 2 };
            if (document.elementFromPoint(point.x, point.y)?.closest(".react-flow__node") === node) {
              return point;
            }
          }
          return null;
        });
        expect(target, "at least one node must be clickable").not.toBeNull();
        const selectionStart = Date.now();
        await page.mouse.click(target!.x, target!.y);
        await expect(page.getByLabel("Entity inspector")).toBeVisible();
        const selectionToInspectorMs = Date.now() - selectionStart;

        // Pan/zoom responsiveness: a real wheel-zoom plus a drag.
        const box = (await page.locator(".cv-graph").boundingBox())!;
        const centre = { x: box.x + box.width / 2, y: box.y + box.height / 2 };
        const panStart = Date.now();
        await page.mouse.move(centre.x, centre.y);
        await page.mouse.wheel(0, -200);
        await page.mouse.down();
        await page.mouse.move(centre.x + 120, centre.y + 60, { steps: 8 });
        await page.mouse.up();
        await page.evaluate(
          () => new Promise((resolve) => requestAnimationFrame(() => resolve(null))),
        );
        const panZoomMs = Date.now() - panStart;

        return {
          cold,
          ...perf!,
          selectionToInspectorMs,
          panZoomMs,
          jsHeapBytes: await readHeap(page),
        };
      };

      // One COLD run: a fresh context, empty HTTP cache, cold JS engine.
      {
        const context = await browser.newContext();
        try {
          const page = await context.newPage();
          await page.goto(`${server.baseURL}/?view=${VIEW}`);
          await waitForGraphReady(page);
          samples.push(await collect(page, true));
          progress(`${testCase.id}: cold run done`);
        } finally {
          await context.close();
        }
      }

      // WARM runs: prime one context once, then reload within it. Cold and warm
      // are recorded separately and never averaged together.
      {
        const context = await browser.newContext();
        try {
          const page = await context.newPage();
          await page.goto(`${server.baseURL}/?view=${VIEW}`);
          await waitForGraphReady(page);
          for (let run = 0; run < REPEATS; run++) {
            await page.reload();
            await waitForGraphReady(page);
            samples.push(await collect(page, false));
            progress(`${testCase.id}: warm run ${run + 1}/${REPEATS} done`);
          }
        } finally {
          await context.close();
        }
      }

      const cold = samples.filter((s) => s.cold);
      const warm = samples.filter((s) => !s.cold);
      const shape = samples[samples.length - 1];
      results.push({
        case: testCase.id,
        description: testCase.description,
        snapshot: testCase.snapshot,
        view: VIEW,
        renderFull: testCase.renderFull ?? false,
        repeats: REPEATS,
        counts: {
          projectedFullNodes: shape.fullNodes,
          visibleNodes: shape.visibleNodes,
          visibleEdges: shape.visibleEdges,
          reducedMode: shape.reduced,
        },
        cold: {
          runs: cold.length,
          firstUsableGraphMs: summarise(cold, "firstUsableGraphMs"),
          artifactsLoadMs: summarise(cold, "artifactsLoadMs"),
        },
        warm: {
          runs: warm.length,
          modelBuildMs: summarise(warm, "modelBuildMs"),
          adapterMs: summarise(warm, "adapterMs"),
          reduceMs: summarise(warm, "reduceMs"),
          workerMs: summarise(warm, "workerMs"),
          firstUsableGraphMs: summarise(warm, "firstUsableGraphMs"),
          selectionToInspectorMs: summarise(warm, "selectionToInspectorMs"),
          panZoomMs: summarise(warm, "panZoomMs"),
        },
        jsHeapBytes: summarise(samples, "jsHeapBytes"),
      });
      progress(`${testCase.id}: ${JSON.stringify(results[results.length - 1])}`);
    } finally {
      await server.stop();
    }
  }

  // Environment. Recorded from the machine, never assumed.
  const version = (command: string, args: string[]) => {
    try {
      return execFileSync(command, args, { encoding: "utf8", shell: true }).trim().split("\n")[0];
    } catch {
      return "unavailable";
    }
  };
  const report = {
    generatedAt: new Date().toISOString().slice(0, 10),
    environment: {
      os: `${platform()} ${release()} (${arch()})`,
      cpu: cpus()[0]?.model ?? "unavailable",
      logicalCores: cpus().length,
      systemMemoryBytes: totalmem(),
      node: process.version,
      npm: version("npm", ["--version"]),
      playwright: version("npx", ["playwright", "--version"]),
      chromium: browser.version(),
      rust: version("cargo", ["--version"]),
    },
    notes: [
      "All timings are milliseconds, measured in the pinned Playwright Chromium",
      "against the real embedded production bundle served by the real binary.",
      "`cold` is a fresh context with an empty HTTP cache; `warm` reuses a primed",
      "context. They are reported separately and never averaged together.",
      "jsHeapBytes comes from Chromium's non-standard, quantised performance.memory;",
      "null means the browser did not expose it and no value was estimated.",
    ],
    cases: results,
  };

  const out = join(repoRoot, "docs", "benchmarks");
  mkdirSync(out, { recursive: true });
  writeFileSync(join(out, "prd-07-large-graph.json"), JSON.stringify(report, null, 2) + "\n");
  progress(`wrote ${join(out, "prd-07-large-graph.json")}`);
});
