import { describe, it, expect } from "vitest";
import { render } from "@testing-library/react";
import { SafeMarkdown } from "../src/markdown/SafeMarkdown.tsx";

function html(md: string): string {
  const { container } = render(<SafeMarkdown>{md}</SafeMarkdown>);
  return container.innerHTML;
}

describe("SafeMarkdown sanitization", () => {
  it("renders safe markdown (gfm)", () => {
    const { container } = render(
      <SafeMarkdown>{"**bold** and `code`\n\n- a\n- b"}</SafeMarkdown>,
    );
    expect(container.querySelector("strong")?.textContent).toBe("bold");
    expect(container.querySelector("code")).not.toBeNull();
    expect(container.querySelectorAll("li").length).toBe(2);
  });

  it("strips <script>", () => {
    const { container } = render(
      <SafeMarkdown>{"before<script>window.__x=1</script>after"}</SafeMarkdown>,
    );
    expect(container.querySelector("script")).toBeNull();
    expect((window as unknown as { __x?: number }).__x).toBeUndefined();
  });

  it("strips event-handler attributes (onerror)", () => {
    const { container } = render(
      <SafeMarkdown>{'<img src="x" onerror="window.__y=1">'}</SafeMarkdown>,
    );
    const img = container.querySelector("img");
    if (img) expect(img.getAttribute("onerror")).toBeNull();
    expect(container.innerHTML.toLowerCase()).not.toContain("onerror");
  });

  it("neutralizes javascript: links", () => {
    const out = html("[click](javascript:window.__z=1)");
    expect(out.toLowerCase()).not.toContain("javascript:");
  });

  it("neutralizes encoded javascript: links", () => {
    const out = html("[click](javascript%3Aalert%281%29)");
    expect(out.toLowerCase()).not.toContain("javascript:");
    // rendered as inert text/span, not an active anchor href
    expect(out).not.toMatch(/href="javascript/i);
  });

  it("tolerates malformed HTML without crashing", () => {
    expect(() => html("<div><span>oops")).not.toThrow();
  });

  it("hardens allowed external links", () => {
    const { container } = render(
      <SafeMarkdown>{"[ex](https://example.com)"}</SafeMarkdown>,
    );
    const a = container.querySelector("a");
    expect(a?.getAttribute("href")).toBe("https://example.com");
    expect(a?.getAttribute("rel")).toBe("noopener noreferrer");
    expect(a?.getAttribute("target")).toBe("_blank");
  });

  it("allows same-origin relative links without target", () => {
    const { container } = render(
      <SafeMarkdown>{"[rel](./page)"}</SafeMarkdown>,
    );
    const a = container.querySelector("a");
    expect(a?.getAttribute("href")).toBe("./page");
    expect(a?.getAttribute("target")).toBeNull();
  });
});
