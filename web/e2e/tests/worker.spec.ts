// The REAL ELK worker, exercised in a real browser against the production
// bundle. Unit tests mock the worker, so they cannot prove any of this: an
// earlier build passed every unit test while the production worker threw
// "o is not a constructor" and every node sat at (0,0).
import { expect, test, nodeIds, nodePositions, waitForGraph } from "../support/fixtures";

test.describe("real ELK worker", () => {
  test("is created from a same-origin HTTP asset, never a blob:", async ({ page, normalURL }) => {
    const workers: string[] = [];
    page.on("worker", (worker) => workers.push(worker.url()));

    await page.goto(normalURL);
    await waitForGraph(page);

    expect(workers.length, "a layout worker must be created").toBeGreaterThan(0);
    const origin = new URL(normalURL).origin;
    for (const url of workers) {
      expect(url, "worker must be same-origin HTTP(S)").toMatch(/^https?:\/\//);
      expect(url.startsWith(origin), `worker ${url} must be same-origin`).toBe(true);
      // A blob: worker would need a widened CSP; `worker-src 'self'` forbids it.
      expect(url).not.toMatch(/^blob:/);
    }
  });

  test("the worker asset is served successfully", async ({ page, normalURL, request }) => {
    const workerUrls: string[] = [];
    page.on("worker", (worker) => workerUrls.push(worker.url()));
    await page.goto(normalURL);
    await waitForGraph(page);

    for (const url of workerUrls) {
      const response = await request.get(url);
      expect(response.status(), `${url} must be served`).toBe(200);
    }
  });

  test("layout completes: finite, non-placeholder coordinates and real dimensions", async ({
    page,
    normalURL,
  }) => {
    await page.goto(normalURL);
    await waitForGraph(page);

    const positions = await nodePositions(page);
    const entries = Object.entries(positions);
    expect(entries.length).toBeGreaterThan(1);

    for (const [id, { x, y }] of entries) {
      expect(Number.isFinite(x) && Number.isFinite(y), `${id} must have finite coords`).toBe(true);
    }
    // The placeholder state is every node at (0,0); a real layout separates them.
    const distinct = new Set(entries.map(([, p]) => `${p.x},${p.y}`));
    expect(distinct.size, "nodes must not all share placeholder coordinates").toBe(entries.length);

    // Nodes must have real rendered dimensions.
    const box = await page.locator(".react-flow__node").first().boundingBox();
    expect(box!.width).toBeGreaterThan(0);
    expect(box!.height).toBeGreaterThan(0);
  });

  test("the layout is left-to-right and edges are routed", async ({ page, normalURL }) => {
    await page.goto(normalURL);
    await waitForGraph(page);

    // The default view is the workspace overview: workspace → package → target.
    // A RIGHT-direction layered layout must place dependents at increasing x.
    const positions = await nodePositions(page);
    const workspaceX = positions["workspace"]?.x;
    const packageXs = Object.entries(positions)
      .filter(([id]) => id.startsWith("package:"))
      .map(([, p]) => p.x);
    const targetXs = Object.entries(positions)
      .filter(([id]) => id.startsWith("target:"))
      .map(([, p]) => p.x);

    expect(workspaceX, "the workspace node must be laid out").toBeDefined();
    expect(Math.min(...packageXs), "packages sit right of the workspace").toBeGreaterThan(
      workspaceX!,
    );
    expect(Math.max(...targetXs), "targets sit right of the packages").toBeGreaterThan(
      Math.min(...packageXs),
    );

    // Orthogonal routing yields real edge paths, not zero-length stubs.
    const paths = await page.locator(".react-flow__edge-path").evaluateAll((els) =>
      els.map((el) => el.getAttribute("d") ?? ""),
    );
    expect(paths.length, "edges must render").toBeGreaterThan(0);
    for (const d of paths) expect(d.length).toBeGreaterThan(0);
  });

  test("no worker errors occur during a normal session", async ({ page, normalURL, problems }) => {
    await page.goto(normalURL);
    await waitForGraph(page);
    // `problems` fails on uncaught errors; assert explicitly for clarity.
    expect(problems.pageErrors).toEqual([]);
    expect(await page.getByText("Layout failed.").count()).toBe(0);
  });
});

test.describe("relayout policy", () => {
  /** Counts layout worker messages by instrumenting Worker.postMessage. */
  async function countLayoutRequests(page: import("@playwright/test").Page): Promise<number> {
    return page.evaluate(() => (window as unknown as { __layoutPosts: number }).__layoutPosts);
  }

  test.beforeEach(async ({ page }) => {
    // Installed before app scripts: wrap postMessage so we can count how many
    // layout requests the app actually issues.
    await page.addInitScript(() => {
      (window as unknown as { __layoutPosts: number }).__layoutPosts = 0;
      const original = Worker.prototype.postMessage;
      Worker.prototype.postMessage = function (this: Worker, ...args: unknown[]) {
        (window as unknown as { __layoutPosts: number }).__layoutPosts++;
        return original.apply(this, args as never);
      };
    });
  });

  test("selecting an entity does not trigger a new layout", async ({ page, normalURL }) => {
    await page.goto(normalURL);
    await waitForGraph(page);
    const before = await countLayoutRequests(page);

    await page.locator(".react-flow__node").first().click();
    await expect(page.getByRole("heading", { level: 2 }).first()).toBeVisible();
    await page.waitForTimeout(500);

    expect(await countLayoutRequests(page), "selection must reuse the layout").toBe(before);
  });

  test("opening and closing the inspector does not trigger a new layout", async ({
    page,
    normalURL,
  }) => {
    await page.goto(normalURL);
    await waitForGraph(page);

    await page.locator(".react-flow__node").first().click();
    await page.waitForTimeout(300);
    const afterOpen = await countLayoutRequests(page);

    await page.keyboard.press("Escape");
    await page.waitForTimeout(500);

    expect(await countLayoutRequests(page), "closing must not relayout").toBe(afterOpen);
  });

  test("switching view triggers a new layout", async ({ page, normalURL }) => {
    await page.goto(normalURL);
    await waitForGraph(page);
    const before = await countLayoutRequests(page);

    const tabs = page.getByRole("tab");
    await tabs.nth(1).click();
    await waitForGraph(page);

    expect(await countLayoutRequests(page), "a new view needs a new layout").toBeGreaterThan(before);
  });
});

test.describe("deterministic layout", () => {
  test("a reload yields the same nodes, ordering and coordinates", async ({ page, normalURL }) => {
    await page.goto(normalURL);
    await waitForGraph(page);
    const firstIds = await nodeIds(page);
    const firstPositions = await nodePositions(page);

    await page.reload();
    await waitForGraph(page);
    const secondIds = await nodeIds(page);
    const secondPositions = await nodePositions(page);

    // Same ids, same relative ordering — not merely the same set.
    expect(secondIds).toEqual(firstIds);

    // Coordinates within a documented tolerance rather than hard-coded pixels:
    // ELK is deterministic for identical input, so any drift is a real change.
    const TOLERANCE_PX = 0.5;
    for (const [id, first] of Object.entries(firstPositions)) {
      const second = secondPositions[id];
      expect(second, `${id} must still be laid out`).toBeDefined();
      expect(Math.abs(second.x - first.x)).toBeLessThanOrEqual(TOLERANCE_PX);
      expect(Math.abs(second.y - first.y)).toBeLessThanOrEqual(TOLERANCE_PX);
      expect(Number.isFinite(second.x) && Number.isFinite(second.y)).toBe(true);
    }
    // Never the all-zero placeholder.
    expect(new Set(Object.values(secondPositions).map((p) => `${p.x},${p.y}`)).size).toBe(
      Object.keys(secondPositions).length,
    );
  });

  test("a fresh browser context yields the same layout", async ({ browser, normalURL, page }) => {
    await page.goto(normalURL);
    await waitForGraph(page);
    const expected = await nodePositions(page);

    const context = await browser.newContext();
    try {
      const fresh = await context.newPage();
      await fresh.goto(normalURL);
      await waitForGraph(fresh);
      const actual = await nodePositions(fresh);

      expect(Object.keys(actual).sort()).toEqual(Object.keys(expected).sort());
      for (const [id, first] of Object.entries(expected)) {
        expect(Math.abs(actual[id].x - first.x)).toBeLessThanOrEqual(0.5);
        expect(Math.abs(actual[id].y - first.y)).toBeLessThanOrEqual(0.5);
      }
    } finally {
      await context.close();
    }
  });
});
