// Live reload in a real browser, under the real production bundle and the real
// CSP, driven by a controllable watch server (see support/watch-server.ts).
//
// Every test inherits the always-on instrumentation from watch-fixtures: a CSP
// violation, an uncaught page error or a failed same-origin request fails it in
// teardown. So "zero CSP violations and zero page errors" is asserted for the
// whole suite by construction, not just where a test names it.
import { expect, test, waitForGraph } from "../support/watch-fixtures";
import { markedSnapshot } from "../support/watch-server";
import type { Page } from "@playwright/test";

/** Plants a value on `window` that a full-page navigation would wipe. Reading it
 *  back after a reload proves the swap happened in place. */
async function plantSurvivalSentinel(page: Page): Promise<void> {
  await page.evaluate(() => {
    (window as unknown as { __survived: boolean }).__survived = true;
  });
}
async function sentinelSurvived(page: Page): Promise<boolean> {
  return page.evaluate(() => (window as unknown as { __survived?: boolean }).__survived === true);
}

/** A node bearing the given marker label. */
function markerNode(page: Page, marker: string) {
  return page.locator(".react-flow__node", { hasText: marker });
}

test.describe("live reload", () => {
  test("1. a published success swaps the document in place", async ({ page, server, baseURL }) => {
    server.setSnapshot(markedSnapshot("snapshot-1", "FIRST-DOC"));
    await page.goto(baseURL);
    await waitForGraph(page);
    await expect(markerNode(page, "FIRST-DOC")).toBeVisible();
    await plantSurvivalSentinel(page);

    // A regeneration lands: new snapshot on the server, then the event.
    server.setSnapshot(markedSnapshot("snapshot-2", "SECOND-DOC"));
    server.emit({ name: "generation-succeeded" });

    await expect(markerNode(page, "SECOND-DOC")).toBeVisible();
    await expect(markerNode(page, "FIRST-DOC")).toHaveCount(0);
    // No full-page navigation: the app reloaded data, not the page.
    expect(await sentinelSurvived(page)).toBe(true);
  });

  test("2. started keeps the graph; failed keeps it and shows a banner", async ({
    page,
    server,
    baseURL,
  }) => {
    server.setSnapshot(markedSnapshot("snapshot-1", "STABLE-DOC"));
    await page.goto(baseURL);
    await waitForGraph(page);
    await plantSurvivalSentinel(page);

    // A recognizable interaction state: select the marker node.
    await markerNode(page, "STABLE-DOC").click();

    server.emit({ name: "generation-started" });
    await expect(page.getByText("Regenerating…")).toBeVisible();
    // The graph and the selection survive a rebuild starting.
    await expect(markerNode(page, "STABLE-DOC")).toBeVisible();

    server.emit({
      name: "generation-failed",
      code: "watch_generation_failed",
      message: "generation failed; see the terminal for details",
    });

    // A non-blocking failure banner, the graph still there, the indicator gone.
    const alert = page.getByRole("alert").filter({ hasText: "Regeneration failed" });
    await expect(alert).toBeVisible();
    await expect(alert).toContainText("watch_generation_failed");
    await expect(page.getByText("Regenerating…")).toBeHidden();
    await expect(markerNode(page, "STABLE-DOC")).toBeVisible();
    expect(await sentinelSurvived(page)).toBe(true);
    // The banner never leaks a path.
    expect(await alert.textContent()).not.toMatch(/[A-Z]:\\|\/home\/|\/Users\//);
  });

  test("3. a coherence collision resolves to the coherent triple", async ({
    page,
    server,
    baseURL,
  }) => {
    // The first attempt's three artifacts disagree; the second is coherent. Only
    // the coherent document may render, and it must be the swapped-in one.
    server.setSnapshot(markedSnapshot("snapshot-1", "INITIAL-DOC"));
    await page.goto(baseURL);
    await waitForGraph(page);

    server.setSnapshot(markedSnapshot("snapshot-2", "COHERENT-DOC"));
    server.scriptHeaders([["mismatch-a", "mismatch-b", "mismatch-a"]]);
    server.emit({ name: "generation-succeeded" });

    await expect(markerNode(page, "COHERENT-DOC")).toBeVisible();
    // No error surfaced: the retry succeeded silently.
    await expect(page.getByRole("alert")).toHaveCount(0);
  });

  test("4. an exhausted coherence retry keeps the old graph and shows a banner", async ({
    page,
    server,
    baseURL,
  }) => {
    server.setSnapshot(markedSnapshot("snapshot-1", "KEPT-DOC"));
    await page.goto(baseURL);
    await waitForGraph(page);
    await plantSurvivalSentinel(page);

    // Every attempt disagrees: three incoherent triples in a row.
    server.setSnapshot(markedSnapshot("snapshot-2", "NEVER-SHOWN"));
    server.scriptHeaders([
      ["a", "b", "c"],
      ["d", "e", "f"],
      ["g", "h", "i"],
    ]);
    server.emit({ name: "generation-succeeded" });

    const alert = page.getByRole("alert").filter({ hasText: "Could not refresh" });
    await expect(alert).toBeVisible();
    // The old graph stays; the new document never appears; no empty state.
    await expect(markerNode(page, "KEPT-DOC")).toBeVisible();
    await expect(markerNode(page, "NEVER-SHOWN")).toHaveCount(0);
    await expect(page.getByText("No entities match this view.")).toHaveCount(0);
    expect(await sentinelSurvived(page)).toBe(true);
  });

  test("6. a reconnect converges to the current snapshot with no replay", async ({
    page,
    server,
    baseURL,
  }) => {
    server.setSnapshot(markedSnapshot("snapshot-1", "BEFORE-DROP"));
    await page.goto(baseURL);
    await waitForGraph(page);
    await plantSurvivalSentinel(page);

    // Drop the stream, swap the snapshot while disconnected, and publish NOTHING.
    // The only path back to the current document is the reconnect's open-refetch.
    server.dropConnections();
    server.setSnapshot(markedSnapshot("snapshot-2", "AFTER-RECONNECT"));

    await expect(markerNode(page, "AFTER-RECONNECT")).toBeVisible({ timeout: 15_000 });
    expect(await sentinelSurvived(page)).toBe(true);
    // It genuinely reconnected rather than replaying: at least two connections.
    expect(server.eventsConnections).toBeGreaterThanOrEqual(2);
  });
});

test.describe("capability disabled", () => {
  test("5. watch_enabled=false opens no EventSource and errors nowhere", async ({
    page,
    server,
    baseURL,
  }) => {
    server.watchEnabled = false;
    server.setSnapshot(markedSnapshot("snapshot-1", "STATIC-DOC"));

    const eventRequests: string[] = [];
    page.on("request", (request) => {
      if (request.url().includes("/api/events")) eventRequests.push(request.url());
    });

    await page.goto(baseURL);
    await waitForGraph(page);
    await expect(markerNode(page, "STATIC-DOC")).toBeVisible();

    // Give the health probe time to settle, then assert the stream was never opened.
    await expect
      .poll(async () => {
        const health = await page.evaluate(async () => {
          const res = await fetch("/api/health");
          return (await res.json()) as { watch_enabled?: boolean };
        });
        return health.watch_enabled;
      })
      .toBe(false);
    expect(eventRequests).toHaveLength(0);
    expect(server.eventsConnections).toBe(0);
  });
});
