// Playwright fixtures for the live-reload browser tests: the same always-on
// security instrumentation as `fixtures.ts`, plus a per-test `WatchServer`.
//
// Each test gets its own server on an ephemeral port, controlled by direct method
// calls (it runs in this worker process), and torn down afterwards. The CSP,
// console-error, page-error and failed-request recording is identical to the main
// suite, so a live-reload test that provoked a CSP violation or a 404 worker would
// fail in teardown exactly as any other test does.
import { test as base, expect, type Page } from "@playwright/test";
import { WatchServer, baseSnapshot } from "./watch-server";
import type { Problems } from "./fixtures";

interface WatchFixtures {
  problems: Problems;
  server: WatchServer;
  baseURL: string;
}

export const test = base.extend<WatchFixtures>({
  server: async ({}, use) => {
    const server = new WatchServer(baseSnapshot("snapshot-1"));
    await server.listen();
    await use(server);
    await server.close();
  },

  baseURL: async ({ server }, use) => {
    // `listen()` already ran in the `server` fixture; re-listening returns the
    // same address. Recompute the URL from the running server instead.
    await use(await server.listen());
  },

  problems: async ({ page }, use, testInfo) => {
    const problems: Problems = {
      csp: [],
      consoleErrors: [],
      pageErrors: [],
      failedRequests: [],
      all: () => [
        ...problems.csp.map((m) => `CSP violation: ${m}`),
        ...problems.pageErrors.map((m) => `uncaught page error: ${m}`),
        ...problems.consoleErrors.map((m) => `console error: ${m}`),
        ...problems.failedRequests.map((m) => `failed request: ${m}`),
      ],
    };

    await page.addInitScript(() => {
      window.__cspViolations = [];
      document.addEventListener("securitypolicyviolation", (event) => {
        window.__cspViolations?.push(
          `${event.violatedDirective} blocked ${event.blockedURI || "(inline)"}`,
        );
      });
    });

    page.on("console", (message) => {
      if (message.type() === "error") problems.consoleErrors.push(message.text());
    });
    page.on("pageerror", (error) => problems.pageErrors.push(error.message));
    page.on("requestfailed", (request) => {
      const failure = request.failure()?.errorText ?? "unknown";
      if (failure.includes("ERR_ABORTED")) return;
      problems.failedRequests.push(`${request.url()} (${failure})`);
    });

    await use(problems);

    if (!page.isClosed()) {
      const inPage = await page.evaluate(() => window.__cspViolations ?? []).catch(() => []);
      problems.csp.push(...inPage);
    }

    const found = problems.all();
    if (found.length > 0 && !testInfo.errors.length) {
      const allowed = testInfo.annotations.some((a) => a.type === "allow-problems");
      if (!allowed) {
        throw new Error(`Unexpected browser problems:\n  - ${found.join("\n  - ")}`);
      }
    }
  },
});

export { expect };

/** Waits until the real ELK worker has produced a layout. */
export async function waitForGraph(page: Page): Promise<void> {
  await expect(page.locator(".cv-graph")).toBeVisible();
  await expect(page.getByText("Computing layout…")).toBeHidden({ timeout: 30_000 });
  await expect(page.locator(".react-flow__node").first()).toBeVisible({ timeout: 30_000 });
}
