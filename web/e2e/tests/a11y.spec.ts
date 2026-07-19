// Real-Chromium accessibility smoke.
//
// This is evidence for a WCAG 2.1 AA application *baseline*, not a WCAG
// certification, and it asserts only what a real browser can demonstrate. It
// makes no claim about screen-reader or other assistive-technology behaviour:
// that has not been manually tested. Colour-contrast verification is recorded
// separately as a manual check.
import { expect, test, waitForGraph } from "../support/fixtures";

test.describe("keyboard and focus", () => {
  test.beforeEach(async ({ page, normalURL }) => {
    await page.goto(normalURL);
    await waitForGraph(page);
  });

  test("keyboard-focused controls show a visible focus indicator", async ({ page }) => {
    // The indicator must be measured on the real focused element: `:focus-visible`
    // is a pseudo-CLASS, so getComputedStyle cannot query it directly, and it only
    // matches when focus arrives via the keyboard — hence a real Tab press.
    await page.locator("body").click({ position: { x: 2, y: 2 } });
    await page.keyboard.press("Tab");

    const indicator = await page.evaluate(() => {
      const element = document.activeElement;
      if (!element || element === document.body) return null;
      const style = getComputedStyle(element);
      return {
        tag: element.tagName,
        outlineStyle: style.outlineStyle,
        outlineWidth: style.outlineWidth,
        boxShadow: style.boxShadow,
        matchesFocusVisible: element.matches(":focus-visible"),
      };
    });
    expect(indicator, "Tab must move focus to a control").not.toBeNull();
    expect(indicator!.matchesFocusVisible, "keyboard focus must match :focus-visible").toBe(true);

    const hasOutline =
      indicator!.outlineStyle !== "none" && parseFloat(indicator!.outlineWidth) > 0;
    const hasShadow = indicator!.boxShadow !== "none" && indicator!.boxShadow !== "";
    expect(hasOutline || hasShadow, "a keyboard-focused control must be visibly indicated").toBe(
      true,
    );
  });

  test("view tabs support Arrow, Home and End with a roving tabindex", async ({ page }) => {
    const tabs = page.getByRole("tablist", { name: "Views" }).getByRole("tab");
    const count = await tabs.count();
    const selected = () => page.locator('[role="tab"][aria-selected="true"]');

    await tabs.first().focus();
    await expect(tabs.first()).toBeFocused();

    // Activation follows focus, the documented behaviour for this tablist.
    await page.keyboard.press("ArrowRight");
    await waitForGraph(page);
    await expect(tabs.nth(1)).toBeFocused();
    await expect(selected()).toHaveCount(1);
    await expect(tabs.nth(1)).toHaveAttribute("aria-selected", "true");

    await page.keyboard.press("End");
    await waitForGraph(page);
    await expect(tabs.nth(count - 1)).toBeFocused();

    await page.keyboard.press("Home");
    await waitForGraph(page);
    await expect(tabs.first()).toBeFocused();

    // ArrowLeft from the first tab wraps to the last.
    await page.keyboard.press("ArrowLeft");
    await waitForGraph(page);
    await expect(tabs.nth(count - 1)).toBeFocused();

    // Roving tabindex: exactly one tab is in the tab order.
    expect(await page.locator('[role="tab"][tabindex="0"]').count()).toBe(1);
  });

  test("a search result can be chosen with the keyboard alone", async ({ page }) => {
    const search = page.getByRole("searchbox", { name: "Search entities" });
    await search.focus();
    await page.keyboard.type("Widget");

    const results = page.getByRole("listbox", { name: "Search results" });
    await expect(results).toBeVisible();

    // Tab into the results and activate with Enter — no mouse involved.
    await page.keyboard.press("Tab");
    await expect(results.getByRole("option").first()).toBeFocused();
    await page.keyboard.press("Enter");
    await expect(page.getByLabel("Entity inspector")).toBeVisible();
  });

  // Keyboard evidence for the GraphList needs a *reduced* projection, which only
  // the large-graph fixture produces. It lives in `reduced-mode.spec.ts`, which
  // serves that fixture and drives the list with the keyboard alone.

  test("Escape clears the selection", async ({ page }) => {
    await page.locator(".react-flow__node").first().click();
    await expect(page.getByLabel("Entity inspector")).toBeVisible();

    await page.keyboard.press("Escape");
    await expect(page.getByLabel("Entity inspector")).toHaveCount(0);
    expect(new URL(page.url()).searchParams.get("entity")).toBeNull();
  });

  test("the inspector title is focusable so selection is announced in place", async ({ page }) => {
    await page.locator(".react-flow__node").first().click();
    const title = page.getByLabel("Entity inspector").locator(".cv-inspector-title");
    await expect(title).toBeVisible();
    // tabIndex=-1 lets the app move focus to the new content programmatically
    // without inserting the heading into the tab order.
    await expect(title).toHaveAttribute("tabindex", "-1");
  });
});

test.describe("reduced motion", () => {
  test.use({ reducedMotion: "reduce" });

  test("the app is fully usable and nothing essential is animated", async ({ page, normalURL }) => {
    await page.goto(normalURL);
    await waitForGraph(page);

    // The graph still renders and stays interactive.
    expect(await page.locator(".react-flow__node").count()).toBeGreaterThan(0);
    await page.locator(".react-flow__node").first().click();
    await expect(page.getByLabel("Entity inspector")).toBeVisible();

    // No element relies on a long transition/animation to convey state.
    const animated = await page.evaluate(() => {
      const offenders: string[] = [];
      for (const element of document.querySelectorAll("*")) {
        const style = getComputedStyle(element);
        const duration = (value: string) =>
          Math.max(0, ...value.split(",").map((v) => parseFloat(v) || 0));
        if (duration(style.transitionDuration) > 0.05 || duration(style.animationDuration) > 0.05) {
          offenders.push(`${element.tagName}.${element.className}`);
        }
      }
      return offenders.slice(0, 5);
    });
    expect(animated, "reduced motion must suppress non-trivial motion").toEqual([]);
  });
});

test.describe("visual robustness", () => {
  test("state is conveyed by text or badges, not by colour alone", async ({ page, normalURL }) => {
    await page.goto(normalURL);
    await waitForGraph(page);

    // Each node states its kind in a text badge, in addition to the colour coding.
    const kinds = await page.locator(".cv-node .cv-node-badge").allInnerTexts();
    expect(kinds.length).toBeGreaterThan(0);
    for (const text of kinds) expect(text.trim().length).toBeGreaterThan(0);

    // The legend names each kind in text too.
    const legend = page.getByLabel("Legend");
    const entries = await legend.locator("li").allInnerTexts();
    expect(entries.length).toBeGreaterThan(0);
    for (const text of entries) expect(text.trim().length).toBeGreaterThan(0);
  });

  test("no horizontal clipping at the supported desktop viewport", async ({ page, normalURL }) => {
    await page.setViewportSize({ width: 1280, height: 800 });
    await page.goto(normalURL);
    await waitForGraph(page);

    const overflow = await page.evaluate(() => ({
      scrollWidth: document.documentElement.scrollWidth,
      clientWidth: document.documentElement.clientWidth,
    }));
    // The page itself must never scroll sideways at a supported desktop size.
    expect(overflow.scrollWidth).toBeLessThanOrEqual(overflow.clientWidth + 1);
  });
});
