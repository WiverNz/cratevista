// Playwright fixtures with always-on browser security instrumentation.
//
// Every test gets a page that records, from BEFORE the application's own scripts
// run: CSP violations, console errors, uncaught page errors and failed
// same-origin requests (assets, APIs, the worker). Anything unexpected fails the
// test in teardown — a silent CSP violation or a 404 worker must never pass.
import { test as base, expect, type Page } from "@playwright/test";

declare global {
  interface Window {
    __cspViolations?: string[];
  }
}

export interface Problems {
  csp: string[];
  consoleErrors: string[];
  pageErrors: string[];
  failedRequests: string[];
  /** All problems, flattened, for assertions/messages. */
  all: () => string[];
}

interface Fixtures {
  problems: Problems;
  /** Set to true in a test that deliberately provokes an error. */
  allowProblems: boolean;
  normalURL: string;
  partialURL: string;
}

function requiredEnv(name: string): string {
  const value = process.env[name];
  if (!value) throw new Error(`${name} is unset — global-setup did not start the server.`);
  return value;
}

export const test = base.extend<Fixtures>({
  allowProblems: [false, { option: true }],

  normalURL: async ({}, use) => {
    await use(requiredEnv("CRATEVISTA_E2E_NORMAL_URL"));
  },
  partialURL: async ({}, use) => {
    await use(requiredEnv("CRATEVISTA_E2E_PARTIAL_URL"));
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

    // Installed before ANY application script: CSP violations that happen during
    // initial parse/boot are the ones that matter most.
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
      // Only same-origin failures matter; there is no external network in these
      // tests, and aborted navigations are not defects.
      const failure = request.failure()?.errorText ?? "unknown";
      if (failure.includes("ERR_ABORTED")) return;
      problems.failedRequests.push(`${request.url()} (${failure})`);
    });

    await use(problems);

    // Drain violations recorded in the page itself.
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

/**
 * Waits until the real ELK worker has produced a layout: the "Computing layout…"
 * status is gone and React Flow has rendered nodes.
 */
export async function waitForGraph(page: Page): Promise<void> {
  await expect(page.locator(".cv-graph")).toBeVisible();
  await expect(page.getByText("Computing layout…")).toBeHidden({ timeout: 30_000 });
  await expect(page.locator(".react-flow__node").first()).toBeVisible({ timeout: 30_000 });
}

/**
 * Clicks the first rendered edge, at its midpoint.
 *
 * Deliberately naive: it does not hunt for a "clickable" edge. An orthogonal
 * edge's bounding-box centre is usually off the line, so the midpoint is asked
 * of the SVG itself — but which edge is used is not negotiated. If this cannot
 * select a relation, the graph layout is wrong and the test should fail.
 * (`e2e/tests/layout.spec.ts` additionally asserts that *every* rendered edge
 * hit-tests to itself.)
 */
export async function clickAnEdge(page: Page): Promise<void> {
  const point = await page
    .locator(".react-flow__edge-path")
    .first()
    .evaluate((element) => {
      const path = element as unknown as SVGPathElement;
      const local = path.getPointAtLength(path.getTotalLength() / 2);
      const screen = new DOMPoint(local.x, local.y).matrixTransform(path.getScreenCTM()!);
      return { x: screen.x, y: screen.y };
    });
  await page.mouse.click(point.x, point.y);
}

/** The rendered node ids, in DOM order. */
export async function nodeIds(page: Page): Promise<string[]> {
  return page.locator(".react-flow__node").evaluateAll((nodes) =>
    nodes.map((n) => n.getAttribute("data-id") ?? ""),
  );
}

/** Laid-out node positions keyed by node id, read from React Flow's transforms. */
export async function nodePositions(page: Page): Promise<Record<string, { x: number; y: number }>> {
  return page.locator(".react-flow__node").evaluateAll((nodes) => {
    const out: Record<string, { x: number; y: number }> = {};
    for (const node of nodes) {
      const id = node.getAttribute("data-id");
      if (!id) continue;
      const match = /translate\(\s*([-\d.]+)px,\s*([-\d.]+)px\)/.exec(
        (node as HTMLElement).style.transform,
      );
      if (match) out[id] = { x: Number(match[1]), y: Number(match[2]) };
    }
    return out;
  });
}
