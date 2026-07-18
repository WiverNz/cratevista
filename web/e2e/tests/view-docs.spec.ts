// PRD-08 Amendment C, in real Chromium against the real server and the real CSP.
//
// Serves the `flow` snapshot — the only committed artifact carrying schema-1.1
// `View.docs` / `View.examples` — and proves the explorer renders them, that the
// disclosure is keyboard-operable (which jsdom cannot show: it does not
// implement Enter-on-summary), and that **no `/api/source` request is made**,
// because example contents are embedded in the document.
import { expect, test } from "../support/fixtures";
import { startServer, type ServerHandle } from "../support/harness";

const VIEW = "view:flow-checkout";

let server: ServerHandle;

test.beforeAll(async () => {
  server = await startServer("flow");
});
test.afterAll(async () => {
  await server?.stop();
});

test.describe("view docs and examples", () => {
  test("renders description, docs and embedded examples without /api/source", async ({
    page,
    problems,
  }) => {
    // The server is started WITHOUT --source, so any /api/source call would 403.
    // Assert none is even attempted.
    const sourceRequests: string[] = [];
    page.on("request", (request) => {
      if (request.url().includes("/api/source")) sourceRequests.push(request.url());
    });

    await page.goto(`${server.baseURL}/?view=${VIEW}`);
    const panel = page.getByLabel("View documentation");
    await expect(panel).toBeVisible();

    // Description.
    await expect(
      panel.getByText("How an order travels from the browser to storage."),
    ).toBeVisible();

    // Docs, rendered as Markdown (a heading element, not literal "## Checkout").
    // `exact` matters: the panel's own title is "Checkout flow", which would
    // otherwise also match by substring.
    await expect(panel.getByRole("heading", { name: "Checkout", exact: true })).toBeVisible();
    await expect(panel.getByText("Clients → Gateway → Services → Infrastructure.")).toBeVisible();

    // Examples: the summary (title + language-as-text) is visible while collapsed.
    await expect(panel.getByText("Example request")).toBeVisible();
    await expect(panel.getByText("(http)")).toBeVisible();
    await expect(panel.getByText("Example response")).toBeVisible();

    // The body is collapsed until disclosed — that is the point of <details>.
    await expect(panel.getByText("What the web client sends.")).toBeHidden();
    await panel.getByText("Example request").click();

    // Disclosed: description + the embedded content itself.
    await expect(panel.getByText("What the web client sends.")).toBeVisible();
    await expect(panel.locator("pre code").first()).toContainText("POST /checkout HTTP/1.1");
    await expect(panel.locator("pre code").first()).toContainText('{"cart": 42}');

    // The whole point of embedding: no source endpoint involved.
    expect(sourceRequests, "example content must not come from /api/source").toEqual([]);
    // And no CSP violation / console error from rendering it (the `problems`
    // fixture also fails the test on any of these).
    expect(problems.csp).toEqual([]);
    expect(problems.pageErrors).toEqual([]);
  });

  test("an example discloses with the keyboard alone", async ({ page }) => {
    await page.goto(`${server.baseURL}/?view=${VIEW}`);
    const summary = page.getByText("Example request");
    await expect(summary).toBeVisible();

    const isOpen = () =>
      page.locator("details.cv-example").first().evaluate((el) => (el as HTMLDetailsElement).open);
    expect(await isOpen()).toBe(false);

    // Native <details>: focus the summary and press Enter. No pointer.
    await summary.focus();
    await page.keyboard.press("Enter");
    expect(await isOpen()).toBe(true);

    await page.keyboard.press("Enter");
    expect(await isOpen()).toBe(false);
  });

  test("the generated views render no documentation panel", async ({ normalURL, page }) => {
    // Regression: the eight generated views carry no docs/examples, so the
    // panel must not appear at all — their appearance is unchanged by schema 1.1.
    await page.goto(`${normalURL}/`);
    await expect(page.getByRole("toolbar", { name: "Graph controls" })).toBeVisible();
    await expect(page.getByLabel("View documentation")).toHaveCount(0);
  });
});
