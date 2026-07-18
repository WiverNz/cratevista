// URL state and history against the real browser history stack — no mocked
// history object, no simulated popstate.
import { clickAnEdge, expect, test, waitForGraph } from "../support/fixtures";

const params = (page: import("@playwright/test").Page) => new URL(page.url()).searchParams;
const historyLength = (page: import("@playwright/test").Page) =>
  page.evaluate(() => window.history.length);

test.describe("deep links", () => {
  test("a full deep link is honoured and reflected in the UI", async ({ page, normalURL }) => {
    const query =
      "?view=view:types&entity=item:struct:cvcore::model::Widget&q=Widget" +
      "&kinds=struct&focus=item:struct:cvcore::model::Widget&edges=related";
    await page.goto(`${normalURL}/${query}`);
    await waitForGraph(page);

    // Durable state must be visible in the UI, not merely present in the URL.
    await expect(page.getByRole("tab", { name: /types/i }).first()).toHaveAttribute(
      "aria-selected",
      "true",
    );
    await expect(page.getByLabel("Entity inspector")).toBeVisible();
    await expect(page.getByRole("searchbox", { name: "Search entities" })).toHaveValue("Widget");
    await expect(page.getByLabel("Edge visibility")).toHaveValue("related");
    await expect(page.getByRole("button", { name: /Related only/ })).toHaveAttribute(
      "aria-pressed",
      "true",
    );
    expect(params(page).get("kinds")).toBe("struct");
  });

  test("a stale view id falls back to a real view", async ({ page, normalURL }) => {
    await page.goto(`${normalURL}/?view=view:does-not-exist`);
    await waitForGraph(page);
    // Normalised on load: the bogus view is replaced by a genuine one.
    const view = params(page).get("view");
    expect(view).not.toBe("view:does-not-exist");
    expect(view).toMatch(/^view:/);
    await expect(page.getByRole("tablist", { name: "Views" }).getByRole("tab")).toHaveCount(8);
  });

  test("a stale entity id is dropped and the app still loads", async ({ page, normalURL }) => {
    await page.goto(`${normalURL}/?entity=item:struct:nope::Gone`);
    await waitForGraph(page);
    expect(params(page).get("entity")).toBeNull();
    await expect(page.getByLabel("Entity inspector")).toHaveCount(0);
  });

  test("a stale relation id is dropped", async ({ page, normalURL }) => {
    await page.goto(`${normalURL}/?relation=rel:nope`);
    await waitForGraph(page);
    expect(params(page).get("relation")).toBeNull();
  });

  test("a stale stage is dropped when the view defines no stages", async ({ page, normalURL }) => {
    // No generated view carries stages, so any stage parameter is stale.
    await page.goto(`${normalURL}/?stage=stage:nope`);
    await waitForGraph(page);
    expect(params(page).get("stage")).toBeNull();
    await expect(page.getByRole("tablist", { name: "Stages" })).toHaveCount(0);
  });

  test("a relation takes priority over an entity in the same URL", async ({ page, normalURL }) => {
    await page.goto(
      `${normalURL}/?relation=rel:contains:workspace:package:cvcore` +
        "&entity=item:struct:cvcore::model::Widget",
    );
    await waitForGraph(page);
    // Only one selection can win, and relation is the documented winner.
    expect(params(page).get("entity")).toBeNull();
  });

  test("a refresh preserves the normalized query state", async ({ page, normalURL }) => {
    await page.goto(`${normalURL}/?view=view:types&q=Widget&edges=hidden`);
    await waitForGraph(page);
    const before = page.url();

    await page.reload();
    await waitForGraph(page);
    expect(page.url()).toBe(before);
    await expect(page.getByRole("searchbox", { name: "Search entities" })).toHaveValue("Widget");
    await expect(page.getByLabel("Edge visibility")).toHaveValue("hidden");
  });
});

test.describe("back / forward", () => {
  test("goBack and goForward restore durable state across two navigations", async ({
    page,
    normalURL,
  }) => {
    await page.goto(normalURL);
    await waitForGraph(page);
    const tabs = page.getByRole("tablist", { name: "Views" }).getByRole("tab");

    // Step 1: a real pushState navigation (view change).
    await tabs.nth(1).click();
    await waitForGraph(page);
    const firstView = params(page).get("view");
    expect(firstView).toBeTruthy();

    // Step 2: a second meaningful pushState navigation (entity selection).
    await page.locator(".react-flow__node").first().click();
    await expect(page.getByLabel("Entity inspector")).toBeVisible();
    const selectedEntity = params(page).get("entity");
    expect(selectedEntity).toBeTruthy();

    // Back → the entity selection is undone, the view is retained.
    await page.goBack();
    await waitForGraph(page);
    expect(params(page).get("entity")).toBeNull();
    expect(params(page).get("view")).toBe(firstView);
    await expect(page.getByLabel("Entity inspector")).toHaveCount(0);

    // Forward → the entity selection is restored, in the UI as well as the URL.
    await page.goForward();
    await waitForGraph(page);
    expect(params(page).get("entity")).toBe(selectedEntity);
    await expect(page.getByLabel("Entity inspector")).toBeVisible();
  });

  test("going back to a relation selection restores the relation inspector", async ({
    page,
    normalURL,
  }) => {
    await page.goto(normalURL);
    await waitForGraph(page);

    await clickAnEdge(page);
    await expect(page.getByLabel("Relation inspector")).toBeVisible();
    const relation = params(page).get("relation");

    await page.locator(".react-flow__node").first().click();
    await expect(page.getByLabel("Entity inspector")).toBeVisible();

    await page.goBack();
    await expect(page.getByLabel("Relation inspector")).toBeVisible();
    expect(params(page).get("relation")).toBe(relation);
  });

  test("popstate does not push duplicate history entries", async ({ page, normalURL }) => {
    await page.goto(normalURL);
    await waitForGraph(page);
    const tabs = page.getByRole("tablist", { name: "Views" }).getByRole("tab");

    await tabs.nth(1).click();
    await waitForGraph(page);
    await tabs.nth(2).click();
    await waitForGraph(page);
    const afterNavigations = await historyLength(page);

    // Applying a popstate must not itself push a new entry, or Back would
    // never escape the last state.
    await page.goBack();
    await waitForGraph(page);
    expect(await historyLength(page)).toBe(afterNavigations);

    await page.goBack();
    await waitForGraph(page);
    expect(await historyLength(page)).toBe(afterNavigations);
  });

  test("typing in search replaces, rather than pushing an entry per keystroke", async ({
    page,
    normalURL,
  }) => {
    await page.goto(normalURL);
    await waitForGraph(page);

    // Push one real entry first, so there is somewhere to go back TO. Search
    // replaces the current entry rather than pushing, so without this the only
    // entry would be the initial load and Back would leave the app entirely.
    await page.getByRole("tablist", { name: "Views" }).getByRole("tab").nth(1).click();
    await waitForGraph(page);
    const lengthAfterNavigation = await historyLength(page);

    const search = page.getByRole("searchbox", { name: "Search entities" });
    await search.pressSequentially("Widget", { delay: 30 });
    await expect(search).toHaveValue("Widget");
    await expect.poll(() => params(page).get("q")).toBe("Widget");

    // Six keystrokes must add no entries at all: replacement, not push.
    expect(await historyLength(page)).toBe(lengthAfterNavigation);

    // A single Back therefore leaves the whole search behind.
    await page.goBack();
    await waitForGraph(page);
    expect(params(page).get("q")).toBeNull();
    await expect(page.getByRole("searchbox", { name: "Search entities" })).toHaveValue("");
  });
});

test.describe("SPA fallback", () => {
  test("a non-API path without a trailing slash serves the app and its assets", async ({
    page,
    normalURL,
    problems,
  }) => {
    // A single-segment path resolves the bundle's relative `./assets/...`
    // references back to /assets/..., so the app must boot normally.
    await page.goto(`${normalURL}/explore`);
    await waitForGraph(page);

    expect(problems.failedRequests).toEqual([]);
    await expect(page.getByRole("toolbar", { name: "Graph controls" })).toBeVisible();
  });

  test("an unknown API path is not swallowed by the SPA fallback", async ({
    request,
    normalURL,
  }) => {
    // The fallback must not turn a missing API route into a 200 HTML page.
    const response = await request.get(`${normalURL}/api/nope`);
    expect(response.status()).not.toBe(200);
  });
});
