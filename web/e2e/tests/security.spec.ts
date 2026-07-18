// Real-browser security validation against the actual served headers and assets.
import { readFileSync } from "node:fs";
import { join } from "node:path";
import { expect, test, waitForGraph } from "../support/fixtures";
import { webRoot } from "../support/harness";

/** The nine approved PRD-07 directives, exactly. */
const APPROVED_DIRECTIVES = [
  "default-src 'self'",
  "script-src 'self'",
  "style-src 'self'",
  "style-src-attr 'unsafe-inline'",
  "connect-src 'self'",
  "worker-src 'self'",
  "base-uri 'self'",
  "object-src 'none'",
  "frame-ancestors 'none'",
];

test.describe("Content-Security-Policy", () => {
  test("the served header carries exactly the nine approved directives", async ({
    page,
    normalURL,
  }) => {
    const response = await page.goto(normalURL);
    const csp = response?.headers()["content-security-policy"];
    expect(csp, "index.html must be served with a CSP").toBeTruthy();

    const directives = csp!
      .split(";")
      .map((d) => d.trim().replace(/\s+/g, " "))
      .filter(Boolean);
    expect(directives.sort()).toEqual([...APPROVED_DIRECTIVES].sort());
  });

  test("exactly one unsafe-inline token, and it belongs to style-src-attr", async ({
    page,
    normalURL,
  }) => {
    const response = await page.goto(normalURL);
    const csp = response!.headers()["content-security-policy"]!;

    // React Flow writes inline geometry via the style attribute; that is the sole
    // approved relaxation. Any other unsafe-inline would widen the policy.
    expect(csp.match(/'unsafe-inline'/g) ?? []).toHaveLength(1);
    expect(csp).toContain("style-src-attr 'unsafe-inline'");

    // Bare `style-src`/`script-src` must NOT be relaxed.
    expect(csp).toMatch(/style-src 'self'/);
    expect(csp).toMatch(/script-src 'self'/);
  });

  test("no unsafe-eval, no blob:, no remote origin, no permissive CORS", async ({
    page,
    normalURL,
  }) => {
    const response = await page.goto(normalURL);
    const headers = response!.headers();
    const csp = headers["content-security-policy"]!;

    expect(csp).not.toContain("unsafe-eval");
    expect(csp).not.toContain("blob:");
    expect(csp).not.toMatch(/https?:\/\//);
    expect(csp).not.toContain("*");
    // A local-only tool must not hand its APIs to any origin.
    expect(headers["access-control-allow-origin"]).toBeUndefined();
  });

  test("the app boots with zero CSP violations and every request same-origin", async ({
    page,
    normalURL,
    problems,
  }) => {
    const requested: string[] = [];
    page.on("request", (r) => requested.push(r.url()));

    await page.goto(normalURL);
    await waitForGraph(page);

    // The `problems` fixture fails the test on any CSP violation; assert
    // explicitly too so the intent is visible.
    expect(problems.csp).toEqual([]);
    const origin = new URL(normalURL).origin;
    const external = requested.filter((url) => !url.startsWith(origin));
    expect(external, "no external network requests").toEqual([]);
  });
});

test.describe("embedded bundle", () => {
  // rust-embed bakes the bundle in at compile time and Cargo does not track it as
  // an input, so a binary built before a bundle rebuild silently serves a stale UI.
  // Comparing served bytes to the committed bundle (now inside the server crate at
  // crates/cratevista-server/embedded/) proves the binary under test really embeds
  // the current production bundle.
  const distFile = (rel: string) =>
    readFileSync(join(webRoot, "..", "crates", "cratevista-server", "embedded", ...rel.split("/")));

  test("served assets are byte-identical to the committed embedded bundle", async ({
    request,
    normalURL,
  }) => {
    const index = distFile("index.html").toString();
    const served = await request.get(`${normalURL}/index.html`);
    expect(served.status()).toBe(200);
    expect(Buffer.from(await served.body()).equals(distFile("index.html"))).toBe(true);

    // Every hashed asset index.html references must be served, byte-for-byte.
    const referenced = [...index.matchAll(/\.\/(assets\/[^"']+)/g)].map((m) => m[1]);
    expect(referenced.length, "index.html must reference hashed assets").toBeGreaterThan(0);
    for (const rel of referenced) {
      const response = await request.get(`${normalURL}/${rel}`);
      expect(response.status(), `${rel} must be served`).toBe(200);
      expect(
        Buffer.from(await response.body()).equals(distFile(rel)),
        `${rel} must match the committed dist — rebuild the binary after 'npm run build'`,
      ).toBe(true);
    }
  });

  test("fingerprinted assets are immutably cacheable; index.html is not", async ({
    request,
    normalURL,
  }) => {
    const index = distFile("index.html").toString();
    const asset = [...index.matchAll(/\.\/(assets\/[^"']+)/g)][0][1];

    const assetResponse = await request.get(`${normalURL}/${asset}`);
    expect(assetResponse.headers()["cache-control"]).toBe("public, max-age=31536000, immutable");

    const indexResponse = await request.get(`${normalURL}/index.html`);
    expect(indexResponse.headers()["cache-control"]).toBe("no-cache");
  });
});
