// Issue 14 (graph motion, routed relations, visual depth) — real-browser
// verification against the production embedded bundle. Runs against the `normal`
// snapshot (101 entities / 166 relations, 59 parallel node-pairs), which exercises
// routed ELK geometry, node depth and dim focus with the real ELK worker + real
// CSS. Flow-animation *mechanics* (keyframes, reduced-motion, threshold) are
// asserted in the shipped stylesheet and by the component suite; this proves the
// geometry/depth/focus behaviours that only a real browser can show.
import { expect, test, waitForGraph, nodePositions } from "../support/fixtures";

test.describe("Issue 14 — routed relation geometry", () => {
  test("dense-view edges follow routed ELK geometry (bends dominate diagonals)", async ({ page, normalURL }) => {
    await page.goto(`${normalURL}/?view=view:public-api`);
    await waitForGraph(page);
    const paths = page.locator(".react-flow__edge-path");
    expect(await paths.count()).toBeGreaterThan(5);

    // Classify each rendered path: a "diagonal straight" is the OLD getStraightPath
    // signature (a 2-point path differing in BOTH x and y). A "bent" path follows a
    // real route — rounded orthogonal corners (Q) or multiple segments.
    const stats = await paths.evaluateAll((els) => {
      let diagonal = 0;
      let bent = 0;
      let nan = 0;
      for (const el of els) {
        const d = el.getAttribute("d") ?? "";
        if (d.includes("NaN")) nan++;
        const pts = [...d.matchAll(/(-?\d+(?:\.\d+)?),(-?\d+(?:\.\d+)?)/g)].map((m) => [Number(m[1]), Number(m[2])]);
        const hasQ = d.includes("Q");
        if (hasQ || pts.length > 2) bent++;
        else if (pts.length === 2) {
          const [[x1, y1], [x2, y2]] = pts;
          if (Math.abs(x1 - x2) > 0.5 && Math.abs(y1 - y2) > 0.5) diagonal++;
        }
      }
      return { total: els.length, diagonal, bent, nan };
    });
    expect(stats.nan).toBe(0);
    // Real routed bends exist — the old straight renderer could never produce these.
    expect(stats.bent).toBeGreaterThan(0);
    // And routing dominates: bends outnumber the few short direct connections, i.e.
    // this is not a diagonal starburst where every cross-row edge is a straight line.
    expect(stats.bent).toBeGreaterThanOrEqual(stats.diagonal);
  });

  test("directional relations carry an arrow marker (source → target)", async ({ page, normalURL }) => {
    await page.goto(`${normalURL}/?view=view:crate-dependencies`);
    await waitForGraph(page);
    const markered = await page
      .locator(".react-flow__edge-path")
      .evaluateAll((els) => els.filter((e) => (e.getAttribute("marker-end") ?? "").includes("url(#")).length);
    expect(markered).toBeGreaterThan(0);
  });

  test("parallel relations between the same nodes render distinct paths", async ({ page, normalURL }) => {
    await page.goto(`${normalURL}/?view=view:type-relationships`);
    await waitForGraph(page);
    const ds = await page.locator(".react-flow__edge-path").evaluateAll((els) => els.map((e) => e.getAttribute("d") ?? ""));
    // With 59 parallel node-pairs, if separation worked there are far more
    // distinct path strings than a single collapsed one.
    const distinct = new Set(ds);
    expect(distinct.size).toBeGreaterThan(1);
    // No two DIFFERENT edges share the exact same non-empty path unless legitimately
    // identical — a wholesale collapse would make distinct.size tiny.
    expect(distinct.size).toBeGreaterThan(ds.length * 0.5);
  });
});

test.describe("Issue 14 — node depth (real computed styles)", () => {
  test("cards carry a token-derived gradient surface + bounded soft elevation", async ({ page, normalURL }) => {
    await page.goto(`${normalURL}/?view=view:workspace-overview`);
    await waitForGraph(page);
    // A plain (non-diagnostic, non-selected) card so the shadow is pure elevation.
    const style = await page.locator(".cv-node--state-normal").first().evaluate((el) => {
      const cs = getComputedStyle(el);
      return { backgroundImage: cs.backgroundImage, boxShadow: cs.boxShadow };
    });
    // Accent-derived vertical surface (color-mix resolved by the browser).
    expect(style.backgroundImage).toContain("gradient");
    // A soft, bounded, semi-transparent elevation shadow (never a solid glow).
    expect(style.boxShadow).not.toBe("none");
    expect(style.boxShadow).toMatch(/rgba\([^)]*0\.\d+\)/); // low-alpha → soft
    // Bounded blur: no extreme radius in the shadow's px lengths.
    const px = (style.boxShadow.match(/-?\d+(?:\.\d+)?px/g) ?? []).map((n) => Math.abs(parseFloat(n)));
    for (const n of px) expect(n).toBeLessThanOrEqual(24);
  });

  test("selection strengthens elevation without changing card dimensions", async ({ page, normalURL }) => {
    await page.goto(`${normalURL}/?view=view:workspace-overview`);
    await waitForGraph(page);
    const first = page.locator(".cv-node").first();
    const before = await first.evaluate((el) => {
      const r = el.getBoundingClientRect();
      return { w: Math.round(r.width), h: Math.round(r.height), shadow: getComputedStyle(el).boxShadow };
    });
    await page.locator(".react-flow__node").first().click();
    const selected = page.locator(".cv-node--state-selected").first();
    await expect(selected).toBeVisible();
    const after = await selected.evaluate((el) => {
      const r = el.getBoundingClientRect();
      return { w: Math.round(r.width), h: Math.round(r.height), shadow: getComputedStyle(el).boxShadow };
    });
    expect(after.w).toBe(before.w); // no dimension change
    expect(after.h).toBe(before.h);
    expect(after.shadow).not.toBe("none");
  });
});

test.describe("Issue 14 — dim focus (full projection, no relayout, accessible)", () => {
  test("dim keeps the full graph, dims unrelated, and moves the anchor without relayout", async ({ page, normalURL }) => {
    await page.goto(`${normalURL}/?view=view:type-relationships`);
    await waitForGraph(page);
    const before = await page.locator(".react-flow__node").count();

    await page.locator(".react-flow__node").first().click();
    await page.getByRole("button", { name: "Dim unrelated" }).click();
    await waitForGraph(page);

    // Full projection retained (no node removed) and some node is dimmed.
    expect(await page.locator(".react-flow__node").count()).toBe(before);
    await expect(page.locator(".cv-node--dimmed").first()).toBeVisible();
    // Dimmed cards are never aria-hidden and stay clickable (real buttons/nodes).
    const dimmedAriaHidden = await page
      .locator(".cv-node--dimmed")
      .evaluateAll((els) => els.filter((e) => e.closest("[aria-hidden='true']")).length);
    expect(dimmedAriaHidden).toBe(0);

    // Moving the dim anchor must NOT relayout: node positions are unchanged.
    const posA = await nodePositions(page);
    // Click a different (currently dimmed) node to re-anchor.
    const dimmed = page.locator(".cv-node--dimmed").first();
    await dimmed.click();
    await page.waitForTimeout(150); // allow any (unwanted) relayout to settle
    const posB = await nodePositions(page);
    expect(posB).toEqual(posA); // identical positions → no relayout
    expect(new URL(page.url()).searchParams.get("focusmode")).toBe("dim");
  });

  test("no application busy-work while the tab is hidden (CSS-only flow motion)", async ({ page, normalURL }) => {
    await page.goto(`${normalURL}/?view=view:public-api`);
    await waitForGraph(page);
    const posBefore = await nodePositions(page);

    // Instrument: count any rAF / timer scheduling AND assert none fire while hidden.
    await page.evaluate(() => {
      (window as unknown as { __ticks: number }).__ticks = 0;
      const raf = window.requestAnimationFrame.bind(window);
      window.requestAnimationFrame = (cb) => raf(() => {
        (window as unknown as { __ticks: number }).__ticks++;
        return cb(performance.now());
      });
    });
    // Drive the page into the hidden/background state.
    await page.evaluate(() => {
      Object.defineProperty(document, "visibilityState", { value: "hidden", configurable: true });
      Object.defineProperty(document, "hidden", { value: true, configurable: true });
      document.dispatchEvent(new Event("visibilitychange"));
    });
    const ticksAtHide = await page.evaluate(() => (window as unknown as { __ticks: number }).__ticks);
    await page.waitForTimeout(600); // a window in which a busy loop would tick repeatedly
    const ticksAfter = await page.evaluate(() => (window as unknown as { __ticks: number }).__ticks);

    // No continuous rAF loop while hidden (a bounded few is fine; a busy loop would
    // schedule dozens over 600ms). Flow motion is CSS keyframe, browser-throttled.
    expect(ticksAfter - ticksAtHide).toBeLessThanOrEqual(2);
    // No relayout occurred while hidden.
    expect(await nodePositions(page)).toEqual(posBefore);

    // Returning to the foreground leaves the graph functional.
    await page.evaluate(() => {
      Object.defineProperty(document, "visibilityState", { value: "visible", configurable: true });
      Object.defineProperty(document, "hidden", { value: false, configurable: true });
      document.dispatchEvent(new Event("visibilitychange"));
    });
    await page.locator(".react-flow__node").first().click();
    await expect(page.locator(".cv-node--state-selected").first()).toBeVisible();
  });

  test("hide focus reduces the projection (legacy behaviour preserved)", async ({ page, normalURL }) => {
    await page.goto(`${normalURL}/?view=view:type-relationships`);
    await waitForGraph(page);
    const before = await page.locator(".react-flow__node").count();
    await page.locator(".react-flow__node").first().click();
    await page.getByRole("button", { name: "Hide unrelated" }).click();
    await waitForGraph(page);
    // Hide removes unrelated nodes → strictly fewer than the full graph.
    expect(await page.locator(".react-flow__node").count()).toBeLessThan(before);
    expect(new URL(page.url()).searchParams.get("focusmode")).toBeNull();
  });
});
