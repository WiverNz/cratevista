import { defineConfig, devices } from "@playwright/test";

// Browser verification for PRD 07.
//
// These tests run against the REAL stack: the production bundle embedded in the
// compiled `cargo-cratevista serve` binary, its actual same-origin APIs, its
// actual CSP headers and the real same-origin ELK worker. Nothing is mocked —
// the component suite (Vitest) keeps its mocks; Playwright must not use any.
//
// The servers are started by `global-setup.ts`, which polls /api/health for a
// strict 200 before any test runs. Chromium is the pinned browser from the
// locked @playwright/test version.
export default defineConfig({
  testDir: "./e2e/tests",
  globalSetup: "./e2e/global-setup.ts",
  // One worker: the tests share the two long-lived real server processes, and the
  // benchmark needs an unloaded machine to produce meaningful numbers.
  workers: 1,
  fullyParallel: false,
  forbidOnly: !!process.env.CI,
  retries: 0,
  reporter: process.env.CI ? [["github"], ["list"]] : [["list"]],
  // Bounded so a hang fails the run instead of stalling CI.
  timeout: 90_000,
  expect: { timeout: 15_000 },
  use: {
    // Diagnostics on failure only.
    screenshot: "only-on-failure",
    trace: "retain-on-failure",
    video: "off",
    actionTimeout: 15_000,
    navigationTimeout: 30_000,
  },
  projects: [
    {
      name: "chromium",
      use: { ...devices["Desktop Chrome"], viewport: { width: 1440, height: 900 } },
    },
  ],
});
