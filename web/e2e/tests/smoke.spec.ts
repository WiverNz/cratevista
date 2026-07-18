// Full smoke inventory against the real embedded production bundle, the real
// same-origin APIs and the real ELK worker. Nothing is mocked.
import { clickAnEdge, expect, test, waitForGraph } from "../support/fixtures";

/** The eight generated views, as produced by the graph builder. */
const VIEW_IDS = [
  "view:workspace-overview",
  "view:crate-dependencies",
  "view:module-hierarchy",
  "view:types",
  "view:traits-and-impls",
  "view:type-relationships",
  "view:public-api",
  "view:documentation-coverage",
];

test.describe("normal snapshot", () => {
  test.beforeEach(async ({ page, normalURL }) => {
    await page.goto(normalURL);
    await waitForGraph(page);
  });

  test("the application loads and shows its regions", async ({ page }) => {
    // Toolbar region.
    await expect(page.getByRole("toolbar", { name: "Graph controls" })).toBeVisible();
    // View tabs region.
    await expect(page.getByRole("tablist", { name: "Views" })).toBeVisible();
    // Graph region.
    await expect(page.locator(".cv-graph")).toBeVisible();
    // Inspector region.
    await expect(page.getByLabel("Details inspector")).toBeVisible();
    // Legend.
    await expect(page.getByLabel("Legend")).toBeVisible();
  });

  test("the optional stage region is absent when no view defines stages", async ({ page }) => {
    // None of the eight generated views carries stages, so the stage tablist
    // must not render at all (it is optional, not empty chrome).
    await expect(page.getByRole("tablist", { name: "Stages" })).toHaveCount(0);
  });

  test("no partial banner is shown for a complete snapshot", async ({ page }) => {
    await expect(page.getByText(/Partial generation/)).toHaveCount(0);
  });

  test("all eight generated views are offered and each one renders", async ({ page }) => {
    const tabs = page.getByRole("tablist", { name: "Views" }).getByRole("tab");
    await expect(tabs).toHaveCount(VIEW_IDS.length);

    for (let index = 0; index < VIEW_IDS.length; index++) {
      await tabs.nth(index).click();
      await waitForGraph(page);
      await expect(tabs.nth(index)).toHaveAttribute("aria-selected", "true");
      // Each view must produce a real graph, not an empty canvas.
      expect(await page.locator(".react-flow__node").count()).toBeGreaterThan(0);
    }
  });

  test("switching views changes the rendered graph and the URL", async ({ page }) => {
    const tabs = page.getByRole("tablist", { name: "Views" }).getByRole("tab");
    await tabs.first().click();
    await waitForGraph(page);
    const firstCount = await page.locator(".react-flow__node").count();

    // `types` has a very different shape from `workspace-overview`.
    await page.getByRole("tab", { name: /types/i }).first().click();
    await waitForGraph(page);
    expect(new URL(page.url()).searchParams.get("view")).toBeTruthy();
    expect(await page.locator(".react-flow__node").count()).not.toBe(firstCount);
  });

  test("selecting an entity opens the entity inspector", async ({ page }) => {
    await page.locator(".react-flow__node").first().click();
    await expect(page.getByLabel("Entity inspector")).toBeVisible();
    expect(new URL(page.url()).searchParams.get("entity")).toBeTruthy();
  });

  test("selecting a relation opens the relation inspector", async ({ page }) => {
    await clickAnEdge(page);
    await expect(page.getByLabel("Relation inspector")).toBeVisible();
    expect(new URL(page.url()).searchParams.get("relation")).toBeTruthy();
  });

  test("search finds an entity by label", async ({ page }) => {
    const search = page.getByRole("searchbox", { name: "Search entities" });
    await search.fill("Widget");
    const results = page.getByRole("listbox", { name: "Search results" });
    await expect(results).toBeVisible();
    await expect(results.getByRole("option").first()).toContainText("Widget");

    await results.getByRole("option").first().click();
    await expect(page.getByLabel("Entity inspector")).toBeVisible();
  });

  test("search finds an entity by qualified name", async ({ page }) => {
    const search = page.getByRole("searchbox", { name: "Search entities" });
    await search.fill("cvcore::model::Widget");
    const results = page.getByRole("listbox", { name: "Search results" });
    await expect(results).toBeVisible();
    await expect(results.getByRole("option").first()).toContainText("cvcore::model::Widget");
  });

  test("kind filters narrow the graph and can be cleared", async ({ page }) => {
    await page.getByRole("tab", { name: /types/i }).first().click();
    await waitForGraph(page);
    const before = await page.locator(".react-flow__node").count();

    const filters = page.locator(".cv-filters");
    await expect(filters).toBeVisible();
    await filters.getByRole("checkbox").first().check();
    await waitForGraph(page);
    const after = await page.locator(".react-flow__node").count();
    expect(after).toBeLessThanOrEqual(before);
    expect(new URL(page.url()).searchParams.get("kinds")).toBeTruthy();

    await filters.getByRole("button", { name: "Clear" }).click();
    await waitForGraph(page);
    expect(new URL(page.url()).searchParams.get("kinds")).toBeNull();
  });

  test("the legend describes the kinds actually on screen", async ({ page }) => {
    const legend = page.getByLabel("Legend");
    await expect(legend).toBeVisible();
    // Dynamic, not a fixed list: it reflects the current projection.
    expect(await legend.locator("li").count()).toBeGreaterThan(0);
  });

  test("fit, zoom in, zoom out and reset all work", async ({ page }) => {
    const controls = page.getByRole("group", { name: "Canvas controls" });
    const zoom = () => controls.locator(".cv-zoom").innerText();

    await controls.getByRole("button", { name: "Zoom in" }).click();
    const zoomedIn = await zoom();
    await controls.getByRole("button", { name: "Zoom out" }).click();
    expect(await zoom()).not.toBe(zoomedIn);

    await controls.getByRole("button", { name: "Fit" }).click();
    await controls.getByRole("button", { name: "Reset" }).click();
    // Reset re-fits and clears transient view state; the graph stays usable.
    await expect(page.locator(".react-flow__node").first()).toBeVisible();
  });

  test("focus / related-only mode toggles and is reflected in the URL", async ({ page }) => {
    await page.locator(".react-flow__node").first().click();
    const toggle = page.getByRole("button", { name: /Related only/ });
    await expect(toggle).toHaveAttribute("aria-pressed", "false");

    await toggle.click();
    await expect(toggle).toHaveAttribute("aria-pressed", "true");
    await waitForGraph(page);
    expect(new URL(page.url()).searchParams.get("focus")).toBeTruthy();

    await toggle.click();
    await expect(toggle).toHaveAttribute("aria-pressed", "false");
  });

  test("edge modes all / related / hidden each take effect", async ({ page }) => {
    await page.locator(".react-flow__node").first().click();
    const edges = page.getByLabel("Edge visibility");
    const edgeCount = () => page.locator(".react-flow__edge").count();

    await edges.selectOption("all");
    const all = await edgeCount();
    expect(all).toBeGreaterThan(0);
    // `all` is the default, so it is omitted from the URL.
    expect(new URL(page.url()).searchParams.get("edges")).toBeNull();

    await edges.selectOption("related");
    expect(await edgeCount()).toBeLessThanOrEqual(all);
    expect(new URL(page.url()).searchParams.get("edges")).toBe("related");

    await edges.selectOption("hidden");
    await expect(page.locator(".react-flow__edge")).toHaveCount(0);
    expect(new URL(page.url()).searchParams.get("edges")).toBe("hidden");
  });

  test("the source action reports that source is disabled by default", async ({ page }) => {
    // The harness runs `serve` WITHOUT --source, the shipped default. The
    // inspector must offer the location and degrade honestly, never appear broken.
    // `describe` is a method, and only source-bearing entities (impls/methods in
    // this snapshot) render the Source section at all.
    await page.getByRole("searchbox", { name: "Search entities" }).fill("describe");
    await page.getByRole("listbox", { name: "Search results" }).getByRole("option").first().click();

    const source = page.getByLabel("Source contents");
    await expect(source).toBeVisible();
    await source.getByRole("button", { name: "Show source" }).click();
    await expect(
      page.getByText("Source contents are disabled on this server; showing the location only."),
    ).toBeVisible();
  });

  test("the diagnostics region reports the real diagnostics", async ({ page }) => {
    const diagnostics = page.getByLabel("Diagnostics").first();
    await expect(diagnostics).toBeVisible();
    // The normal snapshot carries real generation diagnostics.
    await expect(diagnostics.getByRole("heading", { name: /Diagnostics/ })).toBeVisible();
  });
});

test.describe("partial snapshot", () => {
  test.beforeEach(async ({ page, partialURL }) => {
    await page.goto(partialURL);
    await waitForGraph(page);
  });

  test("the partial banner is shown and persists across view changes", async ({ page }) => {
    const banner = page.getByText(/Partial generation/);
    await expect(banner).toBeVisible();

    // It must not be dismissible-by-accident chrome: it survives navigation.
    await page.getByRole("tablist", { name: "Views" }).getByRole("tab").nth(1).click();
    await waitForGraph(page);
    await expect(banner).toBeVisible();

    await page.reload();
    await waitForGraph(page);
    await expect(page.getByText(/Partial generation/)).toBeVisible();
  });

  test("the target_failed diagnostic is represented in the diagnostics region", async ({
    page,
  }) => {
    const diagnostics = page.getByLabel("Diagnostics").first();
    await expect(diagnostics).toBeVisible();
    // The real rustdoc failure for `cvbroken` must reach the user.
    await expect(diagnostics).toContainText(/target_failed|cvbroken/);
  });

  test("the graph remains usable despite partial generation", async ({ page }) => {
    expect(await page.locator(".react-flow__node").count()).toBeGreaterThan(0);
    const tabs = page.getByRole("tablist", { name: "Views" }).getByRole("tab");
    await tabs.nth(2).click();
    await waitForGraph(page);
    expect(await page.locator(".react-flow__node").count()).toBeGreaterThan(0);
  });

  test("the entity inspector remains usable despite partial generation", async ({ page }) => {
    await page.locator(".react-flow__node").first().click();
    await expect(page.getByLabel("Entity inspector")).toBeVisible();
  });
});
