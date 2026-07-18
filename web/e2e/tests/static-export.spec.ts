// Real static-export browser verification (PRD 10, Phase 4).
//
// This drives a site produced by the REAL `cargo cratevista build` — not `web/dist`
// served directly, and not a server emulating a static host. The produced index
// carries the injected `cratevista-mode` marker, so the application enters true
// static mode: it fetches the sibling `./*.json` artifacts and constructs **no**
// `/api` capability at all — no health probe, no `EventSource`, no source client.
//
// The site is mounted at a non-root subpath (`/cratevista/`) to prove relative-URL
// hosting. Guards installed before any app code fail the test on any `/api/` request
// or any `EventSource` construction.
import { expect, test, type Page } from "@playwright/test";
import {
  buildStaticSite,
  cleanStaticSite,
  serveStaticSite,
  serveStaticSiteAtRoot,
  type StaticSiteServer,
} from "../support/static-site";

let siteDir: string;
let server: StaticSiteServer;

test.beforeAll(async () => {
  siteDir = buildStaticSite();
  server = await serveStaticSite(siteDir);
});

test.afterAll(async () => {
  await server?.close();
  cleanStaticSite();
});

/** Installs the API + EventSource guards before the app's first line runs, and
 *  returns collectors the assertions read. */
async function instrument(page: Page): Promise<{
  apiRequests: string[];
  pageErrors: string[];
  cspViolations: string[];
}> {
  const apiRequests: string[] = [];
  const pageErrors: string[] = [];
  const cspViolations: string[] = [];

  // Any request whose pathname contains `/api/` is a contract breach. No fixed
  // route list: the rule is the whole `/api` namespace, present and future.
  page.on("request", (request) => {
    const pathname = new URL(request.url()).pathname;
    if (pathname.includes("/api/")) apiRequests.push(request.url());
  });
  page.on("pageerror", (error) => pageErrors.push(String(error)));
  page.on("console", (message) => {
    if (message.type() === "error" && /content security policy/i.test(message.text())) {
      cspViolations.push(message.text());
    }
  });

  // Replace EventSource before app startup and fail on any construction.
  await page.addInitScript(() => {
    const w = window as unknown as { __eventSourceCount: number };
    w.__eventSourceCount = 0;
    class Blocked {
      constructor() {
        w.__eventSourceCount += 1;
        throw new Error("static export must not construct EventSource");
      }
    }
    Object.defineProperty(window, "EventSource", { value: Blocked, configurable: true });
  });

  return { apiRequests, pageErrors, cspViolations };
}

test.describe("real static export", () => {
  test("renders from files, opens no EventSource, and issues zero /api requests", async ({
    page,
  }) => {
    const { apiRequests, pageErrors, cspViolations } = await instrument(page);

    const artifactRequests: string[] = [];
    page.on("request", (request) => {
      const pathname = new URL(request.url()).pathname;
      if (/\.(json)$/.test(pathname)) artifactRequests.push(pathname);
    });

    await page.goto(server.baseURL);

    // The injected static marker is present exactly once.
    await expect(page.locator('meta[name="cratevista-mode"][content="static"]')).toHaveCount(1);

    // The explorer renders from the sibling artifacts.
    await expect(page.locator(".react-flow__node").first()).toBeVisible({ timeout: 30_000 });

    // The three sibling artifacts were fetched under the subpath (relative URLs).
    for (const name of ["document.json", "generation.json", "diagnostics.json"]) {
      expect(
        artifactRequests.some((p) => p.endsWith(`/cratevista/${name}`)),
        `${name} must be fetched under /cratevista/`,
      ).toBe(true);
    }

    // Select a node → its inspector opens, and still no /api request is made.
    await page.locator(".react-flow__node").first().click();
    await expect(page.getByRole("region", { name: "Entity inspector" })).toBeVisible();

    // Give any stray health probe / reconnect a chance to appear.
    await page.waitForTimeout(1000);

    expect(apiRequests, `no /api requests: ${apiRequests.join(", ")}`).toHaveLength(0);
    const eventSourceCount = await page.evaluate(
      () => (window as unknown as { __eventSourceCount: number }).__eventSourceCount,
    );
    expect(eventSourceCount).toBe(0);
    expect(pageErrors, pageErrors.join("\n")).toHaveLength(0);
    expect(cspViolations, cspViolations.join("\n")).toHaveLength(0);
  });

  test("static asset URLs are relative and survive a subpath refresh", async ({ page }) => {
    const { apiRequests } = await instrument(page);

    await page.goto(server.baseURL);
    await expect(page.locator(".react-flow__node").first()).toBeVisible({ timeout: 30_000 });

    // Every script/style/worker asset the page loaded lives under the mount subpath
    // (relative resolution), never at the server root.
    const assetPaths = await page.evaluate(() =>
      performance
        .getEntriesByType("resource")
        .map((entry) => new URL(entry.name).pathname)
        .filter((p) => p.endsWith(".js") || p.endsWith(".css")),
    );
    expect(assetPaths.length).toBeGreaterThan(0);
    for (const p of assetPaths) {
      expect(p.startsWith("/cratevista/"), `asset must be under the subpath: ${p}`).toBe(true);
    }

    // A no-op selection updates the query string; reloading at the subpath restores
    // it and re-renders — still no /api, still static.
    await page.locator(".react-flow__node").first().click();
    const before = new URL(page.url());
    await page.reload();
    await expect(page.locator(".react-flow__node").first()).toBeVisible({ timeout: 30_000 });
    expect(new URL(page.url()).search).toBe(before.search);
    expect(apiRequests).toHaveLength(0);
  });

  test("renders a safe repository-root link from the real generated document", async ({
    page,
  }) => {
    const { apiRequests } = await instrument(page);

    await page.goto(server.baseURL);
    await expect(page.locator(".react-flow__node").first()).toBeVisible({ timeout: 30_000 });

    // The UNMODIFIED produced document carries the fixture's real repository URL.
    const repositoryUrl = await page.evaluate(async () => {
      const response = await fetch("./document.json");
      const doc = (await response.json()) as { project?: { repository_url?: string } };
      return doc.project?.repository_url ?? null;
    });
    expect(repositoryUrl).toBe("https://github.com/example/example");

    // Selecting an entity opens the inspector with a safe repository-root link.
    await page.locator(".react-flow__node").first().click();
    await expect(page.getByRole("region", { name: "Entity inspector" })).toBeVisible();
    const repoLink = page.getByRole("link", { name: /Open the repository on GitHub/i });
    await expect(repoLink).toBeVisible();
    await expect(repoLink).toHaveAttribute("href", "https://github.com/example/example");
    await expect(repoLink).toHaveAttribute("target", "_blank");
    await expect(repoLink).toHaveAttribute("rel", "noopener noreferrer");

    // The metadata-only fixture supplies no default_branch and no item SourceLocation,
    // so there is (correctly) no source deep link.
    await expect(page.getByRole("link", { name: /source file/i })).toHaveCount(0);

    // Clicking must not navigate the CURRENT tab. `target="_blank"` opens a popup;
    // block the external host so no real network load happens, then confirm the
    // explorer tab is untouched and the popup was aimed at the repository.
    await page.context().route("https://github.com/**", (route) => route.abort());
    const urlBefore = page.url();
    const [popup] = await Promise.all([
      page.context().waitForEvent("page"),
      repoLink.click(),
    ]);
    // A popup opened (target="_blank"), and the explorer tab is untouched — so no
    // current-tab navigation occurred. The popup's external load was aborted above,
    // so it settled on a chrome error page rather than reaching the network.
    await expect(page.getByRole("region", { name: "Entity inspector" })).toBeVisible();
    expect(page.url()).toBe(urlBefore);
    expect(popup).toBeTruthy();
    await popup.close();
    expect(apiRequests).toHaveLength(0);
  });
});

test.describe("real static export at the URL root", () => {
  let rootServer: StaticSiteServer;

  test.beforeAll(async () => {
    rootServer = await serveStaticSiteAtRoot(siteDir);
  });
  test.afterAll(async () => {
    await rootServer?.close();
  });

  test("renders from the root, selects a node, survives refresh, and issues zero /api requests", async ({
    page,
  }) => {
    const { apiRequests, pageErrors, cspViolations } = await instrument(page);

    await page.goto(rootServer.baseURL);
    await expect(page.locator('meta[name="cratevista-mode"][content="static"]')).toHaveCount(1);
    await expect(page.locator(".react-flow__node").first()).toBeVisible({ timeout: 30_000 });

    // Assets resolved at the root.
    const assetPaths = await page.evaluate(() =>
      performance
        .getEntriesByType("resource")
        .map((entry) => new URL(entry.name).pathname)
        .filter((p) => p.endsWith(".js") || p.endsWith(".css")),
    );
    expect(assetPaths.length).toBeGreaterThan(0);
    for (const p of assetPaths) {
      expect(p.startsWith("/assets/"), `root asset must be under /assets/: ${p}`).toBe(true);
    }

    // Select a node → query string updates; a refresh restores it and re-renders.
    await page.locator(".react-flow__node").first().click();
    const before = new URL(page.url());
    await page.reload();
    await expect(page.locator(".react-flow__node").first()).toBeVisible({ timeout: 30_000 });
    expect(new URL(page.url()).search).toBe(before.search);

    await page.waitForTimeout(500);
    expect(apiRequests, `no /api requests: ${apiRequests.join(", ")}`).toHaveLength(0);
    const eventSourceCount = await page.evaluate(
      () => (window as unknown as { __eventSourceCount: number }).__eventSourceCount,
    );
    expect(eventSourceCount).toBe(0);
    expect(pageErrors, pageErrors.join("\n")).toHaveLength(0);
    expect(cspViolations, cspViolations.join("\n")).toHaveLength(0);
  });
});
