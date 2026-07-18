// Reduced mode and the GraphList, in real Chromium.
//
// This serves the large-graph benchmark fixture (`bench-large`) so the app enters
// reduced mode through the NORMAL production policy — the real
// DEFAULT_LARGE_GRAPH_BUDGET against a real projection — rather than through a
// test-only budget override. The GraphList is the keyboard-reachable equivalent
// of the graph for entities reduced mode hides, so it must be operable with no
// pointer interaction at all.
//
// It starts its own server: the shared E2E servers hold the small snapshots.
import { expect, test } from "@playwright/test";
import { startServer, type ServerHandle } from "../support/harness";

/** The widest projection the generated benchmark document produces. */
const VIEW = "view:traits-and-impls";

let server: ServerHandle;

test.beforeAll(async () => {
  server = await startServer("bench-large");
});
test.afterAll(async () => {
  await server?.stop();
});

test.beforeEach(async ({ page }) => {
  await page.goto(`${server.baseURL}/?view=${VIEW}`);
  await expect(page.locator(".cv-graph")).toBeVisible();
  await expect(page.getByText("Computing layout…")).toBeHidden({ timeout: 120_000 });
  await expect(page.locator(".react-flow__node").first()).toBeVisible({ timeout: 120_000 });
});

test.describe("reduced mode", () => {
  test("the banner and GraphList appear via the normal budget policy", async ({ page }) => {
    // The banner states both counts in text, not by colour or icon alone.
    const banner = page.getByText(/Reduced view — showing/i);
    await expect(banner).toBeVisible();

    const list = page.getByLabel("All entities");
    await expect(list).toBeVisible();

    // The list is complete: every projected entity, not just the visible ones.
    const listed = await list.getByRole("button").count();
    const rendered = await page.locator(".react-flow__node").count();
    expect(listed, "the list must expose more entities than the graph renders").toBeGreaterThan(
      rendered,
    );

    // And it really is reduced: fewer nodes than the full projection.
    const counts = await page.evaluate(() => window.__cratevistaPerf?.counts ?? {});
    expect(counts.reduced).toBe(1);
    expect(counts.visibleNodes).toBeLessThan(counts.fullNodes);
  });

  test("a graph-hidden entity is reachable and selectable by keyboard alone", async ({ page }) => {
    const list = page.getByLabel("All entities");
    await expect(list).toBeVisible();

    // Find a listed entity that the reduced graph does NOT render. Hidden items
    // are marked with text ("· hidden"), not colour alone.
    const hidden = list.locator("button", { hasText: "hidden" }).first();
    await expect(hidden).toBeVisible();
    // The entry renders `label <span>kind</span> · hidden`; the label is the
    // button's first text node, so take that rather than its whole innerText.
    const hiddenLabel = (
      await hidden.evaluate((element) => element.childNodes[0]?.textContent ?? "")
    ).trim();
    expect(hiddenLabel.length).toBeGreaterThan(0);

    // Reach it with the keyboard only — no pointer interaction.
    await hidden.focus();
    await expect(hidden).toBeFocused();
    const renderedBefore = await page.locator(".react-flow__node").count();

    await page.keyboard.press("Enter");

    // The inspector opens on the entity we chose.
    const inspector = page.getByLabel("Entity inspector");
    await expect(inspector).toBeVisible();
    await expect(inspector.locator(".cv-inspector-title")).toContainText(hiddenLabel);

    // The reduced neighbourhood recentres: the previously hidden entity is now
    // rendered in the graph.
    await expect(page.getByText("Computing layout…")).toBeHidden({ timeout: 120_000 });
    await expect
      .poll(async () => page.locator(".react-flow__node").count(), { timeout: 120_000 })
      .toBeGreaterThan(0);
    const selectedInGraph = await page
      .locator(".react-flow__node.selected, .react-flow__node .cv-node-selected")
      .count();
    expect(
      selectedInGraph,
      "the chosen entity must be recentred into the reduced graph",
    ).toBeGreaterThan(0);
    expect(renderedBefore).toBeGreaterThan(0);

    // Escape clears the selection...
    await page.keyboard.press("Escape");
    await expect(page.getByLabel("Entity inspector")).toHaveCount(0);

    // ...and focus returns to a GraphList item, so keyboard work continues where
    // it left off rather than being dumped at the top of the document.
    const focused = await page.evaluate(() => {
      const active = document.activeElement;
      return {
        inList: !!active?.closest(".cv-graphlist"),
        isBody: active === document.body,
      };
    });
    expect(focused.isBody, "focus must not fall back to <body>").toBe(false);
    expect(focused.inList, "focus must return to the originating GraphList item").toBe(true);
  });

  test("Space also activates a GraphList entry", async ({ page }) => {
    const list = page.getByLabel("All entities");
    const entry = list.getByRole("button").first();
    await entry.focus();
    await page.keyboard.press("Space");
    await expect(page.getByLabel("Entity inspector")).toBeVisible();
  });

  test("Render full graph renders the whole projection", async ({ page }) => {
    const counts = () => page.evaluate(() => window.__cratevistaPerf?.counts ?? {});
    const before = await counts();
    expect(before.reduced).toBe(1);

    await page.getByRole("button", { name: /Render full graph/i }).click();
    await expect(page.getByText("Computing layout…")).toBeHidden({ timeout: 300_000 });

    const after = await counts();
    expect(after.reduced).toBe(0);
    expect(after.visibleNodes).toBe(after.fullNodes);
  });
});
