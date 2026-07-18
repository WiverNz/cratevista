// PRD-08 Amendment C: rendering the active view's description, docs and
// embedded examples (schema 1.1 `View.docs` / `View.examples`).
import { describe, expect, it, vi } from "vitest";
vi.mock("@xyflow/react", () => import("./support/xyflow.tsx"));

import { screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";

import { okOutcome, renderApp, sampleDocument } from "./support/harness.tsx";
import type { ExplorerDocument } from "../src/types/index.ts";

/** A document whose `view:types` carries description + docs + examples. */
function documentWithViewDocs(overrides: {
  description?: unknown;
  markdown?: string;
  examples?: unknown[];
}): ExplorerDocument {
  const document = sampleDocument();
  const view = document.views.find((v) => v.id === "view:types")!;
  if (overrides.description !== undefined) {
    (view as Record<string, unknown>).description = overrides.description;
  }
  if (overrides.markdown !== undefined) {
    (view as Record<string, unknown>).docs = {
      markdown: overrides.markdown,
      documented: true,
    };
  }
  if (overrides.examples !== undefined) {
    (view as Record<string, unknown>).examples = overrides.examples;
  }
  return document;
}

const render = (document: ExplorerDocument) =>
  renderApp({ outcome: okOutcome({ document }), search: "?view=view:types" });

describe("view documentation", () => {
  it("renders the active view's description", async () => {
    render(documentWithViewDocs({ description: { default: "How checkout works." } }));
    expect(await screen.findByText("How checkout works.")).toBeInTheDocument();
  });

  it("renders view docs as sanitized Markdown", async () => {
    render(documentWithViewDocs({ markdown: "# Flow\n\nClients **then** gateway." }));
    // Markdown is parsed, not shown raw.
    expect(await screen.findByRole("heading", { name: "Flow" })).toBeInTheDocument();
    expect(screen.getByText("then")).toBeInTheDocument();
    expect(screen.queryByText("# Flow")).not.toBeInTheDocument();
  });

  it("renders examples as collapsible sections with title, language and content", async () => {
    render(
      documentWithViewDocs({
        examples: [
          {
            id: "req",
            title: { default: "Request" },
            language: "http",
            content: "POST /checkout HTTP/1.1",
            description: { default: "A sample request." },
          },
          { id: "res", title: { default: "Response" }, content: "{\"ok\":true}" },
        ],
      }),
    );

    // Native <details>/<summary>: keyboard-operable with no ARIA of our own.
    const request = await screen.findByText("Request");
    const details = request.closest("details")!;
    expect(details).toBeInTheDocument();
    expect(details.open).toBe(false);

    // The language is stated as text, not conveyed by colour alone.
    expect(screen.getByText("(http)")).toBeInTheDocument();
    expect(screen.getByText("A sample request.")).toBeInTheDocument();
    expect(screen.getByText("POST /checkout HTTP/1.1")).toBeInTheDocument();

    // A second example without language/description still renders.
    expect(screen.getByText("Response")).toBeInTheDocument();
    expect(screen.getByText('{"ok":true}')).toBeInTheDocument();
  });

  it("an example discloses its content when activated, and its summary is focusable", async () => {
    // jsdom implements <details> toggling on CLICK but not Enter-on-summary, so
    // keyboard activation is asserted in the real browser instead
    // (`e2e/tests/view-docs.spec.ts`). Here we prove the disclosure works and the
    // summary is reachable by keyboard.
    const user = userEvent.setup();
    render(
      documentWithViewDocs({
        examples: [{ id: "e", title: { default: "Payload" }, content: "body" }],
      }),
    );
    const summary = await screen.findByText("Payload");
    const details = summary.closest("details")!;
    expect(details.open).toBe(false);

    summary.focus();
    expect(summary.closest("summary")).toHaveFocus();

    await user.click(summary);
    expect(details.open).toBe(true);
  });

  it("renders nothing when the view has no description, docs or examples", async () => {
    // The eight generated views carry none of these: their appearance must be
    // byte-for-byte what it was before schema 1.1.
    renderApp({ search: "?view=view:types" });
    expect(await screen.findByRole("tablist", { name: "Views" })).toBeInTheDocument();
    expect(screen.queryByLabelText("View documentation")).not.toBeInTheDocument();
  });

  it("renders nothing when docs markdown is only whitespace", async () => {
    render(documentWithViewDocs({ markdown: "   \n\n  " }));
    expect(await screen.findByRole("tablist", { name: "Views" })).toBeInTheDocument();
    expect(screen.queryByLabelText("View documentation")).not.toBeInTheDocument();
  });
});

describe("view documentation sanitization", () => {
  it("strips scripts and event handlers from hostile view docs", async () => {
    render(
      documentWithViewDocs({
        markdown: [
          "# Title",
          "",
          "<script>window.__pwned = 1;</script>",
          "",
          '<img src="x" onerror="window.__pwned = 2">',
          "",
          '<a href="javascript:window.__pwned=3">click</a>',
          "",
          "<iframe src=\"https://evil.example\"></iframe>",
        ].join("\n"),
      }),
    );

    await screen.findByRole("heading", { name: "Title" });
    const container = screen.getByLabelText("View documentation");

    // No executable or embedding markup survives rehype-sanitize.
    expect(container.querySelector("script")).toBeNull();
    expect(container.querySelector("iframe")).toBeNull();
    expect(container.querySelector("[onerror]")).toBeNull();
    // And nothing executed.
    expect((window as unknown as { __pwned?: number }).__pwned).toBeUndefined();

    // A javascript: link is never rendered as a navigable anchor.
    for (const anchor of container.querySelectorAll("a")) {
      expect(anchor.getAttribute("href") ?? "").not.toMatch(/^javascript:/i);
    }
  });

  it("renders hostile example content as text, never as markup", async () => {
    // An example may legitimately contain markup as its sample payload — e.g. a
    // captured HTTP response. It must appear as characters, not be parsed.
    const hostile = '</code></pre><script>window.__pwned = 4;</script><img src=x onerror=alert(1)>';
    render(
      documentWithViewDocs({
        examples: [{ id: "h", title: { default: "Hostile" }, content: hostile }],
      }),
    );

    const container = await screen.findByLabelText("View documentation");
    // The literal text is present…
    expect(container.textContent).toContain("<script>");
    expect(container.textContent).toContain("onerror=alert(1)");
    // …but no element was created from it, and nothing ran.
    expect(container.querySelector("script")).toBeNull();
    expect(container.querySelector("img")).toBeNull();
    expect((window as unknown as { __pwned?: number }).__pwned).toBeUndefined();
    // Content stays inside the code block.
    expect(container.querySelector("pre code")?.textContent).toBe(hostile);
  });

  it("does not treat example content as Markdown", async () => {
    render(
      documentWithViewDocs({
        examples: [{ id: "m", title: { default: "Md" }, content: "# not a heading\n**not bold**" }],
      }),
    );
    const container = await screen.findByLabelText("View documentation");
    // Rendered verbatim inside <pre><code>, so the Markdown syntax survives.
    expect(container.querySelector("pre code")?.textContent).toContain("# not a heading");
    expect(container.querySelector("pre code")?.textContent).toContain("**not bold**");
    expect(container.querySelector("pre strong")).toBeNull();
  });
});
