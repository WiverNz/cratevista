// Graph-region layout correctness.
//
// Regression cover for a real defect: `fitView` on <ReactFlow> only fits at
// init, when every node is still at the (0,0) placeholder, so it zoomed to fit a
// degenerate point-graph. When the real ELK coordinates arrived nothing re-fit,
// the graph overflowed its overflow-hidden viewport, and clipped content —
// including whole edges — became unclickable. These tests use ordinary pointer
// hit-testing: they must never "find a convenient edge" to pass.
import { expect, test, waitForGraph } from "../support/fixtures";

/** Bounding box of a selector, in viewport coordinates. */
async function box(page: import("@playwright/test").Page, selector: string) {
  const rect = await page.locator(selector).boundingBox();
  expect(rect, `${selector} must be laid out`).not.toBeNull();
  return rect!;
}

/** The screen-space midpoint of the nth rendered edge — no searching. */
async function edgeMidpoint(page: import("@playwright/test").Page, index: number) {
  return page
    .locator(".react-flow__edge-path")
    .nth(index)
    .evaluate((element) => {
      const path = element as unknown as SVGPathElement;
      const local = path.getPointAtLength(path.getTotalLength() / 2);
      const screen = new DOMPoint(local.x, local.y).matrixTransform(path.getScreenCTM()!);
      return { x: screen.x, y: screen.y };
    });
}

test.describe("graph region", () => {
  test.beforeEach(async ({ page, normalURL }) => {
    await page.goto(normalURL);
    await waitForGraph(page);
  });

  test("the graph and inspector occupy separate, non-overlapping regions", async ({ page }) => {
    const canvas = await box(page, ".cv-canvas");
    const inspector = await box(page, ".cv-inspector");

    // Strictly disjoint horizontally: the inspector begins where the graph ends.
    expect(canvas.x + canvas.width).toBeLessThanOrEqual(inspector.x + 1);
    const overlap =
      Math.max(0, Math.min(canvas.x + canvas.width, inspector.x + inspector.width) -
        Math.max(canvas.x, inspector.x));
    expect(overlap, "graph and inspector must not overlap").toBeLessThanOrEqual(1);
  });

  test("the graph viewport equals the visible graph region and has a stable minimum", async ({
    page,
  }) => {
    const canvas = await box(page, ".cv-canvas");
    const graph = await box(page, ".cv-graph");
    expect(Math.abs(graph.width - canvas.width)).toBeLessThanOrEqual(1);
    expect(Math.abs(graph.height - canvas.height)).toBeLessThanOrEqual(1);
    expect(canvas.width).toBeGreaterThanOrEqual(320);
    expect(canvas.height).toBeGreaterThanOrEqual(320);
  });

  test("every rendered node sits inside the graph viewport after layout", async ({ page }) => {
    const graph = await box(page, ".cv-graph");
    const nodes = await page.locator(".react-flow__node").evaluateAll((elements) =>
      elements.map((element) => {
        const r = element.getBoundingClientRect();
        return { left: r.left, right: r.right, top: r.top, bottom: r.bottom };
      }),
    );
    expect(nodes.length).toBeGreaterThan(0);
    for (const node of nodes) {
      // Before the fix these ran out to x≈2184 against a 1080-wide region.
      expect(node.left).toBeGreaterThanOrEqual(graph.x - 1);
      expect(node.right).toBeLessThanOrEqual(graph.x + graph.width + 1);
      expect(node.top).toBeGreaterThanOrEqual(graph.y - 1);
      expect(node.bottom).toBeLessThanOrEqual(graph.y + graph.height + 1);
    }
  });

  test("floating controls and the legend stay inside the graph region", async ({ page }) => {
    const graph = await box(page, ".cv-graph");
    for (const selector of [".cv-canvas-controls", ".cv-legend"]) {
      const rect = await box(page, selector);
      expect(rect.x).toBeGreaterThanOrEqual(graph.x - 1);
      expect(rect.x + rect.width).toBeLessThanOrEqual(graph.x + graph.width + 1);
      expect(rect.y + rect.height).toBeLessThanOrEqual(graph.y + graph.height + 1);
    }
  });
});

test.describe("relation hit-testing", () => {
  test.beforeEach(async ({ page, normalURL }) => {
    await page.goto(normalURL);
    await waitForGraph(page);
  });

  test("the first edge is selectable by an ordinary click on its midpoint", async ({ page }) => {
    // No searching for a clickable edge: take the first one and click it.
    const point = await edgeMidpoint(page, 0);
    const graph = await box(page, ".cv-graph");
    expect(point.x, "the edge midpoint must lie inside the graph region").toBeLessThanOrEqual(
      graph.x + graph.width,
    );

    await page.mouse.click(point.x, point.y);
    await expect(page.getByLabel("Relation inspector")).toBeVisible();
    expect(new URL(page.url()).searchParams.get("relation")).toBeTruthy();
  });

  test("every rendered edge hit-tests to itself, including the one nearest the inspector", async ({
    page,
  }) => {
    const count = await page.locator(".react-flow__edge-path").count();
    expect(count).toBeGreaterThan(0);

    const results: { index: number; hit: boolean; x: number }[] = [];
    for (let index = 0; index < count; index++) {
      const point = await edgeMidpoint(page, index);
      const hit = await page.evaluate(
        (p) => !!document.elementFromPoint(p.x, p.y)?.closest(".react-flow__edge"),
        point,
      );
      results.push({ index, hit, x: point.x });
    }
    // No edge may be swallowed by a panel or clipped out of the region.
    expect(results.filter((r) => !r.hit)).toEqual([]);

    // The edge closest to the inspector must still be selectable by pointer.
    const nearest = results.reduce((a, b) => (b.x > a.x ? b : a));
    const point = await edgeMidpoint(page, nearest.index);
    await page.mouse.click(point.x, point.y);
    await expect(page.getByLabel("Relation inspector")).toBeVisible();
  });

  test("Fit after opening the inspector keeps nodes inside the graph viewport", async ({
    page,
  }) => {
    await page.locator(".react-flow__node").first().click();
    await expect(page.getByLabel("Entity inspector")).toBeVisible();

    await page.getByRole("group", { name: "Canvas controls" }).getByRole("button", { name: "Fit" }).click();
    await page.waitForTimeout(300);

    const graph = await box(page, ".cv-graph");
    const nodes = await page.locator(".react-flow__node").evaluateAll((elements) =>
      elements.map((element) => {
        const r = element.getBoundingClientRect();
        return { left: r.left, right: r.right };
      }),
    );
    for (const node of nodes) {
      expect(node.left).toBeGreaterThanOrEqual(graph.x - 1);
      expect(node.right).toBeLessThanOrEqual(graph.x + graph.width + 1);
    }
  });

  test("closing the inspector leaves no graph content hidden", async ({ page }) => {
    await page.locator(".react-flow__node").first().click();
    await expect(page.getByLabel("Entity inspector")).toBeVisible();
    await page.keyboard.press("Escape");
    await expect(page.getByLabel("Entity inspector")).toHaveCount(0);

    // The graph region is unchanged and every node remains inside it.
    const graph = await box(page, ".cv-graph");
    const nodes = await page.locator(".react-flow__node").evaluateAll((elements) =>
      elements.map((element) => element.getBoundingClientRect().right),
    );
    for (const right of nodes) expect(right).toBeLessThanOrEqual(graph.x + graph.width + 1);
  });

  test("no horizontal page overflow at the supported desktop viewport", async ({ page }) => {
    for (const width of [1280, 1440, 1920]) {
      await page.setViewportSize({ width, height: 900 });
      await page.waitForTimeout(200);
      const overflow = await page.evaluate(() => ({
        scrollWidth: document.documentElement.scrollWidth,
        clientWidth: document.documentElement.clientWidth,
      }));
      expect(
        overflow.scrollWidth,
        `the page must not scroll sideways at ${width}px`,
      ).toBeLessThanOrEqual(overflow.clientWidth + 1);
    }
  });
});
