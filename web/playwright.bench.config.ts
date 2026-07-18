import { defineConfig, devices } from "@playwright/test";

// The large-graph benchmark (PRD 07 phase 15).
//
// Separate from the E2E config on purpose: it starts its own servers per fixture,
// takes minutes rather than seconds, and must not run on every CI push. It uses
// the same pinned Chromium and the same real embedded production bundle.
//
// Run with: npm run benchmark
export default defineConfig({
  testDir: "./e2e/bench",
  // One worker, no parallelism: concurrent work would make the timings noise.
  workers: 1,
  fullyParallel: false,
  retries: 0,
  reporter: [["list"]],
  // Rendering ~3,200 nodes, repeatedly, across five fixtures is genuinely slow.
  timeout: 3_600_000,
  expect: { timeout: 120_000 },
  use: {
    ...devices["Desktop Chrome"],
    viewport: { width: 1440, height: 900 },
    screenshot: "off",
    trace: "off",
    video: "off",
    // Bounded, or a non-actionable element blocks forever: Playwright's default
    // actionTimeout is 0 (no timeout), which once turned an unclickable node
    // into a silent hour-long hang instead of a fast failure.
    actionTimeout: 30_000,
    navigationTimeout: 60_000,
  },
  projects: [{ name: "chromium" }],
});
